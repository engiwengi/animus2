//! A simple 3D scene with light shining over a cube sitting on a plane.

// use std::{
//     io,
//     time::{Duration, Instant, SystemTime},
// };
//
// use bevy::{
//     app::{AppExit, ScheduleRunnerPlugin},
//     ecs::event::ManualEventReader,
//     log::LogPlugin,
//     prelude::*,
//     time::FixedTimestep,
// };
// use iyes_loopless::prelude::AppLooplessFixedTimestepExt;
//
// const TIMESTEP_1_PER_SECOND: f64 = 60.0 / 60.0;
//
// fn main() {
//     let mut app = App::new();
//     app
//         .set_runner(run_in_async)
//         .add_event::<TestEvent>()
//         .add_plugins(MinimalPlugins)
//         .add_plugin(LogPlugin::default())
//         .add_system(send_event)
//         .add_system(read_event)
//         // .add_fixed_timestep(
//         //     Duration::from_millis(50),
//         //     // we need to give it a string name, to refer to it
//         //     "my_fixed_update",
//         // )
//         // .add_fixed_timestep_system("my_fixed_update", 0, send_event)
//         // .add_fixed_timestep_system("my_fixed_update", 0, read_event)
//         // .add_fixed_timestep_system("my_fixed_update", 0, send_event)
//         ;
//
//     run_in_async(app);
// }
//
// fn run_in_async(app: App) {
//     tokio::runtime::Builder::new_multi_thread()
//         .enable_all()
//         .build()
//         .unwrap()
//         .block_on(async move {
//             let now = SystemTime::now()
//                 .duration_since(std::time::UNIX_EPOCH)
//                 .unwrap();
//
//             let secs = now.as_secs();
//
//             let next_second = std::time::UNIX_EPOCH +
// Duration::from_secs(secs + 1u64);             let duration_until_next_second
// = next_second.duration_since(SystemTime::now()).unwrap();             // let
// now = Instant::now()             let _ =
// tokio_timerfd::sleep(duration_until_next_second).await;
// my_runner(app, Duration::from_millis(1000)).await;         })
// }
//
// async fn my_runner(mut app: App, timestep: Duration) {
//     let mut app_exit_event_reader = ManualEventReader::<AppExit>::default();
//     let mut tick = move |app: &mut App, wait: Duration| ->
// Result<Option<Duration>, AppExit> {         let start_time = Instant::now();
//
//         if let Some(app_exit_events) =
// app.world.get_resource_mut::<Events<AppExit>>() {             if let
// Some(exit) = app_exit_event_reader.iter(&app_exit_events).last() {
//                 return Err(exit.clone());
//             }
//         }
//
//         app.update();
//
//         if let Some(app_exit_events) =
// app.world.get_resource_mut::<Events<AppExit>>() {             if let
// Some(exit) = app_exit_event_reader.iter(&app_exit_events).last() {
//                 return Err(exit.clone());
//             }
//         }
//
//         let end_time = Instant::now();
//
//         let exe_time = end_time - start_time;
//         if exe_time < wait {
//             return Ok(Some(wait - exe_time));
//         }
//
//         Ok(None)
//     };
//
//     let mut drift_adjusted_timestep = timestep;
//
//     while let Ok(delay) = tick(&mut app, drift_adjusted_timestep) {
//         let before_delay = Instant::now();
//         if let Some(desired_delay) = delay {
//             let _ = tokio_timerfd::sleep(desired_delay).await;
//
//             let real_delay = before_delay.elapsed();
//             let (drift, drift_is_negative) = if real_delay > desired_delay {
//                 (real_delay - desired_delay, true)
//             } else {
//                 (desired_delay - real_delay, false)
//             };
//
//             if drift > timestep {
//                 error!("waited for way longer than expected");
//                 drift_adjusted_timestep = timestep;
//             } else if drift_is_negative {
//                 drift_adjusted_timestep = timestep - drift;
//             } else {
//                 drift_adjusted_timestep = timestep + drift;
//             }
//         } else {
//             error!("server is experiencing heavy load");
//             drift_adjusted_timestep = timestep;
//         }
//     }
// }
//
// #[derive(Default)]
// struct TestEvent;
//
// /// set up a simple 3D scene
// fn setup(
//     mut commands: Commands,
//     mut meshes: ResMut<Assets<Mesh>>,
//     mut materials: ResMut<Assets<StandardMaterial>>,
// ) {
//     // plane
//     commands.spawn_bundle(PbrBundle {
//         mesh: meshes.add(Mesh::from(shape::Plane { size: 5.0 })),
//         material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
//         ..default()
//     });
//     // cube
//     commands.spawn_bundle(PbrBundle {
//         mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
//         material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
//         transform: Transform::from_xyz(0.0, 0.5, 0.0),
//         ..default()
//     });
//     // light
//     commands.spawn_bundle(PointLightBundle {
//         point_light: PointLight {
//             intensity: 1500.0,
//             shadows_enabled: true,
//             ..default()
//         },
//         transform: Transform::from_xyz(4.0, 8.0, 4.0),
//         ..default()
//     });
//     // camera
//     commands.spawn_bundle(Camera3dBundle {
//         transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO,
// Vec3::Y),         ..default()
//     });
// }
//
// fn send_event(mut test_events: EventWriter<TestEvent>, mut should_send:
// Local<u32>) {     *should_send = (*should_send + 1) % 4;
//     if *should_send == 0 {
//         test_events.send_default();
//         // std::thread::sleep(Duration::from_millis(15));
//     } else if *should_send == 1 {
//         // std::thread::sleep(Duration::from_millis(5));
//     }
// }
//
// fn read_event(mut test_events: EventReader<TestEvent>) {
//     info!("events {}", test_events.iter().count());
// }

use animus_lib::{
    ambit::plugin::AmbitPlugin, network::plugin::NetworkPlugin, path::plugin::PathPlugins,
    time::tick::TickPlugin,
};
use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugin(TickPlugin)
        .add_plugin(NetworkPlugin)
        .add_plugins(PathPlugins)
        .add_plugin(AmbitPlugin)
        .add_startup_system(setup);

    app.run();
}

/// set up a simple 3D scene
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // plane
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Plane { size: 5.0 })),
        material: materials.add(Color::rgb(0.3, 0.5, 0.3).into()),
        ..default()
    });
    // cube
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Cube { size: 1.0 })),
        material: materials.add(Color::rgb(0.8, 0.7, 0.6).into()),
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..default()
    });
    // light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(-2.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
}
