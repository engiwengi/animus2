use animus_lib::{
    ambit::plugin::AmbitPlugin, client::camera::ClientPlugin, network::plugin::NetworkPlugin,
    path::plugin::PathPlugins, time::tick::TickPlugin,
};
use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_plugin(NetworkPlugin);
    app.add_plugin(TickPlugin);
    app.add_plugins(PathPlugins);
    app.add_plugin(AmbitPlugin);
    app.add_plugin(ClientPlugin);
    app.add_startup_system(setup);
    app.run();
}

fn setup(mut commands: Commands, _asset_server: Res<AssetServer>) {
    commands.spawn(Camera2dBundle::default());
}
