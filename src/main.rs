mod compiler;
mod config;
mod executor;
mod stack_serialization;

use crate::compiler::{Compiler, TolkCompilerResult};
use crate::executor::{EmulationResult, Executor};
use crate::stack_serialization::{TupleItem, parse_tuple, serialize_tuple};
use num_bigint::{BigInt, BigUint};
use std::ffi::{CStr, CString, c_char};
use std::fs::read_to_string;
use std::path::Path;
use std::sync::Mutex;
use tonlib_core::TonAddress;
use tonlib_core::cell::{ArcCell, Cell, CellBuilder};
use tonlib_core::tlb_types::block::coins::{CurrencyCollection, Grams};
use tonlib_core::tlb_types::block::message::{CommonMsgInfo, IntMsgInfo, Message};
use tonlib_core::tlb_types::block::msg_address::MsgAddress;
use tonlib_core::tlb_types::block::state_init::StateInit;
use tonlib_core::tlb_types::primitives::either::EitherRef;
use tonlib_core::tlb_types::primitives::reference::Ref;
use tonlib_core::tlb_types::tlb::TLB;
use tycho_types::boc::Boc;
use tycho_types::cell::Load;
use tycho_types::models::{ComputePhase, Transaction, TxInfo};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn cell_to_ffi_boc64(cell: ArcCell) -> *const c_char {
    let str = cell.to_boc_b64(false).unwrap();
    let str = CString::new(str).unwrap();
    str.into_raw().cast_const()
}

static TESTS: Mutex<Vec<String>> = Mutex::new(vec![]);

