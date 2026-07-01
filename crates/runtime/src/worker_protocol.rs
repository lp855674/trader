use crate::{RunSpec, StartupRecoveryUnmatchedOpenOrdersPolicy};
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveWorkerLaunchSpec {
    pub run_id: String,
    pub db_url: String,
    pub config_path: Option<String>,
    pub config_content: String,
    pub config_format: String,
    pub run_spec: Option<RunSpec>,
    pub broker_snapshot_interval_ms: Option<u64>,
    pub startup_recovery_unmatched_open_orders_policy: StartupRecoveryUnmatchedOpenOrdersPolicy,
}

impl LiveWorkerLaunchSpec {
    pub fn validate_no_embedded_secrets(&self) -> anyhow::Result<()> {
        reject_credentialed_db_url(&self.db_url)?;
        match self.config_format.as_str() {
            "TOML" => {
                let parsed: toml::Value = toml::from_str(&self.config_content)
                    .context("failed to parse launch config_content as TOML")?;
                reject_secret_like_toml_values(None, &parsed)
            }
            "JSON" => {
                let parsed: serde_json::Value = serde_json::from_str(&self.config_content)
                    .context("failed to parse launch config_content as JSON")?;
                reject_secret_like_json_values(None, &parsed)
            }
            other => bail!("unsupported launch config_format {other}"),
        }
    }
}

fn reject_secret_like_toml_values(path: Option<&str>, value: &toml::Value) -> anyhow::Result<()> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let path = path.map_or_else(|| key.to_string(), |prefix| format!("{prefix}.{key}"));
                let lower = key.to_ascii_lowercase();
                if is_secret_like_key(&lower) {
                    bail!("launch file contains secret-like key {path}");
                }
                reject_secret_like_toml_values(Some(&path), value)?;
            }
            Ok(())
        }
        toml::Value::Array(values) => {
            for value in values {
                reject_secret_like_toml_values(path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn reject_secret_like_json_values(
    path: Option<&str>,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let path = path.map_or_else(|| key.to_string(), |prefix| format!("{prefix}.{key}"));
                let lower = key.to_ascii_lowercase();
                if is_secret_like_key(&lower) {
                    bail!("launch file contains secret-like key {path}");
                }
                reject_secret_like_json_values(Some(&path), value)?;
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_secret_like_json_values(path, value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn is_secret_like_key(key: &str) -> bool {
    if key.ends_with("_env") {
        return false;
    }
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || matches!(key, "api_key" | "bearer_token" | "auth_token")
}

fn reject_credentialed_db_url(db_url: &str) -> anyhow::Result<()> {
    let Some((_, rest)) = db_url.split_once("://") else {
        return Ok(());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        bail!("launch file contains credentialed db_url");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveWorkerCommand {
    HealthCheck { request_id: String },
    Shutdown { request_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveWorkerEvent {
    WorkerStarted {
        run_id: String,
        pid: u32,
    },
    RuntimeStarted {
        run_id: String,
    },
    Heartbeat {
        run_id: String,
        status: String,
        ts_ms: i64,
    },
    Health {
        run_id: String,
        request_id: String,
        status: String,
    },
    RuntimeStopping {
        run_id: String,
        reason: String,
    },
    RuntimeStopped {
        run_id: String,
        status: String,
    },
    RuntimeFailed {
        run_id: String,
        error: String,
    },
}

pub fn parse_worker_command_line(line: &str) -> anyhow::Result<LiveWorkerCommand> {
    serde_json::from_str(line).context("failed to parse worker command JSONL")
}

pub fn parse_worker_event_line(line: &str) -> anyhow::Result<LiveWorkerEvent> {
    serde_json::from_str(line).context("failed to parse worker event JSONL")
}

pub fn worker_command_line(command: &LiveWorkerCommand) -> anyhow::Result<String> {
    serde_json::to_string(command).context("failed to serialize worker command")
}

pub fn worker_event_line(event: &LiveWorkerEvent) -> anyhow::Result<String> {
    serde_json::to_string(event).context("failed to serialize worker event")
}
