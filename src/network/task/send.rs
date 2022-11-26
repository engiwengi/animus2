use std::time::Duration;

use futures::{pin_mut, AsyncWrite, FutureExt};
use tracing::{error, info};

use crate::{
    channel::BroadcastChannel,
    id::NetworkId,
    network::{
        packet::{EncodedPacket, Heartbeat, Packet},
        socket::Socket,
    },
};

pub(in crate::network) struct SendPacketsTask<W> {
    socket: Socket<W>,
    queued_packets: async_std::channel::Receiver<EncodedPacket>,
    connection_id: NetworkId,
}

impl<W> SendPacketsTask<W>
where
    W: AsyncWrite + Unpin,
{
    pub(in crate::network) fn new(
        io: W,
        queued_packets: async_std::channel::Receiver<EncodedPacket>,
        connection_id: NetworkId,
    ) -> Self {
        Self {
            socket: Socket::new(io),
            queued_packets,
            connection_id,
        }
    }

    pub(in crate::network) async fn _run<P: Packet>(
        mut self,
        stop: async_std::channel::Receiver<()>,
        disconnect_broadcast: BroadcastChannel<()>,
    ) {
        let stop = stop.recv().fuse();
        let disconnect = disconnect_broadcast.notified.recv().fuse();
        // heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        pin_mut!(stop, disconnect);

        loop {
            let packet = futures::select! {
                maybe_packet = self.queued_packets.recv().fuse() => {
                    match maybe_packet {
                        Ok(packet) => packet,
                        Err(_) => break,
                    }
                },
                _ = async_std::task::sleep(Duration::from_secs(1)).fuse() => {
                    EncodedPacket::try_encode::<Heartbeat, P>(Heartbeat).unwrap()
                },
                _ = disconnect => break,
                _ = stop => break,
            };

            if let Err(e) = self.socket.send(packet).await {
                error!("{}", e);
                break;
            }
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting send packets task: {}", self.connection_id);
    }
}
