use runtime::RuntimeManager;
use storage::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config_path: String,
    pub runtime_manager: RuntimeManager,
}

impl AppState {
    pub fn new(db: Db, config_path: String) -> Self {
        Self {
            db,
            config_path,
            runtime_manager: RuntimeManager::default(),
        }
    }
}
