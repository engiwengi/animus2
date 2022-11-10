use std::collections::HashMap;

use animus_lib::{
    chat::{packet::SendMessage, system::broadcast_shouts_to_all_clients},
    network::{
        mediator::{PacketSenderMap, PacketWithConnId},
        packet::{ClientPacket, Heartbeat},
        server::NetworkServer,
    },
};
use tracing_subscriber::FmtSubscriber;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        // .with_max_level(Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    let mut server_map = PacketSenderMap::<ClientPacket>(HashMap::new());
    let (server_tx, _server_rx) = crossbeam_channel::unbounded::<PacketWithConnId<Heartbeat>>();
    server_map.add(server_tx);
    let (server_tx, server_rx) = crossbeam_channel::unbounded::<PacketWithConnId<SendMessage>>();
    server_map.add(server_tx);
    let addr = "127.0.0.1:56565";

    let mut server = NetworkServer::new(addr.parse().unwrap(), server_map);

    loop {
        server.spawn_tasks_for_new_connections(|_| {});
        broadcast_shouts_to_all_clients(&server_rx, &server);
        // std::thread::sleep(Duration::from_millis(10));
    }
}
