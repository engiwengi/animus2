use std::{collections::HashMap, net::SocketAddr};

use anyhow::bail;
use crossbeam_channel::Receiver;
use tokio::{net::TcpSocket, sync::mpsc};

use super::{
    mediator::{NetworkEvent, PacketSenderMap},
    packet::{AcceptConnection, ClientPacket, EncodedPacket, ServerPacket},
    shared::NetworkBase,
};

pub struct NetworkServer {
    base: NetworkBase<ClientPacket>,
    pub events: Receiver<NetworkEvent>,
    clients: HashMap<u64, mpsc::UnboundedSender<EncodedPacket>>,
}

impl NetworkServer {
    pub fn new(addr: SocketAddr, map: PacketSenderMap<ClientPacket>) -> Self {
        let (sender, events) = crossbeam_channel::unbounded();
        let socket = TcpSocket::new_v4().unwrap();
        socket.set_reuseaddr(true).unwrap();
        socket.bind(addr).unwrap();
        let listener = socket.listen(1024).unwrap();
        let base = NetworkBase::new(map, listener, sender);

        Self {
            base,
            events,
            clients: HashMap::new(),
        }
    }

    pub fn send<T>(&self, packet: T, connection_id: u64) -> Result<(), anyhow::Error>
    where
        ServerPacket: From<T>,
    {
        let sender = match self.clients.get(&connection_id) {
            Some(sender) => sender,
            None => bail!("No client with connection id {}", connection_id),
        };
        let packet = ServerPacket::from(packet);
        let encoded_packet = EncodedPacket::try_encode(packet)?;

        sender.send(encoded_packet)?;

        Ok(())
    }

    pub fn broadcast<T>(
        &self,
        packet: T,
        connection_ids: impl IntoIterator<Item = u64>,
    ) -> Result<(), anyhow::Error>
    where
        ServerPacket: From<T>,
    {
        let packet = ServerPacket::from(packet);
        let encoded_packet = EncodedPacket::try_encode(packet)?;
        let iter = connection_ids.into_iter();

        for connection_id in iter {
            let sender = match self.clients.get(&connection_id) {
                Some(sender) => sender,
                None => continue,
            };

            let _ = sender.send(encoded_packet.clone());
        }
        Ok(())
    }

    pub fn clients(&self) -> impl Iterator<Item = u64> + '_ {
        self.clients.keys().copied()
    }

    pub fn spawn_tasks_for_new_connections<F>(&mut self, mut on_new_connection: F)
    where
        F: FnMut(u64),
    {
        self.base
            .spawn_tasks_for_new_connections(|conn, connection_id| {
                let packet = ServerPacket::from(AcceptConnection { connection_id });
                let encoded_packet = EncodedPacket::try_encode(packet).unwrap();
                conn.send(encoded_packet).unwrap();
                self.clients.insert(connection_id, conn);
                on_new_connection(connection_id)
            });
    }

    pub fn handle_disconnections(&mut self) {
        for event in self.events.try_iter() {
            match event {
                NetworkEvent::Disconnected { connection_id } => {
                    let _ = self.clients.remove(&connection_id);
                }
            }
        }
    }
}
