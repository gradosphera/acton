use crate::config::CONFIG;
use crate::{create_emulator, emulate_with_emulator, transaction_emulator_register_extmethod};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::ptr::null;
use std::sync::Mutex;
use tonlib_core::cell::ArcCell;
use tonlib_core::tlb_types::block::message::Message;
use tonlib_core::tlb_types::tlb::TLB;
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellFamily, HashBytes, Lazy, Store};
use tycho_types::models::{OptionalAccount, ShardAccount};
use tycho_types::prelude::CellBuilder;

lazy_static! {
    pub static ref SHARD_ACCOUNTS: Mutex<HashMap<String, ShardAccount>> =
        Mutex::new(HashMap::new());
    pub static ref EXECUTOR: Mutex<Executor> = Mutex::new(Executor::new());
}

pub fn get_account(addr: String) -> ShardAccount {
    let mut result = SHARD_ACCOUNTS.lock().unwrap();
    let account = result.get(&addr);

    match account {
        Some(arg) => arg.clone(),
        None => {
            let acc = ShardAccount {
                account: Lazy::new(&OptionalAccount(None)).unwrap(),
                last_trans_hash: HashBytes::ZERO,
                last_trans_lt: 0,
            };
            result.insert(addr.to_string(), acc.clone());
            acc
        }
    }
}

pub(crate) fn update_account(addr: String, account: ShardAccount) {
    let mut shard_accounts = SHARD_ACCOUNTS.lock().unwrap();
    shard_accounts.insert(addr, account);
}

pub struct Executor {
    inner: *mut c_void,
}

unsafe impl Send for Executor {}
unsafe impl Sync for Executor {}

impl Executor {
    pub fn new() -> Self {
        let config = CString::new(CONFIG).unwrap();
        Executor {
            inner: unsafe { create_emulator(config.as_ptr(), 5) },
        }
    }

    pub fn run_transaction(&self, message: Message) -> EmulationResult {
        self.run_transaction_cell("".to_string(), ArcCell::from(message.to_cell().unwrap()))
    }

    pub fn run_transaction_cell(&self, dst: String, message: ArcCell) -> EmulationResult {
        let message = CString::new(message.to_boc_b64(false).unwrap()).unwrap();

        let shard_account = get_account(dst);
        let mut builder = CellBuilder::new();
        shard_account
            .store_into(&mut builder, Cell::empty_context())
            .unwrap();
        let new_cell = builder.build().unwrap();
        let shard_account_b64 = Boc::encode_base64(new_cell);
        let shard_account_b64_cstring = CString::new(shard_account_b64).unwrap();

        let params = CString::new(r#"{"utime":0,"lt":"0","rand_seed":"0000000000000000000000000000000000000000000000000000000000000000","ignore_chksig":false,"debug_enabled":true}"#).unwrap();

        let result = unsafe {
            emulate_with_emulator(
                self.inner,
                null(),
                shard_account_b64_cstring.as_ptr(),
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
