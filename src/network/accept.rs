use std::{
    io::ErrorKind,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{Future, FutureExt};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{mpsc, oneshot},
};

pub struct Accept<'a, A: ?Sized> {
    acceptor: &'a mut A,
}

impl<'a, A> Future for Accept<'a, A>
where
    A: AsyncAccept + Unpin,
{
    type Output = std::io::Result<A::Connection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        AsyncAccept::poll_accept(Pin::new(self.acceptor), cx)
    }
}

pub trait AsyncAccept {
    type Connection;
    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<Self::Connection>>;
}

impl AsyncAccept for TcpListener {
    type Connection = TcpStream;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<TcpStream>> {
        TcpListener::poll_accept(&self, cx).map(|f| f.map(|t| t.0))
    }
}

pub struct TcpConnector<T: ToSocketAddrs + Send + 'static> {
    connect_to: mpsc::Receiver<T>,
    stream_rx: Option<oneshot::Receiver<std::io::Result<TcpStream>>>,
}

impl<T: ToSocketAddrs + Send + 'static> TcpConnector<T> {
    pub fn new(connect_to: mpsc::Receiver<T>) -> Self {
        Self {
            connect_to,
            stream_rx: None,
        }
    }
}

impl<T: ToSocketAddrs + Send + 'static + Clone> AsyncAccept for TcpConnector<T> {
    type Connection = TcpStream;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<TcpStream>> {
        if let Some(stream_rx) = &mut self.stream_rx {
            // unwrap is fine here since the oneshot is known to never drop without sending
            let stream = futures::ready!(stream_rx.poll_unpin(cx).map(|e| e.unwrap()));
            let _ = self.stream_rx.take();
            return Poll::Ready(stream);
        }

        match futures::ready!(self.connect_to.poll_recv(cx)) {
            Some(addr) => {
                let waker = cx.waker().clone();
                let (tx, rx) = oneshot::channel();
                self.stream_rx = Some(rx);

                // TODO: Is there a better way to do this?
                tokio::task::spawn(async move {
                    loop {
                        let addrx = addr.clone();
                        let stream = TcpStream::connect(addrx).await;
                        if stream.is_err() {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        } else {
                            let _ = tx.send(stream);
                            waker.wake();
                            break;
                        }
                    }
                });

                Poll::Pending
            }
            None => Poll::Ready(Err(ErrorKind::UnexpectedEof.into())),
        }
    }
}

pub trait AsyncAcceptExt: AsyncAccept {
    fn accept(&mut self) -> Accept<'_, Self> {
        Accept { acceptor: self }
    }
}

impl<T> AsyncAcceptExt for T where T: AsyncAccept {}
