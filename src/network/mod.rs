pub(crate) mod accept;
pub(crate) mod connection;
pub(crate) mod error;
pub(crate) mod event;
pub(crate) mod mediator;
pub(crate) mod packet;
pub mod plugin;
pub(crate) mod socket;
pub(crate) mod task;

#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::atomic::{AtomicU64, Ordering};

    use tracing::trace;

    static NEXT_PORT: AtomicU64 = AtomicU64::new(5600);

    pub(crate) fn next_local_addr() -> String {
        let ret = format!("127.0.0.1:{}", NEXT_PORT.fetch_add(1, Ordering::SeqCst));
        trace!("Next local addr: {}", ret);
        ret
    }
}

#[cfg(test)]
mod tests {
    // use std::{
    //     collections::HashMap,
    //     time::{Duration, Instant},
    // };
    //
    // use rstest::rstest;
    // use tracing_test::traced_test;
    //
    // use crate::{
    //     chat::{
    //         entity::MessageKind,
    //         packet::{MessageReceived, SendMessage},
    //     },
    //     id::NetworkId,
    //     network::{
    //         client::NetworkClient,
    //         mediator::{NullSink, PacketSenderMap, PacketWithConnId},
    //         packet::{
    //             AcceptConnection, ClientPacket, ClientPacketKind, Heartbeat,
    // ServerPacket,             ServerPacketKind,
    //         },
    //         server::NetworkServer,
    //         test_utils::next_local_addr,
    //     },
    // };
    //
    // #[rstest]
    // #[tokio::test]
    // #[traced_test]
    // async fn run_client_and_server() {
    //     tokio::task::spawn_blocking(move || {
    //         let mut client_map =
    // PacketSenderMap::<ServerPacket>(HashMap::new());         client_map.
    // 0.insert(             ServerPacketKind::Heartbeat,
    //             NullSink::<ServerPacket,
    // Heartbeat>(Default::default()).into(),         );
    //         let (client_tx, _client_rx) =
    // crossbeam_channel::unbounded::<AcceptConnection>();
    //         client_map.add(client_tx);
    //         let (client_tx, client_rx) =
    // crossbeam_channel::unbounded::<MessageReceived>();
    //         client_map.add(client_tx);
    //         let mut server_map =
    // PacketSenderMap::<ClientPacket>(HashMap::new());         server_map.
    // 0.insert(             ClientPacketKind::Heartbeat,
    //             NullSink::<ClientPacket,
    // Heartbeat>(Default::default()).into(),         );
    //         let (server_tx, server_rx) =
    //
    // crossbeam_channel::unbounded::<PacketWithConnId<SendMessage>>();
    //         server_map.add(server_tx);
    //         let addr = next_local_addr();
    //
    //         let mut server = NetworkServer::new(addr.parse().unwrap(),
    // server_map);
    //
    //         let mut client = NetworkClient::new(client_map);
    //
    //         client.connect(addr.parse().unwrap()).unwrap();
    //
    //         let mut client_connected = false;
    //         let mut server_connected = false;
    //         let now = Instant::now();
    //         while !server_connected || !client_connected {
    //             server.spawn_tasks_for_new_connections(|_| {
    //                 server_connected = true;
    //             });
    //             client.spawn_tasks_for_new_connections(|_| client_connected =
    // true);
    //
    //             if now.elapsed() > Duration::from_secs(1) {
    //                 panic!("waited for connections for too long")
    //             }
    //         }
    //         let sent_packet = SendMessage {
    //             contents: "message".to_owned(),
    //             kind: MessageKind::Shout,
    //         };
    //
    //         client.send(sent_packet.clone()).unwrap();
    //         let packet =
    // server_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    //         assert_eq!(packet.packet, sent_packet);
    //
    //         server
    //             .send(
    //                 MessageReceived {
    //                     contents: "message".to_owned(),
    //                     kind: MessageKind::Shout,
    //                     sender: NetworkId::from(0),
    //                 },
    //                 packet.connection_id,
    //             )
    //             .unwrap();
    //         assert!(client_rx.recv_timeout(Duration::from_secs(1)).is_ok());
    //
    //         std::mem::drop(client);
    //         let mut client_map =
    // PacketSenderMap::<ServerPacket>(HashMap::new());         let
    // (client_tx, _client_rx) =
    // crossbeam_channel::unbounded::<AcceptConnection>();
    //         client_map.add(client_tx);
    //         let (client_tx, client_rx) =
    // crossbeam_channel::unbounded::<MessageReceived>();
    //         client_map.add(client_tx);
    //         let mut client = NetworkClient::new(client_map);
    //         client.connect(addr.parse().unwrap()).unwrap();
    //
    //         let mut client_connected = false;
    //         let mut server_connected = false;
    //         let now = Instant::now();
    //         while !server_connected || !client_connected {
    //             server.spawn_tasks_for_new_connections(|_| {
    //                 server_connected = true;
    //             });
    //             client.spawn_tasks_for_new_connections(|_| client_connected =
    // true);
    //
    //             if now.elapsed() > Duration::from_secs(1) {
    //                 panic!("waited for connections for too long")
    //             }
    //         }
    //
    //         let sent_packet = SendMessage {
    //             contents: "message".to_owned(),
    //             kind: MessageKind::Shout,
    //         };
    //
    //         client.send(sent_packet.clone()).unwrap();
    //         let packet =
    // server_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    //         assert_eq!(packet.packet, sent_packet);
    //
    //         server
    //             .send(
    //                 MessageReceived {
    //                     contents: "message".to_owned(),
    //                     kind: MessageKind::Shout,
    //                     sender: NetworkId::from(0),
    //                 },
    //                 packet.connection_id,
    //             )
    //             .unwrap();
    //         assert!(client_rx.recv_timeout(Duration::from_secs(1)).is_ok());
    //
    //         std::mem::drop(client);
    //         std::mem::drop(server);
    //     })
    //     .await
    //     .unwrap();
    // }
}
