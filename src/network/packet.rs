use std::{hash::Hash, io::Write, sync::Arc};

use speedy::{Readable, Writable};

use super::error::Result;

pub struct AnyPacketWithConnId<T> {
    pub connection_id: u64,
    pub packet: T,
}

impl<T> AnyPacketWithConnId<T>
where
    T: Packet,
{
    pub fn packet_kind<'a>(&'a self) -> T::Kind
    where
        T::Kind: From<&'a T>,
    {
        T::Kind::from(&self.packet)
    }
}

pub trait Packet: Sized + 'static + From<Heartbeat> + Writable<speedy::LittleEndian> {
    type Kind: Hash + Eq + PartialEq + Sized + Send + Sync + std::fmt::Debug;
    type Sender: AnyPacketHandler<Self> + Sync + Send;
    type OtherPacket: Packet;
}

pub use client_packet_enum::*;
pub use server_packet_enum::*;
use tracing::trace;

use super::mediator::AnyPacketHandler;

#[proxy_enum::proxy(ClientPacket)]
mod client_packet_enum {
    use derive_more::TryInto;
    use enum_kinds::EnumKind;

    use super::*;
    use crate::{chat::packet::SendMessage, network::mediator::ClientPacketSender};

    #[derive(Readable, Writable, TryInto, Debug, EnumKind)]
    #[enum_kind(ClientPacketKind, derive(Hash))]
    pub enum ClientPacket {
        SendMessage(SendMessage),
        Heartbeat(Heartbeat),
    }

    impl Packet for ClientPacket {
        type Kind = ClientPacketKind;
        type OtherPacket = ServerPacket;
        type Sender = ClientPacketSender;
    }
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Default)]
pub struct Heartbeat;

#[proxy_enum::proxy(ServerPacket)]
mod server_packet_enum {
    use derive_more::TryInto;
    use enum_kinds::EnumKind;

    use super::*;
    use crate::{chat::packet::MessageReceived, network::mediator::ServerPacketSender};

    #[derive(Readable, Writable, TryInto, Debug, EnumKind)]
    #[enum_kind(ServerPacketKind, derive(Hash))]
    pub enum ServerPacket {
        MessageReceived(MessageReceived),
        AcceptConnection(AcceptConnection),
        Heartbeat(Heartbeat),
    }
    impl Packet for ServerPacket {
        type Kind = ServerPacketKind;
        type OtherPacket = ClientPacket;
        type Sender = ServerPacketSender;
    }
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, Clone, Copy)]
pub struct AcceptConnection {
    pub connection_id: u64,
}

#[derive(Clone, Debug)]
pub struct EncodedPacket {
    pub bytes: Arc<[u8]>,
}

impl EncodedPacket {
    pub fn try_encode<T, P>(packet: T) -> Result<Self>
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
