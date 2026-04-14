use crate::core::debug_executor_handle::tuple_item_to_vm_stack_value;
use crate::core::evaluate::evaluate_expression;
use crate::core::replayer::{EvaluateLocalVar, LocalVarRendered, TolkReplayer};
use crate::core::types_render::{RenderedValue, SlotValue, debug_print_from_stack};
use acton_config::config::ActonConfig;
use anyhow::{Context, Result, anyhow, bail};
use num_bigint::BigInt;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::Builder;
use tolk_syntax::{
    Expr, FuncBody, FunctionLike, HasName, MatchArmBody, MatchPattern, Stmt, TopLevel,
};
use tolkc::Compiler;
use tolkc::source_map::SourceMap;
use tolkc::types_kernel::{Ty, calc_width_on_stack, instantiate_generics};
use ton_executor::get::{GetExecutor, GetMethodResult, RunGetMethodArgs};
use tvmffi::stack::{Tuple, TupleItem};
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellBuilder, CellFamily, Store};
use tycho_types::models::{StdAddr, StdAddrFormat};
use vmlogs::parser::{CellLike, CellSlice as VmCellSlice, VmStackValue};

const DEBUG_EVAL2_METHOD_ID: i32 = 119_973;
const DEBUG_EVAL2_FUNCTION_NAME: &str = "__acton_debug_eval2__";

pub(crate) fn evaluate_expression_in_replayer_with_fallback(
    replayer: &TolkReplayer,
    depth: usize,
    expression: &str,
) -> Result<RenderedValue> {
    evaluate_expression2_with_fallback(
        &replayer.evaluate_locals_for_frame(depth),
        &replayer.locals_for_frame(depth),
        replayer.source_map(),
        replayer.file_id_for_frame(depth),
        expression,
    )
}

pub(crate) fn evaluate_expression_with_fallback(
    locals: &[LocalVarRendered],
    source_map: Option<&SourceMap>,
    _current_file_id: Option<usize>,
    expression: &str,
) -> Result<RenderedValue> {
    evaluate_expression(locals, source_map, expression)
}

fn evaluate_expression2_with_fallback(
    typed_locals: &[EvaluateLocalVar],
    legacy_locals: &[LocalVarRendered],
    source_map: &SourceMap,
    current_file_id: Option<usize>,
    expression: &str,
) -> Result<RenderedValue> {
    let Some(current_file_id) = current_file_id else {
        return evaluate_expression(legacy_locals, Some(source_map), expression);
    };

    match evaluate_expression2(typed_locals, source_map, current_file_id, expression) {
        Ok(value) => Ok(value),
        Err(eval2_err) => {
            evaluate_expression(legacy_locals, Some(source_map), expression).or(Err(eval2_err))
        }
    }
}

pub(crate) fn evaluate_expression2(
    locals: &[EvaluateLocalVar],
    source_map: &SourceMap,
    current_file_id: usize,
    expression: &str,
) -> Result<RenderedValue> {
    let source_file = parse_wrapped_source(expression)?;
    let expr = wrapped_expression(&source_file)
        .ok_or_else(|| anyhow!("expected a single expression statement"))?;
    let referenced_locals = collect_referenced_local_names(expr, source_file.source.as_ref());

    let current_file_path = source_map
        .resolve_file_full_path(current_file_id)
        .ok_or_else(|| anyhow!("Current source file is not available for evaluate2"))?;
    if current_file_path.starts_with("@stdlib/") {
        bail!("evaluate2 cannot compile against stdlib-only virtual files");
    }

    let original_source = fs::read_to_string(current_file_path)
        .with_context(|| format!("Failed to read current source file `{current_file_path}`"))?;
    let (evaluate_params, param_issues) = collect_evaluate_params(locals, source_map);
    validate_referenced_locals(&referenced_locals, &param_issues)?;
    let generated_source = build_generated_source(&original_source, &evaluate_params, expression);

    let current_file_dir = Path::new(current_file_path)
        .parent()
        .ok_or_else(|| anyhow!("Current source file has no parent directory"))?;
    let mut temp_file = Builder::new()
        .prefix("acton-debug-evaluate2-")
        .suffix(".tolk")
        .tempfile_in(current_file_dir)
        .with_context(|| {
            format!(
                "Failed to create temporary evaluate2 file in `{}`",
                current_file_dir.display()
            )
        })?;
    temp_file
        .write_all(generated_source.as_bytes())
        .context("Failed to write temporary evaluate2 source")?;
    temp_file
        .flush()
        .context("Failed to flush temporary evaluate2 source")?;

    let mappings = ActonConfig::load()
        .ok()
        .and_then(|config| config.mappings());
    let compiler = Compiler::new(2)
        .with_allow_no_entrypoint(true)
        .with_mappings(&mappings);

    let compiled = match compiler.compile(temp_file.path(), true) {
        tolkc::CompilerResult::Success(result) => result,
        tolkc::CompilerResult::Error(error) => bail!("evaluate2 compile failed: {}", error.message),
    };

    let stack_b64 = Boc::encode_base64(
        Tuple(
            evaluate_params
                .iter()
                .flat_map(|param| param.stack_items.iter().cloned())
                .collect(),
        )
        .serialize()
        .context("Failed to serialize evaluate2 argument stack")?,
    );
    let empty_data_b64 = Boc::encode_base64(
        CellBuilder::new()
            .build()
            .context("Failed to build empty data cell")?,
    );

    let params = RunGetMethodArgs {
        code: compiled.code_boc64.clone(),
        data: empty_data_b64,
        method_id: DEBUG_EVAL2_METHOD_ID,
        debug_enabled: true,
        ..Default::default()
    };
    let executor = GetExecutor::new(&params).context("Failed to create evaluate2 executor")?;
    let result = executor
        .run_get_method(&stack_b64, &params, None)
        .context("Failed to run evaluate2 getter")?;

    let return_ty = helper_return_ty(
        compiled
            .new_source_map
            .as_ref()
            .ok_or_else(|| anyhow!("evaluate2 compile result does not contain a source map"))?,
    )?;
    render_evaluate_result(result, source_map, &return_ty)
}

