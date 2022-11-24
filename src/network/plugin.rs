use std::{marker::PhantomData, net::SocketAddr, sync::Arc};

use bevy::{
    prelude::{
        App, Commands, Component, Entity, EventWriter, Local, Plugin, Query, Res, ResMut, Resource,
        With,
    },
    tasks::{IoTaskPool, Task},
};
use crossbeam_channel::{Receiver, Sender};
use tracing::info;

use super::{
    accept::{QuicConnector, QuicListener},
    connection::Connection,
    error::{Error, Result},
    mediator::{AnyPacketMediator, PacketSenderMap, PacketWithConnId},
    packet::{AcceptConnection, ClientPacket, EncodedPacket, Packet, ServerPacket},
    task::{accept::AcceptConnectionsTask, recv::ReceivePacketsTask, send::SendPacketsTask},
};
use crate::{
    ambit::packet::{DespawnEntity, QueryEntity, SpawnEntity},
    channel::BroadcastChannel,
    chat::packet::SendMessage,
    id::{NetworkId, NetworkToWorld},
    path::{
        packet::{PathTarget, PathTargetRequest},
        plugin::{Path, Position},
    },
    stat::MovementSpeed,
};

// plugins
pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(spawn_accept_task);
        app.add_event::<NewConnection>();
        app.init_resource::<Quit>();

        #[cfg(feature = "server")]
        {
            app.init_resource::<Disconnections<Server>>();
            app.init_resource::<NetworkToWorld<Server>>();
            app.add_system(spawn_new_client_connections);
            app.add_system(despawn_disconnections::<Server>);
            app.add_system(raise_query_entity_events);

            app.init_resource::<PacketSenderMap<ClientPacket>>();

            app.add_packet::<PacketWithConnId<SendMessage>, ClientPacket>();
            app.add_packet::<PacketWithConnId<PathTargetRequest>, ClientPacket>();
            app.add_packet::<PacketWithConnId<QueryEntity>, ClientPacket>();
            app.add_event::<EntityQuery>();

            let packet_map = app
                .world
                .remove_resource::<PacketSenderMap<ClientPacket>>()
                .unwrap();

            app.insert_resource(AnyPacketMediator::new(Arc::new(packet_map)));
        }

        #[cfg(feature = "client")]
        {
            app.init_resource::<Disconnections<Client>>();
            app.init_resource::<NetworkToWorld<Client>>();
            app.add_system(spawn_server);
            app.add_system(despawn_disconnections::<Client>);
            app.add_system(connect_to_server);
            app.add_system(spawn_self);
            app.init_resource::<PacketSenderMap<ServerPacket>>();

            app.add_packet::<SpawnEntity, ServerPacket>();
            app.add_packet::<DespawnEntity, ServerPacket>();
            app.add_packet::<PathTarget, ServerPacket>();
            app.add_packet::<AcceptConnection, ServerPacket>();

            let packet_map = app
                .world
                .remove_resource::<PacketSenderMap<ServerPacket>>()
                .unwrap();

            app.insert_resource(AnyPacketMediator::new(Arc::new(packet_map)));
        }
    }
}

pub trait AddPacketAppExt {
    fn add_packet<T, P>(&mut self)
    where
        T: Send + Sync + 'static,
        P: Packet,
        P::Sender: TryFrom<Sender<T>>,
        P::Kind: for<'a> From<&'a P::Sender>;
}

impl AddPacketAppExt for App {
    fn add_packet<T, P>(&mut self)
    where
        T: Send + Sync + 'static,
        P: Packet,
        P::Sender: TryFrom<Sender<T>>,
        P::Kind: for<'a> From<&'a P::Sender>,
    {
        let mut packet_map = self.world.get_resource_mut::<PacketSenderMap<P>>().unwrap();
        let (tx, rx) = crossbeam_channel::unbounded::<T>();
        packet_map.add(tx);
        self.insert_resource(Packets::new(rx));
    }
}

// resources
#[derive(Resource)]
pub struct Packets<P> {
    pub receiver: Receiver<P>,
}

impl<P> Packets<P> {
    fn new(receiver: Receiver<P>) -> Self {
        Self { receiver }
    }
}

#[derive(Resource)]
pub struct Quit {
    sender: async_std::channel::Sender<()>,
    receiver: async_std::channel::Receiver<()>,
}
impl Default for Quit {
    fn default() -> Self {
        let (sender, receiver) = async_std::channel::bounded(1);
        Self { receiver, sender }
    }
}

#[derive(Resource)]
pub struct ConnectionReceiver<S>
where
    S: Send + Sync + 'static,
{
    receiver: Receiver<Connection<quinn::Connection>>,
    marker: PhantomData<S>,
}

impl<S> ConnectionReceiver<S>
where
    S: Send + Sync + 'static,
{
    fn new(receiver: Receiver<Connection<quinn::Connection>>) -> Self {
        Self {
            receiver,
            marker: PhantomData::default(),
        }
    }
}

