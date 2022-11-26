use bevy::{
    app::PluginGroupBuilder,
    prelude::{
        Changed, Commands, Component, Deref, DerefMut, DetectChanges, Entity, EventReader,
        EventWriter, Input, IntoSystemDescriptor, MouseButton, Plugin, PluginGroup, Query, Res,
        ResMut, Resource, With,
    },
};
use tracing::{error, info};

use super::packet::{PathTarget, PathTargetRequest};
use crate::{
    client::camera::MouseWorldCoordinates,
    id::{NetworkId, NetworkToWorld},
    network::{
        mediator::PacketWithConnId,
        plugin::{Client, EntityQuery, Me, Network, Packets, Server},
    },
    stat::MovementSpeed,
    time::{
        schedule::{InnerTimingWheelTree, Scheduler},
        tick::Tick,
    },
};

pub struct PathPlugins;

impl PluginGroup for PathPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group.add(BasePathPlugin);

        #[cfg(feature = "client")]
        {
            group = group.add(ClientPathPlugin);
        }

        #[cfg(feature = "server")]
        {
            group = group.add(ServerPathPlugin);
        }

        group
    }
}

struct BasePathPlugin;
struct ServerPathPlugin;
struct ClientPathPlugin;

impl Plugin for BasePathPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_event::<Target>();
        app.insert_resource(PathResource(InnerTimingWheelTree::new(2)));
        app.add_system(pathfind.label("pathfind"));
        app.add_system(schedule_next_position.after("pathfind").label("schedule"));
        app.add_system(
            set_position_from_next_position
                .after("schedule")
                .label("set_position"),
        );
    }
}

impl Plugin for ServerPathPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_system(receive_from_client.after("set_position"));
        app.add_system(respond_to_queries);
    }
}

impl Plugin for ClientPathPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_system(request_path);
        app.add_system(receive_from_server);
    }
}

// resources
#[derive(Resource, Deref, DerefMut, Default)]
struct PathResource<T>(T);

// components
#[derive(Clone, Copy, Debug, PartialEq, Eq, Component)]
pub(crate) struct Position {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl Position {
    pub(crate) fn taxi_distance(&self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Component, Default)]
pub(crate) struct MaybeNextPosition {
    next_position: Option<NextPosition>,
}

impl MaybeNextPosition {
    pub(crate) fn position(self) -> Option<Position> {
        self.next_position.map(|p| p.position)
    }

