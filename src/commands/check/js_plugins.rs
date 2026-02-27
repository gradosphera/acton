use acton_config::config::ActonConfig;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tolk_linter::Rule;
use tolk_linter::ast::js_plugin::JsPlugin;
use tolk_linter::diagnostic::{Annotation, Diagnostic, Severity};
use tolk_resolver::file_db::FileInfo;
use tolk_resolver::file_index::Span;
use tree_sitter::Node;

const NODE_RUNNER: &str = r#"
const fs = require('node:fs');
const { pathToFileURL } = require('node:url');

function comparePoints(left, right) {
  if (left.row !== right.row) {
    return left.row - right.row;
  }
  return left.column - right.column;
}

class TsNode {
  constructor(raw, source, parent = null, childIndex = -1) {
    this._raw = raw;
    this._source = source;
    this._parent = parent;
    this._childIndex = childIndex;
    this._children = null;
    this._namedChildren = null;
  }

  get type() {
    return this._raw.kind;
  }

  get kind() {
    return this._raw.kind;
  }

  get isNamed() {
    return this._raw.named;
  }

  get hasError() {
    return this._raw.hasError;
  }

  get isError() {
    return this._raw.isError;
  }

  get isMissing() {
    return this._raw.isMissing;
  }

  get startIndex() {
    return this._raw.startByte;
  }

  get endIndex() {
    return this._raw.endByte;
  }

  get startByte() {
    return this._raw.startByte;
  }

  get endByte() {
    return this._raw.endByte;
  }

  get startPosition() {
    return this._raw.startPosition;
  }

  get endPosition() {
    return this._raw.endPosition;
  }

  get text() {
    return this._source.slice(this.startIndex, this.endIndex);
  }

  get parent() {
    return this._parent;
  }

  get fieldName() {
    return this._raw.fieldName ?? null;
  }

  get children() {
    if (this._children !== null) {
      return this._children;
    }

    const rawChildren = Array.isArray(this._raw.children) ? this._raw.children : [];
    this._children = rawChildren.map(
      (child, index) => new TsNode(child, this._source, this, index),
    );
    return this._children;
  }

  get namedChildren() {
    if (this._namedChildren !== null) {
      return this._namedChildren;
    }
    this._namedChildren = this.children.filter((child) => child.isNamed);
    return this._namedChildren;
  }

  get childCount() {
    return this.children.length;
  }

  get namedChildCount() {
    return this.namedChildren.length;
  }

  child(index) {
    if (!Number.isInteger(index) || index < 0) {
      return null;
    }
    return this.children[index] ?? null;
  }

  namedChild(index) {
    if (!Number.isInteger(index) || index < 0) {
      return null;
    }
    return this.namedChildren[index] ?? null;
  }

  childForFieldName(fieldName) {
    if (typeof fieldName !== 'string') {
      return null;
    }
    for (const child of this.children) {
      if (child.fieldName === fieldName) {
        return child;
      }
    }
    return null;
  }

  childrenForFieldName(fieldName) {
    if (typeof fieldName !== 'string') {
      return [];
    }
    return this.children.filter((child) => child.fieldName === fieldName);
  }

  get firstChild() {
    return this.child(0);
  }

  get lastChild() {
    return this.child(this.childCount - 1);
  }

  get firstNamedChild() {
    return this.namedChild(0);
  }

  get lastNamedChild() {
    return this.namedChild(this.namedChildCount - 1);
  }

  get nextSibling() {
    if (this._parent === null || this._childIndex < 0) {
      return null;
    }
    return this._parent.child(this._childIndex + 1);
  }

  get previousSibling() {
    if (this._parent === null || this._childIndex < 0) {
      return null;
    }
    return this._parent.child(this._childIndex - 1);
  }

  get nextNamedSibling() {
    let sibling = this.nextSibling;
    while (sibling !== null && !sibling.isNamed) {
      sibling = sibling.nextSibling;
    }
    return sibling;
  }

  get previousNamedSibling() {
    let sibling = this.previousSibling;
    while (sibling !== null && !sibling.isNamed) {
      sibling = sibling.previousSibling;
    }
    return sibling;
  }

  descendantForIndex(startIndex, endIndex = startIndex) {
    if (
      !Number.isInteger(startIndex) ||
      !Number.isInteger(endIndex) ||
      startIndex > endIndex
    ) {
      return null;
    }
    if (startIndex < this.startIndex || endIndex > this.endIndex) {
      return null;
    }

    let current = this;
    while (true) {
      let next = null;
      for (const child of current.children) {
        if (child.startIndex <= startIndex && endIndex <= child.endIndex) {
          next = child;
          break;
        }
      }
      if (next === null) {
        return current;
      }
      current = next;
    }
  }

  namedDescendantForIndex(startIndex, endIndex = startIndex) {
    const node = this.descendantForIndex(startIndex, endIndex);
    if (node === null) {
      return null;
    }
    let current = node;
    while (current !== null && !current.isNamed) {
      current = current.parent;
    }
    return current;
  }

