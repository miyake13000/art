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
    os::{
        fd::{AsFd, BorrowedFd},
        unix::io::{AsRawFd, RawFd},
    },
    sync::{Arc, Mutex},
    task::Waker,
};

fn write_eventfd<Fd: AsFd>(fd: Fd, n: usize) {
    // usizeを*const u8に変換
    let ptr = &n as *const usize as *const u8;
    let val = unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of_val(&n)) };
    // writeシステムコール呼び出し
    write(fd, val).unwrap();
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub(crate) enum IOOps {
    ADD(EpollFlags, RawFd, Waker),
    REMOVE(RawFd),
}

#[derive(Debug)]
pub(crate) struct IOSelector {
    wakers: Mutex<HashMap<RawFd, Waker>>,
    queue: Mutex<VecDeque<IOOps>>,
    epoll: Epoll,
    event: EventFd,
}

impl IOSelector {
    pub(crate) fn new() -> Arc<Self> {
        let selector = Arc::new(IOSelector {
            wakers: Mutex::new(HashMap::new()),
            queue: Mutex::new(VecDeque::new()),
            epoll: Epoll::new(EpollCreateFlags::empty()).unwrap(),
            event: EventFd::from_value(0).unwrap(),
        });

        let s = selector.clone();
        // epoll用のスレッド生成
        std::thread::spawn(move || s.select());

        selector
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
    pub(crate) fn register(&self, flags: EpollFlags, fd: RawFd, waker: Waker) {
        let mut q = self.queue.lock().unwrap();
        q.push_back(IOOps::ADD(flags, fd, waker));
        write_eventfd(&self.event, 1);
    }

    // ファイルディスクリプタ削除用関数
    pub(crate) fn unregister(&self, fd: RawFd) {
        let mut q = self.queue.lock().unwrap();
        q.push_back(IOOps::REMOVE(fd));
        write_eventfd(&self.event, 1);
    }
}
