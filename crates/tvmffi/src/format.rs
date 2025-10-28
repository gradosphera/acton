use crate::stack::{Tuple, TupleItem};
use num_bigint::BigInt;
use std::fmt;
use tonlib_core::cell::ArcCell;

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
        TupleItem::Slice(cell) => {
            if cell.bit_len() == 0 && cell.references().len() == 0 {
                return "empty slice".to_string();
            }

            if let Some(string) = Tuple::parse_snake_string(&cell) {
                return format!("\"{}\"", string);
            }

            format_slice(&cell)
        }
        _ => format!("{}", item),
    }
}

fn format_slice(slice: &ArcCell) -> String {
    let mut parser = slice.parser();

    if parser.remaining_bits() == 2 && parser.load_u8(2).unwrap_or(0) == 0 {
        return "addr_none".to_string();
    }

    if parser.remaining_bits() == 267
        && let Ok(address) = parser.load_address()
    {
        return address.to_string();
    }

    "Slice(...)".to_string()
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
            TupleItem::Slice(cell) => {
                if let Some(string) = Tuple::parse_snake_string(cell) {
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
            TupleItem::TypedTuple { type_name, items } => {
                if type_name == "address" && items.len() == 1 {
                    let addr = &items[0];
                    return write!(f, "{}", format_item_with_type(addr, type_name));
                }

                if items.len() == 1 {
                    write!(f, "{}", items[0])
                } else {
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
