use acton_config::color::OwoColorize;
use anyhow::anyhow;
use log::error;
use tolkc::abi::{ABIType, ContractABI};
use tvmffi::stack::{Tuple, TupleItem};
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell as TyCell, CellBuilder as TyCellBuilder};
use vmlogs::parser::{CellLike, VmStackValue, vm_stack_value};

pub(crate) fn parse_stack_args(args: Vec<String>) -> anyhow::Result<Tuple> {
    let mut items = Vec::new();
    for arg in args {
        let mut input = arg.as_str();
        let value = vm_stack_value(&mut input).map_err(|e| {
            error!("Failed to parse stack value '{arg}': {e}");
            anyhow!("Failed to parse argument {}", arg.yellow())
        })?;

        if !input.trim().is_empty() {
            return Err(anyhow!(
                "Failed to parse argument '{arg}': trailing characters"
            ));
        }

        let item = convert_vm_value_to_tuple_item(value)?;
        items.push(item);
    }
    Ok(Tuple(items).unwrap_tuple())
}

fn convert_vm_value_to_tuple_item(value: VmStackValue<'_>) -> anyhow::Result<TupleItem> {
    match value {
        VmStackValue::Null => Ok(TupleItem::Null),
        VmStackValue::NaN => Ok(TupleItem::Nan),
        VmStackValue::Integer(s) => {
            let bi = s.parse().map_err(|_| anyhow!("Invalid integer: {s}"))?;
            Ok(TupleItem::Int(bi))
        }
        VmStackValue::Tuple(values) => {
            let mut inner_items = Vec::new();
            for v in values {
                inner_items.push(convert_vm_value_to_tuple_item(v)?);
            }
            Ok(TupleItem::Tuple(Tuple(inner_items)))
        }
        VmStackValue::Cell(cell_like) => convert_cell_like(cell_like).map(TupleItem::Cell),
        VmStackValue::Builder(_) => Err(anyhow!(
            "Builder values are not supported in script arguments"
        )),
        VmStackValue::CellSlice(cs) => {
            let cell = Boc::decode_hex(cs.value)?;
            Ok(TupleItem::Slice(cell))
        }
        VmStackValue::Continuation(_) => {
            Err(anyhow!("Continuation not supported in script arguments"))
        }
        VmStackValue::String(s) => Ok(TupleItem::Cell(string_to_slice(s)?)),
        VmStackValue::Unknown => Err(anyhow!("Unknown stack value type")),
    }
}

fn convert_cell_like(cell_like: CellLike<'_>) -> anyhow::Result<TyCell> {
    match cell_like {
        CellLike::Cell(hex) => Ok(Boc::decode_hex(hex)?),
        CellLike::Builder(hex) => Ok(Boc::decode_hex(hex)?),
    }
}

fn string_to_slice(s: &str) -> anyhow::Result<TyCell> {
    let bytes = s.as_bytes();
    let total_bits = bytes.len() * 8;

    if total_bits <= 1023 {
        // Fast path, the string fits in one cell
        let mut b = TyCellBuilder::new();
        b.store_raw(bytes, total_bits as u16)?;
        return Ok(b.build()?);
    }

    let mut remaining_bytes = bytes;
    let mut cell_data = Vec::new();

    while !remaining_bytes.is_empty() {
        let chunk_size = std::cmp::min(remaining_bytes.len(), 127); // 127 bytes = 1016 bits < 1023
        let chunk = &remaining_bytes[..chunk_size];
        cell_data.push((chunk, chunk.len() * 8));
        remaining_bytes = &remaining_bytes[chunk_size..];
    }

    // build cells from last to first
    let mut next_cell: Option<TyCell> = None;

    for (chunk, bits) in cell_data.into_iter().rev() {
        let mut b = TyCellBuilder::new();
        b.store_raw(chunk, bits as u16)?;

        if let Some(next) = next_cell {
            b.store_reference(next)?;
        }

        next_cell = Some(b.build()?);
    }

    if let Some(root_cell) = next_cell {
        return Ok(root_cell);
    }

    anyhow::bail!("No root cell for string");
}

