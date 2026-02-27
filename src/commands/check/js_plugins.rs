use acton_config::config::ActonConfig;
use rquickjs::{
    Array, CatchResultExt, Context, Ctx, FromJs, Function, Module, Object, Promise, Runtime, Value,
};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

function toPoint(value) {
  if (!value || typeof value !== 'object') {
    return null;
  }
  if (typeof value.row !== 'number' || typeof value.column !== 'number') {
    return null;
  }
  return value;
}

function normalizeTypes(types) {
  const values = Array.isArray(types) ? types : [types];
  return values.filter((value) => typeof value === 'string');
}

class TsNode {
  constructor(id, tree, parent = null, childIndex = -1) {
    this.__id = id;
    this._tree = tree;
    this._parent = parent;
    this._childIndex = childIndex;
    this._infoCache = null;
    this._textCache = null;
    this._childrenCache = null;
    this._namedChildrenCache = null;
  }

  _info() {
    if (this._infoCache === null) {
      this._infoCache = this._tree._bridge.nodeInfo(this.__id);
    }
    return this._infoCache;
  }

  _ensureParent() {
    if (this._parent !== null) {
      return;
    }
    const parentId = this._info().parentId;
    if (typeof parentId === 'number') {
      this._parent = this._tree._nodeFromId(parentId);
    }
  }

  _ensureChildIndex() {
    if (this._childIndex >= 0) {
      return;
    }
    this._ensureParent();
    if (this._parent === null) {
      return;
    }
    const siblings = this._parent.children;
    for (let i = 0; i < siblings.length; i += 1) {
      if (siblings[i].__id === this.__id) {
        this._childIndex = i;
        return;
      }
    }
  }

  get type() {
    return this._info().kind;
  }

  get kind() {
    return this._info().kind;
  }

  get isNamed() {
    return this._info().named;
  }

  get hasError() {
    return this._info().hasError;
  }

  get isError() {
    return this._info().isError;
  }

  get isMissing() {
    return this._info().isMissing;
  }

  get startIndex() {
    return this._info().startByte;
  }

  get endIndex() {
    return this._info().endByte;
  }

  get startByte() {
    return this._info().startByte;
  }

  get endByte() {
    return this._info().endByte;
  }

  get startPosition() {
    const info = this._info();
    return {
      row: info.startRow,
      column: info.startColumn,
    };
  }

  get endPosition() {
    const info = this._info();
    return {
      row: info.endRow,
      column: info.endColumn,
    };
  }

  get text() {
    if (this._textCache === null) {
      this._textCache = this._tree._bridge.nodeText(this.__id);
    }
    return this._textCache;
  }

  get parent() {
    this._ensureParent();
    return this._parent;
  }

  get fieldName() {
    return this._info().fieldName ?? null;
  }

  get inferredType() {
    return this._tree._bridge.typeOfNode(this.__id);
  }

  typeOf() {
    return this.inferredType;
  }

  get children() {
    if (this._childrenCache !== null) {
      return this._childrenCache;
    }
    const ids = this._tree._bridge.childIds(this.__id);
    this._childrenCache = ids.map((id, index) =>
      this._tree._nodeFromId(id, this, index),
    );
    return this._childrenCache;
  }

  get namedChildren() {
    if (this._namedChildrenCache !== null) {
      return this._namedChildrenCache;
    }
    const ids = this._tree._bridge.namedChildIds(this.__id);
    this._namedChildrenCache = ids.map((id) => this._tree._nodeFromId(id));
    return this._namedChildrenCache;
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
    const id = this._tree._bridge.childForFieldName(this.__id, fieldName);
    return typeof id === 'number' ? this._tree._nodeFromId(id) : null;
  }

