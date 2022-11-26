use speedy::{Readable, Writable};

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct TickSync {
    pub(crate) current: usize,
}