pub fn validate_script_stack_against_compiler_abi(
    stack: &Tuple,
    abi: Option<&ContractABI>,
) -> anyhow::Result<()> {
    let Some(abi) = abi else {
        return Ok(());
    };

    let Some(main_method) = abi
        .get_methods
        .iter()
        .find(|method| method.tvm_method_id == 0)
        .or_else(|| abi.get_methods.iter().find(|method| method.name == "main"))
    else {
        return Ok(());
    };

    let expected_args_count = main_method.parameters.len();
    let actual_args_count = stack.len();
    if actual_args_count != expected_args_count {
        anyhow::bail!(
            "Script argument count mismatch for '{}': expected {}, got {}",
            main_method.name,
            expected_args_count,
            actual_args_count
        );
    }

    for (index, (arg, parameter)) in stack.iter().zip(main_method.parameters.iter()).enumerate() {
        if !is_supported_script_abi_type(&parameter.ty) {
            anyhow::bail!(
                "Script argument #{} ('{}') uses unsupported ABI type '{}'. Supported argument kinds: NaN, integer, null, tuple, tensor, string, cell, slice",
                index + 1,
                parameter.name,
                abi_type_human_name(&parameter.ty),
            );
        }

        if !abi_type_matches_stack_item(&parameter.ty, arg) {
            anyhow::bail!(
                "Script argument #{} ('{}') type mismatch: expected '{}', got '{}'",
                index + 1,
                parameter.name,
                abi_type_human_name(&parameter.ty),
                stack_item_human_name(arg)
            );
        }
    }

    Ok(())
}

pub(crate) fn is_supported_script_abi_type(abi_type: &ABIType) -> bool {
    match abi_type {
        ABIType::Int
        | ABIType::Coins
        | ABIType::UintN { .. }
        | ABIType::IntN { .. }
        | ABIType::VarUintN { .. }
        | ABIType::VarIntN { .. }
        | ABIType::NullLiteral
        | ABIType::Cell
        | ABIType::Slice
        | ABIType::String => true,
        ABIType::Nullable { inner } => is_supported_script_abi_type(inner),
        ABIType::Tensor { items } | ABIType::ShapedTuple { items } => {
            items.iter().all(is_supported_script_abi_type)
        }
        _ => false,
    }
}

fn abi_type_matches_stack_item(abi_type: &ABIType, item: &TupleItem) -> bool {
    abi_type_matches_stack_item_inner(abi_type, item)
}

fn abi_type_matches_stack_item_inner(abi_type: &ABIType, item: &TupleItem) -> bool {
    match abi_type {
        ABIType::Int
        | ABIType::Coins
        | ABIType::UintN { .. }
        | ABIType::IntN { .. }
        | ABIType::VarUintN { .. }
        | ABIType::VarIntN { .. } => matches!(item, TupleItem::Int(_) | TupleItem::Nan),
        ABIType::Cell => matches!(item, TupleItem::Cell(_)),
        ABIType::Slice | ABIType::String => {
            matches!(item, TupleItem::Slice(_) | TupleItem::Cell(_))
        }
        ABIType::NullLiteral => matches!(item, TupleItem::Null),
        ABIType::Nullable { inner } => {
            if matches!(item, TupleItem::Null) {
                return true;
            }
            abi_type_matches_stack_item_inner(inner, item)
        }
        ABIType::Tensor { items } | ABIType::ShapedTuple { items } => {
            let Some(tuple) = tuple_from_item(item) else {
                return false;
            };
            if tuple.len() != items.len() {
                return false;
            }
            for (actual_item, expected_ty) in tuple.iter().zip(items.iter()) {
                if !abi_type_matches_stack_item_inner(expected_ty, actual_item) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn tuple_from_item(item: &TupleItem) -> Option<&Tuple> {
    match item {
        TupleItem::Tuple(tuple) => Some(tuple),
        TupleItem::TypedTuple { inner, .. } => Some(inner),
        _ => None,
    }
}

fn abi_type_human_name(abi_type: &ABIType) -> String {
    serde_json::to_string(abi_type).unwrap_or_else(|_| format!("{abi_type:?}"))
}

const fn stack_item_human_name(item: &TupleItem) -> &'static str {
    match item {
        TupleItem::Null => "null",
        TupleItem::Int(_) => "int",
        TupleItem::Nan => "nan",
        TupleItem::Cell(_) => "cell",
        TupleItem::Slice(_) => "slice",
        TupleItem::Builder(_) => "builder",
        TupleItem::Tuple(_) => "tuple",
        TupleItem::TypedTuple { .. } => "typedTuple",
    }
}