#[derive(Resource)]
pub struct Disconnections<S>
where
    S: Send + Sync + 'static,
{
    receiver: Receiver<NetworkId>,
    sender: Sender<NetworkId>,
    marker: PhantomData<S>,
}

impl<S> Default for Disconnections<S>
where
    S: Send + Sync + 'static,
{
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            receiver,
            sender,
            marker: PhantomData::default(),
        }
    }
}

#[derive(Resource)]
pub struct ConnectionRequester(async_std::channel::Sender<SocketAddr>);

#[derive(Resource)]
pub struct AcceptTask<S>
where
    S: Send + Sync + 'static,
{
    task: Task<()>,
    marker: PhantomData<S>,
}

impl<S> AcceptTask<S>
where
    S: Send + Sync + 'static,
{
    fn new(task: Task<()>) -> Self {
        Self {
            task,
            marker: PhantomData::default(),
        }
    }
}

// components
#[derive(Component)]
pub struct Network<S>
where
    S: Send + Sync + 'static,
{
    sender: async_std::channel::Sender<EncodedPacket>,
    marker: PhantomData<S>,
}

impl<S> Network<S>
where
    S: Send + Sync + 'static,
{
    fn new(sender: async_std::channel::Sender<EncodedPacket>) -> Self {
        Self {
            sender,
            marker: PhantomData::default(),
        }
    }

    pub fn send<T>(&self, packet: T) -> Result<()>
    where
        S: Service,
        S::Packet: From<T>,
    {
        let encoded_packet = EncodedPacket::try_encode::<T, S::Packet>(packet)?;

        self.sender
            .send_blocking(encoded_packet)
            .map_err(|_| Error::Generic("Sender unexpectedly closed".to_owned()))?;

        Ok(())
    }

    pub fn send_all<'a, T, I>(iter: I, packet: T) -> Result<()>
    where
        S: Service,
        S::Packet: From<T>,
        I: Iterator<Item = &'a Self>,
    {
        let encoded_packet = EncodedPacket::try_encode::<T, S::Packet>(packet)?;

        for network in iter {
            let _ = network
                .sender
                .send_blocking(encoded_packet.clone())
                .map_err(|_| Error::Generic("Sender unexpectedly closed".to_owned()));
        }

        Ok(())
    }
}

// events
pub struct NewConnection {
    id: NetworkId,
}

pub struct EntityQuery {
    pub entity: Entity,
    pub querier: Entity,
}

// util
#[derive(Component, Default)]
pub struct Server;

impl Service for Server {
    type Other = Client;
    type Packet = ClientPacket;
}

#[derive(Component, Default)]
pub struct Client;

impl Service for Client {
    type Other = Server;
    type Packet = ServerPacket;
}

pub trait Service: Send + Sync + 'static + Component {
    type Packet: Packet;
    type Other: Service;
}

// systems
fn spawn_accept_task(mut commands: Commands, quit: Res<Quit>) {
    let io_pool = IoTaskPool::get();

    #[cfg(feature = "client")]
    {
        let (connect_to_tx, connect_to_rx) = async_std::channel::bounded(1);
        commands.insert_resource(ConnectionRequester(connect_to_tx));

        let (new_connections_tx, new_connections_rx) = crossbeam_channel::unbounded();
        commands.insert_resource(ConnectionReceiver::<Client>::new(new_connections_rx));
        let stop = quit.receiver.clone();

        let task = io_pool.spawn(async move {
            let connector = QuicConnector::new(connect_to_rx);
            AcceptConnectionsTask::new(connector, new_connections_tx)
                ._run(stop)
                .await;
        });
        commands.insert_resource(AcceptTask::<Client>::new(task));
    }

    #[cfg(feature = "server")]
    {
        let (new_connections_tx, new_connections_rx) = crossbeam_channel::unbounded();
        commands.insert_resource(ConnectionReceiver::<Server>::new(new_connections_rx));
        let stop = quit.receiver.clone();

        let task = io_pool.spawn(async move {
            let listener = QuicListener::new("127.0.0.1:56565".parse().unwrap());
            AcceptConnectionsTask::new(listener, new_connections_tx)
                ._run(stop)
                .await;
        });

        commands.insert_resource(AcceptTask::<Server>::new(task));
    }
}

fn connect_to_server(
    connection_requester: Res<ConnectionRequester>,
    query: Query<&Network<Server>>,
    mut requested: Local<bool>,
) {
    if query.is_empty() {
        if *requested {
            return;
        }

        let _ = connection_requester
            .0
            .send_blocking("127.0.0.1:56565".parse().unwrap());
        *requested = true;
    } else {
        *requested = false;
    }
}

