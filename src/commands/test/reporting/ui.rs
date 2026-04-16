use crate::commands::common::error_fmt;
use crate::commands::test::reporting::{FuzzExecutionContext, TestReport, TestReporter};
use acton_config::color::OwoColorize;
use anyhow::Context;
use axum::{
    Router,
    extract::{
        Path as AxumPath, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, put},
};
use futures_util::{SinkExt, StreamExt};
#[cfg(not(debug_assertions))]
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::TcpStream,
    process::Command,
    sync::mpsc::{UnboundedSender, unbounded_channel},
};
#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

// Static directory containing UI assets, embedded into the binary during release builds.
#[cfg(not(debug_assertions))]
static UI_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/crates/acton-test-ui/dist");

#[cfg(target_os = "macos")]
static OPEN_CHROME_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/open_chrome.applescript"
));

pub(crate) struct UiServerState {
    pub reports: Arc<Vec<TestReport>>,
    pub trace_dir: Option<PathBuf>,
    pub project_root: String,
    pub project_root_path: PathBuf,
    pub coverage_lcov: Option<Arc<str>>,
}

pub(crate) struct UiReporter {
    reports: Arc<Mutex<Vec<TestReport>>>,
}

#[derive(Serialize)]
struct UiExecutionSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fuzz: Option<FuzzExecutionContext>,
}

#[derive(Serialize)]
struct UiTestReport {
    name: Arc<str>,
    suite_name: Arc<str>,
    file_path: PathBuf,
    row: usize,
    column: usize,
    duration: std::time::Duration,
    status: crate::commands::test::reporting::TestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detailed_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failed_transactions: Option<Vec<crate::commands::test::trace::TransactionInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failed_transaction_context: Option<crate::commands::test::reporting::FailedTransactionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<ton_source_map::SourceLocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution: Option<UiExecutionSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace_path: Option<String>,
}

impl From<&TestReport> for UiTestReport {
    fn from(test: &TestReport) -> Self {
        Self {
            name: test.name.clone(),
            suite_name: test.suite_name.clone(),
            file_path: test.file_path.clone(),
            row: test.row,
            column: test.column,
            duration: test.duration,
            status: test.status.clone(),
            message: test.message.clone(),
            detailed_message: test.detailed_message.clone(),
            failed_transactions: test.failed_transactions.clone(),
            failed_transaction_context: test.failed_transaction_context.clone(),
            details: test.details.clone(),
            location: test.location.clone(),
            execution: test.execution.as_ref().and_then(|execution| {
                execution
                    .fuzz
                    .clone()
                    .map(|fuzz| UiExecutionSummary { fuzz: Some(fuzz) })
            }),
            trace_path: test.trace_path.clone(),
        }
    }
}

impl UiReporter {
    pub(crate) fn new() -> Self {
        Self {
            reports: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn get_reports_arc(&self) -> Arc<Mutex<Vec<TestReport>>> {
        Arc::clone(&self.reports)
    }
}

impl TestReporter for UiReporter {
    fn on_test_finished(&mut self, test: &TestReport) -> anyhow::Result<()> {
        self.reports
            .lock()
            .expect("cannot lock mutex")
            .push(test.clone());
        Ok(())
    }
}

pub(crate) fn reserve_ui_listener(port: u16) -> anyhow::Result<std::net::TcpListener> {
    let address = format!("127.0.0.1:{port}");
    std::net::TcpListener::bind(&address)
        .with_context(|| error_fmt::port_bind_failure("UI server", &address, "--ui-port"))
}

pub(crate) async fn start_ui_server(
    reports: Vec<TestReport>,
    trace_dir: Option<String>,
    project_root: String,
    coverage_lcov: Option<String>,
    listener: std::net::TcpListener,
) -> anyhow::Result<()> {
    let project_root_path =
        dunce::canonicalize(&project_root).unwrap_or_else(|_| PathBuf::from(&project_root));
    let trace_dir = trace_dir
        .map(PathBuf::from)
        .map(|path| dunce::canonicalize(&path).unwrap_or(path));
    let state = Arc::new(UiServerState {
        reports: Arc::new(reports),
        trace_dir,
        project_root,
        project_root_path,
        coverage_lcov: coverage_lcov.map(Arc::<str>::from),
    });

    let app = build_ui_api_router(state);

    // In debug mode, serve UI assets directly from the filesystem for faster development.
    #[cfg(debug_assertions)]
    let app = {
        let dist_path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/acton-test-ui/dist"
        ));
        app.fallback_service(
            ServeDir::new(&dist_path).fallback(ServeDir::new(dist_path.join("index.html"))),
        )
    };

