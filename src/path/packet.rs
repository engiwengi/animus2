use speedy::{Readable, Writable};

use super::plugin::Position;
use crate::id::NetworkId;

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct PathTargetRequest {
    pub x: i32,
    pub y: i32,
}

#[derive(Readable, Writable, Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
pub struct PathTarget {
    pub id: NetworkId,
    pub x: i32,
    pub y: i32,

    pub current_x: i32,
    pub current_y: i32,
}

impl From<PathTarget> for Position {
    fn from(value: PathTarget) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}
