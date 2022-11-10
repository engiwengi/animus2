use std::{collections::HashMap, io::BufRead};

use animus_lib::{
    chat::{
        entity::MessageKind,
        packet::{MessageReceived, SendMessage},
    },
    network::{
        client::NetworkClient,
        mediator::{NullSink, PacketSenderMap},
        packet::{AcceptConnection, Heartbeat, ServerPacket},
    },
};
use tracing::Level;
use tracing_subscriber::{filter::Targets, FmtSubscriber};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let _filter = Targets::new().with_target("animus2", Level::TRACE);
    let subscriber = FmtSubscriber::builder().finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut client_map = PacketSenderMap::<ServerPacket>(HashMap::new());
    client_map.0.insert(
        animus_lib::network::packet::ServerPacketKind::Heartbeat,
        NullSink::<ServerPacket, Heartbeat>(Default::default()).into(),
    );
    let (client_tx, _client_rx) = crossbeam_channel::unbounded::<AcceptConnection>();
    client_map.add(client_tx);
    let (client_tx, client_rx) = crossbeam_channel::unbounded::<MessageReceived>();
    client_map.add(client_tx);
    let addr = "127.0.0.1:56565";

    let mut client = NetworkClient::new(client_map);

    client.connect(addr.parse().unwrap()).unwrap();

    let (messages_tx, messages_rx) = crossbeam_channel::unbounded::<String>();

    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().flatten() {
            messages_tx.send(line).unwrap();
        }
    });

    std::thread::spawn(move || loop {
        if let Ok(msg) = client_rx.recv() {
            println!("{:?}: {}", msg.sender, msg.contents);
        }
    });

    let mut client_connected = false;
    while !client_connected {
        client.spawn_tasks_for_new_connections(|_| client_connected = true);
    }

    loop {
        if let Ok(contents) = messages_rx.recv() {
            let sent_packet = SendMessage {
                contents,
                kind: MessageKind::Shout,
            };
            client.send(sent_packet.clone()).unwrap();
        }
    }
}
