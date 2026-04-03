mod session;
mod transport;

use crate::build_info;
use acton_config::config::manifest_path;
use acton_config::config::project_root;
use anyhow::Context;
use anyhow::anyhow;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, JsonObject, ListToolsResult,
    PaginatedRequestParams, PromptsCapability, ProtocolVersion, ResourcesCapability,
    ServerCapabilities, ServerInfo, SetLevelRequestParams, Tool, ToolsCapability,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use session::DebugSessionManager;
use session::ScopesRequest;
use session::SessionRequest;
use session::SessionThreadRequest;
use session::SetBreakpointsRequest;
use session::StartRetraceDebugArgs;
use session::StartScriptDebugArgs;
use session::StartTestDebugArgs;
use session::VariablesRequest;
use std::sync::Mutex;
use transport::ContentLengthStdioTransport;

const DEBUG_MCP_INSTRUCTIONS: &str = "Use the Acton debug tools to start a script, test, or retrace debug session and then drive it with stepping and inspection calls.";

pub fn debug_mcp_cmd() -> anyhow::Result<()> {
    let acton_exe = std::env::current_exe().context("Failed to resolve current executable")?;
    let project_root = project_root().to_path_buf();
    let manifest_path = manifest_path().to_path_buf();
    let server = DebugMcpServer::new(DebugSessionManager::new(
        acton_exe,
        project_root,
        manifest_path,
    ));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to initialize tokio runtime for debug MCP server")?;

    runtime.block_on(async move {
        let running = server
            .serve(ContentLengthStdioTransport::new())
            .await
            .context("Failed to start debug MCP server")?;
        running
            .waiting()
            .await
            .context("Debug MCP server task failed")?;
        Ok(())
    })
}

struct DebugMcpServer {
    sessions: Mutex<DebugSessionManager>,
    info: ServerInfo,
    tools: Vec<Tool>,
}

impl DebugMcpServer {
    fn new(sessions: DebugSessionManager) -> Self {
        Self {
            sessions: Mutex::new(sessions),
            info: server_info(),
            tools: tool_descriptors(),
        }
    }

    fn execute_tool_result(&self, name: &str, arguments: Value) -> CallToolResult {
        match self.execute_tool(name, arguments) {
            Ok(result) => CallToolResult::structured(result),
            Err(err) => CallToolResult::error(vec![Content::text(err.to_string())]),
        }
    }

