use speedy::{Readable, Writable};

use super::entity::MessageKind;
use crate::{id::NetworkId, network::mediator::PacketWithConnId};

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct SendMessage {
    pub(crate) kind: MessageKind,
    pub(crate) contents: String,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub(crate) struct MessageReceived {
    pub(crate) sender: NetworkId,
    pub(crate) kind: MessageKind,
    pub(crate) contents: String,
}

impl From<PacketWithConnId<SendMessage>> for MessageReceived {
    fn from(value: PacketWithConnId<SendMessage>) -> Self {
        Self {
            sender: value.connection_id,
            contents: value.packet.contents,
            kind: value.packet.kind,
        }
    }
}
