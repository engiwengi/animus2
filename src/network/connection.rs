use std::sync::atomic::{AtomicU64, Ordering};

use derive_more::{Deref, DerefMut};
use tokio::io::{AsyncRead, AsyncWrite};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

#[derive(Deref, DerefMut)]
pub struct Connection<R> {
    #[deref]
    #[deref_mut]
    pub value: R,
    connection_id: u64,
}

impl<R> Connection<R> {
    pub fn new(io: R) -> Self {
        Self {
            value: io,
            connection_id: next_id(),
        }
    }

    pub fn connection_id(&self) -> u64 {
        self.connection_id
    }

    pub fn map<T, F>(&self, f: F) -> Connection<T>
    where
        F: Fn(&R) -> T,
    {
        Connection::<T> {
            value: f(&self.value),
            connection_id: self.connection_id,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Connection<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncRead::poll_read(std::pin::Pin::new(&mut self.value), cx, buf)
    }
}

impl<R: AsyncWrite + Unpin> AsyncWrite for Connection<R> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        AsyncWrite::poll_write(std::pin::Pin::new(&mut self.value), cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.value), cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncWrite::poll_shutdown(std::pin::Pin::new(&mut self.value), cx)
    }
}