  descendantForPosition(startPosition, endPosition = startPosition) {
    if (!startPosition || !endPosition) {
      return null;
    }
    if (
      typeof startPosition.row !== 'number' ||
      typeof startPosition.column !== 'number' ||
      typeof endPosition.row !== 'number' ||
      typeof endPosition.column !== 'number'
    ) {
      return null;
    }
    if (comparePoints(startPosition, endPosition) > 0) {
      return null;
    }
    if (comparePoints(startPosition, this.startPosition) < 0) {
      return null;
    }
    if (comparePoints(endPosition, this.endPosition) > 0) {
      return null;
    }

    let current = this;
    while (true) {
      let next = null;
      for (const child of current.children) {
        if (
          comparePoints(child.startPosition, startPosition) <= 0 &&
          comparePoints(endPosition, child.endPosition) <= 0
        ) {
          next = child;
          break;
        }
      }
      if (next === null) {
        return current;
      }
      current = next;
    }
  }

  namedDescendantForPosition(startPosition, endPosition = startPosition) {
    const node = this.descendantForPosition(startPosition, endPosition);
    if (node === null) {
      return null;
    }
    let current = node;
    while (current !== null && !current.isNamed) {
      current = current.parent;
    }
    return current;
  }

  descendantsOfType(types, startPosition, endPosition) {
    const typeList = Array.isArray(types) ? types : [types];
    const acceptedTypes = new Set(
      typeList.filter((value) => typeof value === 'string'),
    );
    if (acceptedTypes.size === 0) {
      return [];
    }

    const hasRange =
      startPosition &&
      endPosition &&
      typeof startPosition.row === 'number' &&
      typeof startPosition.column === 'number' &&
      typeof endPosition.row === 'number' &&
      typeof endPosition.column === 'number';

    const inRange = (node) => {
      if (!hasRange) {
        return true;
      }
      return (
        comparePoints(startPosition, node.startPosition) <= 0 &&
        comparePoints(node.endPosition, endPosition) <= 0
      );
    };

    const out = [];
    const visit = (node) => {
      if (acceptedTypes.has(node.type) && inRange(node)) {
        out.push(node);
      }
      for (const child of node.children) {
        visit(child);
      }
    };

    for (const child of this.children) {
      visit(child);
    }

    return out;
  }
}

class TsTree {
  constructor(rawRootNode, source) {
    this.rootNode = new TsNode(rawRootNode, source, null, -1);
  }
}

function createPluginInput(payload) {
  const tree = new TsTree(payload.cst, payload.source);
  return {
    ...payload,
    tree,
    rootNode: tree.rootNode,
  };
}

async function main() {
  const pluginPath = process.argv[1];
  if (!pluginPath) {
    throw new Error('Missing plugin path');
  }

  const input = fs.readFileSync(0, 'utf8');
  const payload = JSON.parse(input);
  const pluginInput = createPluginInput(payload);

  const mod = await import(pathToFileURL(pluginPath).href);
  const candidate = mod.default ?? mod.run ?? mod;

  let runner = null;
  if (typeof candidate === 'function') {
    runner = candidate;
  } else if (candidate && typeof candidate.lint === 'function') {
    runner = candidate.lint;
  }

  if (!runner) {
    throw new Error('Plugin must export default function or { lint() }');
  }

  const diagnostics = await runner(pluginInput);
  if (diagnostics == null) {
    process.stdout.write('[]');
    return;
  }

  if (!Array.isArray(diagnostics)) {
    throw new Error('Plugin result must be an array');
  }

  process.stdout.write(JSON.stringify(diagnostics));
}