    fn execute_tool(&self, name: &str, arguments: Value) -> anyhow::Result<Value> {
        match name {
            "start_script_debug" => {
                let args = parse_args::<StartScriptDebugArgs>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.start_script_debug(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "start_test_debug" => {
                let args = parse_args::<StartTestDebugArgs>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.start_test_debug(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "start_retrace_debug" => {
                let args = parse_args::<StartRetraceDebugArgs>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.start_retrace_debug(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "continue" => {
                let args = parse_args::<SessionThreadRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.continue_execution(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "step_over" => {
                let args = parse_args::<SessionThreadRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.step_over(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "step_into" => {
                let args = parse_args::<SessionThreadRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.step_into(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "step_out" => {
                let args = parse_args::<SessionThreadRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.step_out(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "stack_trace" => {
                let args = parse_args::<SessionThreadRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.stack_trace(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "scopes" => {
                let args = parse_args::<ScopesRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.scopes(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "variables" => {
                let args = parse_args::<VariablesRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.variables(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "set_breakpoints" => {
                let args = parse_args::<SetBreakpointsRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.set_breakpoints(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            "terminate_session" => {
                let args = parse_args::<SessionRequest>(arguments)?;
                self.with_sessions(|sessions| {
                    let result = sessions.terminate_session(args)?;
                    serde_json::to_value(result).map_err(Into::into)
                })
            }
            _ => Err(anyhow!("Unknown tool {name}")),
        }
    }

    fn with_sessions<T>(
        &self,
        op: impl FnOnce(&mut DebugSessionManager) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow!("Debug session state is poisoned"))?;
        op(&mut sessions)
    }
}

impl ServerHandler for DebugMcpServer {
    fn get_info(&self) -> ServerInfo {
        self.info.clone()
    }

    fn set_level(
        &self,
        _request: SetLevelRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<(), ErrorData>> + Send + '_ {
        std::future::ready(Ok(()))
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(self.tools.clone())))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let result = self.execute_tool_result(request.name.as_ref(), arguments);
        std::future::ready(Ok(result))
    }
}

fn parse_args<T: DeserializeOwned>(arguments: Value) -> anyhow::Result<T> {
    serde_json::from_value(arguments).map_err(|err| anyhow!("Invalid tool arguments: {err}"))
}

fn server_info() -> ServerInfo {
    let mut capabilities = ServerCapabilities::default();
    capabilities.prompts = Some(PromptsCapability {
        list_changed: Some(false),
    });
    capabilities.resources = Some(ResourcesCapability {
        subscribe: Some(false),
        list_changed: Some(false),
    });
    capabilities.tools = Some(ToolsCapability {
        list_changed: Some(false),
    });

    ServerInfo::new(capabilities)
        .with_protocol_version(ProtocolVersion::V_2025_03_26)
        .with_server_info(Implementation::new(
            "acton-debug-mcp",
            build_info::SHORT_VERSION,
        ))
        .with_instructions(DEBUG_MCP_INSTRUCTIONS)
}

fn tool_descriptors() -> Vec<Tool> {
    vec![
        tool_descriptor(
            "start_script_debug",
            "Start a source-level debug session for a Tolk script via the existing Acton DAP server.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Script path, relative to the project root or absolute." },
                    "args": {
                        "type": "array",
                        "description": "Script arguments passed through to `acton script`.",
                        "items": { "type": "string" },
                        "default": []
                    },
                    "breakpoints": breakpoint_schema()
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        tool_descriptor(
            "start_test_debug",
            "Start a source-level debug session for an Acton test run.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Optional test file or directory path." },
                    "filter": { "type": "string", "description": "Optional regex filter forwarded to `acton test --filter`." },
                    "breakpoints": breakpoint_schema()
                },
                "additionalProperties": false
            }),
        ),
        tool_descriptor(
            "start_retrace_debug",
            "Start a source-level debug session for a retraced on-chain transaction.",
            json!({
                "type": "object",
                "properties": {
                    "tx_hash": { "type": "string", "description": "Transaction hash to retrace." },
                    "contract": { "type": "string", "description": "Contract name from Acton.toml used to build source maps." },
                    "net": { "type": "string", "description": "Optional network selector." },
                    "api_key": { "type": "string", "description": "Optional TonCenter API key." },
                    "breakpoints": breakpoint_schema()
                },
                "required": ["tx_hash", "contract"],
                "additionalProperties": false
            }),
        ),
        control_tool(
            "continue",
            "Continue execution until the next breakpoint, exception, or termination.",
        ),
        control_tool(
            "step_over",
            "Execute the next source-level step without entering called frames.",
        ),
        control_tool(
            "step_into",
            "Execute the next source-level step and enter called frames when possible.",
        ),
        control_tool("step_out", "Run until the current frame returns."),
        tool_descriptor(
            "stack_trace",
            "Return the current stack trace for the session.",
            session_thread_schema(),
        ),
        tool_descriptor(
            "scopes",
            "Return available variable scopes for a stack frame.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "frame_id": { "type": "integer" }
                },
                "required": ["session_id", "frame_id"],
                "additionalProperties": false
            }),
        ),
        tool_descriptor(
            "variables",
            "Expand variables for a variables reference returned by `scopes` or another `variables` call.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "variables_reference": { "type": "integer" }
                },
                "required": ["session_id", "variables_reference"],
                "additionalProperties": false
            }),
        ),
        tool_descriptor(
            "set_breakpoints",
            "Set or update source breakpoints for one or more files in an existing session.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "breakpoints": breakpoint_schema()
                },
                "required": ["session_id"],
                "additionalProperties": false
            }),
        ),
        tool_descriptor(
            "terminate_session",
            "Terminate a debug session and return child process output tails for diagnostics.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                },
                "required": ["session_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

fn tool_descriptor(name: &'static str, description: &'static str, input_schema: Value) -> Tool {
    Tool::new(name, description, json_object(input_schema))
}

fn control_tool(name: &'static str, description: &'static str) -> Tool {
    tool_descriptor(name, description, session_thread_schema())
}

fn json_object(value: Value) -> JsonObject {
    serde_json::from_value(value).expect("tool schema must be a JSON object")
}

fn session_thread_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" },
            "thread_id": {
                "type": "integer",
                "description": "Optional thread id. Acton debug sessions currently use thread 1.",
                "default": 1
            }
        },
        "required": ["session_id"],
        "additionalProperties": false
    })
}

fn breakpoint_schema() -> Value {
    json!({
        "type": "array",
        "default": [],
        "items": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Source file path, relative to the project root or absolute." },
                "lines": {
                    "type": "array",
                    "items": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "default": []
                }
            },
            "required": ["path"],
            "additionalProperties": false
        }
    })
}