  childrenForFieldName(fieldName) {
    if (typeof fieldName !== 'string') {
      return [];
    }
    const ids = this._tree._bridge.childrenForFieldName(this.__id, fieldName);
    return ids.map((id) => this._tree._nodeFromId(id));
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
    this._ensureChildIndex();
    if (this.parent === null || this._childIndex < 0) {
      return null;
    }
    return this.parent.child(this._childIndex + 1);
  }

  get previousSibling() {
    this._ensureChildIndex();
    if (this.parent === null || this._childIndex < 0) {
      return null;
    }
    return this.parent.child(this._childIndex - 1);
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
    const id = this._tree._bridge.descendantForIndex(
      this.__id,
      startIndex,
      endIndex,
      false,
    );
    return typeof id === 'number' ? this._tree._nodeFromId(id) : null;
  }

  namedDescendantForIndex(startIndex, endIndex = startIndex) {
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
    const id = this._tree._bridge.descendantForIndex(
      this.__id,
      startIndex,
      endIndex,
      true,
    );
    return typeof id === 'number' ? this._tree._nodeFromId(id) : null;
  }

  descendantForPosition(startPosition, endPosition = startPosition) {
    const start = toPoint(startPosition);
    const end = toPoint(endPosition);
    if (start === null || end === null) {
      return null;
    }
    if (comparePoints(start, end) > 0) {
      return null;
    }
    if (comparePoints(start, this.startPosition) < 0) {
      return null;
    }
    if (comparePoints(end, this.endPosition) > 0) {
      return null;
    }
    const id = this._tree._bridge.descendantForPosition(
      this.__id,
      start.row,
      start.column,
      end.row,
      end.column,
      false,
    );
    return typeof id === 'number' ? this._tree._nodeFromId(id) : null;
  }

  namedDescendantForPosition(startPosition, endPosition = startPosition) {
    const start = toPoint(startPosition);
    const end = toPoint(endPosition);
    if (start === null || end === null) {
      return null;
    }
    if (comparePoints(start, end) > 0) {
      return null;
    }
    if (comparePoints(start, this.startPosition) < 0) {
      return null;
    }
    if (comparePoints(end, this.endPosition) > 0) {
      return null;
    }
    const id = this._tree._bridge.descendantForPosition(
      this.__id,
      start.row,
      start.column,
      end.row,
      end.column,
      true,
    );
    return typeof id === 'number' ? this._tree._nodeFromId(id) : null;
  }

  descendantsOfType(types, startPosition, endPosition) {
    const normalizedTypes = normalizeTypes(types);
    if (normalizedTypes.length === 0) {
      return [];
    }

    let startRow = -1;
    let startColumn = -1;
    let endRow = -1;
    let endColumn = -1;
    if (startPosition !== undefined && endPosition !== undefined) {
      const start = toPoint(startPosition);
      const end = toPoint(endPosition);
      if (start === null || end === null) {
        return [];
      }
      if (comparePoints(start, end) > 0) {
        return [];
      }
      startRow = start.row;
      startColumn = start.column;
      endRow = end.row;
      endColumn = end.column;
    }

    const ids = this._tree._bridge.descendantsOfType(
      this.__id,
      normalizedTypes,
      startRow,
      startColumn,
      endRow,
      endColumn,
    );
    return ids.map((id) => this._tree._nodeFromId(id));
  }
}

class TsTree {
  constructor(bridge) {
    this._bridge = bridge;
    this._cache = new Map();
    this.rootNode = this._nodeFromId(bridge.rootId(), null, -1);
  }

  _nodeFromId(id, parent = null, childIndex = -1) {
    if (typeof id !== 'number') {
      return null;
    }
    const cached = this._cache.get(id);
    if (cached) {
      if (parent !== null && cached._parent === null) {
        cached._parent = parent;
      }
      if (childIndex >= 0 && cached._childIndex < 0) {
        cached._childIndex = childIndex;
      }
      return cached;
    }
    const node = new TsNode(id, this, parent, childIndex);
    this._cache.set(id, node);
    return node;
  }
}