    // In release mode, serve UI assets embedded within the binary.
    #[cfg(not(debug_assertions))]
    let app = app.fallback(handle_embedded_ui);

    let address = listener
        .local_addr()
        .context("Failed to inspect reserved UI server address")?;
    listener
        .set_nonblocking(true)
        .with_context(|| format!("Failed to configure UI server socket on {address}"))?;
    let listener = tokio::net::TcpListener::from_std(listener)
        .with_context(|| format!("Failed to activate UI server on {address}"))?;
    let url = format!("http://{address}");
    println!("     {} UI server at {}", "Starting".green().bold(), url);

    open_browser(&url);

    axum::serve(listener, app).await?;
    Ok(())
}

fn build_ui_api_router(state: Arc<UiServerState>) -> Router {
    Router::new()
        .route("/api/reports", get(handle_api_reports))
        .route("/api/test-logs", get(handle_api_test_logs))
        .route("/api/trace/{name}", get(handle_api_trace))
        .route("/api/contract/{name}", get(handle_api_contract))
        .route("/api/file", get(handle_api_file).put(handle_api_save_file))
        .route("/api/debug/ws", get(handle_api_debug_ws))
        .route("/api/coverage.lcov", get(handle_api_coverage_lcov))
        .route("/api/config", get(handle_api_config))
        .with_state(state)
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let chromium_browsers = [
            "Google Chrome",
            "Arc",
            "Brave Browser",
            "Microsoft Edge",
            "Vivaldi",
        ];

        for browser in chromium_browsers {
            if is_process_running(browser) {
                // Execute embedded AppleScript with arguments
                let child = std::process::Command::new("osascript")
                    .arg("-")
                    .arg(url)
                    .arg(browser)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok();

                if let Some(mut child) = child {
                    use std::io::Write;
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(OPEN_CHROME_SCRIPT.as_bytes());
                    }
                    let status = child.wait().ok();
                    if status.is_some_and(|s| s.success()) {
                        return;
                    }
                }
            }
        }
    }

    if let Err(e) = opener::open(url) {
        eprintln!("Warning: Failed to open browser: {e}");
    }
}

#[cfg(target_os = "macos")]
fn is_process_running(process_name: &str) -> bool {
    let output = std::process::Command::new("ps").arg("-cax").output().ok();

    if let Some(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // We look for the exact process name in the list
        stdout.lines().any(|line| line.contains(process_name))
    } else {
        false
    }
}

