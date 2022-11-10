use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
};

use crossbeam_channel::Receiver;
use tokio::sync::mpsc;
use tracing::info;

use super::{
    accept::QuicListener,
    error::{Error, Result},
    mediator::{NetworkEvent, PacketSenderMap},
    packet::{AcceptConnection, ClientPacket, EncodedPacket, ServerPacket},
    shared::NetworkBase,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub struct NetworkServer {
    base: NetworkBase<ClientPacket>,
    pub events: Receiver<NetworkEvent>,
    clients: HashMap<u64, mpsc::UnboundedSender<EncodedPacket>>,
}

impl NetworkServer {
    pub fn new(addr: SocketAddr, map: PacketSenderMap<ClientPacket>) -> Self {
        let (sender, events) = crossbeam_channel::unbounded();
        // let socket = TcpSocket::new_v4().unwrap();
        // socket.set_reuseaddr(true).unwrap();
        // socket.bind(addr).unwrap();
        // let listener = socket.listen(1024).unwrap();
        let base = NetworkBase::new(map, QuicListener::new(addr), sender);

        Self {
            base,
            events,
            clients: HashMap::new(),
        }
    }

    pub fn send<T>(&self, packet: T, connection_id: u64) -> Result<()>
    where
        ServerPacket: From<T>,
    {
        let sender = match self.clients.get(&connection_id) {
            Some(sender) => sender,
            None => {
                return Err(Error::Generic(format!(
                    "No client with connection id {}",
                    connection_id
                )))
            }
        };
        let encoded_packet = EncodedPacket::try_encode::<T, ServerPacket>(packet)?;

        sender
            .send(encoded_packet)
            .map_err(|_| Error::Generic("Sender unexpectedly closed".to_owned()))?;

        Ok(())
    }

    pub fn broadcast<T>(
        &self,
        packet: T,
        connection_ids: impl IntoIterator<Item = u64>,
    ) -> Result<()>
    where
        ServerPacket: From<T>,
    {
        let encoded_packet = EncodedPacket::try_encode::<T, ServerPacket>(packet)?;
        let iter = connection_ids.into_iter();
        info!("Sending packet #{}", next_id());

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
                let packet = AcceptConnection { connection_id };
                let encoded_packet =
                    EncodedPacket::try_encode::<AcceptConnection, ServerPacket>(packet).unwrap();
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
