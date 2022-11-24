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

pub struct SendPacketsTask<W> {
    socket: Socket<W>,
    queued_packets: async_std::channel::Receiver<EncodedPacket>,
    connection_id: NetworkId,
}

impl<W> SendPacketsTask<W>
where
    W: AsyncWrite + Unpin,
{
    pub fn new(
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

    pub async fn _run<P: Packet>(
        mut self,
        stop: async_std::channel::Receiver<()>,
        disconnect_broadcast: BroadcastChannel<()>,
    ) {
        let stop = stop.recv().fuse();
        let disconnect = disconnect_broadcast.notified.recv().fuse();
        let heartbeat = async_timer::Interval::platform_new(Duration::from_secs(1));
        // heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        pin_mut!(stop, disconnect, heartbeat);

        loop {
            let packet = futures::select! {
                maybe_packet = self.queued_packets.recv().fuse() => {
                    match maybe_packet {
                        Ok(packet) => packet,
                        Err(_) => break,
                    }
                },
                _ = heartbeat.fuse() => EncodedPacket::try_encode::<Heartbeat, P>(Heartbeat).unwrap(),
                _ = disconnect => break,
                _ = stop => break,
            };

            if let Err(e) = self.socket.send(packet).await {
                error!("{}", e);
                break;
            }
            // heartbeat.reset();
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting send packets task: {}", self.connection_id);
    }
}