function createPluginInput(payload) {
  const tree = new TsTree(payload.bridge);
  const typeOf = (target, maybeEnd) => {
    if (target && typeof target.typeOf === 'function') {
      return target.typeOf();
    }
    if (target && typeof target.__id === 'number') {
      return payload.bridge.typeOfNode(target.__id);
    }
    if (target && typeof target.start === 'number' && typeof target.end === 'number') {
      return payload.bridge.typeOfSpan(target.start, target.end);
    }
    if (typeof target === 'number' && typeof maybeEnd === 'number') {
      return payload.bridge.typeOfSpan(target, maybeEnd);
    }
    return null;
  };

  const out = {
    filePath: payload.filePath,
    source: payload.source,
    tree,
    rootNode: tree.rootNode,
    typeOf,
  };

  Object.defineProperty(out, 'cst', {
    configurable: true,
    enumerable: true,
    get() {
      const cst = payload.bridge.rawCst();
      Object.defineProperty(out, 'cst', {
        value: cst,
        enumerable: true,
      });
      return cst;
    },
  });

  Object.defineProperty(out, 'expressionTypes', {
    configurable: true,
    enumerable: true,
    get() {
      const expressionTypes = payload.bridge.rawExpressionTypes();
      Object.defineProperty(out, 'expressionTypes', {
        value: expressionTypes,
        enumerable: true,
      });
      return expressionTypes;
    },
  });

  return out;
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

#[derive(Clone)]
struct CstBridgeNode {
    kind: String,
    field_name: Option<String>,
    named: bool,
    start_byte: u32,
    end_byte: u32,
    start_row: u32,
    start_column: u32,
    end_row: u32,
    end_column: u32,
    has_error: bool,
    is_error: bool,
    is_missing: bool,
    parent_id: Option<u32>,
    child_ids: Vec<u32>,
    named_child_ids: Vec<u32>,
    children_by_field: HashMap<String, Vec<u32>>,
}

struct CstBridgeData {
    source: String,
    nodes: Vec<CstBridgeNode>,
    root_id: u32,
    type_by_span: HashMap<(u32, u32), String>,
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

fn get_optional_property<'js, T>(object: &Object<'js>, key: &str) -> rquickjs::Result<Option<T>>
where
    T: FromJs<'js>,
{
    let value: Value<'js> = object.get(key)?;
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        T::from_js(object.ctx(), value).map(Some)
    }
}

impl<'js> FromJs<'js> for JsPluginSpan {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let object = Object::from_value(value)?;
        Ok(Self {
            start: object.get("start")?,
            end: object.get("end")?,
        })
    }
}

impl<'js> FromJs<'js> for JsPluginRuleRegistration {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let object = Object::from_value(value)?;
        Ok(Self {
            code: get_optional_property(&object, "code")?,
            title: get_optional_property(&object, "title")?,
            description: get_optional_property(&object, "description")?,
            help: get_optional_property(&object, "help")?,
            severity: get_optional_property(&object, "severity")?,
            docs_url: get_optional_property(&object, "docsUrl")?,
        })
    }
}

impl<'js> FromJs<'js> for JsPluginRegistration {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let object = Object::from_value(value)?;
        Ok(Self {
            name: get_optional_property(&object, "name")?,
            version: get_optional_property(&object, "version")?,
            description: get_optional_property(&object, "description")?,
            rules: get_optional_property(&object, "rules")?.unwrap_or_default(),
        })
    }
}

impl<'js> FromJs<'js> for JsPluginOutput {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let object = Object::from_value(value)?;
        Ok(Self {
            message: get_optional_property(&object, "message")?,
            rule_id: get_optional_property(&object, "ruleId")?,
            code: get_optional_property(&object, "code")?,
            severity: get_optional_property(&object, "severity")?,
            help: get_optional_property(&object, "help")?,
            description: get_optional_property(&object, "description")?,
            start: get_optional_property(&object, "start")?,
            end: get_optional_property(&object, "end")?,
            span: get_optional_property(&object, "span")?,
        })
    }
}

