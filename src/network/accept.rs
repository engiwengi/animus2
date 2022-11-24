use std::{
    io::ErrorKind,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bevy::tasks::IoTaskPool;
use futures::{Future, FutureExt, StreamExt};
use quinn::{Endpoint, ServerConfig};

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

pub struct QuicListener {
    stream_rx: Option<futures::channel::oneshot::Receiver<std::io::Result<quinn::Connection>>>,
    endpoint: Endpoint,
}
fn generate_self_signed_cert(
) -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn std::error::Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["test".to_string()])?;
    let key = rustls::PrivateKey(cert.serialize_private_key_der());
    Ok((rustls::Certificate(cert.serialize_der()?), key))
}
impl QuicListener {
    pub fn new(addr: SocketAddr) -> Self {
        let (cert, key) = generate_self_signed_cert().unwrap();
        let server_config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();

        Self {
            stream_rx: None,
            endpoint: Endpoint::server(ServerConfig::with_crypto(Arc::new(server_config)), addr)
                .unwrap(),
        }
    }
}
impl AsyncAccept for QuicListener {
    type Connection = quinn::Connection;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<quinn::Connection>> {
        {
            if let Some(stream_rx) = &mut self.stream_rx {
                // unwrap is fine here since the oneshot is known to never drop without sending
                let stream = futures::ready!(stream_rx.poll_unpin(cx).map(|e| e.unwrap()));
                let _ = self.stream_rx.take();
                return Poll::Ready(stream);
            }

            let waker = cx.waker().clone();
            let (tx, rx) = futures::channel::oneshot::channel();
            self.stream_rx = Some(rx);

            let endpoint = self.endpoint.clone();

            let pool = IoTaskPool::get();
            // TODO: Is there a better way to do this?
            pool.spawn(async move {
                loop {
                    let stream = endpoint.accept().await;
                    if stream.is_none() {
                        async_std::task::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    let stream = stream.unwrap().await;
                    if stream.is_err() {
                        async_std::task::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    let _ = tx.send(stream.map_err(|err| err.into()));
                    waker.wake();
                    break;
                }
            })
            .detach();

            Poll::Pending
        }
    }
}

pub struct QuicConnector {
    connect_to: async_std::channel::Receiver<SocketAddr>,
    stream_rx: Option<futures::channel::oneshot::Receiver<std::io::Result<quinn::Connection>>>,
    pub endpoint: Endpoint,
}

impl QuicConnector {
    pub fn new(connect_to: async_std::channel::Receiver<SocketAddr>) -> Self {
        // TODO should not do this
        let crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth();
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
        Self {
            connect_to,
            stream_rx: None,
            endpoint,
        }
    }
}

impl AsyncAccept for QuicConnector {
    type Connection = quinn::Connection;

    fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<quinn::Connection>> {
        if let Some(stream_rx) = &mut self.stream_rx {
            // unwrap is fine here since the oneshot is known to never drop without sending
            let stream = futures::ready!(stream_rx.poll_unpin(cx).map(|e| e.unwrap()));
            let _ = self.stream_rx.take();
            return Poll::Ready(stream);
        }

        match futures::ready!(self.connect_to.poll_next_unpin(cx)) {
            Some(addr) => {
                let waker = cx.waker().clone();
                let (tx, rx) = futures::channel::oneshot::channel();
                self.stream_rx = Some(rx);

                let endpoint = self.endpoint.clone();
                let pool = IoTaskPool::get();

                // TODO: Is there a better way to do this?
                pool.spawn(async move {
                    loop {
                        let addrx = addr;
                        let stream = endpoint.connect(addrx, "test");
                        if stream.is_err() {
                            async_std::task::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                        let stream = stream.unwrap().await;
                        if stream.is_err() {
                            async_std::task::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                        let _ = tx.send(stream.map_err(|err| err.into()));
                        waker.wake();
                        break;
                    }
                })
                .detach();

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

struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

#[cfg(test)]
mod tests {
    // use std::time::Duration;
    //
    // use rstest::rstest;
    // use tracing_test::traced_test;
    //
    // use super::*;
    // use crate::network::test_utils::next_local_addr;
    //
    // #[rstest]
    // #[timeout(Duration::from_secs(5))]
    // #[tokio::test]
    // #[traced_test]
    // async fn server_and_client() {
    //     let addr = next_local_addr();
    //
    //     // server
    //     let addrx = addr.clone();
    //     let server = tokio::task::spawn(async move {
    //         let mut server = QuicListener::new(addrx.parse().unwrap());
    //         let _conn = server.accept().await.unwrap();
    //     });
    //
    //     // client
    //     let (s, r) = mpsc::channel(1);
    //     let client = tokio::task::spawn(async move {
    //         let mut client = QuicConnector::new(r);
    //
    //         let _conn = client.accept().await.unwrap();
    //     });
    //
    //     s.send(addr.parse().unwrap()).await.unwrap();
    //
    //     let (a, b) = futures::join!(client, server);
    //     a.unwrap();
    //     b.unwrap();
    // }
}
