use abi::{ContractAbi, TypeAbi};
use num_bigint::BigInt;
use std::collections::HashMap;
use std::fmt;
use std::ops::{Deref, DerefMut};
use tonlib_core::cell::ArcCell;
use tycho_types::models::{IntAddr, ShardAccount};

#[derive(Default, Debug, Clone)]
pub struct Tuple(pub Vec<TupleItem>);

impl Tuple {
    pub fn empty() -> Tuple {
        Tuple(vec![])
    }

    pub fn unwrap_empty(&self) -> Tuple {
        if self.0.is_empty() {
            return (*self).clone();
        }

        if let TupleItem::Tuple(item) = &self.0[0]
            && item.len() == 0
        {
            return Tuple(vec![]);
        }

        (*self).clone()
    }
    pub fn unwrap_single(&self) -> Tuple {
        if self.0.is_empty() {
            return (*self).clone();
        }

        if let TupleItem::Tuple(item) = &self.0[0]
            && item.len() == 1
        {
            return Tuple(vec![item[0].clone()]);
        }

        (*self).clone()
    }
}

impl fmt::Display for Tuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.len() == 1 {
            write!(f, "{}", self.0[0])
        } else {
            write!(f, "(")?;
            for (i, item) in self.0.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", item)?;
            }
            write!(f, ")")
        }
    }
}

impl Deref for Tuple {
    type Target = Vec<TupleItem>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Tuple {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for Tuple {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationResult {
    pub name: String,
    pub code_boc64: String,
    pub code_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct BuildCache {
    pub built: HashMap<String, CompilationResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownAddress {
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct KnownAddresses {
    pub addresses: HashMap<IntAddr, KnownAddress>,
}

/// Represents a stack value in TON VM
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TupleItem {
    Null,
    Int(BigInt),
    Nan,
    Cell(ArcCell),
    Slice(TupleSlice),
    Builder(ArcCell),
    Tuple(Vec<TupleItem>),
    TypedTuple {
        type_name: String,
        items: Vec<TupleItem>,
        abi: Option<TypeAbi>,
        contract_abi: ContractAbi,
        accounts: HashMap<String, ShardAccount>,
        build_cache: BuildCache,
        known_addresses: KnownAddresses,
    },
}

#[derive(Debug, Clone, Eq)]
pub struct TupleSlice {
    pub cell: ArcCell,
    pub start_bits: u32,
    pub end_bits: u32,
    pub start_refs: u32,
    pub end_refs: u32,
}

impl TupleItem {
    pub fn unwrap_single(&self) -> TupleItem {
        let TupleItem::Tuple(items) = self else {
            return (*self).clone();
        };

        if items.len() == 1 {
            return items[0].clone();
        }

        (*self).clone()
    }
}

impl PartialEq for TupleSlice {
    fn eq(&self, other: &Self) -> bool {
        let self_bits_len = (self.end_bits - self.start_bits) as usize;
        let other_bits_len = (other.end_bits - other.start_bits) as usize;
        let self_refs_count = (self.end_refs - self.start_refs) as usize;
        let other_refs_count = (other.end_refs - other.start_refs) as usize;

        if self_bits_len != other_bits_len || self_refs_count != other_refs_count {
            // fast path
            return false;
        }

        let mut self_parser = self.cell.parser();
        let mut other_parser = other.cell.parser();

        if self_parser.skip_bits(self.start_bits as usize).is_err()
            || other_parser.skip_bits(other.start_bits as usize).is_err()
        {
            return false;
        }

        match (
            self_parser.load_bits(self_bits_len),
            other_parser.load_bits(other_bits_len),
        ) {
            (Ok(self_data), Ok(other_data)) => {
                if self_data != other_data {
                    return false;
                }
            }
            _ => return false,
        }

        let mut self_parser = self.cell.parser();
        let mut other_parser = other.cell.parser();

        for _ in 0..self_refs_count {
            match (self_parser.next_reference(), other_parser.next_reference()) {
                (Ok(self_ref), Ok(other_ref)) => {
                    if self_ref.cell_hash() != other_ref.cell_hash() {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        true
    }
}

impl Default for TupleItem {
    fn default() -> Self {
        TupleItem::Null
    }
}
