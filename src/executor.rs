use crate::config::CONFIG;
use crate::{create_emulator, emulate_with_emulator, transaction_emulator_register_extmethod};
use serde::Deserialize;
use std::ffi::{CString, c_void};
use std::ptr::null;
use tonlib_core::cell::ArcCell;
use tonlib_core::tlb_types::block::message::Message;
use tonlib_core::tlb_types::tlb::TLB;

pub struct Executor {
    inner: *mut c_void,
}

impl Executor {
    pub fn new() -> Self {
        let config = CString::new(CONFIG).unwrap();
        Executor {
            inner: unsafe { create_emulator(config.as_ptr(), 5) },
        }
    }

    pub fn run_transaction(&self, message: Message) -> EmulationResult {
        self.run_transaction_cell(ArcCell::from(message.to_cell().unwrap()))
    }

    pub fn run_transaction_cell(&self, message: ArcCell) -> EmulationResult {
        let message = CString::new(message.to_boc_b64(false).unwrap()).unwrap();
        let shard_account = CString::new(
            "te6cckEBAgEALgABUAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAFAAVi1MQ==",
        )
        .unwrap();
        let params = CString::new(r#"{"utime":0,"lt":"0","rand_seed":"0000000000000000000000000000000000000000000000000000000000000000","ignore_chksig":false,"debug_enabled":true}"#).unwrap();

        let result = unsafe {
            emulate_with_emulator(
                self.inner,
                null(),
                shard_account.as_ptr(),
                message.as_ptr(),
                params.as_ptr(),
            )
        };

        let output_str = unsafe { CString::from_raw(result).to_string_lossy().to_string() };

        let output = serde_json::from_str::<EmulationInternalResult>(&output_str).unwrap();
        output.output
    }

    pub fn register_ext_method(
        &mut self,
        id: i32,
        callback: unsafe extern "C" fn(
            arg1: *const ::std::os::raw::c_char,
        ) -> *const ::std::os::raw::c_char,
    ) {
        let _ = unsafe {
            transaction_emulator_register_extmethod(self.inner, id, Some(callback));
        };
    }
}

#[derive(Deserialize)]
struct EmulationInternalResult {
    pub output: EmulationResult,
    pub logs: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum EmulationResult {
    Success(ResultSuccess),
    Error(ResultError),
}

#[derive(Deserialize)]
pub struct ResultSuccess {
    pub transaction: String,
    pub shard_account: String,
    pub vm_log: String,
    pub actions: Option<String>,
}

#[derive(Deserialize)]
pub struct ResultError {
    pub error: String,
    pub vm_log: Option<String>,
    pub vm_exit_code: Option<i64>,
}