main().catch((err) => {
  const text = err && err.stack ? err.stack : String(err);
  process.stderr.write(text);
  process.exit(1);
});
"#;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsPluginInput<'a> {
    file_path: &'a str,
    source: &'a str,
    cst: JsCstNode,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsCstNode {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    field_name: Option<String>,
    named: bool,
    start_byte: u32,
    end_byte: u32,
    start_position: JsPoint,
    end_position: JsPoint,
    has_error: bool,
    is_error: bool,
    is_missing: bool,
    children: Vec<JsCstNode>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsPoint {
    row: u32,
    column: u32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPluginOutput {
    message: String,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    start: Option<u32>,
    #[serde(default)]
    end: Option<u32>,
    #[serde(default)]
    span: Option<JsPluginSpan>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPluginSpan {
    start: u32,
    end: u32,
}

pub(super) fn resolve_plugins(
    project_root: &Path,
    config: &ActonConfig,
) -> anyhow::Result<Vec<PathBuf>> {
    let Some(plugins) = config
        .lint
        .as_ref()
        .and_then(|lint| lint.js_plugins.as_ref())
    else {
        return Ok(vec![]);
    };

    let mut resolved = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let plugin_path = PathBuf::from(plugin);
        let absolute = if plugin_path.is_absolute() {
            plugin_path
        } else {
            project_root.join(plugin_path)
        };
        let canonical = dunce::canonicalize(&absolute).map_err(|err| {
            anyhow::anyhow!(
                "failed to resolve lint JS plugin path '{}': {err}",
                absolute.display()
            )
        })?;
        resolved.push(canonical);
    }
    Ok(resolved)
}

pub(super) fn run_plugins_for_file(file: &FileInfo, plugins: &[PathBuf]) -> Vec<Diagnostic> {
    let source = file.source().source.as_ref();
    let file_path = file.path().to_string_lossy().to_string();
    let cst = build_cst(file.source().root_node());
    let input = JsPluginInput {
        file_path: &file_path,
        source,
        cst,
    };
    let payload = match serde_json::to_string(&input) {
        Ok(payload) => payload,
        Err(err) => {
            return vec![plugin_error(
                file.id(),
                format!("failed to serialize JS plugin input: {err}"),
            )];
        }
    };

    let mut diagnostics = Vec::new();
    for plugin_path in plugins {
        match run_single_plugin(plugin_path, &payload) {
            Ok(outputs) => {
                let plugin_name = plugin_label(plugin_path);
                for output in outputs {
                    diagnostics.push(convert_output(file.id(), source, &plugin_name, output));
                }
            }
            Err(err) => diagnostics.push(plugin_error(
                file.id(),
                format!("failed to run JS plugin '{}': {err}", plugin_path.display()),
            )),
        }
    }
    diagnostics
}

fn run_single_plugin(path: &Path, payload: &str) -> Result<Vec<JsPluginOutput>, String> {
    let mut child = Command::new("node")
        .arg("-e")
        .arg(NODE_RUNNER)
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("cannot start node process: {err}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|err| format!("cannot write plugin input: {err}"))?;
    } else {
        return Err("node stdin is not available".to_string());
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("cannot wait for node process: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if stderr.is_empty() { stdout } else { stderr };
        return Err(if details.is_empty() {
            format!("node exited with status {}", output.status)
        } else {
            details
        });
    }

    serde_json::from_slice::<Vec<JsPluginOutput>>(&output.stdout)
        .map_err(|err| format!("invalid plugin JSON output: {err}"))
}

fn build_cst(node: Node<'_>) -> JsCstNode {
    build_cst_with_field(node, None)
}

fn build_cst_with_field(node: Node<'_>, field_name: Option<String>) -> JsCstNode {
    let mut children = Vec::new();
    for idx in 0..node.child_count() {
        if let Some(child) = node.child(idx) {
            let child_field = node.field_name_for_child(idx as u32).map(ToOwned::to_owned);
            children.push(build_cst_with_field(child, child_field));
        }
    }

    let start = node.start_position();
    let end = node.end_position();
    JsCstNode {
        kind: node.kind().to_string(),
        field_name,
        named: node.is_named(),
        start_byte: node.start_byte() as u32,
        end_byte: node.end_byte() as u32,
        start_position: JsPoint {
            row: start.row as u32,
            column: start.column as u32,
        },
        end_position: JsPoint {
            row: end.row as u32,
            column: end.column as u32,
        },
        has_error: node.has_error(),
        is_error: node.is_error(),
        is_missing: node.is_missing(),
        children,
    }
}

fn convert_output(
    file_id: tolk_resolver::FileId,
    source: &str,
    plugin_name: &str,
    output: JsPluginOutput,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::warning_for(file_id, JsPlugin);
    diagnostic.rule = Rule::JsPlugin;
    diagnostic.message = format!("[{plugin_name}] {}", output.message);
    diagnostic.severity = match output.severity.as_deref() {
        Some("error") => Severity::Error,
        Some("info") => Severity::Info,
        Some("warning") | Some("warn") | None => Severity::Warning,
        Some("help") => Severity::Help,
        Some("fatal") => Severity::Fatal,
        Some(_) => Severity::Warning,
    };

    let span = parse_span(source, &output);
    diagnostic.help = output.help;
    if let Some(span) = span {
        diagnostic.annotations.push(Annotation {
            span,
            message: None,
            is_primary: true,
            tags: vec![],
        });
    }
    diagnostic
}

fn parse_span(source: &str, output: &JsPluginOutput) -> Option<Span> {
    let source_len = source.len() as u32;
    let (start, end) = if let Some(span) = &output.span {
        (span.start, span.end)
    } else {
        (output.start?, output.end?)
    };
    if start > end || end > source_len {
        return None;
    }
    Some(Span { start, end })
}

fn plugin_error(file_id: tolk_resolver::FileId, message: String) -> Diagnostic {
    let mut diagnostic = Diagnostic::error_for(file_id, JsPlugin);
    diagnostic.rule = Rule::JsPlugin;
    diagnostic.message = message;
    diagnostic
}

fn plugin_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
