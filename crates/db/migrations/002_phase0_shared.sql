-- Phase 0: Foundation Shared Schema
-- These tables are used across all subsystems as the central ledger

-- System metadata and configuration
CREATE TABLE IF NOT EXISTS system_config (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT,
    description TEXT,
    updated_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- Audit log for all system operations
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_type TEXT NOT NULL CHECK (
        operation_type IN (
            'INSERT', 'UPDATE', 'DELETE', 
            'CONFIG_CHANGE', 'SYSTEM_EVENT', 'ERROR'
        )
    ),
    target_table TEXT,
    target_id TEXT,
    user_id TEXT,
    old_value TEXT,
    new_value TEXT,
    ip_address TEXT,
    user_agent TEXT,
    timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
CREATE INDEX idx_audit_log_target ON audit_log(target_table, target_id);

-- Cross-system metrics and monitoring data
CREATE TABLE IF NOT EXISTS system_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    metric_name TEXT NOT NULL,
    metric_type TEXT NOT NULL CHECK (
        metric_type IN ('COUNTER', 'GAUGE', 'HISTOGRAM')
    ),
    value REAL NOT NULL,
    unit TEXT,
    timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    labels TEXT  -- JSON format labels
);

CREATE INDEX idx_system_metrics_timestamp ON system_metrics(timestamp);
CREATE INDEX idx_system_metrics_name ON system_metrics(metric_name);

-- Phase 0: Shared Schema End
