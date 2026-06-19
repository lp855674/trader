CREATE TABLE IF NOT EXISTS config_releases (
    id TEXT PRIMARY KEY,
    config_id TEXT NOT NULL,
    version TEXT NOT NULL,
    status TEXT NOT NULL,
    released_by TEXT,
    notes TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(config_id) REFERENCES configs(id),
    UNIQUE(config_id, version)
);

CREATE TABLE IF NOT EXISTS run_config_versions (
    run_id TEXT PRIMARY KEY,
    config_id TEXT NOT NULL,
    version TEXT NOT NULL,
    bound_at INTEGER NOT NULL,
    FOREIGN KEY(config_id, version) REFERENCES config_releases(config_id, version)
);

CREATE TABLE IF NOT EXISTS config_audits (
    id TEXT PRIMARY KEY,
    config_id TEXT NOT NULL,
    version TEXT,
    action TEXT NOT NULL,
    actor TEXT,
    reason TEXT,
    ts INTEGER NOT NULL,
    FOREIGN KEY(config_id) REFERENCES configs(id)
);
