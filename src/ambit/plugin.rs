use bevy::prelude::{
    Changed, Commands, Component, EventReader, EventWriter, IntoSystemDescriptor, Plugin, Query,
    Res, ResMut,
};
use tracing::error;

use super::packet::{DespawnEntity, QueryEntity, SpawnEntity};
use crate::{
    id::{NetworkId, NetworkToWorld},
    network::plugin::{Client, Network, Packets, Server},
    path::plugin::{MaybeNextPosition, Position},
    stat::MovementSpeed,
};

// plugin
pub struct AmbitPlugin;

impl Plugin for AmbitPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        #[cfg(feature = "server")]
        {
            app.add_event::<VisibilityCollision>();
            app.add_system(
                raise_events_on_collisions
                    .after("set_position")
                    .label("collision"),
            );
            app.add_system(notify_visibility_change_to_clients);
        }

        #[cfg(feature = "client")]
        {
            app.add_system(receive_visibility_change_from_server);
        }
    }
}

// resource

// component
#[derive(Component)]
pub(crate) struct Player;

// event
struct VisibilityCollision {
    ids: [NetworkId; 2],
    kind: VisibilityCollisionKind,
}

// util
enum VisibilityCollisionKind {
    Enter,
    Leave,
}

// system
fn raise_events_on_collisions(
    changed_positions: Query<
        (&Position, Option<&MaybeNextPosition>, &NetworkId),
        Changed<Position>,
    >,
    positions: Query<(&Position, &NetworkId)>,
    mut collisions: EventWriter<VisibilityCollision>,
) {
    for (position1, maybe_next_position, network1) in changed_positions.iter() {
        for (position2, network2) in positions.iter() {
            if *network2 == *network1 {
                continue;
            }

            let kind = if position1.taxi_distance(*position2) < 12
                && maybe_next_position
                    .and_then(|m| m.position())
                    .map_or(false, |pos| pos.taxi_distance(*position2) >= 12)
            {
                VisibilityCollisionKind::Leave
            } else if position1.taxi_distance(*position2) >= 10
                && maybe_next_position
                    .and_then(|m| m.position())
                    .map_or(true, |pos| pos.taxi_distance(*position2) < 10)
            {
                VisibilityCollisionKind::Enter
            } else {
                continue;
            };

            collisions.send(VisibilityCollision {
                ids: [*network1, *network2],
                kind,
            });
        }
    }
}

fn notify_visibility_change_to_clients(
    mut collisions: EventReader<VisibilityCollision>,
    network_to_world: Res<NetworkToWorld<Server>>,
    clients: Query<(&NetworkId, &Network<Client>)>,
) {
    for collision in collisions.iter() {
        for (id1, id2) in [(0, 1), (1, 0)] {
            let Some(entity) = network_to_world.get(&collision.ids[id1]) else {
                continue;
            };

            let Ok((id, client)) = clients.get(*entity) else {
                continue;
            };

            if *id == collision.ids[id2] {
                continue;
            }

            let _ = match collision.kind {
                VisibilityCollisionKind::Enter => client.send(SpawnEntity {
                    id: collision.ids[id2],
                }),
                VisibilityCollisionKind::Leave => client.send(DespawnEntity {
                    id: collision.ids[id2],
                }),
            };
        }
    }
}

fn receive_visibility_change_from_server(
    mut commands: Commands,
    spawn_entities: Res<Packets<SpawnEntity>>,
    despawn_entities: Res<Packets<DespawnEntity>>,
    mut network_to_world: ResMut<NetworkToWorld<Client>>,
    server: Query<&Network<Server>>,
) {
    if !spawn_entities.is_empty() {
        let Ok(server) = server.get_single() else {
            error!("Client not yet connected");
            return;
        };

        for spawn in spawn_entities.iter() {
            let entity = commands.spawn((spawn.id, MovementSpeed(3), Player)).id();
            if let Some(prev) = network_to_world.insert(spawn.id, entity) {
                error!("entity already existed");
                commands.entity(prev).despawn();
            }
            let _ = server.send(QueryEntity { id: spawn.id });
        }
    }

    for despawn in despawn_entities.iter() {
        let Some(entity) = network_to_world.remove(&despawn.id) else {
            error!("received unknown network id");
            continue;
        };
        commands.entity(entity).despawn();
    }
}
