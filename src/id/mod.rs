use std::marker::PhantomData;

use bevy::{
    prelude::{Component, Entity, Resource},
    utils::HashMap,
};
use derive_more::{Deref, DerefMut, Display};
use speedy::{Readable, Writable};

#[derive(
    Readable,
    Writable,
    Deref,
    DerefMut,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Hash,
    Display,
    Component,
)]
pub struct NetworkId(u64);

impl From<u64> for NetworkId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct NetworkToWorld<S>
where
    S: Send + Sync + 'static,
{
    #[deref]
    #[deref_mut]
    map: HashMap<NetworkId, Entity>,
    marker: PhantomData<S>,
}
