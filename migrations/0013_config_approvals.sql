CREATE TABLE IF NOT EXISTS config_approvals (
    id TEXT PRIMARY KEY,
    config_id TEXT NOT NULL,
    version TEXT NOT NULL,
    target_env TEXT,
    approved_by TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    actor_role TEXT NOT NULL,
    reason TEXT,
    FOREIGN KEY(config_id) REFERENCES configs(id),
    UNIQUE(config_id, version, approved_by)
);

CREATE INDEX IF NOT EXISTS idx_config_approvals_config_version
ON config_approvals(config_id, version, approved_at);
