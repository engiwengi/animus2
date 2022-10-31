use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::{broadcast, mpsc, watch},
};

use super::{
    accept::AsyncAccept,
    connection::Connection,
    mediator::{AnyPacketMediator, NetworkEvent, PacketSenderMap},
    packet::{EncodedPacket, Packet},
    task::{AcceptConnectionsTask, BroadcastChannel, ReceivePacketsTask, SendPacketsTask},
};

pub struct NetworkBase<P>
where
    P: Packet,
{
    events: Sender<NetworkEvent>,
    new_connections: Receiver<Connection<TcpStream>>,
    packet_senders: Arc<PacketSenderMap<P>>,
    stop: watch::Sender<()>,
}

impl<P> NetworkBase<P>
where
    P: Packet,
{
    pub fn new<A>(
        packet_senders: PacketSenderMap<P>,
        acceptor: A,
        events: Sender<NetworkEvent>,
    ) -> Self
    where
        A: AsyncAccept<Connection = TcpStream> + Unpin + Send + 'static,
    {
        let (stop, quit_rx) = watch::channel(());
        let (new_connections_tx, new_connections) = crossbeam_channel::unbounded();

        Self::spawn_accept_task(acceptor, new_connections_tx, quit_rx);

        Self {
            events,
            stop,
            new_connections,
            packet_senders: Arc::new(packet_senders),
        }
    }

    pub fn spawn_tasks_for_new_connections<'a, 'd, F>(&mut self, mut on_new_connection: F)
    where
        F: FnMut(mpsc::UnboundedSender<EncodedPacket>, u64),
        P: speedy::Readable<'d, speedy::LittleEndian>,
        P::Kind: for<'r> From<&'r P>,
    {
        for connection in self.new_connections.try_iter() {
            let conn_id = connection.connection_id();
            let (reader, writer) = connection.value.into_split();

            let broadcast_disconnect = BroadcastChannel::channel();

            Self::spawn_disconnect_task(
                broadcast_disconnect.notify.subscribe(),
                self.events.clone(),
                conn_id,
            );

            Self::spawn_receive_task(
                reader,
                AnyPacketMediator::new(Arc::clone(&self.packet_senders)),
                conn_id,
                self.stop.subscribe(),
                broadcast_disconnect.clone(),
            );
            let (sender, receiver) = mpsc::unbounded_channel();

            Self::spawn_send_task(
                writer,
                receiver,
                conn_id,
                self.stop.subscribe(),
                broadcast_disconnect,
            );

            on_new_connection(sender, conn_id);
        }
    }

    fn spawn_accept_task<A>(
        acceptor: A,
        new_connections_tx: Sender<Connection<TcpStream>>,
        quit_rx: watch::Receiver<()>,
    ) where
        A: AsyncAccept<Connection = TcpStream> + Unpin + Send + 'static,
    {
        tokio::task::spawn(async move {
            AcceptConnectionsTask::new(acceptor, new_connections_tx)
                ._run(quit_rx)
                .await;
        });
    }

    fn spawn_disconnect_task(
        mut disconnect: broadcast::Receiver<()>,
        network_events: Sender<NetworkEvent>,
        connection_id: u64,
    ) {
        tokio::task::spawn(async move {
            let _ = disconnect.recv().await;
            let _ = network_events.send(NetworkEvent::Disconnected { connection_id });
        });
    }

    pub fn spawn_receive_task<'d>(
        io: OwnedReadHalf,
        mediator: AnyPacketMediator<P>,
        connection_id: u64,
        stop: watch::Receiver<()>,
        receive_task: BroadcastChannel<()>,
    ) where
        P: speedy::Readable<'d, speedy::LittleEndian>,
        P::Kind: for<'r> From<&'r P>,
    {
        tokio::task::spawn(async move {
            ReceivePacketsTask::new(io, mediator, connection_id)
                ._run(stop, receive_task)
                .await;
        });
    }

    pub fn spawn_send_task(
        io: OwnedWriteHalf,
        receiver: mpsc::UnboundedReceiver<EncodedPacket>,
        connection_id: u64,
        stop: watch::Receiver<()>,
        receive_task: BroadcastChannel<()>,
    ) {
        tokio::task::spawn(async move {
            SendPacketsTask::new(io, receiver, connection_id)
                ._run(stop, receive_task)
                .await;
        });
    }
}

impl<T> Drop for NetworkBase<T>
where
    T: Packet,
{
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use rstest::rstest;
    use tokio::net::TcpListener;

    use super::*;
    use crate::{
        chat::packet::MessageReceived,
        network::{packet::ServerPacket, test_utils::next_local_addr},
    };

    #[rstest]
    #[timeout(Duration::from_secs(5))]
    #[tokio::test]
    async fn launch_shared() {
        let (packet_sender, _packet_receiver) = crossbeam_channel::unbounded::<MessageReceived>();

        let mut sender_map: PacketSenderMap<ServerPacket> = PacketSenderMap(HashMap::new());

        sender_map.add(packet_sender);

        let addr = next_local_addr();
        let (sender, receiver) = crossbeam_channel::unbounded();
        let mut network_shared =
            NetworkBase::new(sender_map, TcpListener::bind(&addr).await.unwrap(), sender);

        let stream = TcpStream::connect(&addr).await.unwrap();

        tokio::task::spawn_blocking(move || {
            let mut senders = vec![];

            let mut connected = false;
            let now = Instant::now();
            while !connected {
                network_shared.spawn_tasks_for_new_connections(|sender, id| {
                    senders.push((sender, id));
                    connected = true;
                });
                if now.elapsed() > Duration::from_secs(1) {
                    panic!("waited for connections for too long")
                }
            }

            assert_eq!(senders.len(), 1);

            std::mem::drop(stream);

            assert_eq!(
                receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
                NetworkEvent::Disconnected {
                    connection_id: senders[0].1
                }
            );
        })
        .await
        .unwrap();
    }
}
