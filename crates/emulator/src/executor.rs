use crate::config::CONFIG;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::ptr::null;
use std::sync::Mutex;
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellFamily, HashBytes, Lazy, Store};
use tycho_types::models::{Message, OptionalAccount, ShardAccount};
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

pub fn update_account(addr: String, account: ShardAccount) {
    let mut shard_accounts = SHARD_ACCOUNTS.lock().unwrap();
    shard_accounts.insert(addr, account);
}

pub struct Executor {
    inner: *mut c_void,
}

unsafe impl Send for Executor {}
unsafe impl Sync for Executor {}

pub trait StoreExt: Store {
    fn to_cell(&self) -> Cell;
}

impl<T: Store + ?Sized> StoreExt for T {
    fn to_cell(&self) -> Cell {
        let mut builder = CellBuilder::new();
        self.store_into(&mut builder, Cell::empty_context())
            .unwrap();
        builder.build().unwrap()
    }
}

impl Executor {
    pub fn new() -> Self {
        let config = CString::new(CONFIG).unwrap();
        Executor {
            inner: unsafe { create_emulator(config.as_ptr(), 5) },
        }
    }

    pub fn run_transaction(&self, dst_addr: String, message: Message) -> EmulationResult {
        let msg_cell = message.to_cell();
        self.run_transaction_cell(dst_addr, msg_cell)
    }

    pub fn run_transaction_cell(&self, dst_addr: String, message: Cell) -> EmulationResult {
        let message = CString::new(Boc::encode_base64(message)).unwrap();

        let shard_account = get_account(dst_addr);
        let shard_account_cell = shard_account.to_cell();
        let shard_account_b64 = Boc::encode_base64(shard_account_cell);
        let shard_account_b64_cst = CString::new(shard_account_b64).unwrap();

        let params = CString::new(r#"{"utime":0,"lt":"0","rand_seed":"0000000000000000000000000000000000000000000000000000000000000000","ignore_chksig":false,"debug_enabled":true}"#).unwrap();

        let result = unsafe {
            emulate_with_emulator(
                self.inner,
                null(),
                shard_account_b64_cst.as_ptr(),
                message.as_ptr(),
                params.as_ptr(),
            )
        };

        let output_cstr = unsafe { CString::from_raw(result).to_string_lossy().to_string() };

        let output = serde_json::from_str::<EmulationInternalResult>(&output_cstr).unwrap();
        output.output
    }

    pub fn register_ext_method(&mut self, id: i32, callback: RegisterExtMethodCallback) {
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

unsafe extern "C" {
    pub fn create_emulator(
        config: *const ::std::os::raw::c_char,
        verbosity: ::std::os::raw::c_int,
    ) -> *mut ::std::os::raw::c_void;
}
pub type ExtFunc = Option<
    unsafe extern "C" fn(arg1: *const ::std::os::raw::c_char) -> *const ::std::os::raw::c_char,
>;
unsafe extern "C" {
    pub fn emulate_with_emulator(
        em: *mut ::std::os::raw::c_void,
        libs: *const ::std::os::raw::c_char,
        account: *const ::std::os::raw::c_char,
        message: *const ::std::os::raw::c_char,
        params: *const ::std::os::raw::c_char,
    ) -> *mut ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn transaction_emulator_register_extmethod(
        transaction_emulator: *mut ::std::os::raw::c_void,
        id: ::std::os::raw::c_int,
        callback: ExtFunc,
    ) -> *const ::std::os::raw::c_char;
}

type RegisterExtMethodCallback =
    unsafe extern "C" fn(arg1: *const ::std::os::raw::c_char) -> *const ::std::os::raw::c_char;
