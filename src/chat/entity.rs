use speedy::{Readable, Writable};

use super::packet::MessageReceived;
use crate::id::NetworkId;

#[derive(Default)]
pub struct Chat {
    pub messages: Vec<Message>,
}

impl Chat {
    pub fn push(&mut self, message: MessageReceived) {
        self.messages.push(Message::from(message));
    }
}

#[derive(Default)]
pub struct ChatInput {
    pub input: String,
}

pub struct Message {
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
pub enum MessageKind {
    Shout,
}
