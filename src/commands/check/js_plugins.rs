use acton_config::config::ActonConfig;
use rquickjs::{CatchResultExt, Context, Ctx, Function, Module, Promise, Runtime, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tolk_linter::Rule;
use tolk_linter::ast::js_plugin::JsPlugin;
use tolk_linter::diagnostic::{Annotation, Diagnostic, Severity};
use tolk_resolver::file_db::FileInfo;
use tolk_resolver::file_index::{FileId, Span, SymbolId};
use tolk_ty::{InferenceResult, TypeDb};
use tree_sitter::Node;

const QUICKJS_RUNNER_MODULE: &str = r#"
function comparePoints(left, right) {
  if (left.row !== right.row) {
    return left.row - right.row;
  }
  return left.column - right.column;
}

class TsNode {
  constructor(raw, source, resolveType, parent = null, childIndex = -1) {
    this._raw = raw;
    this._source = source;
    this._resolveType = resolveType;
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

  get inferredType() {
    return this._resolveType(this.startIndex, this.endIndex);
  }

  typeOf() {
    return this.inferredType;
  }

  get children() {
    if (this._children !== null) {
      return this._children;
    }

    const rawChildren = Array.isArray(this._raw.children) ? this._raw.children : [];
    this._children = rawChildren.map(
      (child, index) =>
        new TsNode(child, this._source, this._resolveType, this, index),
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
  constructor(rawRootNode, source, resolveType) {
    this.rootNode = new TsNode(rawRootNode, source, resolveType, null, -1);
  }
}

function buildTypeIndex(expressionTypes) {
  const map = new Map();
  if (!Array.isArray(expressionTypes)) {
    return map;
  }
  for (const item of expressionTypes) {
    if (!item || typeof item !== 'object') {
      continue;
    }
    if (
      typeof item.start !== 'number' ||
      typeof item.end !== 'number' ||
      typeof item.type !== 'string'
    ) {
      continue;
    }
    map.set(`${item.start}:${item.end}`, item.type);
  }
  return map;
}

function createPluginInput(payload) {
  const typeIndex = buildTypeIndex(payload.expressionTypes);
  const resolveType = (start, end) => typeIndex.get(`${start}:${end}`) ?? null;
  const tree = new TsTree(payload.cst, payload.source, resolveType);
  const typeOf = (target, maybeEnd) => {
    if (target && typeof target.startIndex === 'number' && typeof target.endIndex === 'number') {
      return resolveType(target.startIndex, target.endIndex);
    }
    if (target && typeof target.start === 'number' && typeof target.end === 'number') {
      return resolveType(target.start, target.end);
    }
    if (typeof target === 'number' && typeof maybeEnd === 'number') {
      return resolveType(target, maybeEnd);
    }
    return null;
  };
  return {
    ...payload,
    tree,
    rootNode: tree.rootNode,
    typeOf,
  };
}

function normalizeRules(rawRules) {
  const rules = {};
  if (Array.isArray(rawRules)) {
    for (const rule of rawRules) {
      if (!rule || typeof rule !== 'object') {
        continue;
      }
      const id = typeof rule.id === 'string' ? rule.id : null;
      if (!id) {
        continue;
      }
      const { id: _id, ...meta } = rule;
      rules[id] = meta ?? {};
    }
    return rules;
  }

  if (rawRules && typeof rawRules === 'object') {
    for (const [id, value] of Object.entries(rawRules)) {
      if (!id) {
        continue;
      }
      rules[id] = value && typeof value === 'object' ? value : {};
    }
  }
  return rules;
}

function normalizeRegistration(rawRegistration) {
  if (!rawRegistration || typeof rawRegistration !== 'object') {
    return null;
  }

  const out = {};
  if (typeof rawRegistration.name === 'string') {
    out.name = rawRegistration.name;
  }
  if (typeof rawRegistration.version === 'string') {
    out.version = rawRegistration.version;
  }
  if (typeof rawRegistration.description === 'string') {
    out.description = rawRegistration.description;
  }
  const rules = normalizeRules(rawRegistration.rules);
  if (Object.keys(rules).length > 0) {
    out.rules = rules;
  }
  return out;
}

function resolvePlugin(mod) {
  const exported = mod.default ?? mod;

  let runner = null;
  if (typeof exported === 'function') {
    runner = exported;
  } else if (exported && typeof exported.lint === 'function') {
    runner = exported.lint.bind(exported);
  } else if (typeof mod.lint === 'function') {
    runner = mod.lint.bind(mod);
  } else if (typeof mod.run === 'function') {
    runner = mod.run.bind(mod);
  }

  let register = null;
  if (exported && typeof exported.register === 'function') {
    register = exported.register.bind(exported);
  } else if (typeof mod.register === 'function') {
    register = mod.register.bind(mod);
  }

  const meta = exported?.meta ?? mod.meta ?? null;

  return {
    runner,
    register,
    meta,
  };
}

export async function runPlugin(mod, payload) {
  const pluginInput = createPluginInput(payload);
  const plugin = resolvePlugin(mod);
  let registration = normalizeRegistration(plugin.meta);

  if (plugin.register) {
    const declared = await plugin.register();
    const normalized = normalizeRegistration(declared);
    if (normalized) {
      registration = normalized;
    }
  }

  if (!plugin.runner) {
    throw new Error('Plugin must export a lint function or { lint() } object');
  }

  const diagnostics = await plugin.runner(pluginInput);
  if (diagnostics == null) {
    return { registration, diagnostics: [] };
  }

  if (!Array.isArray(diagnostics)) {
    throw new Error('Plugin result must be an array');
  }

  return { registration, diagnostics };
}
"#;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsPluginInput<'a> {
    file_path: &'a str,
    source: &'a str,
    cst: JsCstNode,
    expression_types: Vec<JsExpressionType>,
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsExpressionType {
    start: u32,
    end: u32,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsPluginOutput {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    rule_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    description: Option<String>,
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

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct JsPluginRunResult {
    #[serde(default)]
    registration: Option<JsPluginRegistration>,
    #[serde(default)]
    diagnostics: Vec<JsPluginOutput>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct JsPluginRegistration {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    rules: BTreeMap<String, JsPluginRuleRegistration>,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct JsPluginRuleRegistration {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    docs_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct JsPluginInvocationTiming {
    spawn: Duration,
    write_input: Duration,
    wait_for_exit: Duration,
    parse_output: Duration,
    total: Duration,
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

pub(super) fn run_plugins_for_file(
    file: &FileInfo,
    plugins: &[PathBuf],
    type_db: &TypeDb<'_>,
    body_types: &HashMap<FileId, HashMap<SymbolId, InferenceResult>>,
) -> Vec<Diagnostic> {
    let total_started = Instant::now();
    let source = file.source().source.as_ref();
    let file_path = file.path().to_string_lossy().to_string();

    let cst_started = Instant::now();
    let cst = build_cst(file.source().root_node());
    let cst_elapsed = cst_started.elapsed();

    let expr_types_started = Instant::now();
    let expression_types = collect_expression_types(file.id(), type_db, body_types);
    let expr_types_elapsed = expr_types_started.elapsed();
    let expression_types_count = expression_types.len();

    let input = JsPluginInput {
        file_path: &file_path,
        source,
        cst,
        expression_types,
    };
    let serialize_started = Instant::now();
    let payload = match serde_json::to_string(&input) {
        Ok(payload) => payload,
        Err(err) => {
            return vec![plugin_error(
                file.id(),
                format!("failed to serialize JS plugin input: {err}"),
            )];
        }
    };
    let serialize_elapsed = serialize_started.elapsed();
    let payload_prep_elapsed = cst_elapsed + expr_types_elapsed + serialize_elapsed;

    log::debug!(
        "js plugin payload for '{}' prepared in {:?} (cst={:?}, expression_types={:?}, serialize={:?}, expression_type_count={}, bytes={})",
        file_path,
        payload_prep_elapsed,
        cst_elapsed,
        expr_types_elapsed,
        serialize_elapsed,
        expression_types_count,
        payload.len(),
    );

    let mut diagnostics = Vec::new();
    let mut plugin_calls_elapsed = Duration::default();
    for plugin_path in plugins {
        let plugin_started = Instant::now();
        match run_single_plugin(plugin_path, &payload) {
            Ok((result, timing)) => {
                let registration = result.registration.as_ref();
                let plugin_name = plugin_display_name(plugin_path, registration);
                log::debug!(
                    "js plugin '{}' for '{}' took {:?} (spawn={:?}, write={:?}, wait={:?}, parse={:?}) and returned {} diagnostics",
                    plugin_name,
                    file_path,
                    timing.total,
                    timing.spawn,
                    timing.write_input,
                    timing.wait_for_exit,
                    timing.parse_output,
                    result.diagnostics.len(),
                );
                for output in result.diagnostics {
                    diagnostics.push(convert_output(
                        file.id(),
                        source,
                        &plugin_name,
                        registration,
                        output,
                    ));
                }
            }
            Err(err) => {
                let plugin_label = plugin_label(plugin_path);
                let plugin_elapsed = plugin_started.elapsed();
                log::debug!(
                    "js plugin '{}' for '{}' failed in {:?}: {}",
                    plugin_label,
                    file_path,
                    plugin_elapsed,
                    err,
                );
                diagnostics.push(plugin_error(
                    file.id(),
                    format!("failed to run JS plugin '{}': {err}", plugin_path.display()),
                ));
            }
        }
        plugin_calls_elapsed += plugin_started.elapsed();
    }

    log::debug!(
        "js plugins total for '{}': {:?} (prepare_payload={:?}, plugin_calls={:?}, plugin_count={})",
        file_path,
        total_started.elapsed(),
        payload_prep_elapsed,
        plugin_calls_elapsed,
        plugins.len(),
    );

    diagnostics
}

fn collect_expression_types(
    file_id: FileId,
    type_db: &TypeDb<'_>,
    body_types: &HashMap<FileId, HashMap<SymbolId, InferenceResult>>,
) -> Vec<JsExpressionType> {
    let Some(file_body_types) = body_types.get(&file_id) else {
        return vec![];
    };

    let mut by_span = BTreeMap::<(u32, u32), String>::new();
    for inference in file_body_types.values() {
        for (span, ty) in &inference.expression_types {
            by_span
                .entry((span.start, span.end))
                .or_insert_with(|| type_db.intrn.display(*ty).to_string());
        }
    }

    by_span
        .into_iter()
        .map(|((start, end), ty)| JsExpressionType { start, end, ty })
        .collect()
}

fn run_single_plugin(
    path: &Path,
    payload: &str,
) -> Result<(JsPluginRunResult, JsPluginInvocationTiming), String> {
    let total_started = Instant::now();
    let spawn_started = Instant::now();
    let runtime = Runtime::new().map_err(|err| format!("cannot create QuickJS runtime: {err}"))?;
    let context =
        Context::full(&runtime).map_err(|err| format!("cannot create QuickJS context: {err}"))?;
    let spawn_elapsed = spawn_started.elapsed();

    let write_started = Instant::now();
    let plugin_source = fs::read(path)
        .map_err(|err| format!("cannot read JS plugin '{}': {err}", path.display()))?;
    let write_elapsed = write_started.elapsed();

    let wait_started = Instant::now();
    let output_json = context
        .with(|ctx| run_plugin_in_quickjs(ctx, path, &plugin_source, payload))
        .map_err(|err| format!("QuickJS plugin execution failed: {err}"))?;
    let wait_elapsed = wait_started.elapsed();

    let parse_started = Instant::now();
    let parsed = serde_json::from_str::<JsPluginRunResult>(&output_json)
        .map_err(|err| format!("invalid plugin JSON output: {err}"))?;
    let parse_elapsed = parse_started.elapsed();
    let total_elapsed = total_started.elapsed();

    Ok((
        parsed,
        JsPluginInvocationTiming {
            spawn: spawn_elapsed,
            write_input: write_elapsed,
            wait_for_exit: wait_elapsed,
            parse_output: parse_elapsed,
            total: total_elapsed,
        },
    ))
}

fn run_plugin_in_quickjs(
    ctx: Ctx<'_>,
    plugin_path: &Path,
    plugin_source: &[u8],
    payload: &str,
) -> Result<String, String> {
    let helper_module = Module::declare(
        ctx.clone(),
        "__acton_js_plugin_runtime__.mjs",
        QUICKJS_RUNNER_MODULE,
    )
    .catch(&ctx)
    .map_err(|err| err.to_string())?;
    let (helper_module, helper_eval) = helper_module
        .eval()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    helper_eval
        .finish::<()>()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let helper_namespace = helper_module
        .namespace()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let run_plugin: Function<'_> = helper_namespace
        .get("runPlugin")
        .catch(&ctx)
        .map_err(|err| err.to_string())?;

    let plugin_name = plugin_path.to_string_lossy().to_string();
    let plugin_module = Module::declare(ctx.clone(), plugin_name, plugin_source.to_vec())
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let (plugin_module, plugin_eval) = plugin_module
        .eval()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    plugin_eval
        .finish::<()>()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let plugin_namespace = plugin_module
        .namespace()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;

    let payload_value = ctx
        .json_parse(payload.as_bytes().to_vec())
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let execution: Promise<'_> = run_plugin
        .call((plugin_namespace, payload_value))
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let output_value: Value<'_> = execution
        .finish()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;

    let Some(serialized) = ctx
        .json_stringify(output_value)
        .catch(&ctx)
        .map_err(|err| err.to_string())?
    else {
        return Err("plugin returned a non-serializable value".to_string());
    };
    serialized
        .to_string()
        .map_err(|err| format!("failed to read plugin output string: {err}"))
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
    file_id: FileId,
    source: &str,
    plugin_name: &str,
    registration: Option<&JsPluginRegistration>,
    output: JsPluginOutput,
) -> Diagnostic {
    let rule_registration = find_rule_registration(registration, output.rule_id.as_deref());
    let default_severity = parse_severity(
        rule_registration.and_then(|rule| rule.severity.as_deref()),
        Severity::Warning,
    );

    let mut diagnostic = Diagnostic::warning_for(file_id, JsPlugin);
    diagnostic.rule = Rule::JsPlugin;
    diagnostic.message = compose_message(plugin_name, &output, rule_registration);
    diagnostic.severity = parse_severity(output.severity.as_deref(), default_severity);
    if let Some(code) = output
        .code
        .as_deref()
        .map(ToOwned::to_owned)
        .or_else(|| rule_registration.and_then(|rule| rule.code.clone()))
    {
        diagnostic.code = Some(code);
    }

    let span = parse_span(source, &output);
    diagnostic.help = compose_help(registration, &output, rule_registration);
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

fn find_rule_registration<'a>(
    registration: Option<&'a JsPluginRegistration>,
    rule_id: Option<&str>,
) -> Option<&'a JsPluginRuleRegistration> {
    let rule_id = rule_id?;
    registration?.rules.get(rule_id)
}

fn parse_severity(input: Option<&str>, default: Severity) -> Severity {
    match input {
        Some("error") => Severity::Error,
        Some("info") => Severity::Info,
        Some("warning") | Some("warn") => Severity::Warning,
        Some("help") => Severity::Help,
        Some("fatal") => Severity::Fatal,
        Some(_) | None => default,
    }
}

fn compose_message(
    plugin_name: &str,
    output: &JsPluginOutput,
    rule_registration: Option<&JsPluginRuleRegistration>,
) -> String {
    let raw = if let Some(message) = output.message.as_deref() {
        message.to_owned()
    } else if let Some(title) = rule_registration.and_then(|rule| rule.title.as_deref()) {
        title.to_owned()
    } else if let Some(rule_id) = output.rule_id.as_deref() {
        format!("rule `{rule_id}` emitted a diagnostic")
    } else {
        "diagnostic emitted by JavaScript plugin".to_string()
    };
    format!("[{plugin_name}] {raw}")
}

fn compose_help(
    registration: Option<&JsPluginRegistration>,
    output: &JsPluginOutput,
    rule_registration: Option<&JsPluginRuleRegistration>,
) -> Option<String> {
    if let Some(help) = &output.help {
        return Some(help.clone());
    }

    let mut lines = Vec::new();

    if let Some(description) = output
        .description
        .as_ref()
        .or_else(|| rule_registration.and_then(|rule| rule.description.as_ref()))
        .or_else(|| registration.and_then(|meta| meta.description.as_ref()))
    {
        lines.push(description.clone());
    }

    if let Some(help) = rule_registration.and_then(|rule| rule.help.as_ref()) {
        lines.push(help.clone());
    }

    if let Some(docs_url) = rule_registration.and_then(|rule| rule.docs_url.as_ref()) {
        lines.push(format!("Docs: {docs_url}"));
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
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

fn plugin_error(file_id: FileId, message: String) -> Diagnostic {
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

fn plugin_display_name(path: &Path, registration: Option<&JsPluginRegistration>) -> String {
    let fallback = plugin_label(path);
    let Some(meta) = registration else {
        return fallback;
    };
    let Some(name) = meta.name.as_deref() else {
        return fallback;
    };
    if let Some(version) = meta.version.as_deref() {
        format!("{name}@{version}")
    } else {
        name.to_owned()
    }
}
