use derive_more::{Deref, DerefMut};
use speedy::{Readable, Writable};

#[derive(
    Readable, Writable, Deref, DerefMut, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy,
)]
pub struct NetworkId(u64);

impl From<u64> for NetworkId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
