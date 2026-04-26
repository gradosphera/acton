//! This module defines a simple DSL for defining extension functions for the emulator.
#![allow(unsafe_code)]
use num_bigint::BigInt;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use ton_executor::ExtMethodBytesFreeCallback;
use tvm_ffi::from_stack::{ArgError, FromStack};
use tvm_ffi::serde::{parse_tuple, parse_tuple_item, serialize_tuple, serialize_tuple_item};
use tvm_ffi::stack::Tuple;
use tvm_ffi::stack::TupleItem;
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellBuilder};

pub fn pop_arg<T: FromStack>(t: &mut Tuple) -> Result<T, ArgError> {
    let item = t.pop().ok_or(ArgError::StackUnderflow)?;
    T::from_item(item)
}

#[macro_export]
macro_rules! pop_args {
    ($tuple:expr, $($ty:ty),+ $(,)?) => {{
        let mut __errors: Option<tvm_ffi::from_stack::ArgError> = None;
        let __result = ( $(
            match $crate::extensions::pop_arg::<$ty>($tuple) {
                Ok(v) => v,
                Err(e) => {
                    __errors = Some(e);
                    Default::default()
                }
            }
        , )+ );
        if let Some(e) = __errors {
            Err(e)
        } else {
            Ok(__result)
        }
    }};
}

#[macro_export]
macro_rules! extension {
    ($fn_name:ident in ($ctx_ty:ty) using $body:expr) => {
        unsafe extern "C" fn $fn_name(
            ctx: *mut $ctx_ty,
            ptr: *const u8,
            len: usize,
            out_data: *mut *const u8,
            out_len: *mut usize,
            out_owner: *mut *mut std::ffi::c_void,
            out_free_owner: *mut Option<ton_executor::ExtMethodBytesFreeCallback>,
        ) -> bool {
            unsafe {
                let ctx = &mut *(ctx as *mut $ctx_ty);
                $crate::extensions::with_tuple_bytes(ptr, len, out_data, out_len, out_owner, out_free_owner, |__t: &mut tvm_ffi::stack::Tuple| {
                    let r: anyhow::Result<()> = $body(ctx, __t);
                    if let Err(e) = r {
                        ctx.asserts.fail(format!("{:#}", e));
                        __t.push(tvm_ffi::stack::TupleItem::Null);
                    }
                })
            }
        }
    };
    ($fn_name:ident in ($ctx_ty:ty) with ($an:ident : $ty:ty) using $body:expr) => {
        unsafe extern "C" fn $fn_name(
            ctx: *mut $ctx_ty,
            ptr: *const u8,
            len: usize,
            out_data: *mut *const u8,
            out_len: *mut usize,
            out_owner: *mut *mut std::ffi::c_void,
            out_free_owner: *mut Option<ton_executor::ExtMethodBytesFreeCallback>,
        ) -> bool {
            unsafe {
                let ctx = &mut *(ctx as *mut $ctx_ty);
                $crate::extensions::with_tuple_bytes(ptr, len, out_data, out_len, out_owner, out_free_owner, |__t: &mut tvm_ffi::stack::Tuple| {
                    match $crate::extensions::pop_arg::<$ty>(__t) {
                        Ok($an) => {
                            let r: anyhow::Result<()> = $body(ctx, __t, $an);
                            if let Err(e) = r {
                                ctx.asserts.fail(format!("{:#}", e));
                                __t.push(tvm_ffi::stack::TupleItem::Null);
                            }
                        }
                        Err(e) => {
                            eprintln!("ext_args decode error in {}: {}", stringify!($fn_name), e);
                        }
                    }
                })
            }
        }
    };
    ($fn_name:ident in ($ctx_ty:ty) with ($($an:ident : $ty:ty),+ $(,)?) using $body:expr) => {
        unsafe extern "C" fn $fn_name(
            ctx: *mut $ctx_ty,
            ptr: *const u8,
            len: usize,
            out_data: *mut *const u8,
            out_len: *mut usize,
            out_owner: *mut *mut std::ffi::c_void,
            out_free_owner: *mut Option<ton_executor::ExtMethodBytesFreeCallback>,
        ) -> bool {
            unsafe {
                debug_assert!(!ctx.is_null());
                debug_assert!(!ptr.is_null());
                let ctx = &mut *(ctx as *mut $ctx_ty);
                $crate::extensions::with_tuple_bytes(ptr, len, out_data, out_len, out_owner, out_free_owner, |__t: &mut tvm_ffi::stack::Tuple| {
                    match $crate::pop_args!(__t, $($ty),*) {
                        Ok(__vals) => {
                            #[allow(non_snake_case, unused_variables)]
                            let ($($an, )*) = __vals;
                            let r: anyhow::Result<()> = $body(ctx, __t, $($an, )*);
                            if let Err(e) = r {
                                ctx.asserts.fail(format!("{:#}", e));
                                __t.push(tvm_ffi::stack::TupleItem::Null);
                            }
                        }
                        Err(e) => {
                            eprintln!("ext_args decode error in {}: {}", stringify!($fn_name), e);
                        }
                    }
                })
            }
        }
    };
}

