use std::sync::atomic::{AtomicU64, Ordering};

use derive_more::{Deref, DerefMut};
use futures::{AsyncRead, AsyncWrite};

use crate::id::NetworkId;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

#[derive(Deref, DerefMut)]
pub(crate) struct Connection<R> {
    #[deref]
    #[deref_mut]
    pub(crate) value: R,
    connection_id: NetworkId,
}

impl<R> Connection<R> {
    pub(crate) fn new(io: R) -> Self {
        Self {
            value: io,
            connection_id: NetworkId::from(next_id()),
        }
    }

    pub(crate) fn connection_id(&self) -> NetworkId {
        self.connection_id
    }

    pub(crate) fn map<T, F>(&self, f: F) -> Connection<T>
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
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
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

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        AsyncWrite::poll_close(std::pin::Pin::new(&mut self.value), cx)
    }
}