fn spawn_server(
    mut commands: Commands,
    conn_receiver: Res<ConnectionReceiver<Client>>,
    disconnections: Res<Disconnections<Client>>,
    packet_mediator: Res<AnyPacketMediator<ServerPacket>>,
    server: Query<Entity, With<Network<Server>>>,
    quit: Res<Quit>,
) {
    if conn_receiver.receiver.is_empty() {
        return;
    }

    let pool = IoTaskPool::get();

    for connection in conn_receiver.receiver.try_iter() {
        info!("{:?}", *packet_mediator);
        let sender =
            spawn_connection_tasks(&disconnections, pool, &packet_mediator, &quit, connection);

        commands.spawn(Network::<Server>::new(sender));

        if let Ok(entity) = server.get_single() {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_new_client_connections(
    mut commands: Commands,
    mut network_to_world: ResMut<NetworkToWorld<Server>>,
    mut new_connections: EventWriter<NewConnection>,
    conn_receiver: Res<ConnectionReceiver<Server>>,
    disconnections: Res<Disconnections<Server>>,
    packet_mediator: Res<AnyPacketMediator<ClientPacket>>,
    quit: Res<Quit>,
) {
    if conn_receiver.receiver.is_empty() {
        return;
    }

    let pool = IoTaskPool::get();

    for connection in conn_receiver.receiver.try_iter() {
        let conn_id = connection.connection_id();

        let sender =
            spawn_connection_tasks(&disconnections, pool, &packet_mediator, &quit, connection);
        let network = Network::<Client>::new(sender);

        let _ = network.send(AcceptConnection {
            connection_id: conn_id,
        });

        let entity = commands
            .spawn((
                conn_id,
                network,
                Position { x: 0, y: 0 },
                MovementSpeed(1),
                Path::default(),
            ))
            .id();

        info!("creating network entity: {}", conn_id);
        network_to_world.insert(conn_id, entity);
        new_connections.send(NewConnection { id: conn_id });
    }
}

fn spawn_connection_tasks<'d, S>(
    disconnections: &Disconnections<S>,
    pool: &IoTaskPool,
    packet_mediator: &AnyPacketMediator<<S as Service>::Packet>,
    quit: &Quit,
    connection: Connection<quinn::Connection>,
) -> async_std::channel::Sender<EncodedPacket>
where
    S: Send + Sync + 'static + Service,
    <S::Packet as Packet>::Kind: for<'r> From<&'r S::Packet>,
    S::Packet: speedy::Readable<'d, speedy::LittleEndian>,
{
    let conn_id = connection.connection_id();
    let broadcast_disconnect = BroadcastChannel::channel();
    let (sender, receiver) = async_std::channel::unbounded();
    let disconnect = broadcast_disconnect.notified.clone();
    let disc_sender = disconnections.sender.clone();
    pool.spawn(async move {
        let _ = disconnect.recv().await;
        info!("{} disconnected", conn_id);
        let _ = disc_sender.send(conn_id);
    })
    .detach();
    let mediator = packet_mediator.clone();
    let receive_task = broadcast_disconnect.clone();
    let stop = quit.receiver.clone();
    let conn = connection.value.clone();
    pool.spawn(async move {
        let reader = conn.accept_uni().await.unwrap();
        ReceivePacketsTask::new(reader, mediator, conn_id)
            ._run(stop, receive_task)
            .await;
    })
    .detach();
    let stop = quit.receiver.clone();
    pool.spawn(async move {
        let writer = connection.value.open_uni().await.unwrap();
        SendPacketsTask::new(writer, receiver, conn_id)
            ._run::<<S::Packet as Packet>::OtherPacket>(stop, broadcast_disconnect)
            .await;
    })
    .detach();
    sender
}

fn spawn_self(
    mut commands: Commands,
    mut network_to_world: ResMut<NetworkToWorld<Client>>,
    accept_connections: Res<Packets<AcceptConnection>>,
) {
    for conn in accept_connections.receiver.try_iter() {
        let entity = commands
            .spawn((
                conn.connection_id,
                Position { x: 0, y: 0 },
                MovementSpeed(1),
                Path::default(),
            ))
            .id();

        info!("creating network entity: {}", conn.connection_id);
        network_to_world.insert(conn.connection_id, entity);
    }
}

fn despawn_disconnections<S>(
    mut commands: Commands,
    disconnections: Res<Disconnections<S>>,
    mut network_to_world: ResMut<NetworkToWorld<S>>,
) where
    S: Send + Sync + 'static,
{
    for disconnection in disconnections.receiver.try_iter() {
        let Some(entity) = network_to_world.remove(&disconnection) else {
            continue;
        };

        commands.entity(entity).despawn();
    }
}

fn raise_query_entity_events(
    packets: Res<Packets<PacketWithConnId<QueryEntity>>>,
    network_to_world: ResMut<NetworkToWorld<Server>>,
    mut events: EventWriter<EntityQuery>,
) {
    events.send_batch(packets.receiver.try_iter().filter_map(|packet| {
        return network_to_world
            .get(&packet.connection_id)
            .and_then(|entity| {
                network_to_world
                    .get(&packet.packet.id)
                    .map(|querier| (entity, querier))
            })
            .map(|(entity, querier)| EntityQuery {
                entity: *entity,
                querier: *querier,
            });
    }));
}
