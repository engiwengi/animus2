use std::time::Duration;

use crossbeam_channel::Sender;
use futures::{pin_mut, FutureExt};
use speedy::Readable;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, watch},
};
use tracing::{error, info};

use super::{
    accept::{AsyncAccept, AsyncAcceptExt},
    connection::Connection,
    error::Result,
    mediator::AnyPacketMediator,
    packet::{AnyPacketWithConnId, EncodedPacket, Packet},
    socket::Socket,
};
use crate::network::packet::Heartbeat;

pub struct ReceivePacketsTask<R, T>
where
    T: Packet,
{
    socket: Socket<R>,
    packet_mediator: AnyPacketMediator<T>,
    connection_id: u64,
}

impl<'d, R, T> ReceivePacketsTask<R, T>
where
    R: AsyncRead + Unpin + 'd,
    T: Packet,
{
    pub fn new(socket: R, packet_mediator: AnyPacketMediator<T>, connection_id: u64) -> Self {
        Self {
            socket: Socket::new(socket),
            packet_mediator,
            connection_id,
        }
    }

    pub async fn _run(
        mut self,
        mut stop: watch::Receiver<()>,
        mut disconnect_broadcast: BroadcastChannel<()>,
    ) where
        <T as Packet>::Kind: for<'a> From<&'a T>,
        T: Readable<'d, speedy::LittleEndian>,
    {
        let stop = stop.changed().fuse();
        let disconnect = disconnect_broadcast.notified.recv().fuse();
        pin_mut!(stop, disconnect);

        loop {
            futures::select! {
                length = self.socket.ready().fuse() => {
                    if let Err(e) = length {
                        error!("Failed to receive packet length: {}", e);
                        break;
                    }

                    let packet = match self.socket.next().await {
                        Ok(packet) => packet,
                        Err(e) => {
                            error!("Failed to receive packet: {}", e);
                            break;
                        }
                    };

                    let packet_with_conn_id = AnyPacketWithConnId { packet, connection_id: self.connection_id };
                    if let Err(e) = self.packet_mediator.send(packet_with_conn_id) {
                        error!("Failed to mediate packet: {}", e);
                        break;
                    }
                },
                _ = disconnect => break,
                _ = stop => break,
            };
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting receive packets task: {}", self.connection_id);
    }
}

pub struct BroadcastChannel<T> {
    pub notify: broadcast::Sender<T>,
    pub notified: broadcast::Receiver<T>,
}

impl<T: Clone> BroadcastChannel<T> {
    pub fn channel() -> Self {
        let (notify, notified) = broadcast::channel(1);
        Self { notify, notified }
    }
}

impl<T: Clone> Clone for BroadcastChannel<T> {
    fn clone(&self) -> Self {
        Self {
            notify: self.notify.clone(),
            notified: self.notify.subscribe(),
        }
    }
}

pub struct SendPacketsTask<W> {
    socket: Socket<W>,
    queued_packets: mpsc::UnboundedReceiver<EncodedPacket>,
    connection_id: u64,
}

impl<W> SendPacketsTask<W>
where
    W: AsyncWrite + Unpin,
{
    pub fn new(
        io: W,
        queued_packets: mpsc::UnboundedReceiver<EncodedPacket>,
        connection_id: u64,
    ) -> Self {
        Self {
            socket: Socket::new(io),
            queued_packets,
            connection_id,
        }
    }

    pub async fn _run<P: Packet>(
        mut self,
        mut stop: watch::Receiver<()>,
        mut disconnect_broadcast: BroadcastChannel<()>,
    ) {
        let stop = stop.changed().fuse();
        let disconnect = disconnect_broadcast.notified.recv().fuse();
        let heartbeat = tokio::time::interval(Duration::from_secs(1));
        pin_mut!(stop, disconnect, heartbeat);

        loop {
            futures::select! {
                maybe_packet = self.queued_packets.recv().fuse() => {
                    let packet = match maybe_packet {
                        Some(packet) => packet,
                        None => {
                            error!("Packet sender unexpectedly closed");
                            break;
                        }
                    };

                    if let Err(e) = self.send_packet(packet).await {
                        error!("{}", e);
                        break;
                    }
                },
                _ = heartbeat.tick().fuse() => {
                    if let Err(e) = self.send_packet(EncodedPacket::try_encode::<Heartbeat, P>(Heartbeat).unwrap()).await {
                        error!("{}", e);
                        break;
                    }
                },
                _ = disconnect => break,
                _ = stop => break,
            };
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting send packets task: {}", self.connection_id);
    }

    async fn send_packet(&mut self, packet: EncodedPacket) -> Result<()> {
        self.socket.send(packet).await?;
        Ok(())
    }
}

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
        info!("Disconnecting accept packets task");
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

    use crossbeam_channel::Receiver;
    use quinn::VarInt;
    use rstest::rstest;
    use tokio::{
        net::{TcpListener, TcpStream},
        sync::mpsc,
    };
    use tokio_test::io::Mock;

    use super::*;
    use crate::{
        chat::{entity::MessageKind, packet::SendMessage},
        network::{
            accept::{QuicConnector, QuicListener},
            mediator::{
                AnyPacketMediator, ClientPacketSender, NetworkEvent, PacketSenderMap,
                PacketWithConnId,
            },
            packet::ClientPacket,
            test_utils::next_local_addr,
        },
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

    #[rstest]
    #[case(SendMessage { kind: MessageKind::Shout, contents: "message".to_owned()})]
    #[timeout(Duration::from_secs(1))]
    #[tracing_test::traced_test]
    #[tokio::test(flavor = "multi_thread")]
    async fn should_receive_packet<T>(#[case] packet: T)
    where
        T: PartialEq + Debug + Clone + TryFrom<ClientPacket> + Send + 'static,
        ClientPacket: From<T>,
        ClientPacketSender:
            TryFrom<Sender<PacketWithConnId<T>>> + From<Sender<PacketWithConnId<T>>>,
    {
        let any_packet = ClientPacket::from(packet.clone());
        let serialized_packet = speedy::Writable::write_to_vec(&any_packet).unwrap();
        let length = u32::to_le_bytes(serialized_packet.len() as u32);

        let (reader, _handle) = tokio_test::io::Builder::new() // keeping the handle makes the mock reader stay open and not EOF
            .read(&length)
            .read(&serialized_packet)
            .build_with_handle();

        let (_network_events, packets, receive_packets_task) =
            make_receive_packets_task::<T>(reader);

        let (quit, quit_receiver) = watch::channel(());
        let receive_task = BroadcastChannel::channel();
        let thread = tokio::task::spawn(async move {
            receive_packets_task
                ._run(quit_receiver, receive_task.clone())
                .await;
        });

        tokio::task::spawn_blocking(move || {
            let recv_packet = packets.recv().unwrap().packet;
            assert_eq!(packet, recv_packet);
        })
        .await
        .unwrap();

        quit.send(()).unwrap();

        thread.await.unwrap();
    }

    fn make_receive_packets_task<T>(
        reader: Mock,
    ) -> (
        Receiver<NetworkEvent>,
        Receiver<PacketWithConnId<T>>,
        ReceivePacketsTask<Mock, ClientPacket>,
    )
    where
        T: PartialEq + Debug + Clone + TryFrom<ClientPacket>,
        ClientPacketSender: From<Sender<PacketWithConnId<T>>>,
    {
        let (_network_events_sender, network_events_receiver) = crossbeam_channel::unbounded();
        let (packet_sender, packet_receiver) =
            crossbeam_channel::unbounded::<PacketWithConnId<T>>();

        let mut sender_map = PacketSenderMap::<ClientPacket>(HashMap::new());

        sender_map.add(packet_sender);

        let mediator = AnyPacketMediator::new(Arc::new(sender_map));

        let receive_packets_task = ReceivePacketsTask::new(reader, mediator, 0);

        (
            network_events_receiver,
            packet_receiver,
            receive_packets_task,
        )
    }
}
