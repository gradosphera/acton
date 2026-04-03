use anyhow::{Context, anyhow};
use dap::events::Event;
use dap::events::StoppedEventBody;
use dap::responses::SetBreakpointsResponse;
use dap::types::Breakpoint;
use dap::types::Scope;
use dap::types::Source;
use dap::types::SourceBreakpoint;
use dap::types::StackFrame;
use dap::types::Variable;
use dap_client::DapClient;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

const DEFAULT_THREAD_ID: i64 = 1;
const START_TIMEOUT: Duration = Duration::from_secs(60);
const EVENT_TIMEOUT: Duration = Duration::from_secs(30);
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_TAIL_LIMIT: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub(super) struct StartScriptDebugArgs {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub breakpoints: Vec<BreakpointSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StartTestDebugArgs {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub breakpoints: Vec<BreakpointSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StartRetraceDebugArgs {
    #[serde(rename = "tx_hash")]
    pub tx_hash: String,
    pub contract: String,
    #[serde(default)]
    pub net: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub breakpoints: Vec<BreakpointSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SessionRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct SessionThreadRequest {
    pub session_id: String,
    #[serde(default)]
    pub thread_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SetBreakpointsRequest {
    pub session_id: String,
    #[serde(default)]
    pub breakpoints: Vec<BreakpointSpec>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ScopesRequest {
    pub session_id: String,
    pub frame_id: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct VariablesRequest {
    pub session_id: String,
    pub variables_reference: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct BreakpointSpec {
    pub path: String,
    #[serde(default)]
    pub lines: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub(super) struct SessionStartResult {
    pub session_id: String,
    pub target: String,
    pub child_pid: u32,
    pub thread_id: i64,
    pub stop: Option<StopInfo>,
    pub session_ended: bool,
    pub top_frame: Option<StackFrameView>,
    pub breakpoints: Vec<BreakpointView>,
}

#[derive(Debug, Serialize)]
pub(super) struct ExecutionControlResult {
    pub session_id: String,
    pub thread_id: i64,
    pub stop: Option<StopInfo>,
    pub session_ended: bool,
    pub top_frame: Option<StackFrameView>,
}

#[derive(Debug, Serialize)]
pub(super) struct SetBreakpointsResult {
    pub session_id: String,
    pub breakpoints: Vec<BreakpointView>,
}

#[derive(Debug, Serialize)]
pub(super) struct StackTraceResult {
    pub session_id: String,
    pub thread_id: i64,
    pub stack_frames: Vec<StackFrameView>,
}

#[derive(Debug, Serialize)]
pub(super) struct ScopesResult {
    pub session_id: String,
    pub frame_id: i64,
    pub scopes: Vec<ScopeView>,
}

#[derive(Debug, Serialize)]
pub(super) struct VariablesResult {
    pub session_id: String,
    pub variables_reference: i64,
    pub variables: Vec<VariableView>,
}

#[derive(Debug, Serialize)]
pub(super) struct TerminateSessionResult {
    pub session_id: String,
    pub terminated_via_dap: bool,
    pub forced_kill: bool,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct StopInfo {
    pub reason: String,
    pub description: Option<String>,
    pub text: Option<String>,
    pub hit_breakpoint_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct StackFrameView {
    pub id: i64,
    pub name: String,
    pub source_name: Option<String>,
    pub source_path: Option<String>,
    pub line: i64,
    pub column: i64,
    pub end_line: Option<i64>,
    pub end_column: Option<i64>,
    pub is_subtle: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ScopeView {
    pub name: String,
    pub variables_reference: i64,
    pub expensive: bool,
    pub named_variables: Option<i64>,
    pub indexed_variables: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct VariableView {
    pub name: String,
    pub value: String,
    pub type_name: Option<String>,
    pub variables_reference: i64,
    pub evaluate_name: Option<String>,
    pub memory_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct BreakpointView {
    pub id: Option<i64>,
    pub verified: bool,
    pub source_path: Option<String>,
    pub line: Option<i64>,
    pub column: Option<i64>,
    pub message: Option<String>,
}

pub(super) struct DebugSessionManager {
    acton_exe: PathBuf,
    project_root: PathBuf,
    manifest_path: PathBuf,
    launch_target: LaunchTarget,
    next_session_id: u64,
    sessions: BTreeMap<String, DebugSession>,
}

#[derive(Clone, Copy)]
enum LaunchTarget {
    ProjectRoot,
    ManifestPath,
}

impl DebugSessionManager {
    pub(super) fn new(acton_exe: PathBuf, project_root: PathBuf, manifest_path: PathBuf) -> Self {
        let launch_target = if manifest_path.parent() == Some(project_root.as_path())
            && manifest_path
                .file_name()
                .is_some_and(|name| name == "Acton.toml")
        {
            LaunchTarget::ProjectRoot
        } else {
            LaunchTarget::ManifestPath
        };
        Self {
            acton_exe,
            project_root,
            manifest_path,
            launch_target,
            next_session_id: 1,
            sessions: BTreeMap::new(),
        }
    }

    pub(super) fn start_script_debug(
        &mut self,
        args: StartScriptDebugArgs,
    ) -> anyhow::Result<SessionStartResult> {
        let port = free_tcp_port()?;
        let mut command = self.base_command();
        command.arg("script");
        command.arg("--debug");
        command.arg("--debug-port");
        command.arg(port.to_string());
        command.arg(&args.path);
        if !args.args.is_empty() {
            command.arg("--");
            for arg in &args.args {
                command.arg(arg);
            }
        }

        self.start_session("script", port, command, args.breakpoints)
    }

    pub(super) fn start_test_debug(
        &mut self,
        args: StartTestDebugArgs,
    ) -> anyhow::Result<SessionStartResult> {
        let port = free_tcp_port()?;
        let mut command = self.base_command();
        command.arg("test");
        command.arg("--debug");
        command.arg("--debug-port");
        command.arg(port.to_string());

        if let Some(filter) = &args.filter {
            command.arg("--filter");
            command.arg(filter);
        }

        if let Some(path) = &args.path {
            command.arg(path);
        }

        self.start_session("test", port, command, args.breakpoints)
    }

    pub(super) fn start_retrace_debug(
        &mut self,
        args: StartRetraceDebugArgs,
    ) -> anyhow::Result<SessionStartResult> {
        let port = free_tcp_port()?;
        let mut command = self.base_command();
        command.arg("retrace");
        command.arg(&args.tx_hash);
        command.arg("--contract");
        command.arg(&args.contract);
        command.arg("--dap-port");
        command.arg(port.to_string());

        if let Some(net) = &args.net {
            command.arg("--net");
            command.arg(net);
        }

        if let Some(api_key) = &args.api_key {
            command.arg("--api-key");
            command.arg(api_key);
        }

        self.start_session("retrace", port, command, args.breakpoints)
    }

    pub(super) fn set_breakpoints(
        &mut self,
        args: SetBreakpointsRequest,
    ) -> anyhow::Result<SetBreakpointsResult> {
        let project_root = self.project_root.clone();
        let session = self.session_mut(&args.session_id)?;
        let response = session.set_breakpoints(&args.breakpoints, &project_root)?;
        Ok(SetBreakpointsResult {
            session_id: args.session_id,
            breakpoints: breakpoint_views(response.breakpoints),
        })
    }

    pub(super) fn continue_execution(
        &mut self,
        args: SessionThreadRequest,
    ) -> anyhow::Result<ExecutionControlResult> {
        let session = self.session_mut(&args.session_id)?;
        let thread_id = args.thread_id.unwrap_or(DEFAULT_THREAD_ID);
        session.ensure_operational()?;
        session.client.continue_execution(thread_id)?;
        session.wait_for_execution_result()
    }

    pub(super) fn step_over(
        &mut self,
        args: SessionThreadRequest,
    ) -> anyhow::Result<ExecutionControlResult> {
        let session = self.session_mut(&args.session_id)?;
        let thread_id = args.thread_id.unwrap_or(DEFAULT_THREAD_ID);
        session.ensure_operational()?;
        session.client.step_over(thread_id)?;
        session.wait_for_execution_result()
    }

    pub(super) fn step_into(
        &mut self,
        args: SessionThreadRequest,
    ) -> anyhow::Result<ExecutionControlResult> {
        let session = self.session_mut(&args.session_id)?;
        let thread_id = args.thread_id.unwrap_or(DEFAULT_THREAD_ID);
        session.ensure_operational()?;
        session.client.step_in(thread_id)?;
        session.wait_for_execution_result()
    }

    pub(super) fn step_out(
        &mut self,
        args: SessionThreadRequest,
    ) -> anyhow::Result<ExecutionControlResult> {
        let session = self.session_mut(&args.session_id)?;
        let thread_id = args.thread_id.unwrap_or(DEFAULT_THREAD_ID);
        session.ensure_operational()?;
        session.client.step_out(thread_id)?;
        session.wait_for_execution_result()
    }

    pub(super) fn stack_trace(
        &mut self,
        args: SessionThreadRequest,
    ) -> anyhow::Result<StackTraceResult> {
        let session = self.session_mut(&args.session_id)?;
        let thread_id = args.thread_id.unwrap_or(DEFAULT_THREAD_ID);
        session.ensure_operational()?;
        let response = session.client.stack_trace(thread_id)?;
        Ok(StackTraceResult {
            session_id: args.session_id,
            thread_id,
            stack_frames: stack_frame_views(response.stack_frames),
        })
    }

    pub(super) fn scopes(&mut self, args: ScopesRequest) -> anyhow::Result<ScopesResult> {
        let session = self.session_mut(&args.session_id)?;
        session.ensure_operational()?;
        let response = session.client.scopes(args.frame_id)?;
        Ok(ScopesResult {
            session_id: args.session_id,
            frame_id: args.frame_id,
            scopes: scope_views(response.scopes),
        })
    }

    pub(super) fn variables(&mut self, args: VariablesRequest) -> anyhow::Result<VariablesResult> {
        let session = self.session_mut(&args.session_id)?;
        session.ensure_operational()?;
        let response = session.client.variables(args.variables_reference)?;
        Ok(VariablesResult {
            session_id: args.session_id,
            variables_reference: args.variables_reference,
            variables: variable_views(response.variables),
        })
    }

    pub(super) fn terminate_session(
        &mut self,
        args: SessionRequest,
    ) -> anyhow::Result<TerminateSessionResult> {
        let Some(mut session) = self.sessions.remove(&args.session_id) else {
            anyhow::bail!("Unknown session {}", args.session_id);
        };

        let terminated_via_dap = if session.ended {
            false
        } else {
            match session.client.terminate() {
                Ok(()) => true,
                Err(err) if is_closed_transport_error(&err) => false,
                Err(err) => return Err(err).context("Failed to terminate DAP session"),
            }
        };

        let (forced_kill, status) = session.wait_for_child_exit(TERMINATE_TIMEOUT)?;
        let (stdout_tail, stderr_tail) = session.output_tails();

        Ok(TerminateSessionResult {
            session_id: args.session_id,
            terminated_via_dap,
            forced_kill,
            exit_code: status.code(),
            stdout_tail,
            stderr_tail,
        })
    }

    fn start_session(
        &mut self,
        target: &str,
        port: u16,
        command: Command,
        breakpoints: Vec<BreakpointSpec>,
    ) -> anyhow::Result<SessionStartResult> {
        let mut process = SpawnedProcess::spawn(command)?;
        let address = format!("127.0.0.1:{port}");
        let mut client = connect_with_retry(&address, &mut process, START_TIMEOUT)?;

        client.start()?;
        client.initialize()?;
        wait_for_initialized(&client, START_TIMEOUT)?;

        let applied_breakpoints = apply_breakpoints(&mut client, &self.project_root, &breakpoints)?;

        client.launch()?;
        client.configuration_done()?;

        let session_id = self.alloc_session_id();
        let mut session =
            DebugSession::new(session_id.clone(), target.to_string(), process, client);
        let child_pid = session.child_pid();
        let control = session.wait_for_execution_result()?;

        self.sessions.insert(session_id.clone(), session);

        Ok(SessionStartResult {
            session_id,
            target: target.to_string(),
            child_pid,
            thread_id: DEFAULT_THREAD_ID,
            stop: control.stop,
            session_ended: control.session_ended,
            top_frame: control.top_frame,
            breakpoints: applied_breakpoints,
        })
    }

    fn base_command(&self) -> Command {
        let mut command = Command::new(&self.acton_exe);
        command.current_dir(&self.project_root);
        command.arg("--color");
        command.arg("never");
        match self.launch_target {
            LaunchTarget::ProjectRoot => {
                command.arg("--project-root");
                command.arg(&self.project_root);
            }
            LaunchTarget::ManifestPath => {
                command.arg("--manifest-path");
                command.arg(&self.manifest_path);
            }
        }
        command.env("NO_COLOR", "1");
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command
    }

    fn alloc_session_id(&mut self) -> String {
        let id = self.next_session_id;
        self.next_session_id += 1;
        id.to_string()
    }

    fn session_mut(&mut self, session_id: &str) -> anyhow::Result<&mut DebugSession> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Unknown session {session_id}"))
    }
}

struct DebugSession {
    session_id: String,
    target: String,
    process: SpawnedProcess,
    client: DapClient,
    ended: bool,
    last_stop: Option<StopInfo>,
    last_exit_code: Option<i32>,
}

impl DebugSession {
    fn new(session_id: String, target: String, process: SpawnedProcess, client: DapClient) -> Self {
        Self {
            session_id,
            target,
            process,
            client,
            ended: false,
            last_stop: None,
            last_exit_code: None,
        }
    }

    fn child_pid(&self) -> u32 {
        self.process.child.id()
    }

    fn ensure_operational(&mut self) -> anyhow::Result<()> {
        if self.ended {
            anyhow::bail!(
                "Session {} ({}) has already ended",
                self.session_id,
                self.target
            );
        }

        if let Some(status) = self.process.child.try_wait()? {
            self.ended = true;
            let (stdout_tail, stderr_tail) = self.output_tails();
            anyhow::bail!(
                "Session {} ({}) exited with {:?}\nstdout:\n{}\nstderr:\n{}",
                self.session_id,
                self.target,
                status.code(),
                stdout_tail,
                stderr_tail
            );
        }

        Ok(())
    }

    fn wait_for_execution_result(&mut self) -> anyhow::Result<ExecutionControlResult> {
        self.ensure_operational()?;

        let outcome = wait_for_stop_event(&self.client, EVENT_TIMEOUT)?;
        match outcome {
            StopOutcome::Stopped(stop) => {
                self.last_stop = Some(stop.clone());
                let top_frame = self.top_frame()?;
                Ok(ExecutionControlResult {
                    session_id: self.session_id.clone(),
                    thread_id: DEFAULT_THREAD_ID,
                    stop: Some(stop),
                    session_ended: false,
                    top_frame,
                })
            }
            StopOutcome::Terminated(exit_code) => {
                self.ended = true;
                self.last_exit_code = exit_code;
                Ok(ExecutionControlResult {
                    session_id: self.session_id.clone(),
                    thread_id: DEFAULT_THREAD_ID,
                    stop: None,
                    session_ended: true,
                    top_frame: None,
                })
            }
        }
    }

    fn top_frame(&mut self) -> anyhow::Result<Option<StackFrameView>> {
        let response = self.client.stack_trace(DEFAULT_THREAD_ID)?;
        Ok(response
            .stack_frames
            .into_iter()
            .next()
            .map(StackFrameView::from))
    }

    fn set_breakpoints(
        &mut self,
        breakpoints: &[BreakpointSpec],
        project_root: &Path,
    ) -> anyhow::Result<SetBreakpointsResponse> {
        self.ensure_operational()?;
        let responses = apply_breakpoints(&mut self.client, project_root, breakpoints)?;
        Ok(SetBreakpointsResponse {
            breakpoints: responses
                .into_iter()
                .map(BreakpointView::into_breakpoint)
                .collect(),
        })
    }

    fn wait_for_child_exit(&mut self, timeout: Duration) -> anyhow::Result<(bool, ExitStatus)> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.process.child.try_wait()? {
                self.process.finish_output_threads();
                return Ok((false, status));
            }

            if Instant::now() >= deadline {
                self.process.child.kill().ok();
                let status = self.process.child.wait()?;
                self.process.finish_output_threads();
                return Ok((true, status));
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    fn output_tails(&self) -> (String, String) {
        (self.process.stdout_tail(), self.process.stderr_tail())
    }
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        if let Ok(None) = self.process.child.try_wait() {
            self.process.child.kill().ok();
            self.process.child.wait().ok();
        }
        self.process.finish_output_threads();
    }
}

struct SpawnedProcess {
    child: Child,
    stdout_tail: Arc<Mutex<String>>,
    stderr_tail: Arc<Mutex<String>>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

impl SpawnedProcess {
    fn spawn(mut command: Command) -> anyhow::Result<Self> {
        let mut child = command.spawn().with_context(|| {
            let program = command.get_program().to_string_lossy().into_owned();
            format!("Failed to spawn {program}")
        })?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture child stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture child stderr")?;

        let stdout_tail = Arc::new(Mutex::new(String::new()));
        let stderr_tail = Arc::new(Mutex::new(String::new()));

        let stdout_thread = Some(spawn_tail_reader(stdout, Arc::clone(&stdout_tail)));
        let stderr_thread = Some(spawn_tail_reader(stderr, Arc::clone(&stderr_tail)));

        Ok(Self {
            child,
            stdout_tail,
            stderr_tail,
            stdout_thread,
            stderr_thread,
        })
    }

    fn stdout_tail(&self) -> String {
        self.stdout_tail
            .lock()
            .expect("stdout tail mutex poisoned")
            .clone()
    }

    fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .expect("stderr tail mutex poisoned")
            .clone()
    }

    fn finish_output_threads(&mut self) {
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }
}

fn spawn_tail_reader<R: Read + Send + 'static>(
    mut reader: R,
    tail: Arc<Mutex<String>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_size) => {
                    let chunk = String::from_utf8_lossy(&buffer[..read_size]);
                    append_output_tail(&tail, chunk.as_ref());
                }
                Err(_) => break,
            }
        }
    })
}

fn append_output_tail(target: &Arc<Mutex<String>>, chunk: &str) {
    let mut text = target.lock().expect("output tail mutex poisoned");
    text.push_str(chunk);
    if text.len() > OUTPUT_TAIL_LIMIT {
        let drain_until = text.len() - OUTPUT_TAIL_LIMIT;
        text.drain(..drain_until);
    }
}

fn connect_with_retry(
    address: &str,
    process: &mut SpawnedProcess,
    timeout: Duration,
) -> anyhow::Result<DapClient> {
    let deadline = Instant::now() + timeout;

    loop {
        match DapClient::connect(address) {
            Ok(client) => return Ok(client),
            Err(err) => {
                if let Some(io_err) = err.downcast_ref::<std::io::Error>()
                    && io_err.kind() == std::io::ErrorKind::ConnectionRefused
                {
                    if let Some(status) = process.child.try_wait()? {
                        let stdout_tail = process.stdout_tail();
                        let stderr_tail = process.stderr_tail();
                        anyhow::bail!(
                            "Debug target exited before DAP became available: {:?}\nstdout:\n{}\nstderr:\n{}",
                            status.code(),
                            stdout_tail,
                            stderr_tail
                        );
                    }

                    if Instant::now() >= deadline {
                        let stdout_tail = process.stdout_tail();
                        let stderr_tail = process.stderr_tail();
                        return Err(anyhow!(
                            "Timed out waiting for DAP server at {address}\nstdout:\n{stdout_tail}\nstderr:\n{stderr_tail}"
                        ));
                    }

                    thread::sleep(Duration::from_millis(100));
                    continue;
                }

                return Err(err)
                    .with_context(|| format!("Failed to connect to DAP server at {address}"));
            }
        }
    }
}

fn wait_for_initialized(client: &DapClient, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("Timed out waiting for debugger initialization");
        }

        if let Some(event) = client.try_receive_event(Duration::from_millis(100))? {
            if matches!(event, Event::Initialized) {
                return Ok(());
            }
        }
    }
}

enum StopOutcome {
    Stopped(StopInfo),
    Terminated(Option<i32>),
}

fn wait_for_stop_event(client: &DapClient, timeout: Duration) -> anyhow::Result<StopOutcome> {
    let deadline = Instant::now() + timeout;
    let mut exit_code = None;

    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("Timed out waiting for debugger stop event");
        }

        let Some(event) = client.try_receive_event(Duration::from_millis(100))? else {
            continue;
        };

        match event {
            Event::Stopped(body) => return Ok(StopOutcome::Stopped(StopInfo::from(body))),
            Event::Exited(body) => exit_code = Some(body.exit_code as i32),
            Event::Terminated(_) => return Ok(StopOutcome::Terminated(exit_code)),
            _ => {}
        }
    }
}

fn apply_breakpoints(
    client: &mut DapClient,
    project_root: &Path,
    breakpoints: &[BreakpointSpec],
) -> anyhow::Result<Vec<BreakpointView>> {
    let mut applied = Vec::new();

    for spec in breakpoints {
        let path = resolve_source_path(project_root, &spec.path);
        let source = Source {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string),
            path: Some(path.to_string_lossy().to_string()),
            ..Default::default()
        };

        let dap_breakpoints = spec
            .lines
            .iter()
            .map(|line| SourceBreakpoint {
                line: i64::from(*line),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        let response = client.set_breakpoints(source, dap_breakpoints)?;
        applied.extend(breakpoint_views(response.breakpoints));
    }

    Ok(applied)
}

fn resolve_source_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };

    dunce::canonicalize(&resolved).unwrap_or(resolved)
}

fn free_tcp_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn breakpoint_views(breakpoints: Vec<Breakpoint>) -> Vec<BreakpointView> {
    breakpoints.into_iter().map(BreakpointView::from).collect()
}

fn stack_frame_views(frames: Vec<StackFrame>) -> Vec<StackFrameView> {
    frames.into_iter().map(StackFrameView::from).collect()
}

fn scope_views(scopes: Vec<Scope>) -> Vec<ScopeView> {
    scopes.into_iter().map(ScopeView::from).collect()
}

fn variable_views(variables: Vec<Variable>) -> Vec<VariableView> {
    variables.into_iter().map(VariableView::from).collect()
}

fn is_closed_transport_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("Timeout waiting for response")
        || err.downcast_ref::<std::io::Error>().is_some_and(|io_err| {
            matches!(
                io_err.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::UnexpectedEof
            )
        })
}

