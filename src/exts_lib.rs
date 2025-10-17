use crate::stack_serialization::{TupleItem, parse_tuple, serialize_tuple};
use num_bigint::BigInt;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use tonlib_core::cell::{ArcCell, CellBuilder};
use tonlib_core::tlb_types::tlb::TLB;

pub type Tuple = Vec<TupleItem>;

pub fn cell_to_ffi_boc64(cell: ArcCell) -> *const c_char {
    let s = cell.to_boc_b64(false).unwrap(); // при желании можно обернуть в Result
    CString::new(s).unwrap().into_raw().cast_const()
}

pub unsafe fn with_tuple(ptr: *const c_char, f: impl FnOnce(&mut Tuple)) -> *const c_char {
    let c = unsafe { CStr::from_ptr(ptr) };
    let boc = match c.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("").unwrap().into_raw().cast_const(),
    };

    let mut tuple = ArcCell::from_boc_b64(boc)
        .ok()
        .and_then(|c| parse_tuple(&c).ok())
        .unwrap_or_else(|| Vec::new());

    f(&mut tuple);

    cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
}

pub fn take_last_string(tuple: &mut Tuple) -> Option<String> {
    let item = tuple.pop()?;
    slice_to_string(&item)
}

pub fn slice_to_string(item: &TupleItem) -> Option<String> {
    if let TupleItem::Slice {
        cell,
        start_bits,
        end_bits,
        ..
    } = item
    {
        let mut p = cell.parser();
        p.skip_bits(*start_bits as usize).ok()?;
        let bits = p.load_bits((*end_bits - *start_bits) as usize).ok()?;
        String::from_utf8(bits).ok()
    } else {
        None
    }
}

pub fn push_string(tuple: &mut Tuple, s: &str) {
    let mut b = CellBuilder::new();
    b.store_bits(s.len() * 8, s.as_bytes()).unwrap();
    tuple.push(TupleItem::Slice {
        cell: ArcCell::from(b.build().unwrap()),
        start_bits: 0,
        end_bits: (s.len() * 8) as u32,
        end_refs: 0,
        start_refs: 0,
    });
}

pub fn push_bool_as_int(tuple: &mut Tuple, v: bool) {
    tuple.push(TupleItem::Int(if v {
        BigInt::from(-1)
    } else {
        BigInt::from(0)
    }));
}

pub fn pop_two_tuples_and_equal(tuple: &mut Tuple) -> Option<bool> {
    if tuple.len() < 2 {
        return None;
    }
    let right = tuple.pop().unwrap();
    let left = tuple.pop().unwrap();
    match (left, right) {
        (TupleItem::Tuple(l), TupleItem::Tuple(r)) => Some(l == r),
        _ => None,
    }
}

#[macro_export]
macro_rules! ext {
    ($fn_name:ident, $body:expr) => {
        unsafe extern "C" fn $fn_name(ptr: *const c_char) -> *const c_char {
            unsafe { with_tuple(ptr, $body) }
        }
    };
}

#[macro_export]
macro_rules! register_ext_methods {
    ($executor:expr, { $($id:expr => $fname:ident),+ $(,)? }) => {{
        $(
            $executor.register_ext_method($id, $fname);
        )+
    }};
}
