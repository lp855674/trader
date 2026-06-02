use storage::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config_path: String,
}

impl AppState {
    pub fn new(db: Db, config_path: String) -> Self {
        Self { db, config_path }
    }
}
