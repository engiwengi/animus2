use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{broadcast, mpsc, watch},
};
use tracing::info;

use crate::{
    channel::BroadcastChannel,
    network::{
        accept::AsyncAccept,
        connection::Connection,
        mediator::{AnyPacketMediator, NetworkEvent, PacketSenderMap},
        packet::{EncodedPacket, Packet},
        task::{accept::AcceptConnectionsTask, recv::ReceivePacketsTask, send::SendPacketsTask},
    },
};

pub struct NetworkBase<P>
where
    P: Packet,
{
    events: Sender<NetworkEvent>,
    new_connections: Receiver<Connection<quinn::Connection>>,
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
        A: AsyncAccept<Connection = quinn::Connection> + Unpin + Send + 'static,
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

            let broadcast_disconnect = BroadcastChannel::channel();

            let (sender, receiver) = mpsc::unbounded_channel();

            {
                let mut disconnect = broadcast_disconnect.notify.subscribe();
                let network_events = self.events.clone();
                tokio::task::spawn(async move {
                    let _ = disconnect.recv().await;
                    info!("client {} disconnected", conn_id);
                    let _ = network_events.send(NetworkEvent::Disconnected {
                        connection_id: conn_id,
                    });
                });
            };

            let mediator = AnyPacketMediator::new(Arc::clone(&self.packet_senders));
            let receive_task = broadcast_disconnect.clone();
            let stop = self.stop.subscribe();
            let conn = connection.value.clone();
            tokio::task::spawn(async move {
                let reader = conn.accept_uni().await.unwrap();
                ReceivePacketsTask::new(reader, mediator, conn_id)
                    ._run(stop, receive_task)
                    .await;
            });

            let stop = self.stop.subscribe();
            tokio::task::spawn(async move {
                let writer = connection.value.open_uni().await.unwrap();
                SendPacketsTask::new(writer, receiver, conn_id)
                    ._run::<P::OtherPacket>(stop, broadcast_disconnect)
                    .await;
            });

            on_new_connection(sender, conn_id);
        }
    }

    fn spawn_accept_task<A>(
        acceptor: A,
        new_connections_tx: Sender<Connection<quinn::Connection>>,
        quit_rx: watch::Receiver<()>,
    ) where
        A: AsyncAccept<Connection = quinn::Connection> + Unpin + Send + 'static,
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
                ._run::<P::OtherPacket>(stop, receive_task)
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
