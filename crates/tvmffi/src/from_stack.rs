use crate::stack::{Tuple, TupleItem, TupleSlice};
use num_bigint::BigInt;
use thiserror::Error;
use tonlib_core::cell::ArcCell;

#[derive(Debug, Error)]
pub enum ArgError {
    #[error("stack underflow")]
    StackUnderflow,
    #[error("type mismatch: expected {expected}")]
    TypeMismatch { expected: &'static str },
    #[error("utf8 decode error")]
    Utf8,
    #[error("cell parse error")]
    CellParse,
}

pub trait FromStack: Sized {
    fn from_item(item: TupleItem) -> Result<Self, ArgError>;
}

impl FromStack for TupleItem {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        Ok(item)
    }
}

impl FromStack for String {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Slice(slice) => Tuple::parse_snake_string(&slice).ok_or(ArgError::CellParse),
            _ => Err(ArgError::TypeMismatch {
                expected: "Slice(String)",
            }),
        }
    }
}

impl FromStack for Vec<u8> {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Slice(TupleSlice {
                cell,
                start_bits,
                end_bits,
                ..
            }) => {
                let mut p = cell.parser();
                p.skip_bits(start_bits as usize)
                    .map_err(|_| ArgError::CellParse)?;
                let bits = p
                    .load_bits((end_bits - start_bits) as usize)
                    .map_err(|_| ArgError::CellParse)?;
                Ok(bits)
            }
            _ => Err(ArgError::TypeMismatch {
                expected: "Slice(Bytes)",
            }),
        }
    }
}

impl FromStack for BigInt {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Int(i) => Ok(i),
            _ => Err(ArgError::TypeMismatch { expected: "Int" }),
        }
    }
}

impl FromStack for i64 {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Int(i) => i.try_into().map_err(|_| ArgError::TypeMismatch {
                expected: "i64 (from Int)",
            }),
            _ => Err(ArgError::TypeMismatch { expected: "Int" }),
        }
    }
}

impl FromStack for bool {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Int(i) => {
                // TON: true = -1, false = 0
                if i == BigInt::from(-1) {
                    Ok(true)
                } else if i == BigInt::from(0) {
                    Ok(false)
                } else {
                    Err(ArgError::TypeMismatch {
                        expected: "Int(-1/0) as bool",
                    })
                }
            }
            _ => Err(ArgError::TypeMismatch {
                expected: "Int(-1/0) as bool",
            }),
        }
    }
}

impl FromStack for Tuple {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Tuple(v) => Ok(Tuple(v)),
            _ => Err(ArgError::TypeMismatch { expected: "Tuple" }),
        }
    }
}

impl FromStack for ArcCell {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match item {
            TupleItem::Cell(c) => Ok(c),
            _ => Err(ArgError::TypeMismatch { expected: "Cell" }),
        }
    }
}

impl<T: FromStack> FromStack for Option<T> {
    fn from_item(item: TupleItem) -> Result<Self, ArgError> {
        match T::from_item(item) {
            Ok(v) => Ok(Some(v)),
            Err(ArgError::TypeMismatch { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