impl<'js> FromJs<'js> for JsPluginRunResult {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let object = Object::from_value(value)?;
        Ok(Self {
            registration: get_optional_property(&object, "registration")?,
            diagnostics: get_optional_property(&object, "diagnostics")?.unwrap_or_default(),
        })
    }
}

fn invalid_node_id(node_id: u32) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message(
        "number",
        "nodeId",
        format!("unknown CST node id {node_id}"),
    )
}

fn compare_rows_columns(lhs: (u32, u32), rhs: (u32, u32)) -> i32 {
    if lhs.0 < rhs.0 {
        -1
    } else if lhs.0 > rhs.0 {
        1
    } else if lhs.1 < rhs.1 {
        -1
    } else if lhs.1 > rhs.1 {
        1
    } else {
        0
    }
}

impl CstBridgeData {
    fn node(&self, node_id: u32) -> Option<&CstBridgeNode> {
        self.nodes.get(node_id as usize)
    }

    fn child_for_field_name(&self, node_id: u32, field_name: &str) -> Option<u32> {
        let node = self.node(node_id)?;
        node.children_by_field
            .get(field_name)
            .and_then(|ids| ids.first().copied())
    }

    fn children_for_field_name(&self, node_id: u32, field_name: &str) -> Vec<u32> {
        let Some(node) = self.node(node_id) else {
            return vec![];
        };
        node.children_by_field
            .get(field_name)
            .cloned()
            .unwrap_or_default()
    }

    fn descendant_for_index(
        &self,
        node_id: u32,
        start_index: u32,
        end_index: u32,
        named_only: bool,
    ) -> Option<u32> {
        let root = self.node(node_id)?;
        if start_index > end_index || start_index < root.start_byte || end_index > root.end_byte {
            return None;
        }

        let mut current_id = node_id;
        loop {
            let current = self.node(current_id)?;
            let mut next_id = None;
            for child_id in &current.child_ids {
                let child = self.node(*child_id)?;
                if child.start_byte <= start_index && end_index <= child.end_byte {
                    next_id = Some(*child_id);
                    break;
                }
            }
            let Some(next_id) = next_id else {
                if !named_only {
                    return Some(current_id);
                }
                let mut named_id = current_id;
                loop {
                    let named = self.node(named_id)?;
                    if named.named {
                        return Some(named_id);
                    }
                    named_id = named.parent_id?;
                }
            };
            current_id = next_id;
        }
    }

    fn descendant_for_position(
        &self,
        node_id: u32,
        start_row: u32,
        start_column: u32,
        end_row: u32,
        end_column: u32,
        named_only: bool,
    ) -> Option<u32> {
        let root = self.node(node_id)?;
        let start = (start_row, start_column);
        let end = (end_row, end_column);
        if compare_rows_columns(start, end) > 0 {
            return None;
        }
        if compare_rows_columns(start, (root.start_row, root.start_column)) < 0 {
            return None;
        }
        if compare_rows_columns(end, (root.end_row, root.end_column)) > 0 {
            return None;
        }

        let mut current_id = node_id;
        loop {
            let current = self.node(current_id)?;
            let mut next_id = None;
            for child_id in &current.child_ids {
                let child = self.node(*child_id)?;
                if compare_rows_columns((child.start_row, child.start_column), start) <= 0
                    && compare_rows_columns(end, (child.end_row, child.end_column)) <= 0
                {
                    next_id = Some(*child_id);
                    break;
                }
            }
            let Some(next_id) = next_id else {
                if !named_only {
                    return Some(current_id);
                }
                let mut named_id = current_id;
                loop {
                    let named = self.node(named_id)?;
                    if named.named {
                        return Some(named_id);
                    }
                    named_id = named.parent_id?;
                }
            };
            current_id = next_id;
        }
    }

