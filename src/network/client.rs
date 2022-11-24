// use std::net::SocketAddr;
//
// use bevy::prelude::Resource;
// use crossbeam_channel::Receiver;
//
// use super::{
//     accept::QuicConnector,
//     error::{Error, Result},
//     mediator::{NetworkEvent, PacketSenderMap},
//     packet::{ClientPacket, EncodedPacket, ServerPacket},
//     shared::NetworkBase,
// };
// use crate::id::NetworkId;
//
// #[derive(Resource)]
// pub struct NetworkClient {
//     base: NetworkBase<ServerPacket>,
//     events: Receiver<NetworkEvent>,
//     // TODO: Should this be an option? On disconnect the network client can
// be removed such that     // systems dont need to check when sending?
//     server: Option<(async_std::channel::Sender<EncodedPacket>, NetworkId)>,
//     connect_to: mpsc::Sender<SocketAddr>,
// }
//
// impl NetworkClient {
//     pub fn new(map: PacketSenderMap<ServerPacket>) -> Self {
//         let (sender, events) = crossbeam_channel::unbounded();
//         let (connect_to, connect_to_rx) = mpsc::channel(1);
//         let connector = QuicConnector::new(connect_to_rx);
//         let base = NetworkBase::new(map, connector, sender);
//
//         Self {
//             base,
//             events,
//             server: None,
//             connect_to,
//         }
//     }
//
//     pub fn connect(&self, addr: SocketAddr) -> Result<()> {
//         self.connect_to
//             .try_send(addr)
//             .map_err(|_| Error::Generic("Channel is unexpectdly
// closed".to_owned()))?;         Ok(())
//     }
//
//     pub fn is_connected(&self) -> bool {
//         self.server.is_some()
//     }
//
//     pub fn send<T>(&self, packet: T) -> Result<()>
//     where
//         ClientPacket: From<T>,
//     {
//         let sender = match &self.server {
//             Some(sender) => &sender.0,
//             None => return Err(Error::Generic("Not yet connected to
// server".to_owned())),         };
//         let encoded_packet = EncodedPacket::try_encode::<T,
// ClientPacket>(packet)?;
//
//         sender
//             .send(encoded_packet)
//             .map_err(|_| Error::Generic("Channel is unexpectdly
// closed".to_owned()))?;
//
//         Ok(())
//     }
//
//     pub fn spawn_tasks_for_new_connections<F>(&mut self, mut
// on_new_connection: F)     where
//         F: FnMut(NetworkId),
//     {
//         self.base.spawn_tasks_for_new_connections(|conn, conn_id| {
//             self.server = Some((conn, conn_id));
//             on_new_connection(conn_id);
//         });
//     }
//
//     pub fn handle_disconnections(&mut self) {
//         for event in self.events.try_iter() {
//             match event {
//                 NetworkEvent::Disconnected { connection_id } => match
// &self.server {                     Some(s) if s.1 == connection_id =>
// std::mem::drop(self.server.take()),                     _ => {}
//                 },
//             }
//         }
//     }
// }