fn main() {
    let compiler = Compiler::new();
    let compilation_result = compiler.compile(Path::new("main.tolk"));
    let code_cell = match compilation_result {
        Ok(TolkCompilerResult::Success(success)) => {
            // println!("Compilation successful!");
            // println!("Fift code {}", success._fift_code);
            // println!("Code BOC64: {}", success.code_boc64);
            // println!("Code hash: {}", success.code_hash_hex);

            ArcCell::from_boc_b64(&*success.code_boc64).unwrap()
        }
        Ok(TolkCompilerResult::Error(error)) => {
            println!("Compilation failed: {}", error.message);
            return;
        }
        Err(e) => {
            println!("Failed to parse compilation result: {}", e);
            return;
        }
    };

    let mut executor = Executor::new();

    let state_init = CellBuilder::new()
        .store_bit(false)
        .unwrap()
        .store_bit(false)
        .unwrap()
        .store_ref_cell_optional(Some(&code_cell))
        .unwrap()
        .store_ref_cell_optional(Some(&ArcCell::default()))
        .unwrap()
        .store_bit(false)
        .unwrap()
        .build()
        .unwrap();

    let dest_address = TonAddress::new(0, state_init.cell_hash());

    let msg = Message {
        info: CommonMsgInfo::Int(IntMsgInfo {
            ihr_disabled: true,
            bounce: true,
            bounced: false,
            src: MsgAddress::from_boc_hex("b5ee9c724101010100240000438015a63d6ec5cd11f837442aeba86b361f3890e715eca7c2cd44666017b8d6535d30a1578b99").unwrap(),
            dest: dest_address.to_msg_address(),
            value: CurrencyCollection {
                grams: Grams::new(BigUint::from(10000000000000000000u64)),
                other: None,
            },
            ihr_fee: Grams::new(BigUint::from(0u64)),
            fwd_fee: Grams::new(BigUint::from(0u64)),
            created_lt: 0,
            created_at: 0,
        }),
        init: Some(EitherRef::new(StateInit {
            split_depth: None,
            tick_tock: None,
            code: Some(Ref::new(code_cell)),
            data: Some(Ref::new(ArcCell::from_boc_hex("b5ee9c724101010100020000004cacb9cd").unwrap())),
            library: None,
        })),
        body: EitherRef::new(ArcCell::from(Cell::default())),
    };

    unsafe extern "C" fn __ext_print(str: *const c_char) -> *const c_char {
        let arg = unsafe { CStr::from_ptr(str) };

        let mut tuple =
            parse_tuple(&ArcCell::from_boc_b64(arg.to_str().unwrap()).unwrap()).unwrap();

        let arg1 = &tuple[tuple.len() - 1];

        match arg1 {
            TupleItem::Slice {
                cell,
                start_bits,
                end_bits,
                ..
            } => {
                let mut parser = cell.parser();
                parser.skip_bits(*start_bits as usize).unwrap();
                let bits = parser
                    .load_bits((*end_bits - *start_bits) as usize)
                    .unwrap();
                let string = String::from_utf8(bits).unwrap();

                println!("{}", string)
            }
            _ => {}
        }

        tuple.pop();
        cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
    }
    executor.register_ext_method(1, __ext_print);

    unsafe extern "C" fn __ext_eprint(str: *const c_char) -> *const c_char {
        let arg = unsafe { CStr::from_ptr(str) };

        let mut tuple =
            parse_tuple(&ArcCell::from_boc_b64(arg.to_str().unwrap()).unwrap()).unwrap();

        let arg1 = &tuple[tuple.len() - 1];

        match arg1 {
            TupleItem::Slice {
                cell,
                start_bits,
                end_bits,
                ..
            } => {
                let mut parser = cell.parser();
                parser.skip_bits(*start_bits as usize).unwrap();
                let bits = parser
                    .load_bits((*end_bits - *start_bits) as usize)
                    .unwrap();
                let string = String::from_utf8(bits).unwrap();

                eprintln!("{}", string)
            }
            _ => {}
        }

        tuple.pop();
        cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
    }
    executor.register_ext_method(2, __ext_eprint);

    unsafe extern "C" fn __ext_read_file(str: *const c_char) -> *const c_char {
        let arg = unsafe { CStr::from_ptr(str) };

        let mut tuple =
            parse_tuple(&ArcCell::from_boc_b64(arg.to_str().unwrap()).unwrap()).unwrap();

        let arg1 = tuple.pop().unwrap();

        match arg1 {
            TupleItem::Slice {
                cell,
                start_bits,
                end_bits,
                ..
            } => {
                let mut parser = cell.parser();
                parser.skip_bits(start_bits as usize).unwrap();
                let bits = parser.load_bits((end_bits - start_bits) as usize).unwrap();
                let path = String::from_utf8(bits).unwrap();

                let content = read_to_string(path);
                match content {
                    Ok(content) => {
                        let mut response_builder = CellBuilder::new();
                        response_builder
                            .store_bits(content.len() * 8, content.as_bytes())
                            .unwrap();

                        tuple.push(TupleItem::Slice {
                            cell: ArcCell::from(response_builder.build().unwrap()),
                            start_bits: 0,
                            end_bits: (content.len() * 8) as u32,
                            end_refs: 0,
                            start_refs: 0,
                        });
                    }
                    Err(_) => tuple.push(TupleItem::Null),
                }
            }
            _ => {}
        }
        cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
    }
    executor.register_ext_method(3, __ext_read_file);

    unsafe extern "C" fn __ext_assert_equal(str: *const c_char) -> *const c_char {
        let arg = unsafe { CStr::from_ptr(str) };

        let mut tuple =
            parse_tuple(&ArcCell::from_boc_b64(arg.to_str().unwrap()).unwrap()).unwrap();

        if tuple.len() < 4 {
            return cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap());
        }

        let arg1 = tuple.pop().unwrap();
        let arg2 = tuple.pop().unwrap();

        match (arg1, arg2) {
            (TupleItem::Tuple(left), TupleItem::Tuple(right)) => {
                if left.len() != right.len() {
                    eprintln!(
                        "Assertion failed: cannot compare values with different stack width, left: {} and right: {}",
                        left.len(),
                        right.len()
                    );
                    tuple.push(TupleItem::Int(BigInt::from(0)));
                } else if left != right {
                    eprintln!("Assertion failed: {:?} != {:?}", left, right);
                    tuple.push(TupleItem::Int(BigInt::from(0)));
                } else {
                    tuple.push(TupleItem::Int(BigInt::from(-1)));
                }
            }
            _ => {}
        }

        cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
    }
    executor.register_ext_method(4, __ext_assert_equal);

    unsafe extern "C" fn __ext_register_test(str: *const c_char) -> *const c_char {
        let arg = unsafe { CStr::from_ptr(str) };

        let mut tuple =
            parse_tuple(&ArcCell::from_boc_b64(arg.to_str().unwrap()).unwrap()).unwrap();

        if tuple.len() < 1 {
            return cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap());
        }

        let arg1 = tuple.pop().unwrap();

        match arg1 {
            TupleItem::Slice {
                cell,
                start_bits,
                end_bits,
                ..
            } => {
                let mut parser = cell.parser();
                parser.skip_bits(start_bits as usize).unwrap();
                let bits = parser.load_bits((end_bits - start_bits) as usize).unwrap();
                let name = String::from_utf8(bits).unwrap();

                TESTS.lock().unwrap().push(name.clone());
            }
            _ => {}
        }

        cell_to_ffi_boc64(serialize_tuple(&tuple).unwrap())
    }
    executor.register_ext_method(5, __ext_register_test);

    let output = executor.run_transaction(msg);
    match output {
        EmulationResult::Success(result) => {
            let tx_cell: tycho_types::cell::Cell =
                Boc::decode(base64::decode(&result.transaction).unwrap()).unwrap();
            let mut slice = tx_cell.as_slice().unwrap();
            let tx = Transaction::load_from(&mut slice).unwrap();

            let info: TxInfo = tx.info.parse().unwrap();
            println!("{:?}", info);
            let exit_code = match info {
                TxInfo::Ordinary(info) => match info.compute_phase {
                    ComputePhase::Skipped(_) => 0,
                    ComputePhase::Executed(phase) => phase.exit_code,
                },
                TxInfo::TickTock(_) => 0,
            };

            println!("{}", exit_code);
            println!("Transaction: {:?}", tx);
            println!("Shard account: {}", result.shard_account);
            println!("VM log: {}", result.vm_log);
            if let Some(actions) = result.actions {
                println!("Actions: {}", actions);
            }

            TESTS.lock().unwrap().iter().for_each(|test| {
                println!("{}", test);
            })
        }
        EmulationResult::Error(result) => {
            println!("Emulation error: {}", result.error);
            if let Some(vm_log) = result.vm_log {
                println!("VM log: {}", vm_log);
            }
            if let Some(vm_exit_code) = result.vm_exit_code {
                println!("VM exit code: {}", vm_exit_code);
            }
        }
    }
}