    fn descendants_of_type(
        &self,
        node_id: u32,
        types: &[String],
        range: Option<((u32, u32), (u32, u32))>,
    ) -> Vec<u32> {
        let mut out = Vec::new();
        let Some(root) = self.node(node_id) else {
            return out;
        };

        let accepted: std::collections::HashSet<&str> = types.iter().map(String::as_str).collect();
        if accepted.is_empty() {
            return out;
        }

        let mut stack = root.child_ids.clone();
        while let Some(current_id) = stack.pop() {
            let Some(node) = self.node(current_id) else {
                continue;
            };

            let in_range = match range {
                Some((start, end)) => {
                    compare_rows_columns(start, (node.start_row, node.start_column)) <= 0
                        && compare_rows_columns((node.end_row, node.end_column), end) <= 0
                }
                None => true,
            };
            if in_range && accepted.contains(node.kind.as_str()) {
                out.push(current_id);
            }

            for child_id in node.child_ids.iter().rev() {
                stack.push(*child_id);
            }
        }

        out
    }

    fn node_text(&self, node_id: u32) -> Option<String> {
        let node = self.node(node_id)?;
        let bytes = self.source.as_bytes();
        let start = node.start_byte as usize;
        let end = node.end_byte as usize;
        if start > end || end > bytes.len() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes[start..end]).to_string())
    }

    fn type_of_span(&self, start: u32, end: u32) -> Option<String> {
        self.type_by_span.get(&(start, end)).cloned()
    }

    fn type_of_node(&self, node_id: u32) -> Option<String> {
        let node = self.node(node_id)?;
        self.type_of_span(node.start_byte, node.end_byte)
    }
}

fn push_cst_bridge_node(
    node: Node<'_>,
    parent_id: Option<u32>,
    field_name: Option<String>,
    out: &mut Vec<CstBridgeNode>,
) -> u32 {
    let node_id = out.len() as u32;
    let start = node.start_position();
    let end = node.end_position();
    out.push(CstBridgeNode {
        kind: node.kind().to_string(),
        field_name,
        named: node.is_named(),
        start_byte: node.start_byte() as u32,
        end_byte: node.end_byte() as u32,
        start_row: start.row as u32,
        start_column: start.column as u32,
        end_row: end.row as u32,
        end_column: end.column as u32,
        has_error: node.has_error(),
        is_error: node.is_error(),
        is_missing: node.is_missing(),
        parent_id,
        child_ids: vec![],
        named_child_ids: vec![],
        children_by_field: HashMap::new(),
    });

    let mut child_ids = Vec::with_capacity(node.child_count() as usize);
    let mut named_child_ids = Vec::new();
    let mut children_by_field: HashMap<String, Vec<u32>> = HashMap::new();
    for idx in 0..node.child_count() {
        if let Some(child) = node.child(idx) {
            let child_field = node.field_name_for_child(idx as u32).map(ToOwned::to_owned);
            let child_named = child.is_named();
            let child_id = push_cst_bridge_node(child, Some(node_id), child_field.clone(), out);
            child_ids.push(child_id);
            if child_named {
                named_child_ids.push(child_id);
            }
            if let Some(field) = child_field {
                children_by_field.entry(field).or_default().push(child_id);
            }
        }
    }

    let current = &mut out[node_id as usize];
    current.child_ids = child_ids;
    current.named_child_ids = named_child_ids;
    current.children_by_field = children_by_field;
    node_id
}

fn build_cst_bridge(
    root: Node<'_>,
    source: &str,
    type_by_span: HashMap<(u32, u32), String>,
) -> CstBridgeData {
    let mut nodes = Vec::new();
    let root_id = push_cst_bridge_node(root, None, None, &mut nodes);
    CstBridgeData {
        source: source.to_owned(),
        nodes,
        root_id,
        type_by_span,
    }
}