fn cell_to_ffi_boc64(cell: &Cell) -> *const c_char {
    let s = Boc::encode_base64(cell);
    CString::new(s)
        .expect("Failed to create C string from BOC")
        .into_raw()
        .cast_const()
}

unsafe extern "C" fn free_ffi_bytes(owner: *mut c_void) {
    if !owner.is_null() {
        // SAFETY: `owner` is created by `Box::into_raw(Box<Vec<u8>>)` in `bytes_to_ffi`.
        unsafe {
            drop(Box::from_raw(owner.cast::<Vec<u8>>()));
        }
    }
}

fn bytes_to_ffi(
    bytes: Vec<u8>,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_owner: *mut *mut c_void,
    out_free_owner: *mut Option<ExtMethodBytesFreeCallback>,
) -> bool {
    if out_data.is_null() || out_len.is_null() || out_owner.is_null() || out_free_owner.is_null() {
        return false;
    }

    let bytes = Box::new(bytes);
    let data = bytes.as_ptr();
    let len = bytes.len();
    let owner = Box::into_raw(bytes).cast::<c_void>();

    // SAFETY: Output pointers were checked for null and are owned by the native caller.
    unsafe {
        *out_data = data;
        *out_len = len;
        *out_owner = owner;
        *out_free_owner = Some(free_ffi_bytes);
    }

    true
}

fn cell_to_ffi_boc_bytes(
    cell: &Cell,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_owner: *mut *mut c_void,
    out_free_owner: *mut Option<ExtMethodBytesFreeCallback>,
) -> bool {
    bytes_to_ffi(
        Boc::encode(cell),
        out_data,
        out_len,
        out_owner,
        out_free_owner,
    )
}

const STACK_WIRE_MAGIC: &[u8] = b"ASTK1";

#[repr(u8)]
enum StackWireTag {
    Null = 0,
    Int = 1,
    Nan = 2,
    Cell = 3,
    Slice = 4,
    Builder = 5,
    Tuple = 6,
    Cont = 7,
}

