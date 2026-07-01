use crate::{
    LiveWorkerCommand, LiveWorkerEvent, LiveWorkerLaunchSpec, parse_worker_event_line,
    worker_command_line,
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
};
use storage::{Db, SystemLogCommand};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::Mutex,
    time::{Duration, Instant, sleep},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessSupervisorOptions {
    pub trader_exe: PathBuf,
    pub launch_root: PathBuf,
    pub handshake_timeout_ms: u64,
    pub graceful_shutdown_timeout_ms: u64,
    pub heartbeat_stale_after_ms: u64,
    pub health_response_timeout_ms: u64,
    pub stderr_line_limit: usize,
    pub extra_args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
    pub worker_args_override: Option<Vec<String>>,
}

impl Default for LiveProcessSupervisorOptions {
    fn default() -> Self {
        Self {
            trader_exe: default_trader_exe(),
            launch_root: PathBuf::from("data/live-process"),
            handshake_timeout_ms: 5_000,
            graceful_shutdown_timeout_ms: 5_000,
            heartbeat_stale_after_ms: 5_000,
            health_response_timeout_ms: 1_000,
            stderr_line_limit: 256,
            extra_args: Vec::new(),
            extra_env: Vec::new(),
            worker_args_override: None,
        }
    }
}

fn default_trader_exe() -> PathBuf {
    let file_name = if cfg!(windows) {
        "trader.exe"
    } else {
        "trader"
    };
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("trader"));
    if path.parent().and_then(|parent| parent.file_name()) == Some(std::ffi::OsStr::new("deps"))
        && let Some(target_profile_dir) = path.parent().and_then(|parent| parent.parent())
    {
        return target_profile_dir.join(file_name);
    }
    path.set_file_name(file_name);
    path
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveProcessStatus {
    Starting,
    Running,
    StopRequested,
    Exited,
    Failed,
}

impl LiveProcessStatus {
    fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::StopRequested)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveProcessSnapshot {
    pub run_id: String,
    pub pid: Option<u32>,
    pub status: LiveProcessStatus,
    pub started_at_ms: i64,
    pub last_state_change_at_ms: i64,
    pub last_heartbeat_at_ms: Option<i64>,
    pub ipc_status: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveProcessError {
    AlreadyRunning,
    LaunchFailed(String),
    HandshakeTimeout,
}

impl std::fmt::Display for LiveProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => write!(formatter, "live process already running"),
            Self::LaunchFailed(error) => write!(formatter, "live process launch failed: {error}"),
            Self::HandshakeTimeout => write!(formatter, "live process handshake timeout"),
        }
    }
}

impl std::error::Error for LiveProcessError {}

#[derive(Clone)]
pub struct LiveProcessSupervisor {
    db: Db,
    options: LiveProcessSupervisorOptions,
    inner: Arc<Mutex<HashMap<String, LiveChildHandle>>>,
}

struct LiveChildHandle {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    snapshot: LiveProcessSnapshot,
    stderr_lines: VecDeque<String>,
    last_health_at_ms: Option<i64>,
}

impl LiveProcessSupervisor {
    pub fn new(db: Db) -> Self {
        Self::with_options(db, LiveProcessSupervisorOptions::default())
    }