    pub(crate) fn next_position(self) -> Option<NextPosition> {
        self.next_position
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NextPosition {
    position: Position,
    at_tick: usize,
    prev_tick: usize,
}

impl NextPosition {
    fn new(position: Position, at_tick: usize, prev_tick: usize) -> Self {
        Self {
            position,
            at_tick,
            prev_tick,
        }
    }

    pub(crate) fn percent(self, current_tick: usize, overstep_percentage: f64) -> f64 {
        let speed = (self.at_tick - self.prev_tick) as f64;

        if speed == 0.0 {
            return 0.0;
        }

        ((current_tick - self.prev_tick) as f64 / speed) + (overstep_percentage / speed)
    }

    pub(crate) fn position(&self) -> Position {
        self.position
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Component, Default)]
pub(crate) struct Path {
    positions: Vec<Position>,
}

// events
#[derive(Component)]
struct Target {
    entity: Entity,
    position: Position,
    current_or_next_position: Position,
}

// systems

fn set_position_from_next_position(
    mut query: Query<(&mut Position, &mut MaybeNextPosition)>,
    mut scheduler: ResMut<PathResource<InnerTimingWheelTree>>,
    tick: Res<Tick>,
) {
    for entity in scheduler.tasks(tick.current()) {
        let Ok((mut position,  mut maybe_next_position)) = query.get_mut(entity) else {
            continue;
        };

        let Some(next_position) = maybe_next_position.next_position.take() else {
            continue;
        };

        *position = next_position.position;
    }
}

fn schedule_next_position(
    mut query: Query<
        (Entity, &MovementSpeed, &mut Path, &mut MaybeNextPosition),
        Changed<MaybeNextPosition>,
    >,
    mut scheduler: ResMut<PathResource<InnerTimingWheelTree>>,
    tick: Res<Tick>,
) {
    for (entity, speed, mut path, mut next) in query.iter_mut() {
        if next.next_position.is_some() {
            error!("shouldn't happen");
            continue;
        }
        let at_tick = tick.current() + **speed;

        next.next_position = path
            .positions
            .pop()
            .map(|p| NextPosition::new(p, at_tick, tick.current()));

        if next.next_position.is_some() {
            scheduler.schedule(at_tick, entity);
        }
    }
}

fn pathfind(
    mut commands: Commands,
    mut query: Query<(
        Option<&mut Path>,
        Option<&Position>,
        Option<&mut MaybeNextPosition>,
    )>,
    mut path_targets: EventReader<Target>,
) {
    for target in path_targets.iter() {
        let (mut path, position, mut next_position) = match query.get_mut(target.entity) {
            Ok((Some(path), Some(position), Some(next_position))) => {
                (path, position, next_position)
            }
            Ok((..)) => {
                let mut path = Path::default();
                update_path(&mut path, target.position, target.current_or_next_position);
                let next_position = MaybeNextPosition {
                    next_position: None,
                };
                commands.entity(target.entity).insert((
                    path,
                    target.current_or_next_position,
                    next_position,
                ));

                continue;
            }
            Err(_) => continue, // unknown entity
        };

        let current_or_next_position = next_position.position().unwrap_or(*position);
        let current_target = path.positions.first();

        // if current_next_position.is_none() {
        //     position.set_changed();
        // }

        if current_or_next_position
            .x
            .abs_diff(target.current_or_next_position.x)
            > 4
            || current_or_next_position
                .y
                .abs_diff(target.current_or_next_position.y)
                > 4
        {
            update_path(&mut path, target.position, target.current_or_next_position);
        } else if current_target.map_or(true, |&current_target| current_target != target.position) {
            update_path(&mut path, target.position, current_or_next_position);
        };

        if next_position.next_position.is_none() {
            next_position.set_changed();
            // next_position.position = path.positions.pop();
        }
    }
}

fn receive_from_client(
    mut path_targets: EventWriter<Target>,
    packets: Res<Packets<PacketWithConnId<PathTargetRequest>>>,
    clients: Query<&Network<Client>>,
    positions: Query<(&Position, &MaybeNextPosition)>,
    entities: Res<NetworkToWorld<Server>>,
) {
    for packet in packets.iter() {
        let Some(entity) = entities.get(&packet.connection_id) else {
            error!("Packet for unknown entity received");
            continue;
        };

        let Ok((current_position, current_next_position)) = positions.get(*entity) else {
            error!("no next position");
            continue;
        };

        let current_or_next_position = current_next_position
            .position()
            .unwrap_or(*current_position);

        let path_target = PathTarget {
            id: packet.connection_id,
            x: packet.packet.x,
            y: packet.packet.y,
            current_or_next_x: current_or_next_position.x,
            current_or_next_y: current_or_next_position.y,
        };

        path_targets.send(Target {
            entity: *entity,
            position: Position {
                x: packet.packet.x,
                y: packet.packet.y,
            },
            current_or_next_position,
        });

        let _ = Network::<Client>::send_all(clients.iter(), path_target);
    }
}

fn receive_from_server(
    mut path_targets: EventWriter<Target>,
    packets: Res<Packets<PathTarget>>,
    network_to_world: Res<NetworkToWorld<Client>>,
) {
    for packet in packets.iter() {
        let Some(entity) = network_to_world.get(&packet.id) else {
            error!("Packet for unknown entity received");
            continue;
        };

        path_targets.send(Target {
            entity: *entity,
            position: Position {
                x: packet.x,
                y: packet.y,
            },
            current_or_next_position: Position {
                x: packet.current_or_next_x,
                y: packet.current_or_next_y,
            },
        });
    }
}

fn request_path(
    mut path_targets: EventWriter<Target>,
    server: Query<&Network<Server>>,
    me: Query<(Entity, &Position), With<Me>>,
    mouse_world_coords: Res<MouseWorldCoordinates>,
    mouse_events: Res<Input<MouseButton>>,
) {
    if mouse_events.just_pressed(MouseButton::Left) {
        let Ok(server) = server.get_single() else {
            error!("Client not yet connected");
            return;
        };
        let Ok((me, position)) = me.get_single() else {
            error!("Client has not yet spawned itself");
            return;
        };

        path_targets.send(Target {
            entity: me,
            current_or_next_position: Position {
                x: position.x,
                y: position.y,
            },
            position: Position {
                x: mouse_world_coords.x,
                y: mouse_world_coords.y,
            },
        });

        if let Err(e) = server.send(PathTargetRequest {
            x: mouse_world_coords.x,
            y: mouse_world_coords.y,
        }) {
            error!("{:?}", e);
        }
    }
}

fn respond_to_queries(
    mut queries: EventReader<EntityQuery>,
    query: Query<(&NetworkId, &Path, &Position)>,
    clients: Query<&Network<Client>>,
) {
    for entity_query in queries.iter() {
        let Ok((&id, path, position)) = query.get(entity_query.entity) else {
            continue;
        };

        let Ok(client) = clients.get(entity_query.querier) else {
            continue;
        };

        let path_target = PathTarget {
            id,
            x: path.positions.first().map(|p| p.x).unwrap_or(position.x),
            y: path.positions.first().map(|p| p.y).unwrap_or(position.y),
            current_or_next_x: position.x,
            current_or_next_y: position.y,
        };

        let _ = client.send(path_target);
    }
}

fn update_path(path: &mut Path, target: Position, position: Position) {
    info!("path finding");
    path.positions.clear();
    for y in (1..=target.y.abs_diff(position.y)).rev() {
        if target.y > position.y {
            path.positions.push(Position {
                y: position.y + y as i32,
                x: target.x,
            });
        } else {
            path.positions.push(Position {
                y: position.y - y as i32,
                x: target.x,
            });
        }
    }

    for x in (1..=target.x.abs_diff(position.x)).rev() {
        if target.x > position.x {
            path.positions.push(Position {
                x: position.x + x as i32,
                y: position.y,
            });
        } else {
            path.positions.push(Position {
                x: position.x - x as i32,
                y: position.y,
            });
        }
    }
    info!("path length: {}", path.positions.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_update_path_pos_x_and_z() {
        let mut path = Path { positions: vec![] };
        let target = Position { x: 3, y: 3 };
        let position = Position { x: 0, y: 0 };

        update_path(&mut path, target, position);

        assert_eq!(path.positions.len(), 6);
        assert_eq!(path.positions[0], Position { x: 1, y: 0 });
        assert_eq!(path.positions[1], Position { x: 2, y: 0 });
        assert_eq!(path.positions[2], Position { x: 3, y: 0 });
        assert_eq!(path.positions[3], Position { x: 3, y: 1 });
        assert_eq!(path.positions[4], Position { x: 3, y: 2 });
        assert_eq!(path.positions[5], Position { x: 3, y: 3 });
    }

    #[test]
    fn should_update_path_pos_x() {
        let mut path = Path { positions: vec![] };
        let target = Position { x: 3, y: 0 };
        let position = Position { x: 0, y: 0 };

        update_path(&mut path, target, position);

        assert_eq!(path.positions.len(), 3);
        assert_eq!(path.positions[0], Position { x: 1, y: 0 });
        assert_eq!(path.positions[1], Position { x: 2, y: 0 });
        assert_eq!(path.positions[2], Position { x: 3, y: 0 });
    }

    #[test]
    fn should_update_path_pos_z() {
        let mut path = Path { positions: vec![] };
        let target = Position { x: 0, y: 3 };
        let position = Position { x: 0, y: 0 };

        update_path(&mut path, target, position);

        assert_eq!(path.positions.len(), 3);
        assert_eq!(path.positions[0], Position { y: 1, x: 0 });
        assert_eq!(path.positions[1], Position { y: 2, x: 0 });
        assert_eq!(path.positions[2], Position { y: 3, x: 0 });
    }
    #[test]
    fn should_update_path_neg_x_neg_z() {
        let mut path = Path { positions: vec![] };
        let target = Position { x: -3, y: -3 };
        let position = Position { x: 0, y: 0 };

        update_path(&mut path, target, position);

        assert_eq!(path.positions.len(), 6);
        assert_eq!(path.positions[0], Position { x: -1, y: 0 });
        assert_eq!(path.positions[1], Position { x: -2, y: 0 });
        assert_eq!(path.positions[2], Position { x: -3, y: 0 });
        assert_eq!(path.positions[3], Position { x: -3, y: -1 });
        assert_eq!(path.positions[4], Position { x: -3, y: -2 });
        assert_eq!(path.positions[5], Position { x: -3, y: -3 });
    }
    #[test]
    fn should_update_path_pos_x_neg_z() {
        let mut path = Path { positions: vec![] };
        let target = Position { x: 3, y: -3 };
        let position = Position { x: 0, y: 0 };

        update_path(&mut path, target, position);

        assert_eq!(path.positions.len(), 6);
        assert_eq!(path.positions[0], Position { x: 1, y: 0 });
        assert_eq!(path.positions[1], Position { x: 2, y: 0 });
        assert_eq!(path.positions[2], Position { x: 3, y: 0 });
        assert_eq!(path.positions[3], Position { x: 3, y: -1 });
        assert_eq!(path.positions[4], Position { x: 3, y: -2 });
        assert_eq!(path.positions[5], Position { x: 3, y: -3 });
    }
}
