use speedy::{Readable, Writable};

use super::plugin::Position;
use crate::id::NetworkId;

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct PathTargetRequest {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub(crate) struct PathTarget {
    pub(crate) id: NetworkId,
    pub(crate) x: i32,
    pub(crate) y: i32,

    pub(crate) current_or_next_x: i32,
    pub(crate) current_or_next_y: i32,
}

impl From<PathTarget> for Position {
    fn from(value: PathTarget) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}
