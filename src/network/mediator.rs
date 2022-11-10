use std::{collections::HashMap, marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use derive_more::{Deref, DerefMut};
use tracing::info;

use super::{
    error::Result,
    packet::{
        AnyPacketWithConnId, ClientPacket, ClientPacketKind, Packet, ServerPacket, ServerPacketKind,
    },
};
use crate::network::error::Error;

pub trait Mediator<T> {
    fn raise(&self, event: T) -> Result<()>;
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

    pub fn send<'a>(&'a self, packet: AnyPacketWithConnId<P>) -> Result<()>
    where
        P::Kind: for<'b> From<&'b P>,
    {
        let packet_kind = packet.packet_kind();
        let sender = self
            .packet_senders
            .get(&packet_kind)
            .unwrap_or_else(|| panic!("{:?} not added to packet sender map", packet_kind));
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
            panic!("{} not added to packet senders", std::any::type_name::<P>())
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
        Heartbeat(NullSink::<ClientPacket, Heartbeat>),
    }

    impl ClientPacketSender {
        #[implement]
        pub fn handle(&self, any_packet: AnyPacketWithConnId<ClientPacket>) -> Result<()> {}
    }
}

impl From<&ClientPacketSender> for ClientPacketKind {
    fn from(value: &ClientPacketSender) -> Self {
        match value {
            ClientPacketSender::SendMessage(_) => ClientPacketKind::SendMessage,
            ClientPacketSender::Heartbeat(_) => ClientPacketKind::Heartbeat,
        }
    }
}

impl AnyPacketHandler<ClientPacket> for ClientPacketSender {
    fn handle(&self, any_packet: AnyPacketWithConnId<ClientPacket>) -> Result<()> {
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
        Heartbeat(NullSink::<ServerPacket, Heartbeat>),
    }

    impl ServerPacketSender {
        #[implement]
        pub fn handle(&self, any_packet: AnyPacketWithConnId<ServerPacket>) -> Result<()> {}
    }
}

impl From<&ServerPacketSender> for ServerPacketKind {
    fn from(value: &ServerPacketSender) -> Self {
        match value {
            ServerPacketSender::MessageReceived(_) => ServerPacketKind::MessageReceived,
            ServerPacketSender::AcceptConnection(_) => ServerPacketKind::AcceptConnection,
            ServerPacketSender::Heartbeat(_) => ServerPacketKind::Heartbeat,
        }
    }
}

impl AnyPacketHandler<ServerPacket> for ServerPacketSender {
    fn handle(&self, any_packet: AnyPacketWithConnId<ServerPacket>) -> Result<()> {
        ServerPacketSender::handle(self, any_packet)
    }
}

pub trait AnyPacketHandler<P> {
    fn handle(&self, any_packet: AnyPacketWithConnId<P>) -> Result<()>;
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
    fn handle(&self, any_packet: AnyPacketWithConnId<Self::Kind>) -> Result<()>;
}

impl<T> PacketHandler<T> for Sender<PacketWithConnId<T>>
where
    T: Into<ClientPacket> + TryFrom<ClientPacket>,
    <T as TryFrom<ClientPacket>>::Error: std::fmt::Debug,
{
    type Kind = ClientPacket;

    fn handle(&self, any_packet: AnyPacketWithConnId<ClientPacket>) -> Result<()> {
        let message_name = any_packet.packet_kind();
        let packet = TryInto::<T>::try_into(any_packet.packet)
            .expect("Packet kind must be in both sender and any packet enum");

        info!("Mediating packet kind: {:?}", message_name);

        self.send(PacketWithConnId {
            packet,
            connection_id: any_packet.connection_id,
        })
        .map_err(|_| Error::Generic("Sender unexpectedly closed".to_owned()))
    }
}

#[derive(Debug, Default)]
pub struct NullSink<P, T>(pub PhantomData<(P, T)>);

impl<T, P> PacketHandler<T> for NullSink<P, T>
where
    T: Into<P> + TryFrom<P>,
{
    type Kind = P;

    fn handle(&self, _any_packet: AnyPacketWithConnId<P>) -> Result<()> {
        Ok(())
    }
}

impl<T> PacketHandler<T> for Sender<T>
where
    T: Into<ServerPacket> + TryFrom<ServerPacket>,
    <T as TryFrom<ServerPacket>>::Error: std::fmt::Debug,
{
    type Kind = ServerPacket;

    fn handle(&self, any_packet: AnyPacketWithConnId<ServerPacket>) -> Result<()> {
        let message_name = any_packet.packet_kind();
        // Cannot happen since packet type must be in both sender and anypacket enum
        let packet = TryInto::<T>::try_into(any_packet.packet)
            .expect("Packet kind must be in both sender and any packet enum");

        info!("Mediating packet kind: {:?}", message_name);

        self.send(packet)
            .map_err(|_| Error::Generic("Sender unexpectedly closed".to_owned()))
    }
}
