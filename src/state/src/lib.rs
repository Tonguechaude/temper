pub mod player_list;

use crate::player_list::PlayerList;
use bevy_ecs::prelude::Resource;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use temper_config::server_config::get_global_config;
use temper_performance::ServerPerformance;
use temper_threadpool::ThreadPool;
use temper_world::World;
use tempfile::TempDir;

pub struct ServerState {
    pub world: World,
    pub shut_down: AtomicBool,
    pub players: PlayerList, // (UUID, Username)
    pub thread_pool: ThreadPool,
    pub start_time: Instant,
    pub performance: Mutex<ServerPerformance>,
}

pub type GlobalState = Arc<ServerState>;

#[derive(Resource, Clone)]
pub struct GlobalStateResource(pub GlobalState);

/// Creates a minimal GlobalStateResource for testing with a temporary database
pub fn create_test_state() -> (GlobalStateResource, TempDir) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().to_path_buf();

    let server_state = ServerState {
        world: World::new(&db_path, 0),
        shut_down: false.into(),
        players: PlayerList::default(),
        thread_pool: ThreadPool::new(),
        start_time: Instant::now(),
        performance: ServerPerformance::new(20).into(),
    };

    let global_state = Arc::new(server_state);
    (GlobalStateResource(global_state), temp_dir)
}

/// Creates the initial server state with all required components.
pub fn create_state(start_time: Instant) -> ServerState {
    // Fixed seed for world generation. This seed ensures you spawn above land at the default spawn point.
    const SEED: u64 = 380;
    ServerState {
        world: World::new(&get_global_config().database.db_path, SEED),
        shut_down: false.into(),
        players: PlayerList::default(),
        thread_pool: ThreadPool::new(),
        start_time,
        performance: ServerPerformance::new(get_global_config().tps).into(),
    }
}
