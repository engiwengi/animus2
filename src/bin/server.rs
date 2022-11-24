use animus_lib::{
    ambit::plugin::AmbitPlugin, network::plugin::NetworkPlugin, path::plugin::PathPlugins,
    time::tick::TickPlugin,
};
use bevy::{log::LogPlugin, prelude::*};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugin(LogPlugin::default());
    app.add_plugin(NetworkPlugin);
    app.add_plugin(TickPlugin);
    app.add_plugins(PathPlugins);
    app.add_plugin(AmbitPlugin);

    app.run();
}
