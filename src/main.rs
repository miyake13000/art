use futures::{
    future::{BoxFuture, FutureExt},
    task::{ArcWake, waker_ref},
};
use nix::{
    errno::Errno,
    poll::PollTimeout,
    sys::{
        epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
        eventfd::EventFd,
    },
    unistd::{read, write},
};
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    io::{BufRead, BufReader, BufWriter, Write},
    net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    os::{
        fd::{AsFd, BorrowedFd},
        unix::io::{AsRawFd, RawFd},
    },
    pin::Pin,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender}, Arc, Mutex
    },
    task::{Context, Poll, Waker},
};

fn write_eventfd<Fd: AsFd>(fd: Fd, n: usize) {
    // usizeを*const u8に変換
    let ptr = &n as *const usize as *const u8;
    let val = unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of_val(&n)) };
    // writeシステムコール呼び出し
    write(fd, val).unwrap();
}

#[allow(clippy::upper_case_acronyms)]
enum IOOps {
    ADD(EpollFlags, RawFd, Waker),
    REMOVE(RawFd),
}

struct IOSelector {
    wakers: Mutex<HashMap<RawFd, Waker>>,
    queue: Mutex<VecDeque<IOOps>>,
    epoll: Epoll,
    event: EventFd,
}

impl IOSelector {
    fn new() -> Arc<Self> {
        let s = IOSelector {
            wakers: Mutex::new(HashMap::new()),
            queue: Mutex::new(VecDeque::new()),
            epoll: Epoll::new(EpollCreateFlags::empty()).unwrap(),
            event: EventFd::from_value(0).unwrap(),
        };
        let result = Arc::new(s);
        let s = result.clone();

        // epoll用のスレッド生成
        std::thread::spawn(move || s.select());

        result
    }

    // epollで監視するための関数
    fn add_event(
        &self,
        flag: EpollFlags,
        raw_fd: RawFd,
        waker: Waker,
        wakers: &mut HashMap<RawFd, Waker>,
    ) {
        let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        // EPOLLONESHOTを指定して、一度イベントが発生すると
        // そのfdへのイベントは再設定するまで通知されないようになる
        let mut ev = EpollEvent::new(flag | EpollFlags::EPOLLONESHOT, raw_fd as u64);

        // 監視対象に追加
        if let Err(err) = self.epoll.add(fd, ev) {
            match err {
                Errno::EEXIST => {
                    // すでに追加されていた場合は再設定
                    self.epoll.modify(fd, &mut ev).unwrap();
                }
                _ => {
                    panic!("epoll_ctl: {}", err);
                }
            }
        }

        assert!(!wakers.contains_key(&raw_fd));
        wakers.insert(raw_fd, waker);
    }

    // epollの監視から削除するための関数
    fn rm_event(&self, raw_fd: RawFd, wakers: &mut HashMap<RawFd, Waker>) {
        let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        self.epoll.delete(fd).ok();
        wakers.remove(&raw_fd);
    }

    fn select(&self) {
        // eventfdをepollの監視対象に追加
        let ev = EpollEvent::new(EpollFlags::EPOLLIN, self.event.as_raw_fd() as u64);
        self.epoll.add(&self.event, ev).unwrap();

        // event発生を監視
        let mut events = vec![EpollEvent::empty(); 1024];
        while let Ok(nfds) = self.epoll.wait(&mut events, PollTimeout::NONE) {
            let mut t = self.wakers.lock().unwrap();
            for event in events.iter().take(nfds) {
                if event.data() == self.event.as_raw_fd() as u64 {
                    // eventfdの場合、追加、削除要求を処理
                    let mut q = self.queue.lock().unwrap();
                    while let Some(op) = q.pop_front() {
                        match op {
                            // 追加
                            IOOps::ADD(flag, fd, waker) => {
                                self.add_event(flag, fd, waker, &mut t);
                            }
                            // 削除
                            IOOps::REMOVE(fd) => self.rm_event(fd, &mut t),
                        }
                    }
                    let mut buf: [u8; 8] = [0; 8];
                    read(&self.event, &mut buf).unwrap(); // eventfdの通知解除
                } else {
                    // 実行キューに追加
                    let data = event.data() as i32;
                    let waker = t.remove(&data).unwrap();
                    waker.wake_by_ref();
                }
            }
        }
    }

    // ファイルディスクリプタ登録用関数
    fn register(&self, flags: EpollFlags, fd: RawFd, waker: Waker) {
        let mut q = self.queue.lock().unwrap();
        q.push_back(IOOps::ADD(flags, fd, waker));
        write_eventfd(&self.event, 1);
    }

    // ファイルディスクリプタ削除用関数
    fn unregister(&self, fd: RawFd) {
        let mut q = self.queue.lock().unwrap();
        q.push_back(IOOps::REMOVE(fd));
        write_eventfd(&self.event, 1);
    }
}

struct AsyncListener {
    listener: TcpListener,
    selector: Arc<IOSelector>,
}

impl AsyncListener {
    // TcpListenerの初期化処理をラップした関数
    fn listen<A: ToSocketAddrs>(addr: A, selector: Arc<IOSelector>) -> AsyncListener {
        // リッスンアドレスを指定
        let listener = TcpListener::bind(addr).unwrap();

        // ノンブロッキングに指定
        listener.set_nonblocking(true).unwrap();

        AsyncListener { listener, selector }
    }

