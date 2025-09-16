use super::SELECTOR;
use nix::sys::epoll::EpollFlags;
use std::{
    future::Future,
    io::{BufRead, BufReader, BufWriter},
    net::{self, SocketAddr, ToSocketAddrs},
    os::unix::io::{AsRawFd, RawFd},
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug)]
pub struct TcpListener {
    listener: net::TcpListener,
}

impl TcpListener {
    // TcpListenerの初期化処理をラップした関数
    pub fn listen<A: ToSocketAddrs>(addr: A) -> Self {
        // リッスンアドレスを指定
        let listener = net::TcpListener::bind(addr).unwrap();

        // ノンブロッキングに指定
        listener.set_nonblocking(true).unwrap();

        Self { listener }
    }

    // コネクションをアクセプトするためのFutureをリターン
    pub fn accept(&'_ self) -> Accept<'_> {
        Accept { listener: self }
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        SELECTOR
            .get()
            .expect("Selector is not initialized")
            .unregister(self.listener.as_raw_fd());
    }
}

#[derive(Debug)]
pub struct Accept<'a> {
    listener: &'a TcpListener,
}

impl<'a> Future for Accept<'a> {
    // 返り値の型
    type Output = (TcpStream, BufWriter<net::TcpStream>, SocketAddr);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // アクセプトをノンブロッキングで実行
        match self.listener.listener.accept() {
            Ok((stream, addr)) => {
                // アクセプトした場合は読み込みと書き込み用オブジェクトおよびアドレスをリターン
                let stream0 = stream.try_clone().unwrap();
                Poll::Ready((TcpStream::new(stream0), BufWriter::new(stream), addr))
            }
            Err(err) => {
                // アクセプトすべきコネクションがない場合はepollに登録
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    SELECTOR
                        .get()
                        .expect("Selector is not initialized")
                        .register(
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

#[derive(Debug)]
pub struct TcpStream {
    reader: BufReader<net::TcpStream>,
    reader_fd: RawFd,
}

impl TcpStream {
    fn new(stream: net::TcpStream) -> Self {
        // ノンブロッキングに設定
        stream.set_nonblocking(true).unwrap();

        let reader_fd = stream.as_raw_fd();
        let reader = BufReader::new(stream);
        Self { reader, reader_fd }
    }

    // 1行読み込みのためのFutureをリターン
    pub fn read_line(&'_ mut self) -> ReadLine<'_> {
        ReadLine { reader: self }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        SELECTOR
            .get()
            .expect("Selector is not initialized")
            .unregister(self.reader_fd);
    }
}

#[derive(Debug)]
pub struct ReadLine<'a> {
    reader: &'a mut TcpStream,
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
                    SELECTOR
                        .get()
                        .expect("Selector is not initialized")
                        .register(
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
