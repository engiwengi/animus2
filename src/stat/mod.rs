use bevy::prelude::{Component, Deref};

#[derive(Component, Deref)]
pub(crate) struct MovementSpeed(pub usize);
