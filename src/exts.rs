use crate::context::Context;
use emulator::emulator::SendMessageResult;
use emulator::executor::{EmulationResult, Executor};
use emulator::get_executor::{GetExecutor, GetMethodParams, GetMethodResult};
use emulator::tuple::stack::{Tuple, TupleItem, parse_tuple};
use emulator::{extension, pop_args, register_ext_methods};
use num_bigint::BigInt;
use std::collections::HashMap;
use std::path::Path;
use tonlib_core::TonAddress;
use tonlib_core::cell::ArcCell;
use tonlib_core::tlb_types::block::msg_address::MsgAddrIntStd;
use tonlib_core::tlb_types::tlb::TLB;
use tycho_types::boc::Boc;
use tycho_types::cell::Cell;
use tycho_types::models::{AccountState, IntAddr};

extension!(read_file in (Context) with (path: String) using read_file_impl);
fn read_file_impl(_ctx: &mut Context, stack: &mut Tuple, path: String) {
    match std::fs::read_to_string(&path) {
        Ok(content) => stack.push_string(&content),
        Err(_) => stack.push(TupleItem::Null),
    }
}

extension!(build in (Context) with (path: String) using build_impl);
fn build_impl(_ctx: &mut Context, stack: &mut Tuple, path: String) {
    let result = tolkc::compile(Path::new(&path));
    match result {
        tolkc::CompilerResult::Success(success) => {
            let code_cell = ArcCell::from_boc_b64(&*success.code_boc64).unwrap();
            stack.push(TupleItem::Cell(code_cell))
        }
        tolkc::CompilerResult::Error(error) => {
            println!("Compilation failed: {}", error.message);
            stack.push(TupleItem::Null);
        }
    };
}

extension!(send_message in (Context) with (mode: BigInt, message: ArcCell) using send_message_impl);
fn send_message_impl(ctx: &mut Context, stack: &mut Tuple, mode: BigInt, message: ArcCell) {
    let blockchain = &mut ctx.blockchain;
    let emulator = &ctx.emulator;

    let msg_b64 = message.to_boc_b64(false).unwrap();
    let msg_cell = Boc::decode_base64(msg_b64).unwrap();

    // Send from null address for now
    let src_addr = IntAddr::default();
    let emulations = emulator.send_message(blockchain, msg_cell, Some(src_addr));

    let successful_emulations = emulations.iter().filter_map(|emulation| match emulation {
        SendMessageResult::Success(res) => Some(res),
        SendMessageResult::Error(_) => None,
    });

    let transaction_cells = successful_emulations
        .filter_map(|emulation| ArcCell::from_boc_b64(&*emulation.raw_transaction).ok())
        .map(|tx| TupleItem::Cell(tx))
        .collect::<Vec<_>>();
    stack.push(TupleItem::Tuple(transaction_cells));
}

extension!(run_get_method in (Context) with (args: Tuple, return_type_name: String, id: BigInt, code: ArcCell, address: ArcCell) using run_get_method_impl);
fn run_get_method_impl(
    ctx: &mut Context,
    stack: &mut Tuple,
    args: Tuple,
    return_type_name: String,
    id: BigInt,
    code: ArcCell,
    address: ArcCell,
) {
    let blockchain = &mut ctx.blockchain;
    let address_boc = address.to_boc_hex(false).unwrap();

    let address_std = MsgAddrIntStd::from_boc_hex(address_boc.as_str()).unwrap();
    let dst_addr_str = format!(
        "{}:{}",
        &address_std.workchain,
        hex::encode(&address_std.address)
    );

    let dest_address = TonAddress::from_msg_address(address_std).unwrap();

    let shard_account = blockchain.get_account(dst_addr_str);
    let state = shard_account.account.load().unwrap().0.map(|s| s.state);

    let data = if let Some(AccountState::Active(state)) = state {
        state.data.unwrap_or(Cell::default())
    } else {
        Cell::default()
    };

    let params = GetMethodParams {
        code: code.to_boc_b64(false).unwrap().to_string(),
        data: Boc::encode_base64(data),
        verbosity: 5,
        libs: "".to_string(),
        address: dest_address.to_string(),
        unixtime: 0,
        balance: "10".to_string(),
        rand_seed: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        gas_limit: "0".to_string(),
        method_id: if id == BigInt::default() {
            0
        } else {
            id.to_u64_digits().1[0] as i32
        },
        debug_enabled: true,
        extra_currencies: HashMap::new(),
        prev_blocks_info: None,
    };

    let executor = GetExecutor::new(params.clone());

    let result = executor.run_get_method(args, params);

    match result {
        GetMethodResult::Success(result) => {
            let cell = ArcCell::from_boc_b64(&result.stack).unwrap();
            let tuple = parse_tuple(&cell).unwrap();

            stack.push(TupleItem::TypedTuple {
                contract_abi: ctx.abi.clone(),
                abi: ctx.abi.find_type(&return_type_name),
                type_name: return_type_name,
                items: tuple,
            })
        }
        GetMethodResult::Error(result) => {
            println!("Error: {}", result.error);
        }
    };
}

pub fn register_extensions(executor: &mut Executor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        3 => read_file,
        6 => build,
        7 => send_message,
        8 => run_get_method,
    });
}

pub fn register_get_extensions(executor: &mut GetExecutor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        3 => read_file,
        6 => build,
        7 => send_message,
        8 => run_get_method,
    });
}
