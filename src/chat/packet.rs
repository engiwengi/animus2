use speedy::{Readable, Writable};

use super::entity::MessageKind;
use crate::{id::NetworkId, network::mediator::PacketWithConnId};

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SendMessage {
    pub kind: MessageKind,
    pub contents: String,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct MessageReceived {
    pub sender: NetworkId,
    pub kind: MessageKind,
    pub contents: String,
}

impl From<PacketWithConnId<SendMessage>> for MessageReceived {
    fn from(value: PacketWithConnId<SendMessage>) -> Self {
        Self {
            sender: NetworkId::from(value.connection_id),
            contents: value.packet.contents,
            kind: value.packet.kind,
        }
    }
}
