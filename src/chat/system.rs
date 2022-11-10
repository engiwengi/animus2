use crossbeam_channel::Receiver;

use super::{
    entity::{Chat, ChatInput, MessageKind},
    packet::{MessageReceived, SendMessage},
};
use crate::network::{client::NetworkClient, mediator::PacketWithConnId, server::NetworkServer};

pub fn add_messages_to_chats(new_messages: &Receiver<MessageReceived>, chat: &mut Chat) {
    for new_message in new_messages.try_iter() {
        chat.push(new_message);
    }
}

pub fn broadcast_shouts_to_all_clients(
    sent_messages: &Receiver<PacketWithConnId<SendMessage>>,
    network: &NetworkServer,
) {
    for sent_message in sent_messages.try_iter() {
        let _ = network.broadcast(MessageReceived::from(sent_message), network.clients());
    }
}

pub fn send_message(chat: &mut ChatInput, network: &NetworkClient) {
    let contents = std::mem::take(&mut chat.input);

    let send_message = SendMessage {
        kind: MessageKind::Shout,
        contents,
    };

    // TODO: do something if network client is not yet connected
    let _ = network.send(send_message);
}

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use rstest::rstest;
    use tracing::info;
    use tracing_test::traced_test;

    use super::*;
    use crate::{
        chat::packet::{MessageReceived, SendMessage},
        network::{
            client::NetworkClient,
            mediator::{NullSink, PacketSenderMap, PacketWithConnId},
            packet::{AcceptConnection, ClientPacket, Heartbeat, ServerPacket},
            server::NetworkServer,
            test_utils::next_local_addr,
        },
    };

    #[rstest]
    #[tokio::test]
    #[timeout(Duration::from_secs(5))]
    #[traced_test]
    async fn run_client_and_server() {
        let mut client_map = PacketSenderMap::<ServerPacket>(HashMap::new());
        client_map.0.insert(
            crate::network::packet::ServerPacketKind::Heartbeat,
            NullSink::<ServerPacket, Heartbeat>(Default::default()).into(),
        );
        let (client_tx, _client_rx) = crossbeam_channel::unbounded::<AcceptConnection>();
        client_map.add(client_tx);
        let (client_tx, client_rx_recv) = crossbeam_channel::unbounded::<MessageReceived>();
        client_map.add(client_tx);

        let mut client2_map = PacketSenderMap::<ServerPacket>(HashMap::new());
        client2_map.0.insert(
            crate::network::packet::ServerPacketKind::Heartbeat,
            NullSink::<ServerPacket, Heartbeat>(Default::default()).into(),
        );
        let (client_tx, _client_rx) = crossbeam_channel::unbounded::<AcceptConnection>();
        client2_map.add(client_tx);
        let (client2_tx, client2_rx_recv) = crossbeam_channel::unbounded::<MessageReceived>();
        client2_map.add(client2_tx);

        let mut server_map = PacketSenderMap::<ClientPacket>(HashMap::new());
        server_map.0.insert(
            crate::network::packet::ClientPacketKind::Heartbeat,
            NullSink::<ClientPacket, Heartbeat>(Default::default()).into(),
        );
        let (server_tx, server_rx_send) =
            crossbeam_channel::unbounded::<PacketWithConnId<SendMessage>>();
        server_map.add(server_tx);

        let (client, client2, server) = tokio::task::spawn_blocking(move || {
            let addr = next_local_addr();

            let mut server = NetworkServer::new(addr.parse().unwrap(), server_map);

            let mut client = NetworkClient::new(client_map);
            let mut client2 = NetworkClient::new(client2_map);

            client.connect(addr.parse().unwrap()).unwrap();
            client2.connect(addr.parse().unwrap()).unwrap();

            let mut client_connected = false;
            let mut client2_connected = false;
            let mut server_connected = 0;
            let now = Instant::now();
            while server_connected < 2 || !client_connected || !client2_connected {
                client.spawn_tasks_for_new_connections(|_| client_connected = true);
                client2.spawn_tasks_for_new_connections(|_| client2_connected = true);
                server.spawn_tasks_for_new_connections(|_| {
                    server_connected += 1;
                });

                if now.elapsed() > Duration::from_secs(1) {
                    panic!("waited for connections for too long")
                }
            }

            (client, client2, server)
        })
        .await
        .unwrap();

        send_message(
            &mut ChatInput {
                input: "message from 1".to_owned(),
            },
            &client,
        );

        while server_rx_send.is_empty() {
            tokio::task::yield_now().await;
        }

        broadcast_shouts_to_all_clients(&server_rx_send, &server);

        let mut chat = Chat::default();
        let mut chat2 = Chat::default();

        while client_rx_recv.is_empty() || client2_rx_recv.is_empty() {
            tokio::task::yield_now().await;
        }

        add_messages_to_chats(&client_rx_recv, &mut chat);
        add_messages_to_chats(&client2_rx_recv, &mut chat2);

        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat2.messages.len(), 1);

        send_message(
            &mut ChatInput {
                input: "message from 2".to_owned(),
            },
            &client2,
        );

        while server_rx_send.is_empty() {
            tokio::task::yield_now().await;
        }

        broadcast_shouts_to_all_clients(&server_rx_send, &server);

        while client_rx_recv.is_empty() || client2_rx_recv.is_empty() {
            tokio::task::yield_now().await;
        }

        add_messages_to_chats(&client_rx_recv, &mut chat);
        add_messages_to_chats(&client2_rx_recv, &mut chat2);

        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat2.messages.len(), 2);

        info!("dropping");

        std::mem::drop(client);
        std::mem::drop(client2);
        std::mem::drop(server);
    }
}
