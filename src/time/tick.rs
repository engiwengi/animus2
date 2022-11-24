use bevy::{
    prelude::{Plugin, ResMut, Resource, SystemSet},
    time::FixedTimestep,
};
use crossbeam_channel::Receiver;

use super::packet::TickSync;

const TIMESTEP: f64 = 6.0 / 60.0;

#[derive(Clone, Copy, Resource, Default)]
pub struct Tick {
    current: usize,
}

impl Tick {
    pub fn set(&mut self, to: usize) {
        self.current = to;
    }

    pub fn increment(&mut self) {
        self.current = self.current.wrapping_add(1);
    }

    pub fn current(self) -> usize {
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
                .with_run_criteria(FixedTimestep::step(TIMESTEP))
                .with_system(increment_tick),
        );
    }
}

#[derive(Clone, Copy, Resource)]
pub struct TickTarget {
    current: usize,
}

impl TickTarget {
    pub fn set(&mut self, to: usize) {
        self.current = to;
    }

    pub fn increment(&mut self) {
        self.current = self.current.wrapping_add(1);
    }

    pub fn current(self) -> usize {
        self.current
    }
}

pub fn sync_from_server(mut tick_target: ResMut<TickTarget>, packets: Receiver<TickSync>) {
    while let Ok(packet) = packets.try_recv() {
        tick_target.set(packet.current);
    }
}
