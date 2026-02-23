use crate::context::Context;
use anyhow::bail;
use serde_json::Value;
use ton_emulator::{extension, register_ext_methods};
use ton_executor::BaseExecutor;
use tvmffi::stack::{Tuple, TupleItem};

extension!(json_query in (Context) with (query: String, json: String) using json_query_impl);
fn json_query_impl(
    _ctx: &mut Context,
    stack: &mut Tuple,
    query: String,
    json: String,
) -> anyhow::Result<()> {
    match select_values(&json, &query) {
        Ok(items) => {
            if let Some(value) = items.first() {
                stack.push_string(value);
            } else {
                stack.push(TupleItem::Null);
            }
        }
        Err(_) => stack.push(TupleItem::Null),
    }
    Ok(())
}

extension!(json_query_all in (Context) with (query: String, json: String) using json_query_all_impl);
fn json_query_all_impl(
    _ctx: &mut Context,
    stack: &mut Tuple,
    query: String,
    json: String,
) -> anyhow::Result<()> {
    match select_values(&json, &query) {
        Ok(items) => {
            let mut result = Tuple::empty();
            for value in items {
                result.push_string(&value);
            }
            stack.push(TupleItem::Tuple(result));
        }
        Err(_) => stack.push(TupleItem::Null),
    }
    Ok(())
}

fn select_values(json: &str, query: &str) -> anyhow::Result<Vec<String>> {
    let root: Value = serde_json::from_str(json)?;
    let jsonpath = normalize_query(query)?;
    let items = jsonpath_lib::select(&root, &jsonpath)?;
    Ok(items.into_iter().map(render_value).collect())
}

fn normalize_query(query: &str) -> anyhow::Result<String> {
    let query = query.trim();
    if query.is_empty() {
        bail!("query cannot be empty");
    }

    let mut query = if query == "." {
        "$".to_owned()
    } else if query.starts_with('$') {
        query.to_owned()
    } else if query.starts_with('.') || query.starts_with('[') {
        format!("${query}")
    } else {
        format!("$.{query}")
    };

    // Allow jq-like bracket field access after a dot: `.['foo']` -> `$['foo']`
    query = query.replace(".[", "[");

    // jq-like `[]` iterator becomes JSONPath wildcard `[*]`.
    let mut normalized = String::with_capacity(query.len());
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' && chars.peek() == Some(&']') {
            chars.next();
            normalized.push_str("[*]");
            continue;
        }
        normalized.push(ch);
    }

    Ok(normalized)
}

fn render_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

pub fn register_extensions<T: BaseExecutor>(executor: &mut T, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        57 => json_query : 2,
        58 => json_query_all : 2,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_jq_like_paths_to_jsonpath() {
        assert_eq!(
            normalize_query(".data.items[2].name").unwrap(),
            "$.data.items[2].name"
        );
        assert_eq!(normalize_query(".items[]").unwrap(), "$.items[*]");
        assert_eq!(
            normalize_query(".['weird.key'][\"inner-value\"]").unwrap(),
            "$['weird.key'][\"inner-value\"]"
        );
    }

    #[test]
    fn query_returns_scalars_and_structured_values() {
        let json = r#"{"name":"alice","age":42,"nested":{"ok":true}}"#;

        assert_eq!(
            select_values(json, ".name").unwrap(),
            vec!["alice".to_string()]
        );
        assert_eq!(select_values(json, ".age").unwrap(), vec!["42".to_string()]);
        assert_eq!(
            select_values(json, ".nested").unwrap(),
            vec![r#"{"ok":true}"#.to_string()]
        );
    }

    #[test]
    fn query_iterates_arrays() {
        let json = r#"{"items":[{"id":1},{"id":2}]}"#;
        assert_eq!(
            select_values(json, ".items[].id").unwrap(),
            vec!["1".to_string(), "2".to_string()]
        );
    }

    #[test]
    fn direct_jsonpath_is_supported() {
        let json = r#"{"items":[{"id":1},{"id":2}]}"#;
        assert_eq!(
            select_values(json, "$.items[*].id").unwrap(),
            vec!["1".to_string(), "2".to_string()]
        );
    }

    #[test]
    fn invalid_query_fails() {
        assert!(select_values(r#"{"a":1}"#, ".a[").is_err());
    }
}