fn build_node_info_object<'js>(
    ctx: Ctx<'js>,
    node: &CstBridgeNode,
) -> rquickjs::Result<Object<'js>> {
    let out = Object::new(ctx)?;
    out.set("kind", node.kind.as_str())?;
    if let Some(field_name) = &node.field_name {
        out.set("fieldName", field_name.as_str())?;
    }
    out.set("named", node.named)?;
    out.set("startByte", node.start_byte)?;
    out.set("endByte", node.end_byte)?;
    out.set("startRow", node.start_row)?;
    out.set("startColumn", node.start_column)?;
    out.set("endRow", node.end_row)?;
    out.set("endColumn", node.end_column)?;
    out.set("hasError", node.has_error)?;
    out.set("isError", node.is_error)?;
    out.set("isMissing", node.is_missing)?;
    if let Some(parent_id) = node.parent_id {
        out.set("parentId", parent_id)?;
    }
    Ok(out)
}

fn build_raw_cst_object<'js>(
    ctx: Ctx<'js>,
    bridge: &CstBridgeData,
    node_id: u32,
) -> rquickjs::Result<Object<'js>> {
    let Some(node) = bridge.node(node_id) else {
        return Err(invalid_node_id(node_id));
    };

    let out = Object::new(ctx.clone())?;
    out.set("kind", node.kind.as_str())?;
    if let Some(field_name) = &node.field_name {
        out.set("fieldName", field_name.as_str())?;
    }
    out.set("named", node.named)?;
    out.set("startByte", node.start_byte)?;
    out.set("endByte", node.end_byte)?;

    let start_position = Object::new(ctx.clone())?;
    start_position.set("row", node.start_row)?;
    start_position.set("column", node.start_column)?;
    out.set("startPosition", start_position)?;

    let end_position = Object::new(ctx.clone())?;
    end_position.set("row", node.end_row)?;
    end_position.set("column", node.end_column)?;
    out.set("endPosition", end_position)?;

    out.set("hasError", node.has_error)?;
    out.set("isError", node.is_error)?;
    out.set("isMissing", node.is_missing)?;

    let children = Array::new(ctx.clone())?;
    for (index, child_id) in node.child_ids.iter().enumerate() {
        children.set(index, build_raw_cst_object(ctx.clone(), bridge, *child_id)?)?;
    }
    out.set("children", children)?;
    Ok(out)
}

fn build_expression_types_array<'js>(
    ctx: Ctx<'js>,
    bridge: &CstBridgeData,
) -> rquickjs::Result<Array<'js>> {
    let mut spans = bridge
        .type_by_span
        .iter()
        .map(|(&(start, end), ty)| (start, end, ty.as_str()))
        .collect::<Vec<_>>();
    spans.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    let array = Array::new(ctx.clone())?;
    for (index, (start, end, ty)) in spans.into_iter().enumerate() {
        let item = Object::new(ctx.clone())?;
        item.set("start", start)?;
        item.set("end", end)?;
        item.set("type", ty)?;
        array.set(index, item)?;
    }
    Ok(array)
}

