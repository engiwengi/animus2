use futures::{pin_mut, FutureExt};
use speedy::Readable;
use tokio::{io::AsyncRead, sync::watch};
use tracing::{error, info};

use crate::{
    channel::BroadcastChannel,
    network::{
        mediator::AnyPacketMediator,
        packet::{AnyPacketWithConnId, Packet},
        socket::Socket,
    },
};

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
                },
                _ = disconnect => break,
                _ = stop => break,
            };

            let packet = match self.socket.next().await {
                Ok(packet) => packet,
                Err(e) => {
                    error!("Failed to receive packet: {}", e);
                    break;
                }
            };

            let packet_with_conn_id = AnyPacketWithConnId {
                packet,
                connection_id: self.connection_id,
            };
            if let Err(e) = self.packet_mediator.send(packet_with_conn_id) {
                error!("Failed to mediate packet: {}", e);
                break;
            }
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting receive packets task: {}", self.connection_id);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

    use crossbeam_channel::{Receiver, Sender};
    use rstest::rstest;
    use tokio_test::io::Mock;

    use super::*;
    use crate::{
        chat::{entity::MessageKind, packet::SendMessage},
        network::{
            mediator::{
                AnyPacketMediator, ClientPacketSender, NetworkEvent, PacketSenderMap,
                PacketWithConnId,
            },
            packet::ClientPacket,
        },
    };
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
        let receive_task = BroadcastChannel::<()>::channel();
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