fn parse_wrapped_source(input: &str) -> Result<tolk_syntax::SourceFile> {
    let wrapped = format!("fun __acton_debug_eval2_syntax__() {{ {input}; }}");
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

fn collect_evaluate_params(
    locals: &[EvaluateLocalVar],
    source_map: &SourceMap,
) -> (Vec<EvaluateParam>, HashMap<String, EvaluateParamIssue>) {
    let mut params = Vec::new();
    let mut issues = HashMap::new();
    let mut seen = HashSet::new();

    for local in locals.iter().rev() {
        let name = normalize_identifier(&local.var_name);
        if !seen.insert(name.to_owned()) {
            continue;
        }
        match stack_items_for_local(source_map, local) {
            Ok(stack_items) => {
                params.push(EvaluateParam {
                    name: name.to_owned(),
                    ty: local.ty.clone(),
                    stack_items,
                });
            }
            Err(err) => {
                issues.insert(
                    name.to_owned(),
                    EvaluateParamIssue {
                        name: name.to_owned(),
                        ty: local.ty.clone(),
                        reason: err.to_string(),
                    },
                );
            }
        }
    }

    params.reverse();
    (params, issues)
}

fn build_generated_source(
    original_source: &str,
    evaluate_params: &[EvaluateParam],
    expression: &str,
) -> String {
    let mut generated = String::with_capacity(original_source.len() + expression.len() + 256);
    generated.push_str(original_source);
    if !generated.ends_with('\n') {
        generated.push('\n');
    }

    generated.push('\n');
    generated.push_str("// Temporary debugger evaluate2 helper.\n");
    generated.push_str(&format!("@method_id({DEBUG_EVAL2_METHOD_ID})\n"));
    generated.push_str(&format!("fun {DEBUG_EVAL2_FUNCTION_NAME}("));
    for (index, param) in evaluate_params.iter().enumerate() {
        if index > 0 {
            generated.push_str(", ");
        }
        generated.push_str(&render_identifier(&param.name));
        generated.push_str(": ");
        generated.push_str(&render_helper_type(&param.ty));
    }
    generated.push_str(") {\n");
    generated.push_str("    return ");
    generated.push_str(expression);
    generated.push_str(";\n}\n");
    generated
}

fn render_helper_type(ty: &Ty) -> String {
    match ty {
        Ty::Nullable { inner, .. } => {
            let needs_parentheses = needs_parentheses_in_nullable(inner);
            let inner = render_helper_type(inner);
            if needs_parentheses {
                format!("({inner})?")
            } else {
                format!("{inner}?")
            }
        }
        Ty::CellOf { inner } => format!("Cell<{}>", render_helper_type(inner)),
        Ty::ArrayOf { inner } => format!("array<{}>", render_helper_type(inner)),
        Ty::LispListOf { inner } => format!("lisp_list<{}>", render_helper_type(inner)),
        Ty::Tensor { items } => format!(
            "({})",
            items
                .iter()
                .map(render_helper_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Ty::ShapedTuple { items } => format!(
            "[{}]",
            items
                .iter()
                .map(render_helper_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Ty::MapKV { k, v } => {
            format!("map<{}, {}>", render_helper_type(k), render_helper_type(v))
        }
        Ty::StructRef {
            struct_name,
            type_args,
        } => render_named_helper_type(struct_name, type_args.as_deref()),
        Ty::AliasRef {
            alias_name,
            type_args,
        } => render_named_helper_type(alias_name, type_args.as_deref()),
        Ty::Union { variants, .. } => variants
            .iter()
            .map(|variant| render_helper_type(&variant.variant_ty))
            .collect::<Vec<_>>()
            .join(" | "),
        _ => ty.render_type(),
    }
}

fn needs_parentheses_in_nullable(ty: &Ty) -> bool {
    match ty {
        Ty::Union { .. } => true,
        Ty::AliasRef {
            alias_name: _,
            type_args,
        }
        | Ty::StructRef {
            struct_name: _,
            type_args,
        } => type_args
            .as_deref()
            .is_some_and(|type_args| type_args.iter().any(needs_parentheses_in_nullable)),
        Ty::CellOf { inner } | Ty::ArrayOf { inner } | Ty::LispListOf { inner } => {
            needs_parentheses_in_nullable(inner)
        }
        Ty::Tensor { items } | Ty::ShapedTuple { items } => {
            items.iter().any(needs_parentheses_in_nullable)
        }
        Ty::MapKV { k, v } => needs_parentheses_in_nullable(k) || needs_parentheses_in_nullable(v),
        _ => false,
    }
}

fn render_named_helper_type(name: &str, type_args: Option<&[Ty]>) -> String {
    let Some(type_args) = type_args else {
        return name.to_owned();
    };
    format!(
        "{}<{}>",
        name,
        type_args
            .iter()
            .map(render_helper_type)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn helper_return_ty(source_map: &SourceMap) -> Result<Ty> {
    let mut f_idx = 0usize;
    loop {
        let Some(function) = source_map.get_function_by_idx(f_idx) else {
            bail!("evaluate2 helper source-map entry was not emitted");
        };
        if function.name == DEBUG_EVAL2_FUNCTION_NAME {
            return source_map
                .resolve_ty(function.return_ty_idx)
                .cloned()
                .ok_or_else(|| anyhow!("evaluate2 helper return type is missing from source map"));
        }
        f_idx += 1;
    }
}

fn render_evaluate_result(
    result: GetMethodResult,
    source_map: &SourceMap,
    return_ty: &Ty,
) -> Result<RenderedValue> {
    let success = match result {
        GetMethodResult::Success(success) => success,
        GetMethodResult::Error(error) => bail!("evaluate2 execution failed: {}", error.error),
    };

    if success.vm_exit_code != 0 {
        bail!(
            "evaluate2 execution failed with VM exit code {}{}",
            success.vm_exit_code,
            render_vm_log_suffix(success.vm_log.as_ref())
        );
    }

    let stack_cell = Boc::decode_base64(success.stack.as_ref())
        .context("evaluate2 returned an invalid stack BoC")?;
    let stack = Tuple::deserialize(&stack_cell).context("Failed to decode evaluate2 stack")?;
    let stack_values: Vec<_> = stack.iter().map(tuple_item_to_vm_stack_value).collect();
    let slots: Vec<_> = stack_values.iter().map(SlotValue::Live).collect();
    Ok(debug_print_from_stack(source_map, &slots, return_ty))
}

fn stack_items_for_local(
    source_map: &SourceMap,
    local: &EvaluateLocalVar,
) -> Result<Vec<TupleItem>> {
    if let Some(raw_slots) = &local.raw_slots {
        match vm_stack_values_to_tuple_items(raw_slots) {
            Ok(stack_items) => return Ok(stack_items),
            Err(raw_err) if supports_evaluate_param_ty(source_map, &local.ty) => {}
            Err(raw_err) => {
                bail!("runtime value cannot be materialized: {raw_err}");
            }
        }
    }

    if local.raw_slots.is_none() && !supports_evaluate_param_ty(source_map, &local.ty) {
        bail!(
            "value is not fully available in the current frame; some stack slots were optimized out"
        );
    }

    if !supports_evaluate_param_ty(source_map, &local.ty) {
        bail!(
            "type `{}` is not supported without exact runtime stack slots",
            local.ty.render_type()
        );
    }

    Ok(vec![rendered_value_to_tuple_item(
        source_map,
        &local.ty,
        &local.value,
    )?])
}

fn vm_stack_values_to_tuple_items(raw_slots: &[VmStackValue]) -> Result<Vec<TupleItem>> {
    raw_slots.iter().map(vm_stack_value_to_tuple_item).collect()
}

fn vm_stack_value_to_tuple_item(value: &VmStackValue) -> Result<TupleItem> {
    match value {
        VmStackValue::Null => Ok(TupleItem::Null),
        VmStackValue::NaN => Ok(TupleItem::Nan),
        VmStackValue::Integer(value) => value
            .parse::<BigInt>()
            .map(TupleItem::Int)
            .map_err(|err| anyhow!("Failed to parse integer `{value}`: {err}")),
        VmStackValue::Tuple(items) => Ok(TupleItem::Tuple(Tuple(vm_stack_values_to_tuple_items(
            items,
        )?))),
        VmStackValue::Cell(CellLike::Cell(raw)) => {
            Ok(TupleItem::Cell(Boc::decode_hex(raw).map_err(|err| {
                anyhow!("Failed to decode cell hex `{raw}`: {err}")
            })?))
        }
        VmStackValue::Cell(CellLike::Builder(raw)) | VmStackValue::Builder(raw) => {
            Ok(TupleItem::Builder(Boc::decode_hex(raw).map_err(|err| {
                anyhow!("Failed to decode builder hex `{raw}`: {err}")
            })?))
        }
        VmStackValue::CellSlice(slice) => Ok(TupleItem::Slice(
            exact_slice_cell(slice).ok_or_else(|| {
                anyhow!(
                    "Failed to materialize slice from VM stack value `{}`",
                    VmStackValue::CellSlice(slice.clone())
                )
            })?,
        )),
        VmStackValue::String(value) => {
            let mut tuple = Tuple::empty();
            tuple.push_string_slice(value);
            tuple
                .pop()
                .ok_or_else(|| anyhow!("Failed to materialize string argument"))
        }
        VmStackValue::Continuation(_) => {
            bail!("contains a continuation stack value, which evaluate2 cannot pass yet")
        }
        VmStackValue::Unknown => {
            bail!("contains an unknown runtime stack value, which evaluate2 cannot pass yet")
        }
    }
}

fn validate_referenced_locals(
    referenced_locals: &[String],
    param_issues: &HashMap<String, EvaluateParamIssue>,
) -> Result<()> {
    let issues: Vec<_> = referenced_locals
        .iter()
        .filter_map(|name| param_issues.get(name))
        .collect();

    if issues.is_empty() {
        return Ok(());
    }

    if issues.len() == 1 {
        let issue = issues[0];
        bail!(
            "cannot evaluate `{}`: {}",
            issue.render_subject(),
            issue.reason
        );
    }

    let details = issues
        .iter()
        .map(|issue| format!("- `{}`: {}", issue.render_subject(), issue.reason))
        .collect::<Vec<_>>()
        .join("\n");
    bail!(
        "cannot evaluate expression because some referenced locals are unavailable:\n{}",
        details
    );
}

fn collect_referenced_local_names(expr: Expr<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_referenced_local_names_into(expr, source, &mut names, &mut seen);
    names
}

fn collect_referenced_local_names_into(
    expr: Expr<'_>,
    source: &str,
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    match expr {
        Expr::VarDeclLhs(_) => {}
        Expr::Assign(assign) => {
            if let Some(left) = assign.left() {
                collect_referenced_local_names_into(left, source, names, seen);
            }
            if let Some(right) = assign.right() {
                collect_referenced_local_names_into(right, source, names, seen);
            }
        }
        Expr::SetAssign(assign) => {
            if let Some(left) = assign.left() {
                collect_referenced_local_names_into(left, source, names, seen);
            }
            if let Some(right) = assign.right() {
                collect_referenced_local_names_into(right, source, names, seen);
            }
        }
        Expr::Ternary(ternary) => {
            if let Some(condition) = ternary.condition() {
                collect_referenced_local_names_into(condition, source, names, seen);
            }
            if let Some(consequence) = ternary.consequence() {
                collect_referenced_local_names_into(consequence, source, names, seen);
            }
            if let Some(alternative) = ternary.alternative() {
                collect_referenced_local_names_into(alternative, source, names, seen);
            }
        }
        Expr::Bin(bin) => {
            if let Some(left) = bin.left() {
                collect_referenced_local_names_into(left, source, names, seen);
            }
            if let Some(right) = bin.right() {
                collect_referenced_local_names_into(right, source, names, seen);
            }
        }
        Expr::Unary(unary) => {
            if let Some(argument) = unary.argument() {
                collect_referenced_local_names_into(argument, source, names, seen);
            }
        }
        Expr::Lazy(lazy) => {
            if let Some(argument) = lazy.expr() {
                collect_referenced_local_names_into(argument, source, names, seen);
            }
        }
        Expr::AsCast(as_cast) => {
            if let Some(argument) = as_cast.expr() {
                collect_referenced_local_names_into(argument, source, names, seen);
            }
        }
        Expr::IsType(is_type) => {
            if let Some(argument) = is_type.expr() {
                collect_referenced_local_names_into(argument, source, names, seen);
            }
        }
        Expr::NotNull(not_null) => {
            if let Some(argument) = not_null.inner() {
                collect_referenced_local_names_into(argument, source, names, seen);
            }
        }
        Expr::DotAccess(dot_access) => {
            if let Some(obj) = dot_access.obj() {
                collect_referenced_local_names_into(obj, source, names, seen);
            }
        }
        Expr::Call(call) => {
            if let Some(callee) = call.callee() {
                collect_referenced_local_names_into(callee, source, names, seen);
            }
            for argument in call.arguments() {
                if let Some(argument_expr) = argument.expr() {
                    collect_referenced_local_names_into(argument_expr, source, names, seen);
                }
            }
        }
        Expr::Instantiation(instantiation) => {
            if let Some(inner) = instantiation.expr() {
                collect_referenced_local_names_into(inner, source, names, seen);
            }
        }
        Expr::Paren(paren) => {
            if let Some(inner) = paren.inner() {
                collect_referenced_local_names_into(inner, source, names, seen);
            }
        }
        Expr::Match(match_expr) => {
            if let Some(scrutinee) = match_expr.expr() {
                collect_referenced_local_names_into(scrutinee, source, names, seen);
            }
            for arm in match_expr.arms() {
                if let MatchPattern::Expr(pattern_expr) = arm.pattern() {
                    collect_referenced_local_names_into(pattern_expr, source, names, seen);
                }
                match arm.body() {
                    Some(MatchArmBody::Expr(arm_expr)) => {
                        collect_referenced_local_names_into(arm_expr, source, names, seen);
                    }
                    Some(MatchArmBody::Return(return_stmt)) => {
                        if let Some(return_expr) = return_stmt.expr() {
                            collect_referenced_local_names_into(return_expr, source, names, seen);
                        }
                    }
                    Some(MatchArmBody::Throw(throw_stmt)) => {
                        if let Some(throw_expr) = throw_stmt.expr() {
                            collect_referenced_local_names_into(throw_expr, source, names, seen);
                        }
                    }
                    Some(MatchArmBody::Block(_)) | None => {}
                }
            }
        }
        Expr::ObjectLit(object_lit) => {
            for argument in object_lit.arguments() {
                if let Some(value) = argument.value() {
                    collect_referenced_local_names_into(value, source, names, seen);
                } else if let Some(name) = argument.name() {
                    push_referenced_local_name(name.normalized_name(source), names, seen);
                }
            }
        }
        Expr::Tensor(tensor) => {
            for element in tensor.elements() {
                collect_referenced_local_names_into(element, source, names, seen);
            }
        }
        Expr::Tuple(tuple) => {
            for element in tuple.elements() {
                collect_referenced_local_names_into(element, source, names, seen);
            }
        }
        Expr::Lambda(lambda) => {
            for parameter in lambda.parameters() {
                if let Some(default) = parameter.default() {
                    collect_referenced_local_names_into(default, source, names, seen);
                }
            }
        }
        Expr::Ident(ident) => {
            push_referenced_local_name(ident.normalized_name(source), names, seen);
        }
        Expr::NumberLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::NullLit(_)
        | Expr::Underscore(_)
        | Expr::Unmapped(_) => {}
    }
}

fn push_referenced_local_name(name: &str, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    if seen.insert(name.to_owned()) {
        names.push(name.to_owned());
    }
}

fn render_vm_log_suffix(vm_log: &str) -> String {
    let vm_log = vm_log.trim();
    if vm_log.is_empty() {
        String::new()
    } else {
        format!("\nget-method VM log:\n{vm_log}")
    }
}

fn supports_evaluate_param_ty(source_map: &SourceMap, ty: &Ty) -> bool {
    match ty {
        Ty::Int
        | Ty::IntN { .. }
        | Ty::UintN { .. }
        | Ty::VarintN { .. }
        | Ty::VaruintN { .. }
        | Ty::Coins
        | Ty::Bool
        | Ty::Cell
        | Ty::CellOf { .. }
        | Ty::Builder
        | Ty::Slice
        | Ty::String
        | Ty::Remaining
        | Ty::Address
        | Ty::AddressOpt
        | Ty::AddressExt
        | Ty::AddressAny
        | Ty::BitsN { .. }
        | Ty::NullLiteral => true,
        Ty::Nullable { inner, .. } => {
            calc_width_on_stack(source_map, ty) == 1
                && supports_evaluate_param_ty(source_map, inner)
        }
        Ty::AliasRef {
            alias_name,
            type_args,
        } => {
            let alias = source_map.get_alias(alias_name);
            let target = instantiate_generics(
                &alias.target_ty,
                alias.type_params.as_deref().unwrap_or(&[]),
                type_args.as_deref().unwrap_or(&[]),
            );
            supports_evaluate_param_ty(source_map, &target)
        }
        Ty::EnumRef { enum_name } => {
            let encoded_as = source_map.get_enum(enum_name).encoded_as.clone();
            supports_evaluate_param_ty(source_map, &encoded_as)
        }
        _ => false,
    }
}

fn rendered_value_to_tuple_item(
    source_map: &SourceMap,
    ty: &Ty,
    value: &RenderedValue,
) -> Result<TupleItem> {
    match ty {
        Ty::Int
        | Ty::IntN { .. }
        | Ty::UintN { .. }
        | Ty::VarintN { .. }
        | Ty::VaruintN { .. }
        | Ty::Coins => Ok(TupleItem::Int(rendered_value_as_bigint(value)?)),
        Ty::Bool => {
            let mut tuple = Tuple::empty();
            tuple.push_bool(rendered_value_as_bool(value)?);
            tuple
                .pop()
                .ok_or_else(|| anyhow!("Failed to materialize bool argument"))
        }
        Ty::String => {
            let mut tuple = Tuple::empty();
            tuple.push_string_slice(&rendered_value_as_string(value)?);
            tuple
                .pop()
                .ok_or_else(|| anyhow!("Failed to materialize string argument"))
        }
        Ty::Cell | Ty::CellOf { .. } => Ok(TupleItem::Cell(rendered_value_as_cell(value)?)),
        Ty::Builder => Ok(TupleItem::Builder(rendered_value_as_builder(value)?)),
        Ty::Slice | Ty::Remaining | Ty::BitsN { .. } => {
            Ok(TupleItem::Slice(rendered_value_as_slice(value)?))
        }
        Ty::Address | Ty::AddressOpt | Ty::AddressExt | Ty::AddressAny => {
            if rendered_value_is_null(value) {
                return Ok(TupleItem::Null);
            }
            let raw = rendered_value_as_address(value)?;
            let (address, _) = StdAddr::from_str_ext(&raw, StdAddrFormat::any())
                .map_err(|err| anyhow!("Failed to parse address `{raw}`: {err}"))?;
            Ok(TupleItem::Slice(to_cell(&address)))
        }
        Ty::NullLiteral => Ok(TupleItem::Null),
        Ty::Nullable { inner, .. } => {
            if rendered_value_is_null(value) {
                Ok(TupleItem::Null)
            } else {
                rendered_value_to_tuple_item(source_map, inner, value)
            }
        }
        Ty::AliasRef {
            alias_name,
            type_args,
        } => {
            let alias = source_map.get_alias(alias_name);
            let target = instantiate_generics(
                &alias.target_ty,
                alias.type_params.as_deref().unwrap_or(&[]),
                type_args.as_deref().unwrap_or(&[]),
            );
            rendered_value_to_tuple_item(source_map, &target, value)
        }
        Ty::EnumRef { enum_name } => {
            let raw_value = rendered_enum_raw_value(value).unwrap_or(value);
            rendered_value_to_tuple_item(
                source_map,
                &source_map.get_enum(enum_name).encoded_as,
                raw_value,
            )
        }
        _ => bail!(
            "evaluate2 does not support argument type `{}` yet",
            ty.render_type()
        ),
    }
}

fn rendered_value_as_bigint(value: &RenderedValue) -> Result<BigInt> {
    let RenderedValue::Leaf { value, .. } = unwrap_last_seen(value) else {
        bail!("Expected an integer-like leaf value, got `{}`", value);
    };
    value
        .parse::<BigInt>()
        .map_err(|err| anyhow!("Failed to parse integer `{value}`: {err}"))
}

fn rendered_value_as_bool(value: &RenderedValue) -> Result<bool> {
    let RenderedValue::Leaf { value, .. } = unwrap_last_seen(value) else {
        bail!("Expected a bool leaf value, got `{}`", value);
    };
    match value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => bail!("Expected `true` or `false`, got `{other}`"),
    }
}

fn rendered_value_as_string(value: &RenderedValue) -> Result<String> {
    let RenderedValue::Leaf { value, .. } = unwrap_last_seen(value) else {
        bail!("Expected a string leaf value, got `{}`", value);
    };
    if value.starts_with('"') {
        return serde_json::from_str(value)
            .map_err(|err| anyhow!("Failed to decode rendered string `{value}`: {err}"));
    }
    Ok(value.clone())
}

fn rendered_value_as_cell(value: &RenderedValue) -> Result<Cell> {
    match unwrap_last_seen(value).raw_cell_like() {
        Some(CellLike::Cell(raw)) | Some(CellLike::Builder(raw)) => {
            Boc::decode_hex(raw).map_err(|err| anyhow!("Failed to decode cell hex `{raw}`: {err}"))
        }
        None => bail!("Expected a cell-like rendered value, got `{}`", value),
    }
}

fn rendered_value_as_builder(value: &RenderedValue) -> Result<Cell> {
    let Some(raw) = unwrap_last_seen(value).raw_cell_like() else {
        bail!("Expected a builder rendered value, got `{}`", value);
    };
    let CellLike::Builder(raw) = raw else {
        bail!("Expected a builder rendered value, got `{}`", value);
    };
    Boc::decode_hex(raw).map_err(|err| anyhow!("Failed to decode builder hex `{raw}`: {err}"))
}

fn rendered_value_as_slice(value: &RenderedValue) -> Result<Cell> {
    let Some(raw) = unwrap_last_seen(value).raw_cell_like() else {
        bail!("Expected a slice rendered value, got `{}`", value);
    };
    match raw {
        CellLike::Cell(raw) | CellLike::Builder(raw) => {
            Boc::decode_hex(raw).map_err(|err| anyhow!("Failed to decode slice hex `{raw}`: {err}"))
        }
    }
}

fn rendered_value_as_address(value: &RenderedValue) -> Result<String> {
    match unwrap_last_seen(value) {
        RenderedValue::Address { value, .. } => Ok(value.clone()),
        RenderedValue::Leaf { value, .. } => Ok(value.clone()),
        other => bail!("Expected an address value, got `{other}`"),
    }
}

fn rendered_value_is_null(value: &RenderedValue) -> bool {
    matches!(
        unwrap_last_seen(value),
        RenderedValue::Leaf { value, .. } if value == "null"
    )
}

fn rendered_enum_raw_value(value: &RenderedValue) -> Option<&RenderedValue> {
    let RenderedValue::EnumValue { fields, .. } = unwrap_last_seen(value) else {
        return None;
    };
    fields
        .iter()
        .find(|(name, _)| name == "value")
        .map(|(_, value)| value)
}

fn to_cell<T: Store + ?Sized>(value: &T) -> Cell {
    let mut builder = CellBuilder::new();
    value
        .store_into(&mut builder, Cell::empty_context())
        .expect("Failed to store value into cell");
    builder.build().expect("Failed to build cell")
}

fn exact_slice_cell(cs: &VmCellSlice) -> Option<Cell> {
    let cell = Boc::decode_hex(&cs.value).ok()?;
    match (&cs.bits, &cs.refs) {
        (Some((start_bits, end_bits)), Some((start_refs, end_refs))) => {
            let start_bits = start_bits.parse::<u16>().ok()?;
            let end_bits = end_bits.parse::<u16>().ok()?;
            let start_refs = start_refs.parse::<u8>().ok()?;
            let end_refs = end_refs.parse::<u8>().ok()?;

            let mut parser = cell.as_slice_allow_exotic();
            parser.skip_first(start_bits, start_refs).ok()?;

            let bit_len = end_bits.saturating_sub(start_bits);
            let mut root_bits = vec![0u8; bit_len.div_ceil(8) as usize];
            parser.load_raw(&mut root_bits, bit_len).ok()?;

            let mut builder = CellBuilder::new();
            builder.store_raw(&root_bits, bit_len).ok()?;
            for _ in start_refs..end_refs {
                let next_ref = parser.load_reference_cloned().ok()?;
                builder.store_reference(next_ref).ok()?;
            }

            builder.build().ok()
        }
        _ => Some(cell),
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

fn render_identifier(name: &str) -> String {
    if is_plain_identifier(name) && !is_tolk_keyword(name) {
        name.to_owned()
    } else {
        format!("`{name}`")
    }
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_tolk_keyword(name: &str) -> bool {
    matches!(
        name,
        "fun"
            | "get"
            | "val"
            | "var"
            | "return"
            | "if"
            | "else"
            | "match"
            | "while"
            | "repeat"
            | "do"
            | "break"
            | "continue"
            | "throw"
            | "try"
            | "catch"
            | "lazy"
            | "null"
            | "true"
            | "false"
            | "asm"
            | "builtin"
    )
}

#[derive(Debug, Clone)]
struct EvaluateParam {
    name: String,
    ty: Ty,
    stack_items: Vec<TupleItem>,
}

#[derive(Debug, Clone)]
struct EvaluateParamIssue {
    name: String,
    ty: Ty,
    reason: String,
}

impl EvaluateParamIssue {
    fn render_subject(&self) -> String {
        format!("{}: {}", self.name, self.ty.render_type())
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate_expression2;
    use crate::core::replayer::EvaluateLocalVar;
    use crate::types_render::{RenderedValue, render_runtime_vm_value};
    use std::fs;
    use tempfile::tempdir;
    use tolkc::source_map::SourceMap;
    use tolkc::types_kernel::{Ty, UnionVariant};
    use tycho_types::boc::Boc;
    use tycho_types::cell::{Cell, CellBuilder};
    use vmlogs::parser::{CellLike, CellSlice, VmStackValue};

    fn source_map_for_file(path: &str, size_chars: u64) -> SourceMap {
        serde_json::from_value(serde_json::json!({
            "files": [{
                "file_id": 1,
                "file_name": path,
                "size_chars": size_chars
            }],
            "declarations": [],
            "unique_ty": [],
            "functions": [],
            "debug_marks": []
        }))
        .expect("valid source map")
    }

    fn evaluate_local(var_name: &str, ty: Ty, value: RenderedValue) -> EvaluateLocalVar {
        EvaluateLocalVar {
            var_name: var_name.to_owned(),
            ty,
            value,
            raw_slots: None,
        }
    }

    fn evaluate_local_with_raw(
        var_name: &str,
        ty: Ty,
        value: RenderedValue,
        raw_slots: Vec<VmStackValue>,
    ) -> EvaluateLocalVar {
        EvaluateLocalVar {
            var_name: var_name.to_owned(),
            ty,
            value,
            raw_slots: Some(raw_slots),
        }
    }

    fn render_cell(cell: &Cell) -> RenderedValue {
        render_runtime_vm_value(&VmStackValue::Cell(CellLike::Cell(Boc::encode_hex(cell))))
    }

    fn render_slice(cell: &Cell) -> RenderedValue {
        render_runtime_vm_value(&VmStackValue::CellSlice(CellSlice {
            value: Boc::encode_hex(cell),
            bits: None,
            refs: None,
        }))
    }

    fn render_builder(cell: &Cell) -> RenderedValue {
        render_runtime_vm_value(&VmStackValue::Builder(Boc::encode_hex(cell)))
    }

    fn single_uint8_cell(value: u8) -> Cell {
        let mut builder = CellBuilder::new();
        builder
            .store_uint(u64::from(value), 8)
            .expect("store uint8");
        builder.build().expect("build cell")
    }

    fn int_bool_null_union_ty() -> Ty {
        Ty::Union {
            variants: vec![
                UnionVariant {
                    variant_ty: Ty::Int,
                    prefix_str: String::new(),
                    prefix_len: 0,
                    is_prefix_implicit: None,
                    stack_type_id: Some(1),
                    stack_width: Some(1),
                },
                UnionVariant {
                    variant_ty: Ty::Bool,
                    prefix_str: String::new(),
                    prefix_len: 0,
                    is_prefix_implicit: None,
                    stack_type_id: Some(2),
                    stack_width: Some(1),
                },
                UnionVariant {
                    variant_ty: Ty::NullLiteral,
                    prefix_str: String::new(),
                    prefix_len: 0,
                    is_prefix_implicit: None,
                    stack_type_id: Some(0),
                    stack_width: Some(0),
                },
            ],
            stack_width: Some(2),
        }
    }

    fn union_raw_slots_for_variant(
        union_ty: &Ty,
        predicate: impl Fn(&Ty) -> bool,
        variant_values: Vec<VmStackValue>,
    ) -> Vec<VmStackValue> {
        let Ty::Union {
            variants,
            stack_width: Some(stack_width),
        } = union_ty
        else {
            panic!("expected a union with stack width");
        };
        let variant = variants
            .iter()
            .find(|variant| predicate(&variant.variant_ty))
            .expect("expected union to contain the requested variant");
        let variant_width = variant.stack_width.unwrap_or(0);
        let variant_type_id = variant
            .stack_type_id
            .expect("expected union variant to have stack_type_id");

        assert_eq!(
            variant_values.len(),
            variant_width,
            "requested raw union variant width does not match compiled stack width"
        );

        let mut slots = vec![VmStackValue::Null; stack_width - 1 - variant_width];
        slots.extend(variant_values);
        slots.push(VmStackValue::Integer(variant_type_id.to_string()));
        slots
    }

    #[test]
    fn evaluate2_runs_temp_compiled_int_function_call() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_contract.tolk");
        let source = r#"
fun double(x: int): int {
    return x * 2;
}

get fun current(): int {
    return 0;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![
            evaluate_local("x", Ty::Int, RenderedValue::typed_leaf("21", "int")),
            evaluate_local("y", Ty::Int, RenderedValue::typed_leaf("1", "int")),
        ];

        let result = evaluate_expression2(&locals, &source_map, 1, "double(x) + y")
            .expect("evaluate2 should succeed");

        assert_eq!(result.to_string(), "43");
        assert_eq!(result.dap_parts().1.as_deref(), Some("int"));
    }

    #[test]
    fn evaluate2_handles_bool_string_address_and_nullable_primitives() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_primitives.tolk");
        let source = r#"
fun describe(name: string, flag: bool, missing: address?): string {
    if (missing == null && flag) {
        return name;
    }
    return "nope";
}

fun isSameAddress(owner: address): bool {
    return owner == owner;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![
            evaluate_local(
                "name",
                Ty::String,
                RenderedValue::typed_leaf("\"alice\"", "string"),
            ),
            evaluate_local("flag", Ty::Bool, RenderedValue::typed_leaf("true", "bool")),
            evaluate_local(
                "missing",
                Ty::AddressOpt,
                RenderedValue::typed_leaf("null", "address?"),
            ),
            evaluate_local(
                "owner",
                Ty::Address,
                RenderedValue::typed_leaf(
                    "EQC2jeGorIAFh2LXwsDjHfRK-GSo9UzchdIEMh24A7T7AHot",
                    "address",
                ),
            ),
        ];

        let describe =
            evaluate_expression2(&locals, &source_map, 1, "describe(name, flag, missing)")
                .expect("string evaluate2 should succeed");
        assert_eq!(describe.to_string(), "\"alice\"");
        assert_eq!(describe.dap_parts().1.as_deref(), Some("string"));

        let is_same = evaluate_expression2(&locals, &source_map, 1, "isSameAddress(owner)")
            .expect("address evaluate2 should succeed");
        assert_eq!(is_same.to_string(), "true");
        assert_eq!(is_same.dap_parts().1.as_deref(), Some("bool"));
    }

    #[test]
    fn evaluate2_handles_cell_slice_and_builder_params() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_cells.tolk");
        let source = r#"
fun sumKinds(payload: cell, data: slice, scratch: builder): int {
    return payload.beginParse().loadUint(8) + data.preloadUint(8) + scratch.toSlice().loadUint(8);
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let payload_cell = single_uint8_cell(1);
        let data_cell = single_uint8_cell(2);
        let scratch_cell = single_uint8_cell(3);
        let locals = vec![
            evaluate_local("payload", Ty::Cell, render_cell(&payload_cell)),
            evaluate_local("data", Ty::Slice, render_slice(&data_cell)),
            evaluate_local("scratch", Ty::Builder, render_builder(&scratch_cell)),
        ];

        let result =
            evaluate_expression2(&locals, &source_map, 1, "sumKinds(payload, data, scratch)")
                .expect("cell-like evaluate2 should succeed");

        assert_eq!(result.to_string(), "6");
        assert_eq!(result.dap_parts().1.as_deref(), Some("int"));
    }

    #[test]
    fn evaluate2_handles_array_tuple_and_map_params_from_raw_slots() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_collections.tolk");
        let source = r#"
fun sumArray(arr: array<int>): int {
    return arr.get(0) + arr.get(1) + arr.size();
}

fun sumPair(pair: [int, int]): int {
    return pair.0 + pair.1;
}

fun emptyMapFlag(m: map<int32, int32>): bool {
    return m.isEmpty();
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let tuple_items = vec![
            VmStackValue::Integer("7".to_owned()),
            VmStackValue::Integer("9".to_owned()),
        ];
        let locals = vec![
            evaluate_local_with_raw(
                "arr",
                Ty::ArrayOf {
                    inner: Box::new(Ty::Int),
                },
                RenderedValue::leaf("[7, 9]"),
                vec![VmStackValue::Tuple(tuple_items.clone())],
            ),
            evaluate_local_with_raw(
                "pair",
                Ty::ShapedTuple {
                    items: vec![Ty::Int, Ty::Int],
                },
                RenderedValue::leaf("[7, 9]"),
                vec![VmStackValue::Tuple(tuple_items)],
            ),
            evaluate_local_with_raw(
                "m",
                Ty::MapKV {
                    k: Box::new(Ty::IntN { n: 32 }),
                    v: Box::new(Ty::IntN { n: 32 }),
                },
                RenderedValue::leaf("null"),
                vec![VmStackValue::Null],
            ),
        ];

        let array_result = evaluate_expression2(&locals, &source_map, 1, "sumArray(arr)")
            .expect("array evaluate2 should succeed");
        assert_eq!(array_result.to_string(), "18");
        assert_eq!(array_result.dap_parts().1.as_deref(), Some("int"));

        let tuple_result = evaluate_expression2(&locals, &source_map, 1, "sumPair(pair)")
            .expect("tuple evaluate2 should succeed");
        assert_eq!(tuple_result.to_string(), "16");
        assert_eq!(tuple_result.dap_parts().1.as_deref(), Some("int"));

        let map_result = evaluate_expression2(&locals, &source_map, 1, "emptyMapFlag(m)")
            .expect("map evaluate2 should succeed");
        assert_eq!(map_result.to_string(), "true");
        assert_eq!(map_result.dap_parts().1.as_deref(), Some("bool"));
    }

    #[test]
    fn evaluate2_handles_struct_params_from_raw_slots() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_structs.tolk");
        let source = r#"
struct Point {
    x: int
    y: int
}

fun sumPoint(p: Point): int {
    return p.x + p.y;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![evaluate_local_with_raw(
            "p",
            Ty::StructRef {
                struct_name: "Point".to_owned(),
                type_args: None,
            },
            RenderedValue::leaf("Point"),
            vec![
                VmStackValue::Integer("7".to_owned()),
                VmStackValue::Integer("9".to_owned()),
            ],
        )];

        let result = evaluate_expression2(&locals, &source_map, 1, "sumPoint(p)")
            .expect("struct evaluate2 should succeed");

        assert_eq!(result.to_string(), "16");
        assert_eq!(result.dap_parts().1.as_deref(), Some("int"));
    }

    #[test]
    fn evaluate2_handles_inline_union_params_and_results_from_raw_slots() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_inline_union.tolk");
        let source = r#"
fun echo(value: int | bool | null): int | bool | null {
    return value;
}
"#;
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        fs::write(&file_path, source).expect("write source");
        let union_ty = int_bool_null_union_ty();
        let bool_slots = union_raw_slots_for_variant(
            &union_ty,
            |ty| matches!(ty, Ty::Bool),
            vec![VmStackValue::Integer("-1".to_owned())],
        );
        let null_slots =
            union_raw_slots_for_variant(&union_ty, |ty| matches!(ty, Ty::NullLiteral), vec![]);

        let bool_result = evaluate_expression2(
            &[evaluate_local_with_raw(
                "value",
                union_ty.clone(),
                RenderedValue::typed_leaf("true", "bool"),
                bool_slots,
            )],
            &source_map,
            1,
            "echo(value)",
        )
        .expect("inline union bool evaluate2 should succeed");

        let RenderedValue::UnionCase {
            variant_name,
            fields,
            ..
        } = bool_result
        else {
            panic!("expected union case");
        };
        assert_eq!(variant_name, "#bool");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "value");
        assert_eq!(fields[0].1.dap_parts().0, "true");
        assert_eq!(fields[0].1.dap_parts().1.as_deref(), Some("bool"));

        let null_result = evaluate_expression2(
            &[evaluate_local_with_raw(
                "value",
                union_ty,
                RenderedValue::typed_leaf("null", "null"),
                null_slots,
            )],
            &source_map,
            1,
            "echo(value)",
        )
        .expect("inline union null evaluate2 should succeed");

        let RenderedValue::UnionCase {
            variant_name,
            fields,
            ..
        } = null_result
        else {
            panic!("expected union case");
        };
        assert_eq!(variant_name, "#null");
        assert!(fields.is_empty(), "{fields:?}");
    }

    #[test]
    fn evaluate2_handles_named_union_alias_params_from_raw_slots() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_alias_union.tolk");
        let source = r#"
type IntOrBool = int | bool | null

fun asInt(value: IntOrBool): int {
    match (value) {
        int => {
            return value;
        }
        bool => {
            return 100;
        }
        null => {
            return -1;
        }
    }
}
"#;
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        fs::write(&file_path, source).expect("write source");
        let union_ty = int_bool_null_union_ty();
        let int_slots = union_raw_slots_for_variant(
            &union_ty,
            |ty| matches!(ty, Ty::Int),
            vec![VmStackValue::Integer("7".to_owned())],
        );

        let result = evaluate_expression2(
            &[evaluate_local_with_raw(
                "value",
                Ty::AliasRef {
                    alias_name: "IntOrBool".to_owned(),
                    type_args: None,
                },
                RenderedValue::typed_leaf("7", "int"),
                int_slots,
            )],
            &source_map,
            1,
            "asInt(value)",
        )
        .expect("named union alias evaluate2 should succeed");

        assert_eq!(result.to_string(), "7");
        assert_eq!(result.dap_parts().1.as_deref(), Some("int"));
    }

    #[test]
    fn evaluate2_reports_optimized_out_complex_locals_clearly() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_structs_error.tolk");
        let source = r#"
struct Point {
    x: int
    y: int
}

fun sumPoint(p: Point): int {
    return p.x + p.y;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![evaluate_local(
            "p",
            Ty::StructRef {
                struct_name: "Point".to_owned(),
                type_args: None,
            },
            RenderedValue::OptimizedOut,
        )];

        let err = evaluate_expression2(&locals, &source_map, 1, "sumPoint(p)")
            .expect_err("evaluate2 should report missing struct slots");

        let message = err.to_string();
        assert!(message.contains("cannot evaluate `p: Point`"), "{message}");
        assert!(message.contains("optimized out"), "{message}");
    }

    #[test]
    fn evaluate2_reports_unsupported_runtime_stack_values_clearly() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_unknown_error.tolk");
        let source = r#"
fun pass(value: unknown): unknown {
    return value;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![evaluate_local_with_raw(
            "bad",
            Ty::Unknown,
            RenderedValue::leaf("???"),
            vec![VmStackValue::Unknown],
        )];

        let err = evaluate_expression2(&locals, &source_map, 1, "pass(bad)")
            .expect_err("evaluate2 should report unsupported runtime stack values");

        let message = err.to_string();
        assert!(
            message.contains("cannot evaluate `bad: unknown`"),
            "{message}"
        );
        assert!(message.contains("unknown runtime stack value"), "{message}");
    }

    #[test]
    fn evaluate2_includes_get_method_vm_log_on_vm_exit_code() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("eval2_vm_log_error.tolk");
        let source = r#"
fun explode(code: int): int {
    throw code;
}
"#;
        fs::write(&file_path, source).expect("write source");
        let source_map = source_map_for_file(
            file_path.to_str().expect("utf8 path"),
            source.chars().count() as u64,
        );
        let locals = vec![evaluate_local(
            "code",
            Ty::Int,
            RenderedValue::typed_leaf("7", "int"),
        )];

        let err = evaluate_expression2(&locals, &source_map, 1, "explode(code)")
            .expect_err("evaluate2 should surface VM logs after getter failure");

        let message = err.to_string();
        assert!(message.contains("VM exit code 7"), "{message}");
        assert!(message.contains("get-method VM log:"), "{message}");
        assert!(message.contains("execute "), "{message}");
    }
}
