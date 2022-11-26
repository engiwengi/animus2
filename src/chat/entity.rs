use speedy::{Readable, Writable};

use super::packet::MessageReceived;
use crate::id::NetworkId;

#[derive(Default)]
pub(crate) struct Chat {
    pub(crate) messages: Vec<Message>,
}

impl Chat {
    pub(crate) fn push(&mut self, message: MessageReceived) {
        self.messages.push(Message::from(message));
    }
}

#[derive(Default)]
pub(crate) struct ChatInput {
    pub(crate) input: String,
}

pub(crate) struct Message {
    sender: NetworkId,
    contents: String,
    kind: MessageKind,
}

impl From<MessageReceived> for Message {
    fn from(value: MessageReceived) -> Self {
        Self {
            sender: value.sender,
            contents: value.contents,
            kind: value.kind,
        }
    }
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub(crate) enum MessageKind {
    Shout,
}
