//! `acton mcp` — stdio MCP server exposing the Tolk test debugger to LLM clients.
//!
//! The server runs a real `acton test --debug` invocation in a background
//! thread, wired to an in-process [`DapTransport`]. MCP tools translate to
//! DAP requests on that transport and surface the replayer's state
//! (position, stack, locals, eval, breakpoints) back as structured JSON.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use acton_config::test::TestConfig;
use acton_debug::{DapMessage, DapTransport};
use anyhow::{Context, Result, anyhow};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use dap::events::Event;
use dap::prelude::{Command, Request, Response, ResponseBody};
use dap::requests::{
    ContinueArguments, DisconnectArguments, EvaluateArguments, InitializeArguments,
    LaunchRequestArguments, NextArguments, ScopesArguments, SetBreakpointsArguments,
    StackTraceArguments, StepInArguments, StepOutArguments, VariablesArguments,
};
use dap::types::{Source, SourceBreakpoint};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::test::test_cmd_with_transport;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run an `acton mcp` stdio MCP server debugging one test.
///
/// If `test_name` is `None`, the MCP server starts without a selected test;
/// the client must call the `select_test` tool to spawn the runner before
/// any stepping/query tool can be used.
pub fn mcp_cmd(test_name: Option<String>, path: Option<String>) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = (test_name, path);
        anyhow::bail!("`acton mcp` currently requires a Unix-like OS");
    }

    #[cfg(unix)]
    {
        // SAFETY: `hijack_stdout_for_mcp` performs a pair of libc fd
        // operations that are safe to invoke from single-threaded startup
        // code. See the function body for the per-op justification.
        #[allow(unsafe_code)]
        let mcp_writer = unsafe { hijack_stdout_for_mcp()? };
        run_mcp(test_name, path, mcp_writer)
    }
}