impl From<StoppedEventBody> for StopInfo {
    fn from(body: StoppedEventBody) -> Self {
        Self {
            reason: serde_json::to_value(body.reason)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
                .unwrap_or_else(|| String::from("stopped")),
            description: body.description,
            text: body.text,
            hit_breakpoint_ids: body.hit_breakpoint_ids,
        }
    }
}

impl From<StackFrame> for StackFrameView {
    fn from(frame: StackFrame) -> Self {
        let source_name = frame.source.as_ref().and_then(|source| source.name.clone());
        let source_path = frame.source.as_ref().and_then(|source| source.path.clone());
        let is_subtle = matches!(
            frame.presentation_hint,
            Some(dap::types::StackFramePresentationhint::Subtle)
        );

        Self {
            id: frame.id,
            name: frame.name,
            source_name,
            source_path,
            line: frame.line,
            column: frame.column,
            end_line: frame.end_line,
            end_column: frame.end_column,
            is_subtle,
        }
    }
}

impl From<Scope> for ScopeView {
    fn from(scope: Scope) -> Self {
        Self {
            name: scope.name,
            variables_reference: scope.variables_reference,
            expensive: scope.expensive,
            named_variables: scope.named_variables,
            indexed_variables: scope.indexed_variables,
        }
    }
}

impl From<Variable> for VariableView {
    fn from(variable: Variable) -> Self {
        Self {
            name: variable.name,
            value: variable.value,
            type_name: variable.type_field,
            variables_reference: variable.variables_reference,
            evaluate_name: variable.evaluate_name,
            memory_reference: variable.memory_reference,
        }
    }
}

impl From<Breakpoint> for BreakpointView {
    fn from(breakpoint: Breakpoint) -> Self {
        Self {
            id: breakpoint.id,
            verified: breakpoint.verified,
            source_path: breakpoint.source.and_then(|source| source.path),
            line: breakpoint.line,
            column: breakpoint.column,
            message: breakpoint.message,
        }
    }
}

impl BreakpointView {
    fn into_breakpoint(self) -> Breakpoint {
        Breakpoint {
            id: self.id,
            verified: self.verified,
            source: self.source_path.map(|path| Source {
                path: Some(path),
                ..Default::default()
            }),
            line: self.line,
            column: self.column,
            message: self.message,
            ..Default::default()
        }
    }
}