    // コネクションをアクセプトするためのFutureをリターン
    fn accept(&'_ self) -> Accept<'_> {
        Accept { listener: self }
    }
}

impl Drop for AsyncListener {
    fn drop(&mut self) {
        self.selector.unregister(self.listener.as_raw_fd());
    }
}

struct Accept<'a> {
    listener: &'a AsyncListener,
}

impl<'a> Future for Accept<'a> {
    // 返り値の型
    type Output = (AsyncReader, BufWriter<TcpStream>, SocketAddr);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // アクセプトをノンブロッキングで実行
        match self.listener.listener.accept() {
            Ok((stream, addr)) => {
                // アクセプトした場合は読み込みと書き込み用オブジェクトおよびアドレスをリターン
                let stream0 = stream.try_clone().unwrap();
                Poll::Ready((
                    AsyncReader::new(stream0, self.listener.selector.clone()),
                    BufWriter::new(stream),
                    addr,
                ))
            }
            Err(err) => {
                // アクセプトすべきコネクションがない場合はepollに登録
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    self.listener.selector.register(
                        EpollFlags::EPOLLIN,
                        self.listener.listener.as_raw_fd(),
                        cx.waker().clone(),
                    );
                    Poll::Pending
                } else {
                    panic!("accept: {}", err);
                }
            }
        }
    }
}

struct AsyncReader {
    reader: BufReader<TcpStream>,
    reader_fd: RawFd,
    selector: Arc<IOSelector>,
}

impl AsyncReader {
    fn new(stream: TcpStream, selector: Arc<IOSelector>) -> AsyncReader {
        // ノンブロッキングに設定
        stream.set_nonblocking(true).unwrap();

        let reader_fd = stream.as_raw_fd();
        let reader = BufReader::new(stream);
        AsyncReader {
            reader,
            reader_fd,
            selector,
        }
    }

    // 1行読み込みのためのFutureをリターン
    fn read_line(&'_ mut self) -> ReadLine<'_> {
        ReadLine { reader: self }
    }
}

impl Drop for AsyncReader {
    fn drop(&mut self) {
        self.selector.unregister(self.reader_fd);
    }
}

struct ReadLine<'a> {
    reader: &'a mut AsyncReader,
}

impl<'a> Future for ReadLine<'a> {
    // 返り値の型
    type Output = Option<String>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut line = String::new();
        // 非同期読み込み
        match self.reader.reader.read_line(&mut line) {
            Ok(0) => Poll::Ready(None),       // コネクションクローズ
            Ok(_) => Poll::Ready(Some(line)), // 1行読み込み成功
            Err(err) => {
                // 読み込みできない場合はepollに登録
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    self.reader.selector.register(
                        EpollFlags::EPOLLIN,
                        self.reader.reader_fd,
                        cx.waker().clone(),
                    );
                    Poll::Pending
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}

struct Task {
    // 実行するコルーチン
    future: Mutex<BoxFuture<'static, ()>>,
    // Executorへスケジューリングするためのチャネル
    sender: SyncSender<Arc<Task>>,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // 自身をスケジューリング
        let self0 = arc_self.clone();
        arc_self.sender.send(self0).unwrap();
    }
}

struct Executor {
    // 実行キュー
    sender: SyncSender<Arc<Task>>,
    receiver: Receiver<Arc<Task>>,
}

impl Executor {
    fn new() -> Self {
        // チャネルを生成。キューのサイズは最大1024個
        let (sender, receiver) = sync_channel(1024);
        Executor {
            sender: sender.clone(),
            receiver,
        }
    }

    // 新たにTaskを生成するためのSpawnerを作成
    fn get_spawner(&self) -> Spawner {
        Spawner {
            sender: self.sender.clone(),
        }
    }

    fn run(&self) {
        // チャネルからTaskを受信して順に実行
        while let Ok(task) = self.receiver.recv() {
            // コンテキストを生成
            let mut future = task.future.lock().unwrap();
            let waker = waker_ref(&task);
            let mut ctx = Context::from_waker(&waker);
            // pollを呼び出し実行
            let _ = future.as_mut().poll(&mut ctx);
        }
    }
}

struct Spawner {
    sender: SyncSender<Arc<Task>>,
}

impl Spawner {
    fn spawn(&self, future: impl Future<Output = ()> + 'static + Send) {
        let future = future.boxed();
        let task = Arc::new(Task {
            future: Mutex::new(future),
            sender: self.sender.clone(),
        });

        // 実行キューにエンキュー
        self.sender.send(task).unwrap();
    }
}

fn main() {
    let executor = Executor::new();
    let selector = IOSelector::new();
    let spawner = executor.get_spawner();

    let addr = (std::net::Ipv4Addr::new(0, 0, 0, 0), 8000);

    let server = async move {
        // 非同期アクセプト用のリスナを生成
        let listener = AsyncListener::listen(addr, selector.clone());
        loop {
            // 非同期コネクションアクセプト
            let (mut reader, mut writer, addr) = listener.accept().await;
            println!("accept: {}", addr);

            // コネクションごとにタスクを生成
            spawner.spawn(async move {
                // 1行非同期読み込み
                while let Some(buf) = reader.read_line().await {
                    print!("read: {}, {}", addr, buf);
                    writer.write_all(buf.as_bytes()).unwrap();
                    writer.flush().unwrap();
                }
                println!("close: {}", addr);
            });
        }
    };

    // タスクを生成して実行
    executor.get_spawner().spawn(server);
    executor.run();
}