#[cfg(unix)]
#[allow(unsafe_code)]
unsafe fn hijack_stdout_for_mcp() -> Result<tokio::fs::File> {
    use std::os::unix::io::FromRawFd;

    // SAFETY: `libc::dup` is always safe to call; it just allocates a new fd
    // aliasing the one passed in. A negative return value indicates failure.
    let saved_fd = unsafe { libc::dup(1) };
    if saved_fd < 0 {
        return Err(anyhow!(
            "dup(stdout) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: `libc::dup2` is always safe to call; it atomically replaces the
    // destination fd with a copy of the source fd. We use it to redirect
    // process stdout to stderr so stray println! calls from the background
    // test runner do not corrupt the MCP stream on fd 1.
    let dup_rc = unsafe { libc::dup2(2, 1) };
    if dup_rc < 0 {
        return Err(anyhow!(
            "dup2(stderr -> stdout) failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    // SAFETY: `saved_fd` was produced by `libc::dup` above and is owned
    // exclusively here; constructing a `File` from it transfers that ownership.
    let std_file = unsafe { std::fs::File::from_raw_fd(saved_fd) };
    Ok(tokio::fs::File::from_std(std_file))
}

#[cfg(unix)]
fn run_mcp(
    test_name: Option<String>,
    path: Option<String>,
    mcp_writer: tokio::fs::File,
) -> Result<()> {
    let (transport, req_tx, dap_rx) = DapTransport::in_process();
    let session = Arc::new(SessionState::new(
        DapBridge::new(req_tx, dap_rx),
        transport,
        path,
    ));

    // Preselect the test immediately if the user passed it on the command
    // line, keeping the original `acton mcp --test foo` flow zero-surprise.
    if let Some(name) = test_name {
        session.start_runner(name)?;
    }

    let debugger = TolkDebugger::new(session.clone());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    rt.block_on(async move {
        let service = debugger
            .serve((tokio::io::stdin(), mcp_writer))
            .await
            .context("failed to start rmcp service")?;
        service.waiting().await.context("rmcp service error")?;
        Ok::<_, anyhow::Error>(())
    })?;

    // Best-effort disconnect so the runner thread unblocks and unwinds.
    if session.runner_started() {
        let _ = session.bridge.send_and_wait(
            Command::Disconnect(DisconnectArguments {
                restart: Some(false),
                terminate_debuggee: Some(true),
                suspend_debuggee: None,
            }),
            Duration::from_millis(200),
        );
    }

    session.join_runner()
}

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

const THREAD_ID: i64 = 1;
const DEFAULT_WAIT: Duration = Duration::from_secs(30);

/// Shared debugger session owned jointly by the MCP handler and the shutdown
/// path in `run_mcp`. Holds the DAP bridge plus the lazy-spawn state for the
/// background test runner.
struct SessionState {
    bridge: DapBridge,
    /// Test runner pieces that have not yet been consumed. `take()`n by
    /// `start_runner` the first time a test is selected.
    pending: Mutex<Option<PendingRunner>>,
    /// Join handle for the spawned runner thread.
    runner: Mutex<Option<JoinHandle<Result<()>>>>,
    /// Name of the selected test, once chosen.
    selected_test: Mutex<Option<String>>,
}

struct PendingRunner {
    transport: DapTransport,
    path: Option<String>,
}

impl SessionState {
    const fn new(bridge: DapBridge, transport: DapTransport, path: Option<String>) -> Self {
        Self {
            bridge,
            pending: Mutex::new(Some(PendingRunner { transport, path })),
            runner: Mutex::new(None),
            selected_test: Mutex::new(None),
        }
    }

    /// Returns true once `start_runner` has successfully spawned the thread.
    fn runner_started(&self) -> bool {
        self.runner
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Name of the selected test, if any.
    fn selected_test(&self) -> Option<String> {
        self.selected_test
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Consume the pending transport and spawn the background test runner
    /// with the given test filter. Returns an error if a runner is already
    /// spawned (the session is single-shot in its first iteration).
    fn start_runner(&self, test_name: String) -> Result<()> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow!("session mutex poisoned"))?
            .take()
            .ok_or_else(|| {
                anyhow!("debug session already started; cannot select a different test")
            })?;

        let config = TestConfig {
            debug: true,
            filter: Some(test_name.clone()),
            ..Default::default()
        };
        let test_label = test_name.clone();
        let path = pending.path;
        let transport = pending.transport;

        let handle: JoinHandle<Result<()>> = thread::Builder::new()
            .name("acton-mcp-runner".to_string())
            .spawn(move || {
                test_cmd_with_transport(path, &config, transport).with_context(|| {
                    format!("failed to run test `{test_label}` under mcp debugger")
                })
            })
            .context("failed to spawn runner thread")?;

        *self
            .runner
            .lock()
            .map_err(|_| anyhow!("session mutex poisoned"))? = Some(handle);
        *self
            .selected_test
            .lock()
            .map_err(|_| anyhow!("session mutex poisoned"))? = Some(test_name);
        Ok(())
    }

    /// Join the background runner thread (if any) and propagate its result.
    fn join_runner(&self) -> Result<()> {
        let handle = match self.runner.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => return Err(anyhow!("session mutex poisoned")),
        };
        match handle {
            Some(h) => match h.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(anyhow!("runner thread panicked")),
            },
            None => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// DAP bridge
// ---------------------------------------------------------------------------

struct DapBridge {
    inner: Mutex<BridgeInner>,
}

struct BridgeInner {
    req_tx: Sender<Request>,
    dap_rx: Receiver<DapMessage>,
    next_seq: i64,
    pending_events: Vec<Event>,
    initialized: bool,
    terminated: bool,
}

impl DapBridge {
    const fn new(req_tx: Sender<Request>, dap_rx: Receiver<DapMessage>) -> Self {
        Self {
            inner: Mutex::new(BridgeInner {
                req_tx,
                dap_rx,
                next_seq: 1,
                pending_events: Vec::new(),
                initialized: false,
                terminated: false,
            }),
        }
    }

    /// Send one DAP request, block until the matching response arrives, and
    /// collect any events seen along the way.
    #[allow(clippy::significant_drop_tightening)]
    fn send_and_wait(
        &self,
        command: Command,
        timeout: Duration,
    ) -> Result<(Response, Vec<Event>)> {
        let mut inner = self.inner.lock().map_err(|_| anyhow!("bridge mutex poisoned"))?;
        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner
            .req_tx
            .send(Request {
                seq,
                command,
            })
            .map_err(|e| anyhow!("runner thread closed: {e}"))?;

        let mut events = std::mem::take(&mut inner.pending_events);

        loop {
            match inner.dap_rx.recv_timeout(timeout) {
                Ok(DapMessage::Response(rsp)) => {
                    if rsp.request_seq == seq {
                        return Ok((rsp, events));
                    }
                    // Stale response to a dropped request; skip.
                }
                Ok(DapMessage::Event(ev)) => {
                    if matches!(ev, Event::Terminated(_) | Event::Exited(_)) {
                        inner.terminated = true;
                    }
                    events.push(ev);
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(anyhow!(
                        "timed out waiting for DAP response to request #{seq}"
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("DAP transport disconnected before response"));
                }
            }
        }
    }

    /// Ensure the DAP handshake has been driven once. Safe to call many times.
    fn ensure_initialized(&self) -> Result<Vec<Event>> {
        {
            let inner = self.inner.lock().map_err(|_| anyhow!("bridge mutex poisoned"))?;
            if inner.initialized {
                return Ok(Vec::new());
            }
        }

        let mut collected = Vec::new();

        let (_, mut events) = self.send_and_wait(
            Command::Initialize(InitializeArguments {
                adapter_id: "acton-mcp".to_string(),
                client_id: Some("acton-mcp".to_string()),
                client_name: Some("acton mcp".to_string()),
                lines_start_at1: Some(true),
                columns_start_at1: Some(true),
                path_format: None,
                supports_variable_type: Some(true),
                supports_variable_paging: Some(false),
                supports_run_in_terminal_request: Some(false),
                supports_memory_references: Some(false),
                supports_progress_reporting: Some(false),
                supports_invalidated_event: Some(false),
                supports_memory_event: Some(false),
                supports_args_can_be_interpreted_by_shell: Some(false),
                supports_start_debugging_request: Some(false),
                locale: Some("en-US".to_string()),
            }),
            DEFAULT_WAIT,
        )?;
        collected.append(&mut events);

        let (_, mut events) = self.send_and_wait(
            Command::Launch(LaunchRequestArguments {
                no_debug: Some(false),
                restart_data: None,
                additional_data: None,
            }),
            DEFAULT_WAIT,
        )?;
        collected.append(&mut events);

        // Drive through `configurationDone`; this lets the replayer advance to
        // the first Stopped (entry) event.
        let (_, mut events) =
            self.send_and_wait(Command::ConfigurationDone, DEFAULT_WAIT)?;
        collected.append(&mut events);

        {
            let mut inner = self.inner.lock().map_err(|_| anyhow!("bridge mutex poisoned"))?;
            inner.initialized = true;
        }

        Ok(collected)
    }
}

// ---------------------------------------------------------------------------
// Argument schemas (MCP-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
struct SetBreakpointsParams {
    /// Absolute path to the source file.
    file: String,
    /// Line numbers (1-based) where breakpoints should be placed. Pass an empty
    /// list to clear breakpoints in this file.
    lines: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
struct EvaluateParams {
    /// Expression to evaluate in the current frame.
    expression: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
struct SelectTestParams {
    /// Name of the test to run under the debugger. Must match a test declared
    /// in the project's `*.test.tolk` files.
    test: String,
}

// ---------------------------------------------------------------------------
// Debugger (rmcp handler)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct TolkDebugger {
    session: Arc<SessionState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<TolkDebugger>,
}

#[tool_router]
impl TolkDebugger {
    fn new(session: Arc<SessionState>) -> Self {
        Self {
            session,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Choose which project test to debug and spawn the background test runner. Must be called exactly once per session before any other debugging tool (unless the test was already selected via the `--test` CLI flag). Returns the selected test name."
    )]
    async fn select_test(
        &self,
        Parameters(args): Parameters<SelectTestParams>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session.clone();
        let test = args.test.clone();
        tokio::task::spawn_blocking(move || session.start_runner(test))
            .await
            .map_err(internal_err)?
            .map_err(internal_err)?;
        Ok(CallToolResult::structured(json!({
            "selected_test": args.test,
        })))
    }

    #[tool(
        description = "Initialize the debugger and run until the first stop. Requires a test to have been selected via `select_test` or the `--test` CLI flag. Returns the current cursor (file, line) so the agent knows where execution is paused. Safe to call multiple times — subsequent calls are no-ops."
    )]
    async fn start(&self) -> Result<CallToolResult, McpError> {
        self.require_runner()?;
        let session = self.session.clone();
        let events = tokio::task::spawn_blocking(move || session.bridge.ensure_initialized())
            .await
            .map_err(internal_err)?
            .map_err(internal_err)?;

        let cursor = self.fetch_cursor().await.unwrap_or(Value::Null);
        Ok(CallToolResult::structured(json!({
            "selected_test": self.session.selected_test(),
            "cursor": cursor,
            "stop": summarize_stop(&events),
        })))
    }

    #[tool(
        description = "Set source breakpoints in a file. Pass an empty `lines` array to clear. Returns the verified resolved lines."
    )]
    async fn set_breakpoints(
        &self,
        Parameters(args): Parameters<SetBreakpointsParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;

        let source = Source {
            path: Some(args.file.clone()),
            name: std::path::Path::new(&args.file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let breakpoints: Vec<SourceBreakpoint> = args
            .lines
            .iter()
            .map(|line| SourceBreakpoint {
                line: *line,
                ..Default::default()
            })
            .collect();

        let (response, _events) = self
            .call(Command::SetBreakpoints(SetBreakpointsArguments {
                source,
                breakpoints: Some(breakpoints),
                source_modified: Some(false),
                ..Default::default()
            }))
            .await?;

        let body = response_body(&response).ok_or_else(|| tool_err("SetBreakpoints failed"))?;
        let verified = if let ResponseBody::SetBreakpoints(body) = body {
            body.breakpoints
                .iter()
                .map(|bp| {
                    json!({
                        "verified": bp.verified,
                        "line": bp.line,
                        "id": bp.id,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Ok(CallToolResult::structured(json!({
            "file": args.file,
            "breakpoints": verified,
        })))
    }

    #[tool(
        description = "Continue execution until the next breakpoint, exception, or termination. Response includes the new cursor + stop reason."
    )]
    async fn continue_exec(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        self.step_call(Command::Continue(ContinueArguments {
            thread_id: THREAD_ID,
            single_thread: Some(true),
        }))
        .await
    }

    #[tool(description = "Step over the current source line.")]
    async fn step_over(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        self.step_call(Command::Next(NextArguments {
            thread_id: THREAD_ID,
            single_thread: Some(true),
            granularity: None,
        }))
        .await
    }

    #[tool(description = "Step into the call at the current source line.")]
    async fn step_into(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        self.step_call(Command::StepIn(StepInArguments {
            thread_id: THREAD_ID,
            single_thread: Some(true),
            target_id: None,
            granularity: None,
        }))
        .await
    }

    #[tool(description = "Step out of the current function.")]
    async fn step_out(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        self.step_call(Command::StepOut(StepOutArguments {
            thread_id: THREAD_ID,
            single_thread: Some(true),
            granularity: None,
        }))
        .await
    }

    #[tool(
        description = "Return the current call stack. Frame 0 is the innermost; each frame reports file/line/column so the agent knows exactly where the cursor is."
    )]
    async fn where_am_i(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        let frames = self.fetch_stack_frames().await?;
        Ok(CallToolResult::structured(json!({ "frames": frames })))
    }

    #[tool(
        description = "List local variables of the innermost frame, rendered by the Tolk debugger."
    )]
    async fn locals(&self) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;
        let frames = self.fetch_stack_frames().await?;
        let top_id = frames
            .first()
            .and_then(|f| f.get("id").and_then(Value::as_i64))
            .ok_or_else(|| tool_err("no frames available"))?;
        let locals = self.fetch_locals(top_id).await?;
        Ok(CallToolResult::structured(json!({
            "frame_id": top_id,
            "locals": locals,
        })))
    }

    #[tool(
        description = "Evaluate a Tolk expression in the current innermost frame. Equivalent to DAP evaluate."
    )]
    async fn evaluate(
        &self,
        Parameters(args): Parameters<EvaluateParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_started().await?;

        let (response, _) = self
            .call(Command::Evaluate(EvaluateArguments {
                expression: args.expression.clone(),
                frame_id: None,
                context: None,
                format: None,
            }))
            .await?;

        if !response.success {
            let msg = response
                .error
                .as_ref()
                .map(|m| m.format.clone())
                .or_else(|| response.message.as_ref().map(|m| format!("{m:?}")))
                .unwrap_or_else(|| "evaluate failed".to_string());
            return Ok(CallToolResult::structured_error(json!({ "error": msg })));
        }

        let body = response_body(&response);
        let value = if let Some(ResponseBody::Evaluate(body)) = body {
            json!({
                "result": body.result,
                "type": body.type_field,
                "variables_reference": body.variables_reference,
            })
        } else {
            json!({ "result": "<no body>" })
        };

        Ok(CallToolResult::structured(json!({
            "expression": args.expression,
            "value": value,
        })))
    }

    #[tool(description = "Terminate the debug session and let the test runner unwind.")]
    async fn terminate(&self) -> Result<CallToolResult, McpError> {
        if !self.session.runner_started() {
            return Ok(CallToolResult::structured(json!({
                "terminated": false,
                "note": "no debug session to terminate",
            })));
        }
        let (_response, _events) = self
            .call(Command::Disconnect(DisconnectArguments {
                restart: Some(false),
                terminate_debuggee: Some(true),
                suspend_debuggee: None,
            }))
            .await?;
        Ok(CallToolResult::structured(json!({ "terminated": true })))
    }
}

impl TolkDebugger {
    fn require_runner(&self) -> Result<(), McpError> {
        if self.session.runner_started() {
            Ok(())
        } else {
            Err(tool_err(
                "no test selected — call `select_test` first (or relaunch with `--test <name>`)",
            ))
        }
    }

    async fn ensure_started(&self) -> Result<(), McpError> {
        self.require_runner()?;
        let session = self.session.clone();
        tokio::task::spawn_blocking(move || session.bridge.ensure_initialized())
            .await
            .map_err(internal_err)?
            .map_err(internal_err)?;
        Ok(())
    }

    async fn call(&self, command: Command) -> Result<(Response, Vec<Event>), McpError> {
        let session = self.session.clone();
        tokio::task::spawn_blocking(move || session.bridge.send_and_wait(command, DEFAULT_WAIT))
            .await
            .map_err(internal_err)?
            .map_err(internal_err)
    }

    async fn step_call(&self, command: Command) -> Result<CallToolResult, McpError> {
        let (_response, events) = self.call(command).await?;
        let cursor = self.fetch_cursor().await.unwrap_or(Value::Null);
        Ok(CallToolResult::structured(json!({
            "cursor": cursor,
            "stop": summarize_stop(&events),
        })))
    }

    async fn fetch_cursor(&self) -> Result<Value, McpError> {
        let frames = self.fetch_stack_frames().await?;
        Ok(frames.first().cloned().unwrap_or(Value::Null))
    }

    async fn fetch_stack_frames(&self) -> Result<Vec<Value>, McpError> {
        let (response, _) = self
            .call(Command::StackTrace(StackTraceArguments {
                thread_id: THREAD_ID,
                start_frame: Some(0),
                levels: None,
                format: None,
            }))
            .await?;

        if let Some(ResponseBody::StackTrace(body)) = response_body(&response) {
            Ok(body
                .stack_frames
                .iter()
                .map(|f| {
                    json!({
                        "id": f.id,
                        "name": f.name,
                        "file": f.source.as_ref().and_then(|s| s.path.clone()),
                        "line": f.line,
                        "column": f.column,
                        "end_line": f.end_line,
                        "end_column": f.end_column,
                    })
                })
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    async fn fetch_locals(&self, frame_id: i64) -> Result<Vec<Value>, McpError> {
        let (response, _) = self
            .call(Command::Scopes(ScopesArguments { frame_id }))
            .await?;

        let Some(ResponseBody::Scopes(body)) = response_body(&response) else {
            return Ok(Vec::new());
        };
        let locals_scope = body
            .scopes
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case("locals"))
            .cloned();
        let Some(scope) = locals_scope else {
            return Ok(Vec::new());
        };

        let (response, _) = self
            .call(Command::Variables(VariablesArguments {
                variables_reference: scope.variables_reference,
                filter: None,
                start: None,
                count: None,
                format: None,
            }))
            .await?;

        if let Some(ResponseBody::Variables(body)) = response_body(&response) {
            Ok(body
                .variables
                .iter()
                .map(|v| {
                    json!({
                        "name": v.name,
                        "value": v.value,
                        "type": v.type_field,
                        "variables_reference": v.variables_reference,
                    })
                })
                .collect())
        } else {
            Ok(Vec::new())
        }
    }
}

#[tool_handler]
impl ServerHandler for TolkDebugger {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Tolk contract debugger. Workflow: (1) call `select_test` with the name \
                 of the test you want to debug — unless the user launched the server with \
                 `--test <name>`, in which case this is already done. (2) call `start` to \
                 land on the entry point. (3) use `set_breakpoints`, `continue_exec`, \
                 `step_over`, `step_into`, `step_out`, `where_am_i`, `locals`, and \
                 `evaluate` to drive the debugger. Every stepping tool returns the updated \
                 cursor so you always know which source line is active.",
            )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const fn response_body(response: &Response) -> Option<&ResponseBody> {
    response.body.as_ref()
}

fn summarize_stop(events: &[Event]) -> Value {
    for ev in events.iter().rev() {
        match ev {
            Event::Stopped(body) => {
                return json!({
                    "kind": "stopped",
                    "reason": format!("{:?}", body.reason),
                    "description": body.description,
                    "text": body.text,
                    "hit_breakpoint_ids": body.hit_breakpoint_ids,
                });
            }
            Event::Terminated(_) => {
                return json!({ "kind": "terminated" });
            }
            Event::Exited(body) => {
                return json!({ "kind": "exited", "exit_code": body.exit_code });
            }
            _ => {}
        }
    }
    Value::Null
}

fn internal_err<E: std::fmt::Display>(e: E) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

fn tool_err(msg: impl Into<String>) -> McpError {
    McpError::internal_error(msg.into(), None)
}
