use std::ops::Div;

use bevy::{
    prelude::{
        Added, Camera, Color, Commands, Component, Entity, GlobalTransform, IntoSystemDescriptor,
        Plugin, Query, Res, ResMut, Resource, Transform, Vec2, Vec3, With,
    },
    render::camera::RenderTarget,
    sprite::{Sprite, SpriteBundle},
    time::FixedTimesteps,
    window::Windows,
};
use tracing::{error, info};

use crate::{
    ambit::plugin::Player,
    network::plugin::{Client, Network},
    path::plugin::{MaybeNextPosition, Position},
    time::tick::Tick,
};

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<MouseWorldCoordinates>();
        app.add_system(set_cursor_world_coords);
        app.add_system(update_transform.after("set_position"));
        app.add_system(add_player_sprites);
        app.add_system(update_mouse_position_marker);
    }
}

#[derive(Resource, Default)]
pub(crate) struct MouseWorldCoordinates {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

fn set_cursor_world_coords(
    // need to get window dimensions
    wnds: Res<Windows>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut mouse_world_coords: ResMut<MouseWorldCoordinates>,
) {
    // get the camera info and transform
    // assuming there is exactly one main camera entity, so query::single() is OK
    let Ok((camera, camera_transform)) = q_camera.get_single() else {
        error!("no camera");
        return;
    };

    // get the window that the camera is displaying to (or the primary window)
    let wnd = if let RenderTarget::Window(id) = camera.target {
        wnds.get(id).unwrap()
    } else {
        wnds.get_primary().unwrap()
    };

    // check if the cursor is inside the window and get its position
    let Some(screen_pos) = wnd.cursor_position() else {
        return;

    };
    let window_size = Vec2::new(wnd.width(), wnd.height());

    // convert screen position [0..resolution] to ndc [-1..1] (gpu coordinates)
    let ndc = (screen_pos / window_size) * 2.0 - Vec2::ONE;

    let Some(world_pos) = camera.ndc_to_world(camera_transform, ndc.extend(0.)) else {
        return;
    };

    // reduce it to a 2D value
    let world_pos = world_pos.div(30.0).floor().as_ivec3();
    mouse_world_coords.x = world_pos.x;
    mouse_world_coords.y = world_pos.y;
}

fn update_transform(
    mut query: Query<(&mut Transform, &Position, Option<&MaybeNextPosition>)>,
    timesteps: Res<FixedTimesteps>,
    tick: Res<Tick>,
) {
    let timestep = timesteps.get("tick").unwrap();
    let overstep_percentage = timestep.overstep_percentage();
    for (mut transform, position, next_position) in query.iter_mut() {
        if let Some(next_position) = next_position.and_then(|n| n.next_position()) {
            let percent = next_position.percent(tick.current(), overstep_percentage);
            transform.translation.x = (position.x as f64 * (1.0 - percent)
                + next_position.position().x as f64 * percent)
                as f32
                * 30.0
                + 15.0;
            transform.translation.y = (position.y as f64 * (1.0 - percent)
                + next_position.position().y as f64 * percent)
                as f32
                * 30.0
                + 15.0;
        } else {
            transform.translation.x = position.x as f32 * 30.0 + 15.0;
            transform.translation.y = position.y as f32 * 30.0 + 15.0;
        }
    }
}

#[allow(clippy::type_complexity)]
fn add_player_sprites(
    mut commands: Commands,
    new_positions: Query<
        (Entity, &Position, Option<&Network<Client>>),
        (Added<Position>, With<Player>),
    >,
) {
    for (entity, position, is_client) in new_positions.iter() {
        let color = if is_client.is_some() {
            info!("spawning client");
            Color::rgb(1.0, 0.0, 0.0)
        } else {
            Color::rgb(1.0, 1.0, 0.0)
        };

        let custom_size = if is_client.is_some() {
            Some(Vec2::splat(35.0))
        } else {
            Some(Vec2::splat(30.0))
        };
        commands.entity(entity).insert(SpriteBundle {
            sprite: Sprite {
                color,
                custom_size,
                ..Default::default()
            },
            transform: Transform::from_translation(Vec3::new(
                position.x as f32 * 30.0 + 15.0,
                position.y as f32 * 30.0 + 15.0,
                0.0,
            )),
            ..Default::default()
        });
    }
}

#[derive(Component)]
struct MouseMarker;

fn update_mouse_position_marker(
    mut commands: Commands,
    mouse_coords: Res<MouseWorldCoordinates>,
    mut query: Query<&mut Position, With<MouseMarker>>,
) {
    if !mouse_coords.is_changed() {
        return;
    }
    let Ok(mut position) = query.get_single_mut() else {
        commands.spawn((
            MouseMarker,
            SpriteBundle {
                sprite: Sprite {
                    color: Color::rgb(0.5, 0.5, 0.5),
                    custom_size: Some(Vec2::splat(30.0)),
                    ..Default::default()
                },
                // transform: Transform::from_xyz(0.0, 0.0, -1.0),
                ..Default::default()
            },
            Position{ x: 0, y: 0},
        ));
        return;
    };

    position.x = mouse_coords.x;
    position.y = mouse_coords.y;
}
