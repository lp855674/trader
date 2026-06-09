use events::EventBus;
use replay::ReplayController;
use runtime::RuntimeManager;
use std::{collections::HashMap, sync::Arc};
use storage::Db;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config_path: String,
    pub event_bus: EventBus,
    pub runtime_manager: RuntimeManager,
    pub replay_controllers: Arc<Mutex<HashMap<String, Arc<Mutex<ReplayController>>>>>,
}

impl AppState {
    pub fn new(db: Db, config_path: String) -> Self {
        Self {
            db,
            config_path,
            event_bus: EventBus::new(1024),
            runtime_manager: RuntimeManager::default(),
            replay_controllers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
