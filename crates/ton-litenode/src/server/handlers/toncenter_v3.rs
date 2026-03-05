use super::utils::{get_extra, handle_result, parse_method_name};
use crate::api::toncenter_v3;
use crate::litenode::LiteNode;
use crate::server::models::{
    EmulateTraceRequest, GetAddressInformationV3Request, GetJettonMastersRequest,
    GetJettonWalletsRequest, GetTracesQuery, RunGetMethodRequest, SendBocRequest,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::Engine;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use toncenter_v3 as v3;

pub async fn get_traces(
    State(node): State<Arc<LiteNode>>,
    Query(payload): Query<GetTracesQuery>,
) -> Json<Value> {
    handle_result(node.get_traces(payload.hash), v3::map_traces).await
}

pub async fn get_address_information_v3(
    State(node): State<Arc<LiteNode>>,
    Query(payload): Query<GetAddressInformationV3Request>,
) -> Json<Value> {
    let _use_v2 = payload.use_v2.unwrap_or(true);

    handle_result(
        node.get_address_information(payload.address, None),
        toncenter_v3::map_address_information,
    )
    .await
}

pub async fn emulate_trace_v1(State(node): State<Arc<LiteNode>>, body: Bytes) -> impl IntoResponse {
    let payload: EmulateTraceRequest = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(e) => return emulate_bad_request(format!("invalid request: {e}")),
    };

    let boc = payload.boc.unwrap_or_default();
    if boc.is_empty() {
        return emulate_bad_request("invalid request: boc is required");
    }

    if let Err(e) = base64::engine::general_purpose::STANDARD.decode(&boc) {
        return emulate_bad_request(format!("invalid request: invalid boc: {e}"));
    }

    let include_code_data = payload.include_code_data.unwrap_or(false);
    let include_address_book = payload.include_address_book.unwrap_or(false);
    let include_metadata = payload.include_metadata.unwrap_or(false);
    let with_actions = payload.with_actions.unwrap_or(false);

    if include_address_book || include_metadata {
        return emulate_bad_request("invalid request: address book and metadata are not available");
    }

    match node
        .emulate_trace(boc, payload.ignore_chksig, payload.mc_block_seqno)
        .await
    {
        Ok(trace) => {
            let response = v3::map_emulate_trace_response(
                &trace,
                with_actions,
                include_code_data,
                include_address_book,
                include_metadata,
            );
            (StatusCode::OK, Json(response))
        }
        Err(e) => emulate_internal_error(e.to_string()),
    }
}

pub async fn get_jetton_masters(
    State(node): State<Arc<LiteNode>>,
    Query(payload): Query<GetJettonMastersRequest>,
) -> Json<Value> {
    handle_result(
        node.get_jetton_masters(
            payload.address,
            payload.admin_address,
            payload.limit,
            payload.offset,
        ),
        v3::map_jetton_masters,
    )
    .await
}

pub async fn get_jetton_wallets(
    State(node): State<Arc<LiteNode>>,
    Query(payload): Query<GetJettonWalletsRequest>,
) -> Json<Value> {
    handle_result(
        node.get_jetton_wallets(
            payload.address,
            payload.owner_address,
            payload.jetton_address,
            payload.exclude_zero_balance,
            payload.limit,
            payload.offset,
        ),
        v3::map_jetton_wallets,
    )
    .await
}

pub async fn send_message_v3(
    State(node): State<Arc<LiteNode>>,
    Json(payload): Json<SendBocRequest>,
) -> Json<Value> {
    handle_result(node.send_boc(payload.boc), toncenter_v3::map_send_message).await
}

pub async fn run_get_method_v3(
    State(node): State<Arc<LiteNode>>,
    Json(payload): Json<RunGetMethodRequest>,
) -> Json<Value> {
    let method_str = match parse_method_name(&payload.method) {
        Ok(s) => s,
        Err(e) => {
            return Json(json!({
                "ok": false,
                "error": e.to_string(),
                "code": 400,
                "@extra": get_extra()
            }));
        }
    };

    let stack = match normalize_v3_stack(payload.stack) {
        Ok(stack) => stack,
        Err(e) => {
            return Json(json!({
                "ok": false,
                "error": e.to_string(),
                "code": 400,
                "@extra": get_extra()
            }));
        }
    };

    handle_result(
        node.run_get_method(payload.address, method_str, stack, payload.seqno),
        toncenter_v3::map_run_get_method_v3,
    )
    .await
}

fn normalize_v3_stack(stack: Vec<Value>) -> anyhow::Result<Vec<Value>> {
    stack.into_iter().map(normalize_v3_stack_item).collect()
}

fn normalize_v3_stack_item(item: Value) -> anyhow::Result<Value> {
    if item.is_array() {
        return Ok(item);
    }

    let stack_type = item
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("v3 stack entry must contain string `type`"))?;
    let value = item.get("value").cloned().unwrap_or(Value::Null);

    match stack_type {
        "null" => Ok(json!(["null", Value::Null])),
        "num" => Ok(json!(["num", value])),
        "cell" | "slice" | "builder" => {
            let bytes = extract_stack_bytes(&value, stack_type)?;
            Ok(json!([stack_type, { "bytes": bytes }]))
        }
        "tuple" | "list" => {
            let elements = value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("{stack_type} stack value must be an array"))?
                .iter()
                .cloned()
                .map(normalize_v3_stack_item)
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(json!([stack_type, { "elements": elements }]))
        }
        _ => anyhow::bail!("Unsupported v3 stack entry type: {stack_type}"),
    }
}

fn extract_stack_bytes(value: &Value, stack_type: &str) -> anyhow::Result<String> {
    if let Some(b64) = value.as_str() {
        return Ok(b64.to_owned());
    }
    if let Some(b64) = value.get("bytes").and_then(Value::as_str) {
        return Ok(b64.to_owned());
    }
    anyhow::bail!("{stack_type} stack value must be a base64 string or an object with `bytes`")
}

fn emulate_bad_request(error: impl Into<String>) -> (StatusCode, Json<Value>) {
    emulate_error_response(StatusCode::BAD_REQUEST, error)
}

fn emulate_internal_error(error: impl Into<String>) -> (StatusCode, Json<Value>) {
    emulate_error_response(StatusCode::INTERNAL_SERVER_ERROR, error)
}

fn emulate_error_response(
    status: StatusCode,
    error: impl Into<String>,
) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": error.into() })))
}
