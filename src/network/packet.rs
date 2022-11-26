use std::{hash::Hash, io::Write, sync::Arc};

use speedy::{Readable, Writable};
use tracing::trace;

use super::{error::Result, mediator::AnyPacketHandler};
use crate::id::NetworkId;

pub(crate) struct AnyPacketWithConnId<T> {
    pub(crate) connection_id: NetworkId,
    pub(crate) packet: T,
}

impl<T> AnyPacketWithConnId<T>
where
    T: Packet,
{
    pub(crate) fn packet_kind<'a>(&'a self) -> T::Kind
    where
        T::Kind: From<&'a T>,
    {
        T::Kind::from(&self.packet)
    }
}

pub(crate) trait Packet:
    Sized + 'static + From<Heartbeat> + Writable<speedy::LittleEndian>
{
    type Kind: Hash + Eq + PartialEq + Sized + Send + Sync + std::fmt::Debug;
    type Sender: AnyPacketHandler<Self> + Sync + Send + std::fmt::Debug;
    type OtherPacket: Packet;
}

pub(crate) use client_packet_enum::*;
pub(crate) use server_packet_enum::*;

#[proxy_enum::proxy(ClientPacket)]
mod client_packet_enum {
    use derive_more::TryInto;
    use enum_kinds::EnumKind;

    use super::*;
    use crate::{
        ambit::packet::QueryEntity, chat::packet::SendMessage,
        network::mediator::ClientPacketSender, path::packet::PathTargetRequest,
    };

    #[derive(Readable, Writable, TryInto, Debug, EnumKind)]
    #[enum_kind(ClientPacketKind, derive(Hash))]
    pub(crate) enum ClientPacket {
        SendMessage(SendMessage),
        QueryEntity(QueryEntity),
        PathTargetRequest(PathTargetRequest),
        Heartbeat(Heartbeat),
    }

    impl Packet for ClientPacket {
        type Kind = ClientPacketKind;
        type OtherPacket = ServerPacket;
        type Sender = ClientPacketSender;
    }
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Default)]
pub(crate) struct Heartbeat;

#[proxy_enum::proxy(ServerPacket)]
mod server_packet_enum {
    use derive_more::TryInto;
    use enum_kinds::EnumKind;

    use super::*;
    use crate::{
        ambit::packet::{DespawnEntity, SpawnEntity},
        chat::packet::MessageReceived,
        network::mediator::ServerPacketSender,
        path::packet::PathTarget,
    };

    #[derive(Readable, Writable, TryInto, Debug, EnumKind)]
    #[enum_kind(ServerPacketKind, derive(Hash))]
    pub(crate) enum ServerPacket {
        MessageReceived(MessageReceived),
        AcceptConnection(AcceptConnection),
        PathTarget(PathTarget),
        SpawnEntity(SpawnEntity),
        DespawnEntity(DespawnEntity),
        Heartbeat(Heartbeat),
    }
    impl Packet for ServerPacket {
        type Kind = ServerPacketKind;
        type OtherPacket = ClientPacket;
        type Sender = ServerPacketSender;
    }
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct AcceptConnection {
    pub(crate) connection_id: NetworkId,
}

#[derive(Clone, Debug)]
pub(crate) struct EncodedPacket {
    bytes: Arc<[u8]>,
}

impl EncodedPacket {
    pub(crate) fn try_encode<T, P>(packet: T) -> Result<Self>
    where
        P: Packet + From<T> + Writable<speedy::LittleEndian>,
    {
        let p = P::from(packet);
        let length = Writable::<speedy::LittleEndian>::bytes_needed(&p)?;
        trace!("Writing packet requiring length: {}", length);
        let mut bytes: Vec<u8> = vec![0; length + 4];
        (&mut bytes[..4]).write_all(&u32::to_le_bytes(length as u32))?;

        p.write_to_buffer(&mut bytes[4..])?;

        Ok(Self {
            bytes: bytes.into(),
        })
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use rstest::rstest;

    use super::*;
    use crate::chat::{entity::MessageKind, packet::SendMessage};

    #[rstest]
    #[case(SendMessage { contents: "message".to_owned(), kind: MessageKind::Shout})]
    fn back_and_forth<T>(#[case] packet: T)
    where
        T: PartialEq + Debug + Clone + TryFrom<ClientPacket>,
        <T as TryFrom<ClientPacket>>::Error: Debug,
        ClientPacket: From<T>,
    {
        let any_packet = ClientPacket::from(packet.clone());

        let encoded_packet = speedy::Writable::write_to_vec(&any_packet).unwrap();

        let decoded_any_packet: ClientPacket =
            speedy::Readable::read_from_buffer(&encoded_packet).unwrap();
        let decoded_packet = T::try_from(decoded_any_packet).unwrap();

        assert_eq!(decoded_packet, packet);
    }
}
