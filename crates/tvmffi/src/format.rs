use crate::stack::{Tuple, TupleItem};
use num_bigint::BigInt;
use std::fmt;

pub fn format_item_with_type(item: &TupleItem, type_name: &str) -> String {
    let item = item.unwrap_single();

    match item {
        TupleItem::Int(value) if type_name == "bool" => {
            if value == BigInt::from(0) {
                "false".to_string()
            } else if value == BigInt::from(18446744073709551615u64) {
                "true".to_string()
            } else {
                format!("{}", value)
            }
        }
        TupleItem::Slice(slice) if type_name == "address" => {
            let length = slice.end_bits - slice.start_bits;
            let mut parser = slice.cell.parser();
            let Ok(()) = parser.skip_bits(slice.start_bits as usize) else {
                return "Slice(...)".to_string();
            };
            if length == 2 && parser.load_u8(2).unwrap_or(0) == 0 {
                return "addr_none".to_string();
            }
            if length != 267 {
                return "Slice(...)".to_string();
            }
            let Ok(address) = parser.load_address() else {
                return "Slice(...)".to_string();
            };
            address.to_string()
        }
        _ => format!("{}", item),
    }
}

impl fmt::Display for TupleItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TupleItem::Int(value) => {
                if *value == BigInt::from(18446744073709551615u64) {
                    write!(f, "-1")
                } else {
                    write!(f, "{}", value)
                }
            }
            TupleItem::Null => write!(f, "null"),
            TupleItem::Nan => write!(f, "NaN"),
            TupleItem::Cell(cell) => write!(f, "{:?}", cell),
            TupleItem::Slice(slice) => {
                if let Some(string) = Tuple::parse_snake_string(slice) {
                    write!(f, "\"{}\"", string)
                } else {
                    write!(f, "Slice(...)")
                }
            }
            TupleItem::Builder(_) => write!(f, "Builder(...)"),
            TupleItem::Tuple(items) => {
                if items.len() == 1 {
                    write!(f, "{}", items[0])
                } else {
                    write!(f, "(")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", item)?;
                    }
                    write!(f, ")")
                }
            }
            TupleItem::TypedTuple {
                type_name,
                items,
                abi,
            } => {
                if type_name == "address" && items.len() == 1 {
                    let addr = &items[0];
                    return write!(f, "{}", format_item_with_type(addr, type_name));
                }

                if items.len() == 1 {
                    write!(f, "{}", items[0])
                } else {
                    if let Some(struct_desc) = abi {
                        if items.len() == struct_desc.fields.len() {
                            write!(f, "{} {{\n", type_name)?;
                            for (i, (field, item)) in
                                struct_desc.fields.iter().zip(items.iter()).enumerate()
                            {
                                write!(
                                    f,
                                    "    {}: {}",
                                    field.name,
                                    format_item_with_type(item, &field.type_info.human_readable)
                                )?;
                                if i < struct_desc.fields.len() - 1 {
                                    write!(f, ",")?;
                                }
                                write!(f, "\n")?;
                            }
                            write!(f, "}}")?;
                            return Ok(());
                        }
                    }

                    write!(
                        f,
                        "{}({})",
                        type_name,
                        items
                            .iter()
                            .map(|item| format!("{}", item))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
        }
    }
}
