use bevy::{
    app::PluginGroupBuilder,
    prelude::{
        Changed, Commands, Component, Deref, DerefMut, DetectChanges, Entity, EventReader,
        EventWriter, Input, IntoSystemDescriptor, MouseButton, Plugin, PluginGroup, Query, Res,
        ResMut, Resource,
    },
};
use tracing::{error, info};

use super::packet::{PathTarget, PathTargetRequest};
use crate::{
    client::camera::MouseWorldCoordinates,
    id::{NetworkId, NetworkToWorld},
    network::{
        mediator::PacketWithConnId,
        plugin::{Client, EntityQuery, Network, Packets, Server},
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
            set_position_from_path
                .after("schedule")
                .label("set_position"),
        );
    }
}

impl Plugin for ServerPathPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_system(receive_from_client);
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
pub struct PathResource<T>(T);

// components
#[derive(Clone, Copy, Debug, PartialEq, Eq, Component)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn taxi_distance(&self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Component, Default)]
pub struct Path {
    pub positions: Vec<Position>,
}

// events
#[derive(Component)]
pub struct Target {
    pub entity: Entity,
    pub position: Position,
    pub current_position: Position,
}

// systems
pub fn set_position_from_path(
    mut query: Query<(&mut Position, &mut Path)>,
    mut scheduler: ResMut<PathResource<InnerTimingWheelTree>>,
    tick: Res<Tick>,
) {
    for entity in scheduler.tasks(tick.current()) {
        let Ok((mut position, mut path)) = query.get_mut(entity) else {
            continue;
        };

        let Some(next_position) = path.positions.pop() else {
            continue;
        };

        *position = next_position;
    }
}

pub fn schedule_next_position(
    mut query: Query<(Entity, &MovementSpeed, &Path), Changed<Position>>,
    mut scheduler: ResMut<PathResource<InnerTimingWheelTree>>,
    tick: Res<Tick>,
) {
    for (entity, speed, path) in query.iter_mut() {
        if path.positions.last().is_none() {
            continue;
        }

        scheduler.schedule(tick.current() + **speed, entity);
    }
}

pub fn pathfind(
    mut commands: Commands,
    mut query: Query<(Option<&mut Path>, Option<&mut Position>)>,
    mut path_targets: EventReader<Target>,
) {
    for target in path_targets.iter() {
        let (mut path, mut position) = match query.get_mut(target.entity) {
            Ok((Some(path), Some(position))) => (path, position),
            Ok((..)) => {
                let mut path = Path::default();
                update_path(&mut path, target.position, target.current_position);
                commands
                    .entity(target.entity)
                    .insert((path, target.current_position));

                continue;
            }
            Err(_) => continue,
        };

        let current_target_position = path.positions.first();

        if current_target_position.is_none() {
            position.set_changed();
        }

        if position.x.abs_diff(target.current_position.x) > 1
            || position.y.abs_diff(target.current_position.y) > 1
        {
            update_path(&mut path, target.position, target.current_position);
        } else if current_target_position.map_or(true, |&p| p != target.position) {
            update_path(&mut path, target.position, *position);
        };
    }
}

pub fn receive_from_client(
    mut path_targets: EventWriter<Target>,
    packets: Res<Packets<PacketWithConnId<PathTargetRequest>>>,
    clients: Query<&Network<Client>>,
    positions: Query<&Position>,
    entities: Res<NetworkToWorld<Server>>,
) {
    while let Ok(packet) = packets.receiver.try_recv() {
        let Some(entity) = entities.get(&packet.connection_id) else {
            error!("Packet for unknown entity received");
            continue;
        };

        let Ok(current_position) = positions.get(*entity) else {
            continue;
        };

        let path_target = PathTarget {
            id: packet.connection_id,
            x: packet.packet.x,
            y: packet.packet.y,
            current_x: current_position.x,
            current_y: current_position.y,
        };

        path_targets.send(Target {
            entity: *entity,
            position: Position {
                x: packet.packet.x,
                y: packet.packet.y,
            },
            current_position: *current_position,
        });

        let _ = Network::<Client>::send_all(clients.iter(), path_target);
    }
}

pub fn receive_from_server(
    mut path_targets: EventWriter<Target>,
    packets: Res<Packets<PathTarget>>,
    network_to_world: Res<NetworkToWorld<Client>>,
) {
    while let Ok(packet) = packets.receiver.try_recv() {
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
            current_position: Position {
                x: packet.current_x,
                y: packet.current_y,
            },
        });
    }
}

pub fn request_path(
    server: Query<&Network<Server>>,
    mouse_world_coords: Res<MouseWorldCoordinates>,
    mouse_events: Res<Input<MouseButton>>,
) {
    if mouse_events.just_pressed(MouseButton::Left) {
        let Ok(server) = server.get_single() else {
            error!("Client not yet connected");
            return;
        };

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
            current_x: position.x,
            current_y: position.y,
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