fn write_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn write_u32(out: &mut Vec<u8>, value: usize) -> anyhow::Result<()> {
    let value = u32::try_from(value)?;
    out.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> anyhow::Result<()> {
    write_u32(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn write_cell_boc(out: &mut Vec<u8>, cell: &Cell) -> anyhow::Result<()> {
    write_bytes(out, &Boc::encode(cell))
}

fn write_stack_wire_item(out: &mut Vec<u8>, item: &TupleItem) -> anyhow::Result<()> {
    match item {
        TupleItem::Null => write_u8(out, StackWireTag::Null as u8),
        TupleItem::Int(value) => {
            write_u8(out, StackWireTag::Int as u8);
            write_bytes(out, value.to_str_radix(10).as_bytes())?;
        }
        TupleItem::Nan => write_u8(out, StackWireTag::Nan as u8),
        TupleItem::Cell(cell) => {
            write_u8(out, StackWireTag::Cell as u8);
            write_cell_boc(out, cell)?;
        }
        TupleItem::Slice(cell) => {
            write_u8(out, StackWireTag::Slice as u8);
            write_cell_boc(out, cell)?;
        }
        TupleItem::Builder(cell) => {
            write_u8(out, StackWireTag::Builder as u8);
            write_cell_boc(out, cell)?;
        }
        TupleItem::Tuple(items) | TupleItem::TypedTuple { inner: items, .. } => {
            write_u8(out, StackWireTag::Tuple as u8);
            write_u32(out, items.len())?;
            for child in items.iter() {
                write_stack_wire_item(out, child)?;
            }
        }
        TupleItem::Cont(_) => {
            write_u8(out, StackWireTag::Cont as u8);
            let mut builder = CellBuilder::new();
            serialize_tuple_item(&mut builder, item)?;
            write_cell_boc(out, &builder.build()?)?;
        }
    }
    Ok(())
}

fn tuple_to_stack_wire(tuple: &Tuple) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(STACK_WIRE_MAGIC);
    write_u32(&mut out, tuple.len())?;
    for item in tuple.iter() {
        write_stack_wire_item(&mut out, item)?;
    }
    Ok(out)
}

struct StackWireReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> StackWireReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_stack(&mut self) -> anyhow::Result<Tuple> {
        anyhow::ensure!(
            self.data.starts_with(STACK_WIRE_MAGIC),
            "invalid stack wire magic"
        );
        self.offset = STACK_WIRE_MAGIC.len();
        let len = self.read_u32()? as usize;
        let mut items = Vec::with_capacity(len);
        for _ in 0..len {
            items.push(self.read_item()?);
        }
        anyhow::ensure!(self.offset == self.data.len(), "trailing stack wire bytes");
        Ok(Tuple(items))
    }

    fn read_u8(&mut self) -> anyhow::Result<u8> {
        let value = *self
            .data
            .get(self.offset)
            .ok_or_else(|| anyhow::anyhow!("unexpected end of stack wire"))?;
        self.offset += 1;
        Ok(value)
    }

    fn read_u32(&mut self) -> anyhow::Result<u32> {
        let bytes = self.read_fixed::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_fixed<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or_else(|| anyhow::anyhow!("stack wire offset overflow"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| anyhow::anyhow!("unexpected end of stack wire"))?;
        self.offset = end;
        Ok(bytes.try_into()?)
    }

    fn read_bytes(&mut self) -> anyhow::Result<&'a [u8]> {
        let len = self.read_u32()? as usize;
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("stack wire offset overflow"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| anyhow::anyhow!("unexpected end of stack wire"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_cell(&mut self) -> anyhow::Result<Cell> {
        Ok(Boc::decode(self.read_bytes()?)?)
    }

    fn read_item(&mut self) -> anyhow::Result<TupleItem> {
        let tag = self.read_u8()?;
        match tag {
            tag if tag == StackWireTag::Null as u8 => Ok(TupleItem::Null),
            tag if tag == StackWireTag::Int as u8 => {
                let dec = std::str::from_utf8(self.read_bytes()?)?;
                let value = BigInt::parse_bytes(dec.as_bytes(), 10)
                    .ok_or_else(|| anyhow::anyhow!("invalid stack wire integer"))?;
                Ok(TupleItem::Int(value))
            }
            tag if tag == StackWireTag::Nan as u8 => Ok(TupleItem::Nan),
            tag if tag == StackWireTag::Cell as u8 => Ok(TupleItem::Cell(self.read_cell()?)),
            tag if tag == StackWireTag::Slice as u8 => Ok(TupleItem::Slice(self.read_cell()?)),
            tag if tag == StackWireTag::Builder as u8 => Ok(TupleItem::Builder(self.read_cell()?)),
            tag if tag == StackWireTag::Tuple as u8 => {
                let len = self.read_u32()? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.read_item()?);
                }
                Ok(TupleItem::Tuple(Tuple(items)))
            }
            tag if tag == StackWireTag::Cont as u8 => {
                let cell = self.read_cell()?;
                let mut parser = cell.as_slice_allow_exotic();
                let item = parse_tuple_item(&mut parser)?;
                anyhow::ensure!(
                    matches!(item, TupleItem::Cont(_)),
                    "invalid continuation item"
                );
                Ok(item)
            }
            _ => anyhow::bail!("unsupported stack wire tag: {tag}"),
        }
    }
}

fn parse_stack_wire(data: &[u8]) -> anyhow::Result<Tuple> {
    StackWireReader::new(data).read_stack()
}

/// # Safety
///
/// Well...
pub unsafe fn with_tuple(ptr: *const c_char, f: impl FnOnce(&mut Tuple)) -> *const c_char {
    // SAFETY: We assume ptr is always valid C string
    let c = unsafe { CStr::from_ptr(ptr) };
    let Ok(boc) = c.to_str() else {
        return CString::new("")
            .expect("cannot create empty CString")
            .into_raw()
            .cast_const();
    };

    let mut tuple = Boc::decode_base64(boc)
        .ok()
        .and_then(|c| parse_tuple(&c).ok())
        .unwrap_or_else(Tuple::empty);

    f(&mut tuple);

    cell_to_ffi_boc64(&serialize_tuple(&tuple).expect("Failed to serialize tuple"))
}

/// # Safety
///
/// `ptr` must point to `len` bytes of stack data for the duration of the call. The out pointers
/// must be valid writable pointers owned by the native caller.
pub unsafe fn with_tuple_bytes(
    ptr: *const u8,
    len: usize,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_owner: *mut *mut c_void,
    out_free_owner: *mut Option<ExtMethodBytesFreeCallback>,
    f: impl FnOnce(&mut Tuple),
) -> bool {
    let boc = if ptr.is_null() {
        &[]
    } else {
        // SAFETY: The native caller guarantees that `ptr` is valid for `len` bytes.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };

    let is_stack_wire = boc.starts_with(STACK_WIRE_MAGIC);
    let mut tuple = if is_stack_wire {
        let Ok(tuple) = parse_stack_wire(boc) else {
            return false;
        };
        tuple
    } else {
        Boc::decode(boc)
            .ok()
            .and_then(|c| parse_tuple(&c).ok())
            .unwrap_or_else(Tuple::empty)
    };

    f(&mut tuple);

    if is_stack_wire {
        let Ok(bytes) = tuple_to_stack_wire(&tuple) else {
            return false;
        };
        bytes_to_ffi(bytes, out_data, out_len, out_owner, out_free_owner)
    } else {
        let Ok(cell) = serialize_tuple(&tuple) else {
            return false;
        };

        cell_to_ffi_boc_bytes(&cell, out_data, out_len, out_owner, out_free_owner)
    }
}

#[macro_export]
macro_rules! register_ext_methods {
    (@register_one $executor:expr, $ctx:expr, $id:expr => $fname:ident, $stack_items_count:expr) => {
        $executor
            .register_ext_method_bytes($id, ($ctx), $stack_items_count, $fname)
            .expect(&format!("cannot register extension with id: {}", $id));
    };
    (@register_one $executor:expr, $ctx:expr, $id:expr => $fname:ident) => {
        $executor
            .register_ext_method_bytes($id, ($ctx), ton_executor::EXT_METHOD_STACK_ALL_ITEMS, $fname)
            .expect(&format!("cannot register extension with id: {}", $id));
    };
    ($executor:expr, $ctx:expr, { $($id:expr => $fname:ident),+ $(,)? }) => {{
        $(
            $crate::register_ext_methods!(@register_one $executor, $ctx, $id => $fname);
        )+
    }};
    ($executor:expr, $ctx:expr, { $($id:expr => $fname:ident : $stack_items_count:expr),+ $(,)? }) => {{
        $(
            $crate::register_ext_methods!(@register_one $executor, $ctx, $id => $fname, $stack_items_count);
        )+
    }};
}
