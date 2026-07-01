use events::{EventBus, LogWriterMetrics};
use replay::ReplayController;
use runtime::{LiveProcessSupervisor, RuntimeManager};
use std::{collections::HashMap, sync::Arc};
use storage::Db;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub db_url: Option<String>,
    pub server_config: config::ServerConfig,
    pub event_bus: EventBus,
    pub log_writer_metrics: LogWriterMetrics,
    pub runtime_manager: RuntimeManager,
    pub live_process_supervisor: LiveProcessSupervisor,
    pub replay_controllers: Arc<Mutex<HashMap<String, Arc<Mutex<ReplayController>>>>>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        Self::with_server_config(db, config::ServerConfig::default())
    }

    pub fn with_default_run_config(db: Db, config_path: String) -> Self {
        Self::with_server_config(
            db,
            config::ServerConfig::with_default_run_config_path(config_path),
        )
    }

    pub fn with_server_config(db: Db, server_config: config::ServerConfig) -> Self {
        Self::with_server_config_and_db_url(db, server_config, None)
    }

    pub fn with_server_config_and_db_url(
        db: Db,
        server_config: config::ServerConfig,
        db_url: Option<String>,
    ) -> Self {
        let live_process_supervisor = LiveProcessSupervisor::new(db.clone());
        Self {
            db,
            db_url,
            server_config,
            event_bus: EventBus::new(1024),
            log_writer_metrics: LogWriterMetrics::default(),
            runtime_manager: RuntimeManager::default(),
            live_process_supervisor,
            replay_controllers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn default_run_config_path(&self) -> Option<&str> {
        self.server_config.default_run_config_path()
    }
}
