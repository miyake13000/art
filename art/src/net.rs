use super::SELECTOR;
use nix::sys::epoll::EpollFlags;
use std::{
    future::Future,
    io::{self, Read, Write},
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
    pub fn listen<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        // リッスンアドレスを指定
        let listener = net::TcpListener::bind(addr)?;

        // ノンブロッキングに指定
        listener.set_nonblocking(true)?;

        Ok(Self { listener })
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
    type Output = io::Result<(TcpStream, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // アクセプトをノンブロッキングで実行
        match self.listener.listener.accept() {
            Ok((stream, addr)) => {
                // アクセプトした場合は読み込みと書き込み用オブジェクトおよびアドレスをリターン
                Poll::Ready(Ok((TcpStream::new(stream), addr)))
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
                    Poll::Ready(Err(err))
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct TcpStream {
    inner: net::TcpStream,
    fd: RawFd,
}

impl TcpStream {
    fn new(inner: net::TcpStream) -> Self {
        // ノンブロッキングに設定
        inner.set_nonblocking(true).unwrap();
        let fd = inner.as_raw_fd();

        Self { inner, fd }
    }

    // 1行読み込みのためのFutureをリターン
    pub fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadFuture<'a> {
        ReadFuture { reader: self, buf }
    }

    pub fn write<'a>(&'a mut self, buf: &'a [u8]) -> WriteFuture<'a> {
        WriteFuture { writer: self, buf }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        SELECTOR
            .get()
            .expect("Selector is not initialized")
            .unregister(self.fd);
    }
}

#[derive(Debug)]
pub struct ReadFuture<'a> {
    reader: &'a mut TcpStream,
    buf: &'a mut [u8],
}

impl<'a> Future for ReadFuture<'a> {
    // 返り値の型
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 非同期読み込み
        let this = self.as_mut().get_mut();
        match this.reader.inner.read(this.buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(err) => {
                // 読み込みできない場合はepollに登録
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    SELECTOR
                        .get()
                        .expect("Selector is not initialized")
                        .register(EpollFlags::EPOLLIN, this.reader.fd, cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(Err(err))
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct WriteFuture<'a> {
    writer: &'a mut TcpStream,
    buf: &'a [u8],
}

impl<'a> Future for WriteFuture<'a> {
    // 返り値の型
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 非同期読み込み
        let this = self.as_mut().get_mut();
        match this.writer.inner.write(this.buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(err) => {
                // 読み込みできない場合はepollに登録
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    SELECTOR
                        .get()
                        .expect("Selector is not initialized")
                        .register(EpollFlags::EPOLLOUT, this.writer.fd, cx.waker().clone());
                    Poll::Pending
                } else {
                    Poll::Ready(Err(err))
                }
            }
        }
    }
}
