use crate::context::Context;
use num_bigint::BigInt;
use reqwest::Method;
use reqwest::blocking::Client;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;
use ton_emulator::{extension, register_ext_methods};
use ton_executor::BaseExecutor;
use tvmffi::stack::{Tuple, TupleItem};

extension!(fetch_request in (Context) with (headers: TupleItem, body: String, url: String, method: String) using fetch_request_impl);
fn fetch_request_impl(
    _ctx: &mut Context,
    stack: &mut Tuple,
    headers: TupleItem,
    body: String,
    url: String,
    method: String,
) -> anyhow::Result<()> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        fetch_request_value(headers, body, url, method)
    }));

    match result {
        Ok(Some(value)) => stack.push(value),
        Ok(None) | Err(_) => stack.push(TupleItem::Null),
    }

    Ok(())
}

fn fetch_request_value(
    headers: TupleItem,
    body: String,
    url: String,
    method: String,
) -> Option<TupleItem> {
    let method = match Method::from_bytes(method.as_bytes()) {
        Ok(method) => method,
        Err(_) => return None,
    };

    let headers = match parse_request_headers(headers) {
        Some(headers) => headers,
        None => return None,
    };

    let client = match Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(_) => return None,
    };

    let mut request = client.request(method, url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if !body.is_empty() {
        request = request.body(body);
    }

    let response = match request.send() {
        Ok(response) => response,
        Err(_) => return None,
    };

    let status = response.status();
    let headers = build_response_headers(response.headers());
    let body = match response.bytes() {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(_) => return None,
    };

    let mut result = Tuple::empty();
    result.push(TupleItem::Int(BigInt::from(status.as_u16())));
    result.push_bool(status.is_success());
    result.push_string(&body);
    result.push(TupleItem::Tuple(headers));
    Some(TupleItem::Tuple(result))
}

fn parse_request_headers(headers: TupleItem) -> Option<Vec<(String, String)>> {
    let headers = match headers {
        TupleItem::Null => return Some(vec![]),
        TupleItem::Tuple(headers) => headers,
        TupleItem::TypedTuple { inner, .. } => inner,
        _ => return None,
    };

    let mut out = Vec::with_capacity(headers.len());
    for header in headers.0 {
        let pair = match header {
            TupleItem::Tuple(pair) => pair,
            TupleItem::TypedTuple { inner, .. } => inner,
            _ => return None,
        };

        if pair.len() < 2 {
            return None;
        }

        let name = tuple_item_to_string(pair.first()?)?;
        let value = tuple_item_to_string(pair.get(1)?)?;
        out.push((name, value));
    }

    Some(out)
}

fn tuple_item_to_string(item: &TupleItem) -> Option<String> {
    match item {
        TupleItem::Cell(cell) | TupleItem::Slice(cell) => Tuple::parse_snake_string(cell),
        _ => None,
    }
}

fn build_response_headers(headers: &reqwest::header::HeaderMap) -> Tuple {
    let mut out = Tuple::empty();
    for (name, value) in headers {
        let value = value
            .to_str()
            .map(str::to_owned)
            .unwrap_or_else(|_| String::from_utf8_lossy(value.as_bytes()).to_string());

        let mut pair = Tuple::empty();
        pair.push_string(name.as_str());
        pair.push_string(&value);
        out.push(TupleItem::Tuple(pair));
    }
    out
}

pub fn register_extensions<T: BaseExecutor>(executor: &mut T, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        56 => fetch_request : 4,
    });
}
