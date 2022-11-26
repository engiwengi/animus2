use speedy::{Readable, Writable};

use crate::id::NetworkId;

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct SpawnEntity {
    pub(crate) id: NetworkId,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct DespawnEntity {
    pub(crate) id: NetworkId,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct QueryEntity {
    pub(crate) id: NetworkId,
}
