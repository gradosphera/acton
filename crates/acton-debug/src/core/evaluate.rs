use super::replayer::LocalVarRuntime;
use crate::replayer::LocalVarRendered;
use crate::types_render::{
    RenderedValue, SlotValue, debug_print_from_stack, render_cell_like_as_type,
    render_runtime_contract_address, render_runtime_vm_value,
};
use anyhow::{Context, Result, anyhow, bail};
use num_bigint::BigInt;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use tolk_syntax::{
    AstNode, Call, DotAccessField, Expr, FuncBody, FunctionLike, Stmt, TopLevel, Type,
    parse_tolk_int_literal,
};
use tolkc::Compiler;
use tolkc::source_map::{Declaration, SourceMap};
use tolkc::types_kernel::{Ty, calc_width_on_stack, instantiate_generics};
use ton_executor::get::{GetExecutor, GetMethodResult, RunGetMethodArgs};
use tvmffi::serde::serialize_tuple;
use tvmffi::stack::{Tuple, TupleItem};
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellBuilder};
use vmlogs::parser::{CellLike, CellSlice, VmStackValue};

static EVALUATE_HELPER_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Field(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedValuePath {
    root: String,
    segments: Vec<PathSegment>,
}

#[derive(Debug, Clone)]
pub struct EvaluateRuntimeConfig {
    pub run_args: RunGetMethodArgs,
    pub config_b64: Option<String>,
    pub mappings: Option<BTreeMap<String, String>>,
}

pub(crate) fn evaluate_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    expression: &str,
) -> Result<RenderedValue> {
    let source_file = parse_wrapped_source(expression)?;
    let expr = wrapped_expression(&source_file)
        .ok_or_else(|| anyhow!("expected a single expression statement"))?;
    evaluate_parsed_expression(locals, source_map, expr, source_file.source.as_ref())
}

