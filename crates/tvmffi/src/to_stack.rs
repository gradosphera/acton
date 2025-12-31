use crate::stack::{Tuple, TupleItem};
use num_bigint::BigInt;
use thiserror::Error;
use tonlib_core::cell::ArcCell;
use tonlib_core::tlb_types::tlb::TLB;
use tycho_types::cell::{CellBuilder, CellFamily, Store};

#[derive(Debug, Error, PartialEq)]
pub enum SerializationError {
    #[error("cell build error")]
    CellBuild,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SerializationOptions {}

pub trait ToStack {
    fn to_item(&self) -> Result<TupleItem, SerializationError>;
}

impl ToStack for TupleItem {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(self.clone())
    }
}

impl ToStack for BigInt {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Int(self.clone()))
    }
}

impl ToStack for bool {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Int(if *self {
            BigInt::from(-1)
        } else {
            BigInt::from(0)
        }))
    }
}

impl ToStack for i32 {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Int(BigInt::from(*self)))
    }
}

impl ToStack for u32 {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Int(BigInt::from(*self)))
    }
}

impl ToStack for ArcCell {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Cell(self.clone()))
    }
}

impl ToStack for tycho_types::models::IntAddr {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        let mut builder = CellBuilder::new();
        self.store_into(&mut builder, tycho_types::cell::Cell::empty_context())
            .map_err(|_| SerializationError::CellBuild)?;
        let cell = builder.build().map_err(|_| SerializationError::CellBuild)?;

        let boc = tycho_types::boc::Boc::encode(&cell);
        let arc_cell = ArcCell::from_boc(&boc).map_err(|_| SerializationError::CellBuild)?;
        Ok(TupleItem::Cell(arc_cell))
    }
}

impl<T: ToStack> ToStack for Option<T> {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        match self {
            Some(v) => v.to_item(),
            None => Ok(TupleItem::Null),
        }
    }
}

impl ToStack for Tuple {
    fn to_item(&self) -> Result<TupleItem, SerializationError> {
        Ok(TupleItem::Tuple(self.clone()))
    }
}
