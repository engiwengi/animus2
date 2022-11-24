use speedy::{Readable, Writable};

use crate::id::NetworkId;

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct SpawnEntity {
    pub id: NetworkId,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct DespawnEntity {
    pub id: NetworkId,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct QueryEntity {
    pub id: NetworkId,
}