    pub fn with_options(db: Db, options: LiveProcessSupervisorOptions) -> Self {
        Self {
            db,
            options,
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(
        &self,
        run_id: String,
        launch: LiveWorkerLaunchSpec,
    ) -> Result<(), LiveProcessError> {
        {
            let runs = self.inner.lock().await;
            if runs
                .get(&run_id)
                .is_some_and(|handle| handle.snapshot.status.is_active())
            {
                return Err(LiveProcessError::AlreadyRunning);
            }
        }

        let launch_file = self.options.launch_root.join(&run_id).join("launch.json");
        if let Some(parent) = launch_file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| LiveProcessError::LaunchFailed(error.to_string()))?;
        }
        let launch_bytes = serde_json::to_vec_pretty(&launch)
            .map_err(|error| LiveProcessError::LaunchFailed(error.to_string()))?;
        tokio::fs::write(&launch_file, launch_bytes)
            .await
            .map_err(|error| LiveProcessError::LaunchFailed(error.to_string()))?;

        let mut command = Command::new(&self.options.trader_exe);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let mut worker_args = self
            .options
            .worker_args_override
            .clone()
            .unwrap_or_else(|| {
                vec![
                    "live-worker".to_string(),
                    "--launch-file".to_string(),
                    launch_file.to_string_lossy().into_owned(),
                ]
            });
        worker_args.extend(self.options.extra_args.clone());
        command.args(worker_args);
        for (key, value) in &self.options.extra_env {
            command.env(key, value);
        }

        let mut child = command
            .spawn()
            .map_err(|error| LiveProcessError::LaunchFailed(error.to_string()))?;
        let pid = child.id();
        let stdout = child.stdout.take().ok_or_else(|| {
            LiveProcessError::LaunchFailed("child stdout unavailable".to_string())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            LiveProcessError::LaunchFailed("child stderr unavailable".to_string())
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LiveProcessError::LaunchFailed("child stdin unavailable".to_string()))?;
        let child = Arc::new(Mutex::new(child));
        let now = chrono::Utc::now().timestamp_millis();

        {
            let mut runs = self.inner.lock().await;
            runs.insert(
                run_id.clone(),
                LiveChildHandle {
                    child: child.clone(),
                    stdin: Arc::new(Mutex::new(stdin)),
                    snapshot: LiveProcessSnapshot {
                        run_id: run_id.clone(),
                        pid,
                        status: LiveProcessStatus::Starting,
                        started_at_ms: now,
                        last_state_change_at_ms: now,
                        last_heartbeat_at_ms: None,
                        ipc_status: None,
                        exit_code: None,
                    },
                    stderr_lines: VecDeque::new(),
                    last_health_at_ms: None,
                },
            );
        }

        self.spawn_stdout_reader(run_id.clone(), stdout);
        self.spawn_stderr_reader(run_id.clone(), stderr);
        self.spawn_exit_watcher(run_id.clone(), child);

        let deadline = Instant::now() + Duration::from_millis(self.options.handshake_timeout_ms);
        loop {
            if let Some(snapshot) = self.snapshot(&run_id).await {
                if snapshot.status == LiveProcessStatus::Running {
                    return Ok(());
                }
                if snapshot.status == LiveProcessStatus::Failed {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                self.kill_child(&run_id).await;
                self.mark_failed(&run_id, "handshake timeout", None).await;
                return Err(LiveProcessError::HandshakeTimeout);
            }
            sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn stop(&self, run_id: &str) -> bool {
        let Some(stdin) = self.request_stop(run_id).await else {
            return false;
        };
        let command = LiveWorkerCommand::Shutdown {
            request_id: format!("stop-{run_id}"),
            reason: "api_stop".to_string(),
        };
        if let Ok(line) = worker_command_line(&command) {
            let mut stdin = stdin.lock().await;
            let _ = stdin.write_all(line.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            let _ = stdin.flush().await;
        }

        let deadline =
            Instant::now() + Duration::from_millis(self.options.graceful_shutdown_timeout_ms);
        loop {
            if let Some(snapshot) = self.snapshot(run_id).await {
                if !snapshot.status.is_active() {
                    return true;
                }
            } else {
                return true;
            }
            if Instant::now() >= deadline {
                self.kill_child(run_id).await;
                self.mark_failed(run_id, "graceful shutdown timeout", None)
                    .await;
                return true;
            }
            sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn snapshot(&self, run_id: &str) -> Option<LiveProcessSnapshot> {
        self.inner
            .lock()
            .await
            .get(run_id)
            .map(|handle| handle.snapshot.clone())
    }

    pub async fn is_active(&self, run_id: &str) -> bool {
        self.snapshot(run_id)
            .await
            .is_some_and(|snapshot| snapshot.status.is_active())
    }

    pub async fn check_heartbeats(&self) -> usize {
        let now = chrono::Utc::now().timestamp_millis();
        let stale_after = self.options.heartbeat_stale_after_ms as i64;
        let stale_runs = {
            let runs = self.inner.lock().await;
            runs.iter()
                .filter(|(_, handle)| {
                    handle.snapshot.status.is_active()
                        && handle
                            .snapshot
                            .last_heartbeat_at_ms
                            .is_none_or(|last| now.saturating_sub(last) > stale_after)
                })
                .map(|(run_id, handle)| {
                    (
                        run_id.clone(),
                        handle.stdin.clone(),
                        handle.last_health_at_ms,
                    )
                })
                .collect::<Vec<_>>()
        };

        let mut killed = 0usize;
        for (run_id, stdin, previous_health_at_ms) in stale_runs {
            let request_id = format!("health-{run_id}-{now}");
            let command = LiveWorkerCommand::HealthCheck { request_id };
            if let Ok(line) = worker_command_line(&command) {
                let mut stdin = stdin.lock().await;
                let _ = stdin.write_all(line.as_bytes()).await;
                let _ = stdin.write_all(b"\n").await;
                let _ = stdin.flush().await;
            }
            sleep(Duration::from_millis(
                self.options.health_response_timeout_ms,
            ))
            .await;
            let answered = {
                let runs = self.inner.lock().await;
                runs.get(&run_id)
                    .and_then(|handle| handle.last_health_at_ms)
                    .is_some_and(|last| Some(last) != previous_health_at_ms)
            };
            if !answered {
                self.kill_child(&run_id).await;
                self.mark_failed(&run_id, "heartbeat stale", None).await;
                killed += 1;
            }
        }
        killed
    }

    async fn request_stop(&self, run_id: &str) -> Option<Arc<Mutex<ChildStdin>>> {
        let mut runs = self.inner.lock().await;
        let handle = runs.get_mut(run_id)?;
        if !handle.snapshot.status.is_active() {
            return None;
        }
        handle.snapshot.status = LiveProcessStatus::StopRequested;
        handle.snapshot.last_state_change_at_ms = chrono::Utc::now().timestamp_millis();
        Some(handle.stdin.clone())
    }

    fn spawn_stdout_reader(&self, run_id: String, stdout: tokio::process::ChildStdout) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(event) = parse_worker_event_line(&line) {
                    supervisor.apply_worker_event(&run_id, event).await;
                }
            }
        });
    }

    fn spawn_stderr_reader(&self, run_id: String, stderr: tokio::process::ChildStderr) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                supervisor.push_stderr_line(&run_id, line).await;
            }
        });
    }

    fn spawn_exit_watcher(&self, run_id: String, child: Arc<Mutex<Child>>) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            loop {
                let exit_status = {
                    let mut child = child.lock().await;
                    child.try_wait()
                };
                match exit_status {
                    Ok(Some(status)) => {
                        supervisor.handle_exit(&run_id, status.code()).await;
                        return;
                    }
                    Ok(None) => sleep(Duration::from_millis(10)).await,
                    Err(error) => {
                        supervisor
                            .mark_failed(
                                &run_id,
                                &format!("failed to wait for child: {error}"),
                                None,
                            )
                            .await;
                        return;
                    }
                }
            }
        });
    }

    async fn apply_worker_event(&self, run_id: &str, event: LiveWorkerEvent) {
        let mut runs = self.inner.lock().await;
        let Some(handle) = runs.get_mut(run_id) else {
            return;
        };
        let now = chrono::Utc::now().timestamp_millis();
        match event {
            LiveWorkerEvent::WorkerStarted { pid, .. } => {
                handle.snapshot.pid = Some(pid);
                handle.snapshot.last_state_change_at_ms = now;
            }
            LiveWorkerEvent::RuntimeStarted { .. } => {
                handle.snapshot.status = LiveProcessStatus::Running;
                handle.snapshot.ipc_status = Some("running".to_string());
                handle.snapshot.last_state_change_at_ms = now;
            }
            LiveWorkerEvent::Heartbeat { status, ts_ms, .. } => {
                handle.snapshot.last_heartbeat_at_ms = Some(ts_ms);
                handle.snapshot.ipc_status = Some(status);
            }
            LiveWorkerEvent::Health { status, .. } => {
                handle.snapshot.ipc_status = Some(status);
                handle.last_health_at_ms = Some(now);
            }
            LiveWorkerEvent::RuntimeStopping { .. } => {
                handle.snapshot.status = LiveProcessStatus::StopRequested;
                handle.snapshot.ipc_status = Some("stopping".to_string());
                handle.snapshot.last_state_change_at_ms = now;
            }
            LiveWorkerEvent::RuntimeStopped { status, .. } => {
                handle.snapshot.status = LiveProcessStatus::Exited;
                handle.snapshot.ipc_status = Some(status);
                handle.snapshot.last_state_change_at_ms = now;
            }
            LiveWorkerEvent::RuntimeFailed { error, .. } => {
                handle.snapshot.status = LiveProcessStatus::Failed;
                handle.snapshot.ipc_status = Some("failed".to_string());
                handle.snapshot.last_state_change_at_ms = now;
                handle.stderr_lines.push_back(error);
                while handle.stderr_lines.len() > self.options.stderr_line_limit {
                    handle.stderr_lines.pop_front();
                }
            }
        }
    }

    async fn push_stderr_line(&self, run_id: &str, line: String) {
        let mut runs = self.inner.lock().await;
        let Some(handle) = runs.get_mut(run_id) else {
            return;
        };
        handle.stderr_lines.push_back(line);
        while handle.stderr_lines.len() > self.options.stderr_line_limit {
            handle.stderr_lines.pop_front();
        }
    }

    async fn handle_exit(&self, run_id: &str, exit_code: Option<i32>) {
        let (should_fail, stderr) = {
            let mut runs = self.inner.lock().await;
            let Some(handle) = runs.get_mut(run_id) else {
                return;
            };
            handle.snapshot.exit_code = exit_code;
            let should_fail = handle.snapshot.status.is_active();
            if !should_fail && handle.snapshot.status != LiveProcessStatus::Failed {
                handle.snapshot.status = LiveProcessStatus::Exited;
            }
            handle.snapshot.last_state_change_at_ms = chrono::Utc::now().timestamp_millis();
            (
                should_fail,
                handle
                    .stderr_lines
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };
        if should_fail {
            self.mark_failed(run_id, "live worker exited unexpectedly", exit_code)
                .await;
        } else if exit_code.is_some_and(|code| code != 0) {
            self.record_process_log(
                run_id,
                "WARN",
                "live worker exited after terminal runtime state",
                exit_code,
                Some(stderr),
            )
            .await;
        }
    }

    async fn kill_child(&self, run_id: &str) {
        let child = {
            let runs = self.inner.lock().await;
            runs.get(run_id).map(|handle| handle.child.clone())
        };
        if let Some(child) = child {
            let mut child = child.lock().await;
            let _ = child.start_kill();
        }
    }

    async fn mark_failed(&self, run_id: &str, reason: &str, exit_code: Option<i32>) {
        {
            let mut runs = self.inner.lock().await;
            if let Some(handle) = runs.get_mut(run_id) {
                handle.snapshot.status = LiveProcessStatus::Failed;
                handle.snapshot.last_state_change_at_ms = chrono::Utc::now().timestamp_millis();
                if exit_code.is_some() {
                    handle.snapshot.exit_code = exit_code;
                }
            }
        }

        let terminal = match self.db.get_strategy_run(run_id).await {
            Ok(Some(run)) => is_terminal_run_status(&run.status),
            Ok(None) => false,
            Err(_) => false,
        };
        if !terminal {
            let now = chrono::Utc::now().timestamp_millis();
            let _ = self
                .db
                .update_strategy_run_status(run_id, "failed", Some(now), Some(reason))
                .await;
        }
        self.record_process_log(run_id, "ERROR", reason, exit_code, None)
            .await;
    }

    async fn record_process_log(
        &self,
        run_id: &str,
        level: &str,
        message: &str,
        exit_code: Option<i32>,
        stderr: Option<String>,
    ) {
        let _ = self
            .db
            .record_system_log(SystemLogCommand {
                run_id: Some(run_id.to_string()),
                ts_ms: chrono::Utc::now().timestamp_millis(),
                level: level.to_string(),
                target: "runtime.live_process".to_string(),
                message: message.to_string(),
                fields: Some(serde_json::json!({
                    "run_id": run_id,
                    "exit_code": exit_code,
                    "stderr": stderr,
                })),
            })
            .await;
    }
}

fn is_terminal_run_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "stopped")
}
