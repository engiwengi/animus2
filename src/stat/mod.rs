use bevy::prelude::{Component, Deref};

#[derive(Component, Deref)]
pub struct MovementSpeed(pub usize);
