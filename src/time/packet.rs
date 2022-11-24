use speedy::{Readable, Writable};

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct TickSync {
    pub current: usize,
}