pub(crate) fn evaluate_condition_expression(
    locals: &[LocalVarRendered],
    expression: &str,
) -> Result<bool> {
    let source_file = parse_wrapped_source(expression)?;
    let expr = wrapped_expression(&source_file)
        .ok_or_else(|| anyhow!("expected a single expression statement"))?;
    evaluate_boolean_expression(locals, None, expr, source_file.source.as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeBuiltinCall {
    ContractGetAddress,
}

pub(crate) fn evaluate_runtime_builtin_call(
    expression: &str,
    c7: Option<&VmStackValue>,
) -> Result<Option<RenderedValue>> {
    match parse_runtime_builtin_call_expression(expression)? {
        Some(RuntimeBuiltinCall::ContractGetAddress) => {
            let c7 = c7.ok_or_else(|| {
                anyhow!("`contract.getAddress()` is not available without a live c7 snapshot")
            })?;
            let rendered = render_runtime_contract_address(c7).ok_or_else(|| {
                anyhow!("Failed to extract `contract.getAddress()` from the current c7 snapshot")
            })?;
            Ok(Some(rendered))
        }
        None => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FunctionCallParseStatus {
    NotFunctionCall,
    Supported,
    Unsupported(String),
}

pub(crate) fn classify_function_call_expression(expression: &str) -> FunctionCallParseStatus {
    let source_file = match parse_wrapped_source(expression) {
        Ok(source_file) => source_file,
        Err(err) => {
            return if expression.contains('(') {
                FunctionCallParseStatus::Unsupported(err.to_string())
            } else {
                FunctionCallParseStatus::NotFunctionCall
            };
        }
    };
    let Some(expr) = wrapped_expression(&source_file) else {
        return FunctionCallParseStatus::NotFunctionCall;
    };
    classify_parsed_function_call(expr, source_file.source.as_ref())
}

fn classify_parsed_function_call(expr: Expr<'_>, source: &str) -> FunctionCallParseStatus {
    match expr {
        Expr::Call(call) => match parse_call_expression(call, source) {
            Ok(_) => FunctionCallParseStatus::Supported,
            Err(err) => FunctionCallParseStatus::Unsupported(err.to_string()),
        },
        Expr::Paren(paren) => {
            let Some(inner) = paren.inner() else {
                return FunctionCallParseStatus::Unsupported(
                    "expected expression inside parentheses".to_owned(),
                );
            };
            classify_parsed_function_call(inner, source)
        }
        _ => FunctionCallParseStatus::NotFunctionCall,
    }
}

fn parse_runtime_builtin_call_expression(expression: &str) -> Result<Option<RuntimeBuiltinCall>> {
    let source_file = match parse_wrapped_source(expression) {
        Ok(source_file) => source_file,
        Err(_) => return Ok(None),
    };
    let Some(expr) = wrapped_expression(&source_file) else {
        return Ok(None);
    };
    parse_runtime_builtin_call(expr, source_file.source.as_ref())
}

fn parse_runtime_builtin_call(expr: Expr<'_>, source: &str) -> Result<Option<RuntimeBuiltinCall>> {
    match expr {
        Expr::Call(call) => {
            if call.arguments().next().is_some() {
                return Ok(None);
            }
            let normalized = call
                .syntax()
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            if normalized == "contract.getAddress()" {
                return Ok(Some(RuntimeBuiltinCall::ContractGetAddress));
            }
            Ok(None)
        }
        Expr::Paren(paren) => {
            let Some(inner) = paren.inner() else {
                return Ok(None);
            };
            parse_runtime_builtin_call(inner, source)
        }
        _ => Ok(None),
    }
}

pub(crate) fn evaluate_function_call(
    source_path: &Path,
    source_map: &SourceMap,
    runtime: &EvaluateRuntimeConfig,
    runtime_locals: &[LocalVarRuntime],
    expression: &str,
) -> Result<RenderedValue> {
    let parsed_call = parse_supported_function_call_expression(expression)?;

    let helper_id = EVALUATE_HELPER_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    let helper_names = EvaluateHelperNames::new(helper_id);
    let (helper_code, stack) = if parsed_call.arguments.is_empty() {
        (
            build_inline_expression_helper_code(expression, &helper_names),
            Tuple::empty(),
        )
    } else {
        let helper_call =
            plan_runtime_helper_call(source_map, runtime_locals, &parsed_call.arguments)?;
        (
            build_runtime_arg_helper_code(
                &parsed_call.callee_source,
                &helper_call.parameters,
                &helper_call.call_arguments,
                &helper_names,
            ),
            helper_call.stack,
        )
    };
    let helper_source = build_evaluate_helper_source(source_path, &helper_code)?;
    let _cleanup = TempSourceCleanup::new(helper_source.path.clone());

    let compiler = Compiler::new(2)
        .with_mappings(&runtime.mappings)
        .with_allow_no_entrypoint(true);
    let compiled = match compiler.compile(&helper_source.path, true) {
        tolkc::CompilerResult::Success(result) => result,
        tolkc::CompilerResult::Error(error) => {
            bail!(
                "Debugger evaluate failed to compile `{expression}`:\n{}",
                error.message.trim()
            )
        }
    };
    let method_id = compiled
        .abi
        .as_ref()
        .and_then(|abi| {
            abi.get_methods
                .iter()
                .find(|method| method.name == helper_names.entry)
                .map(|method| method.tvm_method_id)
        })
        .ok_or_else(|| anyhow!("Debugger evaluate failed to resolve compiled helper method id"))?;

    let mut params = runtime.run_args.clone();
    params.code = compiled.code_boc64.clone();
    params.method_id = method_id;
    params.debug_enabled = true;

    let stack_b64 = Boc::encode_base64(serialize_tuple(&stack)?);
    let executor = GetExecutor::new(&params)
        .with_context(|| format!("Debugger evaluate failed to prepare `{expression}`"))?;
    let result = executor
        .run_get_method(&stack_b64, &params, runtime.config_b64.as_deref())
        .with_context(|| format!("Debugger evaluate failed to execute `{expression}`"))?;

    render_function_call_result(
        expression,
        compiled.new_source_map.as_ref(),
        compiled.abi.as_ref(),
        method_id,
        result,
    )
}

fn resolve_locals_path(
    locals: &[LocalVarRendered],
    path: &ParsedValuePath,
) -> Result<RenderedValue> {
    let root = locals
        .iter()
        .rev()
        .find(|local| normalize_identifier(&local.var_name) == path.root)
        .ok_or_else(|| anyhow!("Variable `{}` is not in scope", path.root))?;

    resolve_value_path(&root.value, &path.segments)
}

fn resolve_value_path(root: &RenderedValue, segments: &[PathSegment]) -> Result<RenderedValue> {
    let mut current = root.clone();
    for segment in segments {
        current = resolve_segment(&current, segment)?;
    }
    Ok(current)
}

fn resolve_segment(value: &RenderedValue, segment: &PathSegment) -> Result<RenderedValue> {
    let value = unwrap_last_seen(value);
    match segment {
        PathSegment::Field(name) => match value {
            RenderedValue::Struct { fields, .. }
            | RenderedValue::Address { fields, .. }
            | RenderedValue::CellLike { fields, .. }
            | RenderedValue::CellOf { fields, .. }
            | RenderedValue::EnumValue { fields, .. }
            | RenderedValue::UnionCase { fields, .. } => fields
                .iter()
                .find(|(field_name, _)| field_name == name)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| anyhow!("Field `{name}` is not available on `{value}`")),
            _ => bail!("Cannot access field `{name}` on `{value}`"),
        },
        PathSegment::Index(index) => match value {
            RenderedValue::Tensor { items, .. } | RenderedValue::ArrayOf { items, .. } => items
                .get(*index)
                .cloned()
                .ok_or_else(|| anyhow!("Index {index} is out of bounds for `{value}`")),
            _ => bail!("Cannot index into `{value}`"),
        },
    }
}

fn unwrap_last_seen(mut value: &RenderedValue) -> &RenderedValue {
    while let RenderedValue::LastSeen { inner } = value {
        value = inner;
    }
    value
}

fn normalize_identifier(identifier: &str) -> &str {
    identifier
        .strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .unwrap_or(identifier)
}

fn parse_wrapped_source(input: &str) -> Result<tolk_syntax::SourceFile> {
    let wrapped = format!("fun __acton_debug_eval__() {{ {input}; }}");
    let source_file = tolk_syntax::parse(&wrapped)?;
    if source_file.has_errors() {
        bail!("syntax error");
    }
    Ok(source_file)
}

fn wrapped_expression(source_file: &tolk_syntax::SourceFile) -> Option<Expr<'_>> {
    let func = match source_file.top_levels().next()? {
        TopLevel::Func(func) => func,
        _ => return None,
    };
    let body = match func.body()? {
        FuncBody::Block(block) => block,
        _ => return None,
    };
    let stmt = match body.stmts().next()? {
        Stmt::ExprStmt(stmt) => stmt,
        _ => return None,
    };
    stmt.expr()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedFunctionCall {
    callee_source: String,
    arguments: Vec<FunctionCallArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FunctionCallArgument {
    RuntimeLocal(ParsedValuePath),
    Literal { source: String },
}

fn parse_supported_function_call_expression(input: &str) -> Result<ParsedFunctionCall> {
    let source_file = parse_wrapped_source(input)?;
    let expr = wrapped_expression(&source_file)
        .ok_or_else(|| anyhow!("expected a single expression statement"))?;
    let source = source_file.source.as_ref();
    parse_supported_function_call(expr, source)
}

fn parse_supported_function_call(expr: Expr<'_>, source: &str) -> Result<ParsedFunctionCall> {
    match expr {
        Expr::Call(call) => parse_call_expression(call, source),
        Expr::Paren(paren) => {
            let inner = paren
                .inner()
                .ok_or_else(|| anyhow!("expected expression inside parentheses"))?;
            parse_supported_function_call(inner, source)
        }
        _ => bail!("function calls are not supported"),
    }
}

fn parse_call_expression(call: Call<'_>, source: &str) -> Result<ParsedFunctionCall> {
    let callee_source = call_callee_source(&call, source)?;
    let arguments = call.arguments().collect::<Vec<_>>();

    match arguments.as_slice() {
        [] => Ok(ParsedFunctionCall {
            callee_source,
            arguments: Vec::new(),
        }),
        [_first, ..] => {
            if call.callee_qualifier().is_some() {
                bail!("method calls with arguments are not supported yet");
            }

            let parsed_arguments = arguments
                .into_iter()
                .map(|argument| {
                    let arg_expr = argument
                        .expr()
                        .ok_or_else(|| anyhow!("expected function argument"))?;
                    parse_function_call_argument(arg_expr, source)
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(ParsedFunctionCall {
                callee_source,
                arguments: parsed_arguments,
            })
        }
    }
}

fn parse_function_call_argument(expr: Expr<'_>, source: &str) -> Result<FunctionCallArgument> {
    match expr {
        Expr::Paren(paren) => {
            let inner = paren
                .inner()
                .ok_or_else(|| anyhow!("expected expression inside parentheses"))?;
            parse_function_call_argument(inner, source)
        }
        Expr::NumberLit(number_lit) => Ok(FunctionCallArgument::Literal {
            source: number_lit.text(source).to_owned(),
        }),
        Expr::StringLit(string_lit) => Ok(FunctionCallArgument::Literal {
            source: string_lit.text(source).to_owned(),
        }),
        Expr::BoolLit(bool_lit) => Ok(FunctionCallArgument::Literal {
            source: bool_lit.value().to_string(),
        }),
        Expr::NullLit(_) => Ok(FunctionCallArgument::Literal {
            source: "null".to_owned(),
        }),
        Expr::Unary(unary) if unary.operator_name(source) == "-" => {
            let argument = unary
                .argument()
                .ok_or_else(|| anyhow!("expected expression after `-`"))?;
            match argument {
                Expr::NumberLit(number_lit) => Ok(FunctionCallArgument::Literal {
                    source: format!("-{}", number_lit.text(source)),
                }),
                _ => bail!("only numeric literals can be negated in debugger evaluate arguments"),
            }
        }
        _ => {
            let path = parse_expr_to_path(expr, source)?;
            if !path.segments.is_empty() {
                return Ok(FunctionCallArgument::RuntimeLocal(path));
            }
            Ok(FunctionCallArgument::RuntimeLocal(path))
        }
    }
}

fn call_callee_source(call: &Call<'_>, source: &str) -> Result<String> {
    let callee = call
        .callee()
        .ok_or_else(|| anyhow!("expected function name"))?;
    Ok(callee
        .syntax()
        .utf8_text(source.as_bytes())
        .unwrap_or("function")
        .trim()
        .to_owned())
}

#[derive(Debug, Clone)]
struct HelperRuntimeParameter {
    name: String,
    type_name: String,
    stack_items: Vec<TupleItem>,
}

#[derive(Debug, Clone)]
struct PlannedRuntimeHelperCall {
    parameters: Vec<HelperRuntimeParameter>,
    call_arguments: Vec<String>,
    stack: Tuple,
}

fn plan_runtime_helper_call(
    source_map: &SourceMap,
    runtime_locals: &[LocalVarRuntime],
    arguments: &[FunctionCallArgument],
) -> Result<PlannedRuntimeHelperCall> {
    let mut parameters = Vec::<HelperRuntimeParameter>::new();
    let mut call_arguments = Vec::new();

    for argument in arguments {
        match argument {
            FunctionCallArgument::RuntimeLocal(path) => {
                let local = runtime_locals
                    .iter()
                    .rev()
                    .find(|local| normalize_identifier(&local.var_name) == path.root)
                    .ok_or_else(|| anyhow!("Variable `{}` is not in scope", path.root))?;
                let parameter_name = helper_runtime_param_name(parameters.len());
                let parameter =
                    build_runtime_parameter_for_path(source_map, local, path, parameter_name)?;

                call_arguments.push(parameter.name.clone());
                parameters.push(parameter);
            }
            FunctionCallArgument::Literal { source } => {
                call_arguments.push(source.clone());
            }
        }
    }

    let stack = Tuple(
        parameters
            .iter()
            .flat_map(|parameter| parameter.stack_items.iter().cloned())
            .collect(),
    );

    Ok(PlannedRuntimeHelperCall {
        parameters,
        call_arguments,
        stack,
    })
}

fn build_runtime_parameter_for_path(
    source_map: &SourceMap,
    local: &LocalVarRuntime,
    path: &ParsedValuePath,
    parameter_name: String,
) -> Result<HelperRuntimeParameter> {
    let ty = local.ty.as_ref().ok_or_else(|| {
        anyhow!(
            "Variable `{}` has no source-level type available for debugger evaluate",
            path.root
        )
    })?;
    let (resolved_ty, resolved_slot_values) =
        resolve_runtime_path_values(source_map, ty, &local.ir_slot_values, &path.segments)
            .with_context(|| {
                if path.segments.is_empty() {
                    format!(
                        "Variable `{}` is not available on the runtime stack",
                        path.root
                    )
                } else {
                    format!(
                        "Path `{}` is not available on the runtime stack",
                        render_value_path(path)
                    )
                }
            })?;
    let stack_items = resolved_slot_values
        .into_iter()
        .map(convert_vm_value_to_tuple_item)
        .collect::<Result<Vec<_>>>()?;

    Ok(HelperRuntimeParameter {
        name: parameter_name,
        type_name: resolved_ty.to_string(),
        stack_items,
    })
}

fn resolve_runtime_path_values(
    source_map: &SourceMap,
    root_ty: &Ty,
    root_slot_values: &[Option<VmStackValue>],
    segments: &[PathSegment],
) -> Result<(Ty, Vec<VmStackValue>)> {
    let mut current_ty = root_ty.clone();
    let mut current_slot_values = root_slot_values.to_vec();

    for segment in segments {
        let (next_ty, slot_range) = resolve_runtime_segment(source_map, &current_ty, segment)?;
        let slice = current_slot_values
            .get(slot_range.clone())
            .ok_or_else(|| anyhow!("runtime slot range is out of bounds"))?;
        current_slot_values = slice.to_vec();
        current_ty = next_ty;
    }

    let current_slot_values = current_slot_values
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow!("required runtime slots were not observed"))?;
    Ok((current_ty, current_slot_values))
}

fn resolve_runtime_segment(
    source_map: &SourceMap,
    ty: &Ty,
    segment: &PathSegment,
) -> Result<(Ty, Range<usize>)> {
    match ty {
        Ty::AliasRef {
            alias_name,
            type_args,
        } => {
            let alias_ref = source_map.get_alias(alias_name);
            let target_ty = match type_args {
                Some(type_args) => instantiate_generics(
                    &alias_ref.target_ty,
                    alias_ref.type_params.as_deref().unwrap_or(&[]),
                    type_args,
                ),
                None => alias_ref.target_ty.clone(),
            };
            resolve_runtime_segment(source_map, &target_ty, segment)
        }
        Ty::StructRef {
            struct_name,
            type_args,
        } => match segment {
            PathSegment::Field(field_name) => {
                let struct_ref = source_map.get_struct(struct_name);
                let mut offset = 0;
                for field in &struct_ref.fields {
                    let field_ty = match type_args {
                        Some(type_args) => instantiate_generics(
                            &field.ty,
                            struct_ref.type_params.as_deref().unwrap_or(&[]),
                            type_args,
                        ),
                        None => field.ty.clone(),
                    };
                    let width = calc_width_on_stack(source_map, &field_ty);
                    if field.name == *field_name {
                        return Ok((field_ty, offset..offset + width));
                    }
                    offset += width;
                }
                bail!("Field `{field_name}` is not available on type `{ty}`")
            }
            PathSegment::Index(index) => {
                bail!("Cannot index field {index} on struct-like type `{ty}`")
            }
        },
        Ty::Tensor { items } | Ty::ShapedTuple { items } => match segment {
            PathSegment::Index(index) => {
                let item_ty = items
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| anyhow!("Index {index} is out of bounds for type `{ty}`"))?;
                let offset = items
                    .iter()
                    .take(*index)
                    .map(|item| calc_width_on_stack(source_map, item))
                    .sum::<usize>();
                let width = calc_width_on_stack(source_map, &item_ty);
                Ok((item_ty, offset..offset + width))
            }
            PathSegment::Field(field_name) => {
                bail!("Cannot access field `{field_name}` on tuple-like type `{ty}`")
            }
        },
        _ => match segment {
            PathSegment::Field(field_name) => {
                bail!("Cannot access field `{field_name}` on runtime value of type `{ty}`")
            }
            PathSegment::Index(index) => {
                bail!("Cannot index `{index}` on runtime value of type `{ty}`")
            }
        },
    }
}

fn render_value_path(path: &ParsedValuePath) -> String {
    let mut rendered = path.root.clone();
    for segment in &path.segments {
        rendered.push('.');
        match segment {
            PathSegment::Field(field_name) => rendered.push_str(field_name),
            PathSegment::Index(index) => rendered.push_str(&index.to_string()),
        }
    }
    rendered
}

fn helper_runtime_param_name(index: usize) -> String {
    format!("__acton_debug_eval_arg{index}")
}

fn convert_vm_value_to_tuple_item(value: VmStackValue) -> Result<TupleItem> {
    match value {
        VmStackValue::Null => Ok(TupleItem::Null),
        VmStackValue::NaN => Ok(TupleItem::Nan),
        VmStackValue::Integer(value) => {
            let value = value
                .parse()
                .map_err(|_| anyhow!("Invalid integer stack value: {value}"))?;
            Ok(TupleItem::Int(value))
        }
        VmStackValue::Tuple(values) => Ok(TupleItem::Tuple(Tuple(
            values
                .into_iter()
                .map(convert_vm_value_to_tuple_item)
                .collect::<Result<Vec<_>>>()?,
        ))),
        VmStackValue::Cell(cell_like) => Ok(TupleItem::Cell(convert_cell_like_to_cell(cell_like)?)),
        VmStackValue::Builder(hex) => Ok(TupleItem::Builder(Boc::decode_hex(hex)?)),
        VmStackValue::CellSlice(slice) => Ok(TupleItem::Slice(Boc::decode_hex(slice.value)?)),
        VmStackValue::Continuation(_) => {
            bail!("Continuation values are not supported in debugger evaluate arguments yet")
        }
        VmStackValue::String(value) => Ok(TupleItem::Cell(string_to_cell(&value)?)),
        VmStackValue::Unknown => bail!("Unknown runtime stack values are not supported"),
    }
}

fn convert_cell_like_to_cell(cell_like: CellLike) -> Result<Cell> {
    match cell_like {
        CellLike::Cell(hex) | CellLike::Builder(hex) => Ok(Boc::decode_hex(hex)?),
    }
}

fn string_to_cell(value: &str) -> Result<Cell> {
    let bytes = value.as_bytes();
    let total_bits = bytes.len() * 8;

    if total_bits <= 1023 {
        let mut builder = CellBuilder::new();
        builder.store_raw(bytes, total_bits as u16)?;
        return Ok(builder.build()?);
    }

    let mut chunks = Vec::new();
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let chunk_len = remaining.len().min(127);
        chunks.push((&remaining[..chunk_len], chunk_len * 8));
        remaining = &remaining[chunk_len..];
    }

    let mut next_cell = None;
    for (chunk, bits) in chunks.into_iter().rev() {
        let mut builder = CellBuilder::new();
        builder.store_raw(chunk, bits as u16)?;
        if let Some(next_cell) = next_cell {
            builder.store_reference(next_cell)?;
        }
        next_cell = Some(builder.build()?);
    }

    next_cell.ok_or_else(|| anyhow!("No root cell for string"))
}

#[derive(Debug, Clone)]
struct EvaluateHelperNames {
    entry: String,
    inline: String,
}

impl EvaluateHelperNames {
    fn new(id: u64) -> Self {
        Self {
            entry: format!("__acton_debug_eval_entry_{id}"),
            inline: format!("__acton_debug_eval_inline_{id}"),
        }
    }
}

#[derive(Debug)]
struct TempSourceCleanup {
    path: PathBuf,
}

impl TempSourceCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempSourceCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
struct TempEvaluateSource {
    path: PathBuf,
}

fn build_evaluate_helper_source(
    source_path: &Path,
    helper_code: &str,
) -> Result<TempEvaluateSource> {
    let original_source = fs::read_to_string(source_path)
        .with_context(|| format!("Cannot read source file {}", source_path.display()))?;

    let temp_path = unique_temp_source_path(source_path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .with_context(|| format!("Cannot create temp evaluate file {}", temp_path.display()))?;
    file.write_all(original_source.as_bytes())?;
    file.write_all(helper_code.as_bytes())?;

    Ok(TempEvaluateSource { path: temp_path })
}

fn build_inline_expression_helper_code(
    expression: &str,
    helper_names: &EvaluateHelperNames,
) -> String {
    let expression = expression.trim().trim_end_matches(';').trim();
    format!(
        "\n\n@inline_ref\nfun {inline_name}() {{\n    return {expression};\n}}\n\nget fun {entry_name}() {{\n    return {inline_name}();\n}}\n",
        inline_name = helper_names.inline,
        entry_name = helper_names.entry,
    )
}

fn build_runtime_arg_helper_code(
    callee_source: &str,
    parameters: &[HelperRuntimeParameter],
    call_arguments: &[String],
    helper_names: &EvaluateHelperNames,
) -> String {
    let helper_args = parameters
        .iter()
        .map(|parameter| format!("{}: {}", parameter.name, parameter.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    let inline_forward_args = parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let call_args = call_arguments.join(", ");

    format!(
        "\n\n@inline_ref\nfun {inline_name}({helper_args}) {{\n    return {callee_source}({call_args});\n}}\n\nget fun {entry_name}({helper_args}) {{\n    return {inline_name}({inline_forward_args});\n}}\n",
        inline_name = helper_names.inline,
        entry_name = helper_names.entry,
        helper_args = helper_args,
        call_args = call_args,
        inline_forward_args = inline_forward_args,
    )
}

fn unique_temp_source_path(source_path: &Path) -> PathBuf {
    let parent = source_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("acton-debug-eval");
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tolk");
    let id = EVALUATE_HELPER_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    parent.join(format!(".{stem}.acton-debug-eval-{id}.{extension}"))
}

fn render_function_call_result(
    expression: &str,
    source_map: Option<&SourceMap>,
    abi: Option<&tolkc::abi::ContractABI>,
    method_id: i32,
    result: GetMethodResult,
) -> Result<RenderedValue> {
    match result {
        GetMethodResult::Success(success)
            if success.vm_exit_code == 0 || success.vm_exit_code == 1 =>
        {
            let stack_cell = Boc::decode_base64(success.stack.as_ref())
                .context("Failed to decode evaluate result stack")?;
            let stack_tuple = Tuple::deserialize(&stack_cell)
                .context("Failed to deserialize evaluate result stack")?;
            Ok(render_stack_tuple(source_map, abi, method_id, &stack_tuple))
        }
        GetMethodResult::Success(success) => bail!(
            "Debugger evaluate failed for `{expression}` with VM exit code {}.\nVM log:\n{}",
            success.vm_exit_code,
            success.vm_log,
        ),
        GetMethodResult::Error(error) => {
            bail!(
                "Debugger evaluate failed for `{expression}`: {}",
                error.error
            )
        }
    }
}

fn render_stack_tuple(
    source_map: Option<&SourceMap>,
    abi: Option<&tolkc::abi::ContractABI>,
    method_id: i32,
    stack_tuple: &Tuple,
) -> RenderedValue {
    if let (Some(source_map), Some(abi)) = (source_map, abi)
        && let Some(method) = abi
            .get_methods
            .iter()
            .find(|method| method.tvm_method_id == method_id)
    {
        let vm_values = stack_tuple
            .iter()
            .map(tuple_item_to_vm_stack_value)
            .collect::<Vec<_>>();
        let slot_values = vm_values.iter().map(SlotValue::Live).collect::<Vec<_>>();
        return debug_print_from_stack(source_map, &slot_values, &method.return_ty);
    }

    match stack_tuple.as_slice() {
        [] => RenderedValue::typed_leaf("()", "tuple"),
        [value] => render_runtime_vm_value(&tuple_item_to_vm_stack_value(value)),
        items => RenderedValue::Tensor {
            type_name: "tuple".to_string(),
            items: items
                .iter()
                .map(|item| render_runtime_vm_value(&tuple_item_to_vm_stack_value(item)))
                .collect(),
        },
    }
}

fn tuple_item_to_vm_stack_value(item: &TupleItem) -> VmStackValue {
    match item {
        TupleItem::Null => VmStackValue::Null,
        TupleItem::Int(value) => VmStackValue::Integer(value.to_string()),
        TupleItem::Nan => VmStackValue::NaN,
        TupleItem::Cell(cell) => VmStackValue::Cell(CellLike::Cell(Boc::encode_hex(cell))),
        TupleItem::Slice(cell) => VmStackValue::CellSlice(CellSlice {
            value: Boc::encode_hex(cell),
            bits: None,
            refs: None,
        }),
        TupleItem::Builder(cell) => VmStackValue::Builder(Boc::encode_hex(cell)),
        TupleItem::Tuple(items) => {
            VmStackValue::Tuple(items.iter().map(tuple_item_to_vm_stack_value).collect())
        }
        TupleItem::TypedTuple { inner, .. } => {
            VmStackValue::Tuple(inner.iter().map(tuple_item_to_vm_stack_value).collect())
        }
        TupleItem::Cont(cont) => VmStackValue::Continuation(Boc::encode_base64(cont.code.clone())),
    }
}

fn parse_expr_to_path(expr: Expr<'_>, source: &str) -> Result<ParsedValuePath> {
    match expr {
        Expr::Ident(ident) => Ok(ParsedValuePath {
            root: ident.normalized_name(source).to_owned(),
            segments: Vec::new(),
        }),
        Expr::Call(_) => bail!("function calls are not supported"),
        Expr::Paren(paren) => {
            let inner = paren
                .inner()
                .ok_or_else(|| anyhow!("expected expression inside parentheses"))?;
            parse_expr_to_path(inner, source)
        }
        Expr::NotNull(not_null) => {
            let inner = not_null
                .inner()
                .ok_or_else(|| anyhow!("expected expression before `!`"))?;
            parse_expr_to_path(inner, source)
        }
        Expr::DotAccess(dot_access) => {
            let obj = dot_access
                .obj()
                .ok_or_else(|| anyhow!("expected expression before `.`"))?;
            let field = dot_access
                .field()
                .ok_or_else(|| anyhow!("expected field or numeric index after `.`"))?;

            let mut path = parse_expr_to_path(obj, source)?;
            match field {
                DotAccessField::Ident(ident) => path
                    .segments
                    .push(PathSegment::Field(ident.normalized_name(source).to_owned())),
                DotAccessField::NumericIndex(index) => {
                    path.segments.push(PathSegment::Index(parse_numeric_index(
                        index.value(source),
                    )?));
                }
            }
            Ok(path)
        }
        _ => bail!("expected a variable path"),
    }
}

fn parse_numeric_index(raw: &str) -> Result<usize> {
    raw.parse::<usize>()
        .map_err(|err| anyhow!("invalid numeric index `{raw}`: {err}"))
}

fn evaluate_parsed_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    expr: Expr<'_>,
    source: &str,
) -> Result<RenderedValue> {
    match expr {
        Expr::BoolLit(bool_lit) => Ok(render_bool(bool_lit.value())),
        Expr::NumberLit(number_lit) => render_number_literal(number_lit.text(source)),
        Expr::StringLit(string_lit) => Ok(render_string_literal(string_lit.text(source))),
        Expr::NullLit(_) => Ok(render_null()),
        Expr::Paren(paren) => {
            let inner = paren
                .inner()
                .ok_or_else(|| anyhow!("expected expression inside parentheses"))?;
            evaluate_parsed_expression(locals, source_map, inner, source)
        }
        Expr::Unary(unary) => evaluate_unary_expression(locals, source_map, &unary, source),
        Expr::AsCast(as_cast) => evaluate_as_cast_expression(locals, source_map, &as_cast, source),
        Expr::Bin(bin) => match bin.operator_name(source) {
            "&&" => {
                let left = bin
                    .left()
                    .ok_or_else(|| anyhow!("expected left operand for `&&`"))?;
                let left = evaluate_boolean_expression(locals, source_map, left, source)?;
                if !left {
                    return Ok(render_bool(false));
                }

                let right = bin
                    .right()
                    .ok_or_else(|| anyhow!("expected right operand for `&&`"))?;
                Ok(render_bool(evaluate_boolean_expression(
                    locals, source_map, right, source,
                )?))
            }
            "||" => {
                let left = bin
                    .left()
                    .ok_or_else(|| anyhow!("expected left operand for `||`"))?;
                let left = evaluate_boolean_expression(locals, source_map, left, source)?;
                if left {
                    return Ok(render_bool(true));
                }

                let right = bin
                    .right()
                    .ok_or_else(|| anyhow!("expected right operand for `||`"))?;
                Ok(render_bool(evaluate_boolean_expression(
                    locals, source_map, right, source,
                )?))
            }
            "==" => evaluate_equality_expression(locals, source_map, &bin, source, true),
            "!=" => evaluate_equality_expression(locals, source_map, &bin, source, false),
            "<" | "<=" | ">" | ">=" => {
                evaluate_ordering_expression(locals, source_map, &bin, source)
            }
            operator => bail!("binary operator `{operator}` is not supported"),
        },
        _ => {
            let path = parse_expr_to_path(expr, source)?;
            resolve_locals_path(locals, &path)
        }
    }
}

fn evaluate_unary_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    unary: &tolk_syntax::Unary<'_>,
    source: &str,
) -> Result<RenderedValue> {
    let operator = unary.operator_name(source);
    let argument = unary
        .argument()
        .ok_or_else(|| anyhow!("expected expression after `{operator}`"))?;

    match operator {
        "!" => {
            let value = evaluate_parsed_expression(locals, source_map, argument, source)?;
            Ok(render_bool(!rendered_value_as_bool(&value)?))
        }
        "-" => {
            let value = evaluate_parsed_expression(locals, source_map, argument, source)?;
            let number = parse_rendered_number(&value)
                .ok_or_else(|| anyhow!("unary operator `-` requires numeric operand"))?;
            Ok(RenderedValue::typed_leaf((-number).to_string(), "int"))
        }
        _ => bail!("unary operator `{operator}` is not supported"),
    }
}

fn evaluate_boolean_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    expr: Expr<'_>,
    source: &str,
) -> Result<bool> {
    let value = evaluate_parsed_expression(locals, source_map, expr, source)?;
    rendered_value_as_bool(&value)
}

fn rendered_value_as_bool(value: &RenderedValue) -> Result<bool> {
    match unwrap_last_seen(value) {
        RenderedValue::Leaf { value, .. } if value == "true" => Ok(true),
        RenderedValue::Leaf { value, .. } if value == "false" => Ok(false),
        other => bail!("logical operators require boolean operands, got `{other}`"),
    }
}

fn evaluate_equality_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    bin: &tolk_syntax::Bin<'_>,
    source: &str,
    expected_equal: bool,
) -> Result<RenderedValue> {
    let left = bin
        .left()
        .ok_or_else(|| anyhow!("expected left operand for equality comparison"))?;
    let right = bin
        .right()
        .ok_or_else(|| anyhow!("expected right operand for equality comparison"))?;

    let left = evaluate_parsed_expression(locals, source_map, left, source)?;
    let right = evaluate_parsed_expression(locals, source_map, right, source)?;
    let equal = rendered_value_text(&left) == rendered_value_text(&right);
    Ok(render_bool(equal == expected_equal))
}

fn evaluate_ordering_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    bin: &tolk_syntax::Bin<'_>,
    source: &str,
) -> Result<RenderedValue> {
    let operator = bin.operator_name(source);
    let left = bin
        .left()
        .ok_or_else(|| anyhow!("expected left operand for `{operator}`"))?;
    let right = bin
        .right()
        .ok_or_else(|| anyhow!("expected right operand for `{operator}`"))?;

    let left = evaluate_parsed_expression(locals, source_map, left, source)?;
    let right = evaluate_parsed_expression(locals, source_map, right, source)?;
    let comparison = compare_rendered_values_as_numbers(&left, &right, operator)?;

    let result = match operator {
        "<" => comparison == Ordering::Less,
        "<=" => comparison != Ordering::Greater,
        ">" => comparison == Ordering::Greater,
        ">=" => comparison != Ordering::Less,
        _ => unreachable!("unsupported ordering operator"),
    };
    Ok(render_bool(result))
}

fn evaluate_as_cast_expression(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    as_cast: &tolk_syntax::AsCast<'_>,
    source: &str,
) -> Result<RenderedValue> {
    let expr = as_cast
        .expr()
        .ok_or_else(|| anyhow!("expected expression before `as`"))?;
    let target = as_cast
        .casted_to()
        .ok_or_else(|| anyhow!("expected type after `as`"))?;
    let source_map = source_map
        .ok_or_else(|| anyhow!("type casts are not supported in this debugger context"))?;

    let value = evaluate_parsed_expression(locals, Some(source_map), expr, source)?;
    let ty = lower_evaluate_cast_type(target, source, source_map)?;

    match &ty {
        Ty::CellOf { .. } => {
            render_cell_like_as_type(source_map, &value, &ty).map_err(anyhow::Error::msg)
        }
        _ => bail!("Debugger evaluate currently supports only casts to `Cell<T>`"),
    }
}

fn lower_evaluate_cast_type(ty: Type<'_>, source: &str, source_map: &SourceMap) -> Result<Ty> {
    match ty {
        Type::TypeIdent(ident) => {
            lower_named_or_primitive_type(ident.text(source), None, source_map)
        }
        Type::TypeInstantiatedTs(inst) => {
            let name = inst
                .name()
                .ok_or_else(|| anyhow!("expected type name"))?
                .text(source);
            let type_args = inst
                .arguments()
                .map(|args| {
                    args.types()
                        .map(|arg| lower_evaluate_cast_type(arg, source, source_map))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?;
            lower_named_or_primitive_type(name, type_args, source_map)
        }
        Type::ParenthesizedType(paren) => {
            let inner = paren
                .inner()
                .ok_or_else(|| anyhow!("expected type inside parentheses"))?;
            lower_evaluate_cast_type(inner, source, source_map)
        }
        Type::NullableType(nullable) => {
            let inner = nullable
                .inner()
                .ok_or_else(|| anyhow!("expected type before `?`"))?;
            Ok(Ty::Nullable {
                inner: Box::new(lower_evaluate_cast_type(inner, source, source_map)?),
                stack_type_id: None,
                stack_width: None,
            })
        }
        Type::TensorType(tensor) => Ok(Ty::Tensor {
            items: tensor
                .elements()
                .map(|item| lower_evaluate_cast_type(item, source, source_map))
                .collect::<Result<Vec<_>>>()?,
        }),
        Type::TupleType(tuple) => Ok(Ty::ShapedTuple {
            items: tuple
                .elements()
                .map(|item| lower_evaluate_cast_type(item, source, source_map))
                .collect::<Result<Vec<_>>>()?,
        }),
        Type::UnionType(_) => bail!(
            "inline union types are not supported in debugger evaluate casts; cast to a named ABI type instead"
        ),
        Type::FunCallableType(_) => {
            bail!("callable types are not supported in debugger evaluate casts")
        }
        Type::NullLit(_) => Ok(Ty::NullLiteral),
        Type::Unmapped(raw) => bail!(
            "unsupported cast type `{}`",
            raw.0
                .utf8_text(source.as_bytes())
                .unwrap_or("<invalid utf8>")
        ),
    }
}

fn lower_named_or_primitive_type(
    name: &str,
    type_args: Option<Vec<Ty>>,
    source_map: &SourceMap,
) -> Result<Ty> {
    if let Some(ty) = lower_primitive_type(name, type_args.as_deref())? {
        return Ok(ty);
    }

    match resolve_named_declaration_kind(source_map, name) {
        Some(NamedDeclarationKind::Struct) => Ok(Ty::StructRef {
            struct_name: name.to_owned(),
            type_args,
        }),
        Some(NamedDeclarationKind::Alias) => Ok(Ty::AliasRef {
            alias_name: name.to_owned(),
            type_args,
        }),
        Some(NamedDeclarationKind::Enum) => {
            if type_args.as_ref().is_some_and(|args| !args.is_empty()) {
                bail!("enum `{name}` does not take type arguments");
            }
            Ok(Ty::EnumRef {
                enum_name: name.to_owned(),
            })
        }
        None => bail!("type `{name}` is not known in the current SourceMap"),
    }
}

fn lower_primitive_type(name: &str, type_args: Option<&[Ty]>) -> Result<Option<Ty>> {
    let primitive = match name {
        "int" => Some(Ty::Int),
        "coins" => Some(Ty::Coins),
        "bool" => Some(Ty::Bool),
        "cell" | "Cell" if type_args.is_none() => Some(Ty::Cell),
        "builder" => Some(Ty::Builder),
        "slice" => Some(Ty::Slice),
        "string" => Some(Ty::String),
        "RemainingBitsAndRefs" => Some(Ty::Remaining),
        "address" => Some(Ty::Address),
        "ext_address" => Some(Ty::AddressExt),
        "any_address" => Some(Ty::AddressAny),
        "null" => Some(Ty::NullLiteral),
        "Cell" => {
            let [inner] = type_args.unwrap_or_default() else {
                bail!("`Cell` expects exactly one type argument");
            };
            Some(Ty::CellOf {
                inner: Box::new(inner.clone()),
            })
        }
        "array" => {
            let [inner] = type_args.unwrap_or_default() else {
                bail!("`array` expects exactly one type argument");
            };
            Some(Ty::ArrayOf {
                inner: Box::new(inner.clone()),
            })
        }
        "lisp_list" => {
            let [inner] = type_args.unwrap_or_default() else {
                bail!("`lisp_list` expects exactly one type argument");
            };
            Some(Ty::LispListOf {
                inner: Box::new(inner.clone()),
            })
        }
        "map" => {
            let [key, value] = type_args.unwrap_or_default() else {
                bail!("`map` expects exactly two type arguments");
            };
            Some(Ty::MapKV {
                k: Box::new(key.clone()),
                v: Box::new(value.clone()),
            })
        }
        _ => parse_sized_primitive_type(name),
    };

    Ok(primitive)
}

fn parse_sized_primitive_type(name: &str) -> Option<Ty> {
    fn parse_suffix(name: &str, prefix: &str) -> Option<u32> {
        name.strip_prefix(prefix)
            .filter(|suffix| !suffix.is_empty())
            .and_then(|suffix| suffix.parse::<u32>().ok())
    }

    if let Some(n) = parse_suffix(name, "int") {
        return Some(Ty::IntN { n });
    }
    if let Some(n) = parse_suffix(name, "uint") {
        return Some(Ty::UintN { n });
    }
    if let Some(n) = parse_suffix(name, "varint") {
        return Some(Ty::VarintN { n });
    }
    if let Some(n) = parse_suffix(name, "varuint") {
        return Some(Ty::VaruintN { n });
    }
    if let Some(n) = parse_suffix(name, "bits") {
        return Some(Ty::BitsN { n });
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NamedDeclarationKind {
    Struct,
    Alias,
    Enum,
}

fn resolve_named_declaration_kind(
    source_map: &SourceMap,
    name: &str,
) -> Option<NamedDeclarationKind> {
    source_map
        .declarations()
        .iter()
        .find_map(|decl| match decl {
            Declaration::Struct(decl) if decl.name == name => Some(NamedDeclarationKind::Struct),
            Declaration::Alias(decl) if decl.name == name => Some(NamedDeclarationKind::Alias),
            Declaration::Enum(decl) if decl.name == name => Some(NamedDeclarationKind::Enum),
            _ => None,
        })
}

fn compare_rendered_values_as_numbers(
    left: &RenderedValue,
    right: &RenderedValue,
    operator: &str,
) -> Result<Ordering> {
    let left = parse_rendered_number(left)
        .ok_or_else(|| anyhow!("operator `{operator}` requires numeric operands"))?;
    let right = parse_rendered_number(right)
        .ok_or_else(|| anyhow!("operator `{operator}` requires numeric operands"))?;

    Ok(left.cmp(&right))
}

fn parse_rendered_number(value: &RenderedValue) -> Option<BigInt> {
    let text = rendered_value_text(value);
    text.parse::<BigInt>().ok()
}

fn rendered_value_text(value: &RenderedValue) -> String {
    unwrap_last_seen(value).to_string()
}

fn render_number_literal(raw: &str) -> Result<RenderedValue> {
    let parsed = parse_tolk_int_literal(raw)
        .ok_or_else(|| anyhow!("numeric literal `{raw}` is not supported"))?;
    let normalized = match parsed.radix() {
        10 => parsed.digits().to_owned(),
        16 => BigInt::parse_bytes(parsed.digits().as_bytes(), 16)
            .ok_or_else(|| anyhow!("numeric literal `{raw}` is not supported"))?
            .to_string(),
        2 => BigInt::parse_bytes(parsed.digits().as_bytes(), 2)
            .ok_or_else(|| anyhow!("numeric literal `{raw}` is not supported"))?
            .to_string(),
        _ => bail!("numeric literal `{raw}` is not supported"),
    };

    Ok(RenderedValue::typed_leaf(normalized, "int"))
}

fn render_string_literal(raw: &str) -> RenderedValue {
    RenderedValue::typed_leaf(raw, "string")
}

fn render_null() -> RenderedValue {
    RenderedValue::typed_leaf("null", "null")
}

fn render_bool(value: bool) -> RenderedValue {
    RenderedValue::typed_leaf(value.to_string(), "bool")
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedValuePath, PathSegment, evaluate_condition_expression, evaluate_expression,
        parse_expr_to_path, parse_wrapped_source, wrapped_expression,
    };
    use crate::replayer::LocalVarRendered;
    use crate::types_render::{RenderedValue, render_runtime_vm_value};
    use anyhow::anyhow;
    use tolkc::source_map::SourceMap;
    use tycho_types::boc::Boc;
    use tycho_types::cell::CellBuilder;
    use vmlogs::parser::{CellLike, CellSlice, VmStackValue};

    fn parse_value_path(input: &str) -> anyhow::Result<ParsedValuePath> {
        let source_file = parse_wrapped_source(input)?;
        let expr = wrapped_expression(&source_file)
            .ok_or_else(|| anyhow!("expected a single expression statement"))?;
        parse_expr_to_path(expr, source_file.source.as_ref())
    }

    fn foo_source_map() -> SourceMap {
        serde_json::from_value(serde_json::json!({
            "files": [],
            "declarations": [{
                "kind": "struct",
                "name": "Foo",
                "ident_loc": [0, 0, 0, 0, 0],
                "fields": [{
                    "name": "value",
                    "ty": {"kind": "uintN", "n": 32}
                }]
            }],
            "unique_ty": [],
            "functions": [],
            "debug_marks": []
        }))
        .expect("valid source map")
    }

    fn foo_value_cell() -> tycho_types::cell::Cell {
        let mut builder = CellBuilder::new();
        builder.store_u32(42).expect("must store field");
        builder.build().expect("must build cell")
    }

    #[test]
    fn parses_dot_and_numeric_index_segments() {
        let path = parse_value_path(" foo.bar.0.baz.1 ").expect("path should parse");
        assert_eq!(
            path,
            ParsedValuePath {
                root: "foo".to_owned(),
                segments: vec![
                    PathSegment::Field("bar".to_owned()),
                    PathSegment::Index(0),
                    PathSegment::Field("baz".to_owned()),
                    PathSegment::Index(1),
                ],
            }
        );
    }

    #[test]
    fn parses_backticked_identifiers_and_not_null_operators() {
        let path = parse_value_path("`foo bar`!.`child value`.2!.baz").expect("path should parse");
        assert_eq!(
            path,
            ParsedValuePath {
                root: "foo bar".to_owned(),
                segments: vec![
                    PathSegment::Field("child value".to_owned()),
                    PathSegment::Index(2),
                    PathSegment::Field("baz".to_owned()),
                ],
            }
        );
    }

    #[test]
    fn resolves_nested_fields_indices_and_last_seen_values() {
        let locals = vec![LocalVarRendered {
            var_name: "foo".to_owned(),
            value: RenderedValue::Struct {
                type_name: "Root".to_owned(),
                fields: vec![(
                    "bar".to_owned(),
                    RenderedValue::ArrayOf {
                        type_name: "Bar[]".to_owned(),
                        items: vec![RenderedValue::LastSeen {
                            inner: Box::new(RenderedValue::Struct {
                                type_name: "Leaf".to_owned(),
                                fields: vec![(
                                    "baz".to_owned(),
                                    RenderedValue::typed_leaf("42", "int"),
                                )],
                            }),
                        }],
                    },
                )],
            },
        }];

        let value =
            evaluate_expression(&locals, None, "foo.bar.0.baz").expect("path should resolve");
        assert_eq!(value.to_string(), "42");
    }

    #[test]
    fn prefers_last_visible_variable_when_names_shadow() {
        let locals = vec![
            LocalVarRendered {
                var_name: "foo".to_owned(),
                value: RenderedValue::typed_leaf("1", "int"),
            },
            LocalVarRendered {
                var_name: "foo".to_owned(),
                value: RenderedValue::typed_leaf("2", "int"),
            },
        ];

        let value = evaluate_expression(&locals, None, "foo").expect("path should resolve");
        assert_eq!(value.to_string(), "2");
    }

    #[test]
    fn rejects_function_calls_explicitly() {
        let err = parse_value_path("foo()").expect_err("call syntax should not parse");
        assert_eq!(err.to_string(), "function calls are not supported");
    }

    #[test]
    fn rejects_function_calls_inside_paths_explicitly() {
        let err = parse_value_path("foo().bar").expect_err("call syntax should not parse");
        assert_eq!(err.to_string(), "function calls are not supported");
    }

    #[test]
    fn rejects_bracket_index_syntax() {
        let err = parse_value_path("foo[0]").expect_err("bracket syntax should not parse");
        assert_eq!(err.to_string(), "expected a variable path");
    }

    #[test]
    fn evaluates_basic_logical_operators() {
        let locals = vec![LocalVarRendered {
            var_name: "foo".to_owned(),
            value: RenderedValue::Struct {
                type_name: "Flags".to_owned(),
                fields: vec![
                    (
                        "enabled".to_owned(),
                        RenderedValue::typed_leaf("true", "bool"),
                    ),
                    (
                        "blocked".to_owned(),
                        RenderedValue::typed_leaf("false", "bool"),
                    ),
                ],
            },
        }];

        let value = evaluate_expression(&locals, None, "foo.enabled && (!foo.blocked || false)")
            .expect("logical expression should resolve");
        assert_eq!(value.to_string(), "true");
    }

    #[test]
    fn evaluates_boolean_conditions() {
        let locals = vec![LocalVarRendered {
            var_name: "flag".to_owned(),
            value: RenderedValue::typed_leaf("true", "bool"),
        }];

        assert!(
            evaluate_condition_expression(&locals, "flag && true")
                .expect("boolean condition should resolve")
        );
    }

    #[test]
    fn logical_operators_short_circuit() {
        let locals = Vec::new();

        let value = evaluate_expression(&locals, None, "false && missing.flag")
            .expect("short-circuit should avoid rhs lookup");
        assert_eq!(value.to_string(), "false");

        let value = evaluate_expression(&locals, None, "true || missing.flag")
            .expect("short-circuit should avoid rhs lookup");
        assert_eq!(value.to_string(), "true");
    }

    #[test]
    fn rejects_non_boolean_logical_operands() {
        let locals = vec![LocalVarRendered {
            var_name: "count".to_owned(),
            value: RenderedValue::typed_leaf("42", "int"),
        }];

        let err = evaluate_expression(&locals, None, "count && true")
            .expect_err("non-boolean operand should be rejected");
        assert_eq!(
            err.to_string(),
            "logical operators require boolean operands, got `42`"
        );
    }

    #[test]
    fn evaluates_equality_and_inequality_by_rendered_text() {
        let locals = vec![
            LocalVarRendered {
                var_name: "lhs".to_owned(),
                value: RenderedValue::typed_leaf("42", "int"),
            },
            LocalVarRendered {
                var_name: "rhs".to_owned(),
                value: RenderedValue::typed_leaf("42", "uint32"),
            },
            LocalVarRendered {
                var_name: "other".to_owned(),
                value: RenderedValue::typed_leaf("7", "int"),
            },
        ];

        let value =
            evaluate_expression(&locals, None, "lhs == rhs").expect("equality should resolve");
        assert_eq!(value.to_string(), "true");

        let value =
            evaluate_expression(&locals, None, "lhs != other").expect("inequality should resolve");
        assert_eq!(value.to_string(), "true");
    }

    #[test]
    fn evaluates_numeric_comparisons() {
        let locals = vec![
            LocalVarRendered {
                var_name: "small".to_owned(),
                value: RenderedValue::typed_leaf("7", "int"),
            },
            LocalVarRendered {
                var_name: "big".to_owned(),
                value: RenderedValue::typed_leaf("42", "int"),
            },
        ];

        assert_eq!(
            evaluate_expression(&locals, None, "small < big")
                .expect("comparison should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "big >= small")
                .expect("comparison should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "big >= 42")
                .expect("comparison should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "small > -1")
                .expect("negative literal comparison should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "-small < 0")
                .expect("unary minus on variables should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "small < 10")
                .expect("literal comparison should resolve")
                .to_string(),
            "true"
        );
    }

    #[test]
    fn rejects_non_numeric_ordering_operands() {
        let locals = vec![LocalVarRendered {
            var_name: "name".to_owned(),
            value: RenderedValue::typed_leaf("alice", "string"),
        }];

        let err = evaluate_expression(&locals, None, "name < 10")
            .expect_err("non-numeric ordering operand should be rejected");
        assert_eq!(err.to_string(), "operator `<` requires numeric operands");
    }

    #[test]
    fn evaluates_string_literals_and_compares_them() {
        let locals = vec![LocalVarRendered {
            var_name: "name".to_owned(),
            value: RenderedValue::typed_leaf("\"alice\"", "string"),
        }];

        assert_eq!(
            evaluate_expression(&locals, None, "\"alice\"")
                .expect("string literal should resolve")
                .to_string(),
            "\"alice\""
        );
        assert_eq!(
            evaluate_expression(&locals, None, "name == \"alice\"")
                .expect("string equality should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "name != \"bob\"")
                .expect("string inequality should resolve")
                .to_string(),
            "true"
        );
    }

    #[test]
    fn evaluates_null_literals_and_compares_them() {
        let locals = vec![
            LocalVarRendered {
                var_name: "missing".to_owned(),
                value: RenderedValue::typed_leaf("null", "address?"),
            },
            LocalVarRendered {
                var_name: "present".to_owned(),
                value: RenderedValue::typed_leaf("7", "int"),
            },
        ];

        assert_eq!(
            evaluate_expression(&locals, None, "null")
                .expect("null literal should resolve")
                .to_string(),
            "null"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "missing == null")
                .expect("null equality should resolve")
                .to_string(),
            "true"
        );
        assert_eq!(
            evaluate_expression(&locals, None, "present != null")
                .expect("null inequality should resolve")
                .to_string(),
            "true"
        );
    }

    #[test]
    fn evaluates_cell_cast_to_typed_cell_from_source_map() {
        let source_map = foo_source_map();
        let cell = foo_value_cell();
        let locals = vec![LocalVarRendered {
            var_name: "payload".to_owned(),
            value: render_runtime_vm_value(&VmStackValue::Cell(CellLike::Cell(Boc::encode_hex(
                &cell,
            )))),
        }];

        let rendered = evaluate_expression(&locals, Some(&source_map), "payload as Cell<Foo>")
            .expect("cell cast should decode");

        let RenderedValue::CellOf {
            type_name, fields, ..
        } = rendered
        else {
            panic!("expected Cell<Foo>");
        };
        assert_eq!(type_name, "Cell<Foo>");
        assert_eq!(fields[0].0, "decoded");
        let RenderedValue::Struct {
            type_name,
            fields: decoded_fields,
        } = &fields[0].1
        else {
            panic!("expected decoded Foo");
        };
        assert_eq!(type_name, "Foo");
        assert_eq!(decoded_fields[0].0, "value");
        assert_eq!(decoded_fields[0].1.dap_parts().0, "42");
        assert_eq!(decoded_fields[0].1.dap_parts().1.as_deref(), Some("uint32"));
    }

    #[test]
    fn evaluates_slice_cast_to_typed_cell_from_source_map() {
        let source_map = foo_source_map();
        let cell = foo_value_cell();
        let locals = vec![LocalVarRendered {
            var_name: "payload".to_owned(),
            value: render_runtime_vm_value(&VmStackValue::CellSlice(CellSlice {
                value: Boc::encode_hex(&cell),
                bits: None,
                refs: None,
            })),
        }];

        let rendered = evaluate_expression(&locals, Some(&source_map), "payload as Cell<Foo>")
            .expect("slice cast should decode");

        let RenderedValue::CellOf { fields, .. } = rendered else {
            panic!("expected Cell<Foo>");
        };
        let RenderedValue::Struct {
            fields: decoded_fields,
            ..
        } = &fields[0].1
        else {
            panic!("expected decoded Foo");
        };
        assert_eq!(decoded_fields[0].0, "value");
        assert_eq!(decoded_fields[0].1.dap_parts().0, "42");
        assert_eq!(decoded_fields[0].1.dap_parts().1.as_deref(), Some("uint32"));
    }
}