/// Handles requests for UI assets when they are embedded in the binary (release mode).
#[cfg(not(debug_assertions))]
async fn handle_embedded_ui(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    // default to index.html for root requests
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = UI_DIR.get_file(path) {
        // Map common file extensions to their respective MIME types.
        let content_type = match path.split('.').last() {
            Some("html") => "text/html",
            Some("js") => "application/javascript",
            Some("css") => "text/css",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("json") => "application/json",
            _ => "application/octet-stream",
        };
        return (([("content-type", content_type)]), file.contents()).into_response();
    }

    // fallback to index.html for SPA routing.
    // this allows browser refreshes on sub-routes to work correctly
    if let Some(index) = UI_DIR.get_file("index.html") {
        return (([("content-type", "text/html")]), index.contents()).into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn handle_api_reports(State(state): State<Arc<UiServerState>>) -> impl IntoResponse {
    let reports = state
        .reports
        .iter()
        .map(UiTestReport::from)
        .collect::<Vec<_>>();
    Json(reports)
}

#[derive(Deserialize)]
struct TestLogsQuery {
    file_path: String,
    name: String,
    row: usize,
    column: usize,
}

#[derive(Default, Serialize)]
struct TestExecutionLogsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vm_log_diff: Option<String>,
}

async fn handle_api_test_logs(
    Query(query): Query<TestLogsQuery>,
    State(state): State<Arc<UiServerState>>,
) -> impl IntoResponse {
    let file_path = Path::new(&query.file_path);
    let Some(test) = state.reports.iter().find(|report| {
        report.file_path == file_path
            && report.name.as_ref() == query.name
            && report.row == query.row
            && report.column == query.column
    }) else {
        return (StatusCode::NOT_FOUND, "Test not found").into_response();
    };

    let response =
        test.execution
            .as_ref()
            .map_or_else(TestExecutionLogsResponse::default, |execution| {
                TestExecutionLogsResponse {
                    stdout: non_empty_text(&execution.stdout),
                    stderr: non_empty_text(&execution.stderr),
                    vm_log_diff: execution.vm_log_diff.clone(),
                }
            });

    Json(response).into_response()
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

#[derive(Deserialize)]
struct SaveFileRequest {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct DebugSessionQuery {
    file_path: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ClientDebugSocketMessage {
    Dap { payload: Value },
    Stop,
}

async fn handle_api_file(
    Query(query): Query<FileQuery>,
    State(state): State<Arc<UiServerState>>,
) -> impl IntoResponse {
    let requested_path = PathBuf::from(&query.path);
    let Some(file_path) = resolve_path_within_root(&state.project_root_path, &requested_path)
    else {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    };

    match tokio::fs::read_to_string(file_path).await {
        Ok(content) => content.into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

async fn handle_api_save_file(
    State(state): State<Arc<UiServerState>>,
    Json(payload): Json<SaveFileRequest>,
) -> impl IntoResponse {
    let requested_path = PathBuf::from(&payload.path);
    let Some(file_path) = resolve_path_within_root(&state.project_root_path, &requested_path)
    else {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    };

    match tokio::fs::write(file_path, payload.content).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response(),
    }
}

async fn handle_api_debug_ws(
    ws: WebSocketUpgrade,
    Query(query): Query<DebugSessionQuery>,
    State(state): State<Arc<UiServerState>>,
) -> impl IntoResponse {
    let requested_path = PathBuf::from(&query.file_path);
    let Some(file_path) = resolve_path_within_root(&state.project_root_path, &requested_path)
    else {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    };

    let test_exists = state
        .reports
        .iter()
        .any(|report| report.file_path == file_path && report.name.as_ref() == query.name);
    if !test_exists {
        return (StatusCode::NOT_FOUND, "Test not found").into_response();
    }

    ws.on_upgrade(move |socket| run_debug_socket(socket, state, file_path, query.name))
        .into_response()
}

#[derive(Serialize)]
struct ConfigResponse {
    project_root: String,
}

async fn handle_api_config(State(state): State<Arc<UiServerState>>) -> impl IntoResponse {
    Json(ConfigResponse {
        project_root: state.project_root.clone(),
    })
}

async fn handle_api_coverage_lcov(State(state): State<Arc<UiServerState>>) -> impl IntoResponse {
    let Some(coverage_lcov) = &state.coverage_lcov else {
        return (StatusCode::NOT_FOUND, "Coverage not enabled").into_response();
    };

    (
        [("content-type", "text/plain; charset=utf-8")],
        coverage_lcov.to_string(),
    )
        .into_response()
}

async fn handle_api_trace(
    AxumPath(name): AxumPath<String>,
    State(state): State<Arc<UiServerState>>,
) -> impl IntoResponse {
    let Some(trace_dir) = &state.trace_dir else {
        return (StatusCode::NOT_FOUND, "Traces not enabled").into_response();
    };

    let Some(trace_path) = resolve_path_within_root(trace_dir, Path::new(&name)) else {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    };

    match tokio::fs::read_to_string(trace_path).await {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(json) => Json(json).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid trace JSON").into_response(),
        },
        Err(_) => (StatusCode::NOT_FOUND, "Trace not found").into_response(),
    }
}

async fn handle_api_contract(
    AxumPath(name): AxumPath<String>,
    State(state): State<Arc<UiServerState>>,
) -> impl IntoResponse {
    let Some(trace_dir) = &state.trace_dir else {
        return (StatusCode::NOT_FOUND, "Traces not enabled").into_response();
    };

    let contracts_dir = trace_dir.join("contracts");
    let contract_name = format!("{name}.json");
    let Some(contract_path) = resolve_path_within_root(&contracts_dir, Path::new(&contract_name))
    else {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    };

    match tokio::fs::read_to_string(contract_path).await {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(json) => Json(json).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Invalid contract JSON").into_response(),
        },
        Err(_) => (StatusCode::NOT_FOUND, "Contract not found").into_response(),
    }
}

async fn run_debug_socket(
    socket: WebSocket,
    state: Arc<UiServerState>,
    file_path: PathBuf,
    test_name: String,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (out_tx, mut out_rx) = unbounded_channel::<Message>();

    let writer_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if ws_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    let session_result = drive_debug_session(
        &mut ws_receiver,
        out_tx.clone(),
        state,
        file_path,
        test_name,
    )
    .await;
    if let Err(err) = session_result {
        let _ = send_socket_json(
            &out_tx,
            serde_json::json!({
                "type": "error",
                "message": err.to_string(),
            }),
        );
    }

    drop(out_tx);
    let _ = writer_task.await;
}

async fn drive_debug_session(
    ws_receiver: &mut futures_util::stream::SplitStream<WebSocket>,
    out_tx: UnboundedSender<Message>,
    state: Arc<UiServerState>,
    file_path: PathBuf,
    test_name: String,
) -> anyhow::Result<()> {
    send_socket_json(
        &out_tx,
        serde_json::json!({
            "type": "status",
            "status": "launching",
        }),
    )?;

    let debug_port = reserve_ephemeral_port()?;
    let executable = std::env::current_exe().context("Failed to determine acton executable")?;
    let filter = format!("^{}$", regex::escape(&test_name));

    let mut child = Command::new(executable);
    child
        .current_dir(&state.project_root_path)
        .arg("test")
        .arg(&file_path)
        .arg("--filter")
        .arg(filter)
        .arg("--debug")
        .arg("--debug-port")
        .arg(debug_port.to_string())
        .env("ACTON_INTERNAL_SKIP_BUILD", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = child.spawn().with_context(|| {
        format!(
            "Failed to start debug process for '{}' in '{}'",
            test_name,
            file_path.display()
        )
    })?;

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(forward_process_output("stdout", stdout, out_tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(forward_process_output("stderr", stderr, out_tx.clone()));
    }

    send_socket_json(
        &out_tx,
        serde_json::json!({
            "type": "status",
            "status": "connecting",
        }),
    )?;

    let stream = connect_to_dap_server(debug_port, &mut child).await?;
    let (dap_read, mut dap_write) = stream.into_split();
    let out_tx_for_dap = out_tx.clone();
    let dap_reader_task = tokio::spawn(async move {
        let result = forward_dap_messages(dap_read, out_tx_for_dap.clone()).await;
        if let Err(err) = result {
            let _ = send_socket_json(
                &out_tx_for_dap,
                serde_json::json!({
                    "type": "error",
                    "message": format!("DAP bridge failed: {err}"),
                }),
            );
        }
    });

    send_socket_json(
        &out_tx,
        serde_json::json!({
            "type": "status",
            "status": "ready",
        }),
    )?;

    while let Some(message) = ws_receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let client_message: ClientDebugSocketMessage =
                    serde_json::from_str(&text).context("Invalid websocket control message")?;
                match client_message {
                    ClientDebugSocketMessage::Dap { payload } => {
                        write_dap_message(&mut dap_write, &payload).await?;
                    }
                    ClientDebugSocketMessage::Stop => break,
                }
            }
            Ok(Message::Binary(_)) => {}
            Ok(Message::Ping(_)) => {}
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(err) => return Err(err).context("Websocket receive failed"),
        }
    }

    let _ = dap_write.shutdown().await;
    let _ = dap_reader_task.await;

    let _ = child.kill().await;
    let status = child.wait().await.ok().and_then(|status| status.code());
    let _ = send_socket_json(
        &out_tx,
        serde_json::json!({
            "type": "process-exit",
            "status": status,
        }),
    );

    Ok(())
}

async fn connect_to_dap_server(
    port: u16,
    child: &mut tokio::process::Child,
) -> anyhow::Result<TcpStream> {
    let address = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);

    loop {
        match TcpStream::connect(&address).await {
            Ok(stream) => return Ok(stream),
            Err(connect_err) => {
                if let Some(status) = child.try_wait().context("Failed to poll debug process")? {
                    anyhow::bail!(
                        "Debug process exited before debugger connection was ready (status: {status}): {connect_err}"
                    );
                }

                if tokio::time::Instant::now() >= deadline {
                    anyhow::bail!("Timed out waiting for debug server on {address}: {connect_err}");
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn forward_process_output<R>(
    stream: &'static str,
    reader: R,
    out_tx: UnboundedSender<Message>,
) where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if send_socket_json(
                    &out_tx,
                    serde_json::json!({
                        "type": "process-output",
                        "stream": stream,
                        "text": line,
                    }),
                )
                .is_err()
                {
                    break;
                }
            }
            Ok(None) => break,
            Err(err) => {
                let _ = send_socket_json(
                    &out_tx,
                    serde_json::json!({
                        "type": "error",
                        "message": format!("Failed to read {stream}: {err}"),
                    }),
                );
                break;
            }
        }
    }
}

async fn forward_dap_messages<R>(reader: R, out_tx: UnboundedSender<Message>) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    while let Some(payload) = read_dap_message(&mut reader).await? {
        send_socket_json(
            &out_tx,
            serde_json::json!({
                "type": "dap",
                "payload": payload,
            }),
        )?;
    }

    Ok(())
}

async fn read_dap_message<R>(reader: &mut BufReader<R>) -> anyhow::Result<Option<Value>>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let read_size = reader.read_line(&mut line).await?;
        if read_size == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            if let Some(length) = content_length {
                let mut content = vec![0; length];
                reader.read_exact(&mut content).await?;
                let payload = serde_json::from_slice(&content)
                    .context("Failed to decode DAP message payload")?;
                return Ok(Some(payload));
            }
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':')
            && key.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse()
                    .context("Invalid Content-Length header in DAP stream")?,
            );
        }
    }
}

async fn write_dap_message<W>(writer: &mut W, payload: &Value) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let payload = serde_json::to_string(payload).context("Failed to serialize DAP request")?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

fn reserve_ephemeral_port() -> anyhow::Result<u16> {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).context("Failed to reserve debug port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("Failed to inspect reserved debug port")
}

fn send_socket_json(
    out_tx: &UnboundedSender<Message>,
    payload: serde_json::Value,
) -> anyhow::Result<()> {
    let text = serde_json::to_string(&payload).context("Failed to encode websocket payload")?;
    out_tx
        .send(Message::Text(text.into()))
        .map_err(|_| anyhow::anyhow!("Websocket closed"))?;
    Ok(())
}

fn resolve_path_within_root(root: &Path, requested: &Path) -> Option<PathBuf> {
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let candidate = dunce::canonicalize(candidate).ok()?;
    candidate.starts_with(root).then_some(candidate)
}

fn non_empty_text(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}
