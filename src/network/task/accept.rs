use crossbeam_channel::Sender;
use futures::{pin_mut, FutureExt};
use tokio::sync::watch;
use tracing::{error, info};

use crate::network::{
    accept::{AsyncAccept, AsyncAcceptExt},
    connection::Connection,
};

pub struct AcceptConnectionsTask<A>
where
    A: AsyncAccept,
{
    acceptor: A,
    new_connections: Sender<Connection<<A as AsyncAccept>::Connection>>,
}

impl<A> AcceptConnectionsTask<A>
where
    A: AsyncAccept + Unpin,
{
    pub fn new(acceptor: A, new_connections: Sender<Connection<A::Connection>>) -> Self {
        Self {
            acceptor,
            new_connections,
        }
    }

    pub async fn _run(mut self, mut stop: watch::Receiver<()>) {
        let stop = stop.changed().fuse();
        pin_mut!(stop);
        loop {
            let maybe_stream = futures::select! {
                maybe_stream = &mut self.acceptor.accept().fuse() => maybe_stream,
                _ = stop => {
                    break;
                },
            };

            let stream = match maybe_stream {
                Ok(socket) => socket,
                Err(e) => {
                    error!("Failed to accept new connection: {}", e);
                    break;
                }
            };

            info!("New incoming connection");
            match self.new_connections.send(Connection::new(stream)) {
                Ok(_) => continue,
                Err(e) => {
                    error!("Failed to send new connection: {}", e);
                    break;
                }
            };
        }
        info!("Disconnecting accept connections task");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use quinn::VarInt;
    use rstest::rstest;
    use tokio::{
        net::{TcpListener, TcpStream},
        sync::mpsc,
    };

    use super::*;
    use crate::network::{
        accept::{QuicConnector, QuicListener},
        test_utils::next_local_addr,
    };

    #[rstest]
    #[timeout(Duration::from_secs(1))]
    #[tokio::test]
    async fn should_accept_connections_as_listener() {
        let (network_events_sender, network_events_receiver) = crossbeam_channel::unbounded();
        let (quit_sender, quit_receiver) = watch::channel(());

        let addr = next_local_addr();

        let tcp_listener = TcpListener::bind(&addr).await.unwrap();

        let receive_packets_task = AcceptConnectionsTask::new(tcp_listener, network_events_sender);

        let thread = tokio::task::spawn(async move {
            receive_packets_task._run(quit_receiver).await;
        });

        let stream = TcpStream::connect(&addr).await.unwrap();
        let recv = network_events_receiver.clone();
        let connection = tokio::task::spawn_blocking(move || recv.recv().unwrap())
            .await
            .unwrap();
        assert_eq!(
            stream.local_addr().unwrap(),
            connection.peer_addr().unwrap()
        );

        let stream = TcpStream::connect(&addr).await.unwrap();
        let connection =
            tokio::task::spawn_blocking(move || network_events_receiver.recv().unwrap())
                .await
                .unwrap();
        assert_eq!(
            stream.local_addr().unwrap(),
            connection.peer_addr().unwrap()
        );

        quit_sender.send(()).unwrap();

        thread.await.unwrap();
    }

    #[rstest]
    #[timeout(Duration::from_secs(1))]
    #[tokio::test]
    async fn should_accept_connections_as_connector() {
        let (network_events_sender, network_events_receiver) = crossbeam_channel::unbounded();
        let (quit_sender, quit_receiver) = watch::channel(());
        let (reconnect_sender, reconnect_receiver) = mpsc::channel(1);

        let addr = next_local_addr();

        let tcp_listener = QuicListener::new(addr.parse().unwrap());
        reconnect_sender.send(addr.parse().unwrap()).await.unwrap();

        let tcp_connector = QuicConnector::new(reconnect_receiver);

        let receive_packets_task = AcceptConnectionsTask::new(tcp_connector, network_events_sender);

        let thread = tokio::task::spawn(async move {
            receive_packets_task._run(quit_receiver).await;
        });

        let recv = network_events_receiver.clone();
        let connection = tokio::task::spawn_blocking(move || recv.recv().unwrap())
            .await
            .unwrap();
        // assert_eq!(
        //     tcp_listener.local_addr().unwrap(),
        //     connection.peer_addr().unwrap()
        // );

        connection.close(VarInt::from(1u8), &[]);

        std::mem::drop(tcp_listener);
        let addr = next_local_addr();

        let _tcp_listener = QuicListener::new(addr.parse().unwrap());
        reconnect_sender.send(addr.parse().unwrap()).await.unwrap();

        let _connection =
            tokio::task::spawn_blocking(move || network_events_receiver.recv().unwrap())
                .await
                .unwrap();

        // assert_eq!(
        //     tcp_listener.local_addr().unwrap(),
        //     connection.peer_addr().unwrap()
        // );

        quit_sender.send(()).unwrap();

        thread.await.unwrap();
    }
}
