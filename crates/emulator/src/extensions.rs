//! This module defines a simple DSL for defining extension functions for the emulator.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use tonlib_core::cell::ArcCell;
use tonlib_core::tlb_types::tlb::TLB;
use tvmffi::from_stack::{ArgError, FromStack};
use tvmffi::stack::Tuple;

pub fn pop_arg<T: FromStack>(t: &mut Tuple) -> Result<T, ArgError> {
    let item = t.pop().ok_or(ArgError::StackUnderflow)?;
    T::from_item(item)
}

#[macro_export]
macro_rules! pop_args {
    ($tuple:expr, $($ty:ty),+ $(,)?) => {{
        #[allow(non_snake_case)]
        {
            let mut __errors: Option<tvmffi::from_stack::ArgError> = None;
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
        }
    }};
}

#[macro_export]
macro_rules! extension {
    ($fn_name:ident in ($ctx_ty:ty) with ($an:ident : $ty:ty) using $body:expr) => {
        unsafe extern "C" fn $fn_name(ctx: *mut std::os::raw::c_void, ptr: *const std::os::raw::c_char) -> *const std::os::raw::c_char {
            unsafe {
                let ctx = std::mem::transmute::<*mut std::os::raw::c_void, &mut $ctx_ty>(ctx);
                $crate::extensions::with_tuple(ptr, |__t: &mut tvmffi::stack::Tuple| {
                    match (|| -> Result<$ty, tvmffi::from_stack::ArgError> {
                        $crate::extensions::pop_arg::<$ty>(__t)
                    })() {
                        Ok($an) => {
                            $body(ctx, __t, $an);
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
        unsafe extern "C" fn $fn_name(ctx: *mut std::os::raw::c_void, ptr: *const std::os::raw::c_char) -> *const std::os::raw::c_char {
            unsafe {
                let ctx = std::mem::transmute::<*mut std::os::raw::c_void, &mut $ctx_ty>(ctx);
                $crate::extensions::with_tuple(ptr, |__t: &mut tvmffi::stack::Tuple| {
                    match (|| -> Result<($($ty),*), tvmffi::from_stack::ArgError> {
                        pop_args!(__t, $($ty),*)
                    })() {
                        Ok(__vals) => {
                            #[allow(non_snake_case, unused_variables)]
                            let ($($an, )*) = __vals;
                            $body(ctx, __t, $($an, )*);
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

fn cell_to_ffi_boc64(cell: ArcCell) -> *const c_char {
    let s = cell.to_boc_b64(false).unwrap();
    CString::new(s).unwrap().into_raw().cast_const()
}

pub unsafe fn with_tuple(ptr: *const c_char, f: impl FnOnce(&mut Tuple)) -> *const c_char {
    let c = unsafe { CStr::from_ptr(ptr) };
    let boc = match c.to_str() {
        Ok(s) => s,
        Err(_) => return CString::new("").unwrap().into_raw().cast_const(),
    };

    let mut tuple = Tuple(
        ArcCell::from_boc_b64(boc)
            .ok()
            .and_then(|c| tvmffi::serde::parse_tuple(&c).ok())
            .unwrap_or_else(|| Vec::new()),
    );

    f(&mut tuple);

    cell_to_ffi_boc64(tvmffi::serde::serialize_tuple(&tuple).unwrap())
}

#[macro_export]
macro_rules! register_ext_methods {
    ($executor:expr, $ctx:expr, { $($id:expr => $fname:ident),+ $(,)? }) => {{
        $(
            $executor.register_ext_method($id, ($ctx) as *mut _ as *mut std::ffi::c_void, $fname);
        )+
    }};
}
