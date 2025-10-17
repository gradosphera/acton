use crate::config::CONFIG;
use crate::exts_lib::Tuple;
use crate::stack_serialization::serialize_tuple;
use crate::{create_tvm_emulator, run_get_method, tvm_emulator_register_extmethod};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{CString, c_void};
use tonlib_core::tlb_types::tlb::TLB;

pub struct GetExecutor {
    inner: *mut c_void,
}

impl GetExecutor {
    pub fn new(params: GetMethodInternalParams) -> Self {
        let params = serde_json::to_string(&params).unwrap();
        let params = CString::new(params.as_str()).unwrap();
        GetExecutor {
            inner: unsafe { create_tvm_emulator(params.as_ptr()) },
        }
    }

    pub fn run_get_method(&self, args: GetMethodArgs) -> GetMethodResult {
        let stack = serialize_tuple(&**args.stack).unwrap();
        let params = serde_json::to_string(&args.params).unwrap();
        let config = CString::new(CONFIG).unwrap();

        let stack_b64 = stack.to_boc_b64(false).unwrap();
        let result = unsafe {
            let params = CString::new(params.as_str()).unwrap();
            let stack_b64 = CString::new(stack_b64).unwrap();
            run_get_method(
                self.inner,
                params.into_raw(),
                stack_b64.into_raw(),
                config.into_raw(),
            )
        };

        let output_str = unsafe { CString::from_raw(result).to_string_lossy().to_string() };

        serde_json::from_str::<GetInternalResult>(&output_str)
            .unwrap()
            .output
    }

    pub fn register_ext_method(
        &mut self,
        id: i32,
        callback: unsafe extern "C" fn(
            arg1: *const ::std::os::raw::c_char,
        ) -> *const ::std::os::raw::c_char,
    ) {
        let _ = unsafe {
            tvm_emulator_register_extmethod(self.inner, id, Some(callback));
        };
    }
}

pub struct GetMethodArgs {
    pub stack: Tuple,
    pub params: GetMethodInternalParams,
}

#[derive(Serialize, Clone)]
pub struct GetMethodInternalParams {
    pub code: String,
    pub data: String,
    pub verbosity: i32,
    pub libs: String,
    pub address: String,
    pub unixtime: i64,
    pub balance: String,
    pub rand_seed: String,
    pub gas_limit: String,
    pub method_id: i32,
    pub debug_enabled: bool,
    #[serde(default)]
    pub extra_currencies: HashMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_blocks_info: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GetInternalResult {
    output: GetMethodResult,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum GetMethodResult {
    Success(GetMethodResultSuccess),
    Error(GetMethodResultError),
}

#[derive(Deserialize, Debug)]
pub struct GetMethodResultSuccess {
    pub success: bool, // This should always be true for success
    pub stack: String,
    pub gas_used: String,
    pub vm_exit_code: i32,
    pub vm_log: String,
    pub missing_library: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GetMethodResultError {
    pub success: bool, // This should always be false for error
    pub error: String,
}
