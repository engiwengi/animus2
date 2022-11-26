use bevy::{
    prelude::{Plugin, Res, ResMut, Resource, SystemSet},
    time::FixedTimestep,
};

use super::packet::TickSync;
use crate::network::plugin::Packets;

const TIMESTEP: f64 = 3.0 / 60.0;

#[derive(Clone, Copy, Resource, Default)]
pub(crate) struct Tick {
    current: usize,
}

impl Tick {
    pub(crate) fn set(&mut self, to: usize) {
        self.current = to;
    }

    pub(crate) fn increment(&mut self) {
        self.current = self.current.wrapping_add(1);
    }

    pub(crate) fn current(self) -> usize {
        self.current
    }
}

fn increment_tick(mut tick: ResMut<Tick>) {
    tick.increment();
}

pub struct TickPlugin;

impl Plugin for TickPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<Tick>();

        app.add_system_set(
            SystemSet::new()
                .label("increment_tick")
                .with_run_criteria(FixedTimestep::step(TIMESTEP).with_label("tick"))
                .with_system(increment_tick),
        );
    }
}

#[derive(Clone, Copy, Resource)]
pub(crate) struct TickTarget {
    current: usize,
}

impl TickTarget {
    pub(crate) fn set(&mut self, to: usize) {
        self.current = to;
    }

    pub(crate) fn increment(&mut self) {
        self.current = self.current.wrapping_add(1);
    }

    pub(crate) fn current(self) -> usize {
        self.current
    }
}

pub(crate) fn sync_from_server(
    mut tick_target: ResMut<TickTarget>,
    packets: Res<Packets<TickSync>>,
) {
    for packet in packets.iter() {
        tick_target.set(packet.current);
    }
}
