use std::{collections::HashMap, sync::Arc};

use crossbeam_channel::Sender;
use derive_more::{Deref, DerefMut};
use tracing::trace;

use super::packet::{
    AnyPacketWithConnId, ClientPacket, ClientPacketKind, Packet, ServerPacket, ServerPacketKind,
};

pub trait Mediator<T> {
    fn raise(&self, event: T) -> Result<(), anyhow::Error>;
}

#[derive(Clone)]
pub struct AnyPacketMediator<P>
where
    P: Packet,
{
    packet_senders: Arc<PacketSenderMap<P>>,
}

impl<P> AnyPacketMediator<P>
where
    P: Packet,
{
    pub fn new(packet_senders: Arc<PacketSenderMap<P>>) -> Self {
        Self { packet_senders }
    }

    pub fn send<'a>(&'a self, packet: AnyPacketWithConnId<P>) -> Result<(), anyhow::Error>
    where
        P::Kind: for<'b> From<&'b P>,
    {
        let packet_kind = packet.packet_kind();
        let sender = self.packet_senders.get(&packet_kind).unwrap();
        sender.handle(packet)
    }
}

#[derive(Deref, DerefMut, Default)]
pub struct PacketSenderMap<P>(pub HashMap<P::Kind, P::Sender>)
where
    P: Packet;

impl<P> PacketSenderMap<P>
where
    P: Packet,
{
    pub fn add<S>(&mut self, sender: S)
    where
        P::Sender: TryFrom<S>,
        P::Kind: for<'a> From<&'a P::Sender>,
    {
        let s = P::Sender::try_from(sender).unwrap_or_else(|_| {
            panic!(
                "{} not yet added to packet senders",
                std::any::type_name::<P>()
            )
        });
        self.0.insert((&s).into(), s);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetworkEvent {
    Disconnected { connection_id: u64 },
}

pub use client_packet_sender_enum::*;
pub use server_packet_sender_enum::*;

#[proxy_enum::proxy(ClientPacketSender)]
mod client_packet_sender_enum {
    use derive_more::TryInto;

    use super::*;
    use crate::{chat::packet::SendMessage, network::packet::*};

    #[rustfmt::skip]
    #[derive(Debug, TryInto)]
    pub enum ClientPacketSender {
        SendMessage(Sender::<PacketWithConnId<SendMessage>>),
    }

    impl ClientPacketSender {
        #[implement]
        pub fn handle(
            &self,
            any_packet: AnyPacketWithConnId<ClientPacket>,
        ) -> Result<(), anyhow::Error> {
        }
    }
}

impl From<&ClientPacketSender> for ClientPacketKind {
    fn from(value: &ClientPacketSender) -> Self {
        match value {
            ClientPacketSender::SendMessage(_) => ClientPacketKind::SendMessage,
        }
    }
}

impl AnyPacketHandler<ClientPacket> for ClientPacketSender {
    fn handle(&self, any_packet: AnyPacketWithConnId<ClientPacket>) -> Result<(), anyhow::Error> {
        ClientPacketSender::handle(self, any_packet)
    }
}
#[proxy_enum::proxy(ServerPacketSender)]
mod server_packet_sender_enum {
    use derive_more::TryInto;

    use super::*;
    use crate::{chat::packet::MessageReceived, network::packet::*};

    #[rustfmt::skip]
    #[derive(Debug, TryInto)]
    pub enum ServerPacketSender {
        MessageReceived(Sender::<MessageReceived>),
        AcceptConnection(Sender::<AcceptConnection>),
    }

    impl ServerPacketSender {
        #[implement]
        pub fn handle(
            &self,
            any_packet: AnyPacketWithConnId<ServerPacket>,
        ) -> Result<(), anyhow::Error> {
        }
    }
}

impl From<&ServerPacketSender> for ServerPacketKind {
    fn from(value: &ServerPacketSender) -> Self {
        match value {
            ServerPacketSender::MessageReceived(_) => ServerPacketKind::MessageReceived,
            ServerPacketSender::AcceptConnection(_) => ServerPacketKind::AcceptConnection,
        }
    }
}

impl AnyPacketHandler<ServerPacket> for ServerPacketSender {
    fn handle(&self, any_packet: AnyPacketWithConnId<ServerPacket>) -> Result<(), anyhow::Error> {
        ServerPacketSender::handle(self, any_packet)
    }
}

pub trait AnyPacketHandler<P> {
    fn handle(&self, any_packet: AnyPacketWithConnId<P>) -> Result<(), anyhow::Error>;
}

#[derive(Debug)]
pub struct PacketWithConnId<T> {
    pub packet: T,
    pub connection_id: u64,
}

trait PacketHandler<T>
where
    T: Into<Self::Kind> + TryFrom<Self::Kind>,
{
    type Kind;
    fn handle(&self, any_packet: AnyPacketWithConnId<Self::Kind>) -> Result<(), anyhow::Error>;
}

impl<T> PacketHandler<T> for Sender<PacketWithConnId<T>>
where
    T: Into<ClientPacket> + TryFrom<ClientPacket>,
    <T as TryFrom<ClientPacket>>::Error: std::fmt::Debug,
{
    type Kind = ClientPacket;

    fn handle(&self, any_packet: AnyPacketWithConnId<ClientPacket>) -> Result<(), anyhow::Error> {
        let message_name = any_packet.packet_kind();
        // Cannot happen since packet type must be in both sender and anypacket enum
        let packet = TryInto::<T>::try_into(any_packet.packet).unwrap();
        trace!("Mediating packet type: {:?}", message_name);
        self.send(PacketWithConnId {
            packet,
            connection_id: any_packet.connection_id,
        })
        .map_err(|_| anyhow::anyhow!("Sender unexpectedly closed"))
    }
}

impl<T> PacketHandler<T> for Sender<T>
where
    T: Into<ServerPacket> + TryFrom<ServerPacket>,
    <T as TryFrom<ServerPacket>>::Error: std::fmt::Debug,
{
    type Kind = ServerPacket;

    fn handle(&self, any_packet: AnyPacketWithConnId<ServerPacket>) -> Result<(), anyhow::Error> {
        let message_name = any_packet.packet_kind();
        // Cannot happen since packet type must be in both sender and anypacket enum
        let packet = TryInto::<T>::try_into(any_packet.packet).unwrap();
        trace!("Mediating packet type: {:?}", message_name);
        self.send(packet)
            .map_err(|_| anyhow::anyhow!("Sender unexpectedly closed"))
    }
}
