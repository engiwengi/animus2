use std::ops::Div;

use bevy::{
    prelude::{
        Added, Camera, Changed, Commands, Entity, GlobalTransform, Local, Plugin, Query, Res,
        ResMut, Resource, Transform, Vec2, Vec3, Without,
    },
    render::camera::RenderTarget,
    sprite::{Sprite, SpriteBundle},
    window::Windows,
};
use tracing::{error, info};

use crate::{
    network::plugin::{Client, Network},
    path::plugin::Position,
};

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<MouseWorldCoordinates>();
        app.add_system(set_cursor_world_coords);
        app.add_system(update_transform);
        app.add_system(add_sprites);
    }
}

#[derive(Resource, Default)]
pub struct MouseWorldCoordinates {
    pub x: i32,
    pub y: i32,
}

fn set_cursor_world_coords(
    // need to get window dimensions
    wnds: Res<Windows>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut mouse_world_coords: ResMut<MouseWorldCoordinates>,
    mut count: Local<u32>,
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
    let world_pos = world_pos.div(30.0).as_ivec3();
    mouse_world_coords.x = world_pos.x;
    mouse_world_coords.y = world_pos.y;
    *count = count.wrapping_add(1);
    if *count % 100 == 0 {
        info!("world pos: {}", world_pos);
    }
}

fn update_transform(mut query: Query<(&mut Transform, &Position), Changed<Position>>) {
    for (mut transform, position) in query.iter_mut() {
        transform.translation = Vec3::new(position.x as f32 * 30.0, position.y as f32 * 30.0, 0.0);
    }
}

#[allow(clippy::type_complexity)]
fn add_sprites(
    mut commands: Commands,
    new_positions: Query<(Entity, &Position), (Added<Position>, Without<Network<Client>>)>,
) {
    for (entity, position) in new_positions.iter() {
        commands.entity(entity).insert(SpriteBundle {
            sprite: Sprite {
                custom_size: Some(Vec2::splat(30.0)),
                ..Default::default()
            },
            transform: Transform::from_translation(Vec3::new(
                position.x as f32 * 30.0,
                position.y as f32 * 30.0,
                0.0,
            )),
            ..Default::default()
        });
    }
}