fn create_bridge_object<'js>(
    ctx: Ctx<'js>,
    bridge: Arc<CstBridgeData>,
) -> rquickjs::Result<Object<'js>> {
    let bridge_object = Object::new(ctx.clone())?;

    let bridge_for_root = Arc::clone(&bridge);
    bridge_object.set(
        "rootId",
        Function::new(ctx.clone(), move || bridge_for_root.root_id)?,
    )?;

    let bridge_for_node_info = Arc::clone(&bridge);
    bridge_object.set(
        "nodeInfo",
        Function::new(ctx.clone(), move |ctx: Ctx<'js>, node_id: u32| {
            let Some(node) = bridge_for_node_info.node(node_id) else {
                return Err(invalid_node_id(node_id));
            };
            build_node_info_object(ctx, node)
        })?,
    )?;

    let bridge_for_children = Arc::clone(&bridge);
    bridge_object.set(
        "childIds",
        Function::new(ctx.clone(), move |node_id: u32| {
            let Some(node) = bridge_for_children.node(node_id) else {
                return vec![];
            };
            node.child_ids.clone()
        })?,
    )?;

    let bridge_for_named_children = Arc::clone(&bridge);
    bridge_object.set(
        "namedChildIds",
        Function::new(ctx.clone(), move |node_id: u32| {
            let Some(node) = bridge_for_named_children.node(node_id) else {
                return vec![];
            };
            node.named_child_ids.clone()
        })?,
    )?;

    let bridge_for_child_field = Arc::clone(&bridge);
    bridge_object.set(
        "childForFieldName",
        Function::new(ctx.clone(), move |node_id: u32, field_name: String| {
            bridge_for_child_field.child_for_field_name(node_id, &field_name)
        })?,
    )?;

    let bridge_for_children_field = Arc::clone(&bridge);
    bridge_object.set(
        "childrenForFieldName",
        Function::new(ctx.clone(), move |node_id: u32, field_name: String| {
            bridge_for_children_field.children_for_field_name(node_id, &field_name)
        })?,
    )?;

    let bridge_for_desc_index = Arc::clone(&bridge);
    bridge_object.set(
        "descendantForIndex",
        Function::new(
            ctx.clone(),
            move |node_id: u32, start: u32, end: u32, named_only: bool| {
                bridge_for_desc_index.descendant_for_index(node_id, start, end, named_only)
            },
        )?,
    )?;

    let bridge_for_desc_position = Arc::clone(&bridge);
    bridge_object.set(
        "descendantForPosition",
        Function::new(
            ctx.clone(),
            move |node_id: u32,
                  start_row: u32,
                  start_column: u32,
                  end_row: u32,
                  end_column: u32,
                  named_only: bool| {
                bridge_for_desc_position.descendant_for_position(
                    node_id,
                    start_row,
                    start_column,
                    end_row,
                    end_column,
                    named_only,
                )
            },
        )?,
    )?;

    let bridge_for_descendants = Arc::clone(&bridge);
    bridge_object.set(
        "descendantsOfType",
        Function::new(
            ctx.clone(),
            move |node_id: u32,
                  types: Vec<String>,
                  start_row: i32,
                  start_column: i32,
                  end_row: i32,
                  end_column: i32| {
                let range = if start_row < 0 || start_column < 0 || end_row < 0 || end_column < 0 {
                    None
                } else {
                    Some((
                        (start_row as u32, start_column as u32),
                        (end_row as u32, end_column as u32),
                    ))
                };
                bridge_for_descendants.descendants_of_type(node_id, &types, range)
            },
        )?,
    )?;

    let bridge_for_text = Arc::clone(&bridge);
    bridge_object.set(
        "nodeText",
        Function::new(ctx.clone(), move |node_id: u32| {
            bridge_for_text.node_text(node_id).unwrap_or_default()
        })?,
    )?;

    let bridge_for_type_span = Arc::clone(&bridge);
    bridge_object.set(
        "typeOfSpan",
        Function::new(ctx.clone(), move |start: u32, end: u32| {
            bridge_for_type_span.type_of_span(start, end)
        })?,
    )?;

    let bridge_for_type_node = Arc::clone(&bridge);
    bridge_object.set(
        "typeOfNode",
        Function::new(ctx.clone(), move |node_id: u32| {
            bridge_for_type_node.type_of_node(node_id)
        })?,
    )?;

    let bridge_for_raw_expression_types = Arc::clone(&bridge);
    bridge_object.set(
        "rawExpressionTypes",
        Function::new(ctx.clone(), move |ctx: Ctx<'js>| {
            build_expression_types_array(ctx, &bridge_for_raw_expression_types)
        })?,
    )?;

    let bridge_for_raw_cst = Arc::clone(&bridge);
    bridge_object.set(
        "rawCst",
        Function::new(ctx, move |ctx: Ctx<'js>| {
            build_raw_cst_object(ctx, &bridge_for_raw_cst, bridge_for_raw_cst.root_id)
        })?,
    )?;

    Ok(bridge_object)
}

