use std::time::Duration;

use futures::{pin_mut, FutureExt};
use tokio::{
    io::AsyncWrite,
    sync::{mpsc, watch},
};
use tracing::{error, info};

use crate::{
    channel::BroadcastChannel,
    network::{
        packet::{EncodedPacket, Heartbeat, Packet},
        socket::Socket,
    },
};

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
        let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        pin_mut!(stop, disconnect, heartbeat);

        loop {
            let packet = futures::select! {
                maybe_packet = self.queued_packets.recv().fuse() => {
                    match maybe_packet {
                        Some(packet) => packet,
                        None => break,
                    }
                },
                _ = heartbeat.tick().fuse() => EncodedPacket::try_encode::<Heartbeat, P>(Heartbeat).unwrap(),
                _ = disconnect => break,
                _ = stop => break,
            };

            if let Err(e) = self.socket.send(packet).await {
                error!("{}", e);
                break;
            }
            heartbeat.reset();
        }
        let _ = disconnect_broadcast.notify.send(());
        info!("Disconnecting send packets task: {}", self.connection_id);
    }
}