fn create_plugin_payload<'js>(
    ctx: Ctx<'js>,
    file_path: &str,
    source: &str,
    bridge: Arc<CstBridgeData>,
) -> Result<Object<'js>, String> {
    let payload = Object::new(ctx.clone()).map_err(|err| err.to_string())?;
    payload
        .set("filePath", file_path)
        .map_err(|err| err.to_string())?;
    payload
        .set("source", source)
        .map_err(|err| err.to_string())?;
    payload
        .set(
            "bridge",
            create_bridge_object(ctx, bridge).map_err(|err| err.to_string())?,
        )
        .map_err(|err| err.to_string())?;
    Ok(payload)
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

    let expr_types_started = Instant::now();
    let type_by_span = collect_expression_types(file.id(), type_db, body_types);
    let expr_types_elapsed = expr_types_started.elapsed();
    let expression_types_count = type_by_span.len();

    let cst_started = Instant::now();
    let cst_bridge = Arc::new(build_cst_bridge(
        file.source().root_node(),
        source,
        type_by_span,
    ));
    let cst_elapsed = cst_started.elapsed();

    let serialize_elapsed = Duration::ZERO;
    let payload_prep_elapsed = cst_elapsed + expr_types_elapsed + serialize_elapsed;

    log::debug!(
        "js plugin payload for '{}' prepared in {:?} (cst={:?}, expression_types={:?}, serialize={:?}, expression_type_count={})",
        file_path,
        payload_prep_elapsed,
        cst_elapsed,
        expr_types_elapsed,
        serialize_elapsed,
        expression_types_count,
    );

    let mut diagnostics = Vec::new();
    let mut plugin_calls_elapsed = Duration::default();
    for plugin_path in plugins {
        let plugin_started = Instant::now();
        match run_single_plugin(plugin_path, &file_path, source, Arc::clone(&cst_bridge)) {
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
) -> HashMap<(u32, u32), String> {
    let Some(file_body_types) = body_types.get(&file_id) else {
        return HashMap::new();
    };

    let mut type_by_span = HashMap::<(u32, u32), String>::new();
    for inference in file_body_types.values() {
        for (span, ty) in &inference.expression_types {
            type_by_span
                .entry((span.start, span.end))
                .or_insert_with(|| type_db.intrn.display(*ty).to_string());
        }
    }

    type_by_span
}

fn run_single_plugin(
    path: &Path,
    file_path: &str,
    source: &str,
    cst_bridge: Arc<CstBridgeData>,
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
    let (parsed, parse_elapsed) = context
        .with(|ctx| {
            run_plugin_in_quickjs(
                ctx,
                path,
                &plugin_source,
                file_path,
                source,
                Arc::clone(&cst_bridge),
            )
        })
        .map_err(|err| format!("QuickJS plugin execution failed: {err}"))?;
    let wait_elapsed = wait_started.elapsed();
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
    file_path: &str,
    source: &str,
    cst_bridge: Arc<CstBridgeData>,
) -> Result<(JsPluginRunResult, Duration), String> {
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

    let payload_value = create_plugin_payload(ctx.clone(), file_path, source, cst_bridge)?;
    let execution: Promise<'_> = run_plugin
        .call((plugin_namespace, payload_value))
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let output_value: Value<'_> = execution
        .finish()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let decode_started = Instant::now();
    let parsed: JsPluginRunResult = output_value
        .get()
        .catch(&ctx)
        .map_err(|err| err.to_string())?;
    let decode_elapsed = decode_started.elapsed();
    Ok((parsed, decode_elapsed))
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
