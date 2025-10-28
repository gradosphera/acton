use crate::stack::{BuildCache, KnownAddresses, TupleItem, TupleSlice};
use abi::ContractAbi;
use num_bigint::BigInt;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fmt;
use tonlib_core::tlb_types::tlb::TLB;
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, Load};
use tycho_types::models::{
    AccountState, AccountStatus, ComputePhase, IntAddr, MsgInfo, ShardAccount, Transaction, TxInfo,
};

pub fn format_item_with_type(item: &TupleItem, type_name: &str) -> String {
    let item = item.unwrap_single();

    match item {
        TupleItem::Int(value) if type_name == "bool" => {
            if value == BigInt::from(0) {
                "false".to_string()
            } else if value == BigInt::from(18446744073709551615u64) {
                "true".to_string()
            } else {
                format!("{}", value)
            }
        }
        TupleItem::Slice(TupleSlice {
            cell,
            start_bits,
            end_bits,
            ..
        }) if type_name == "address" => {
            let length = end_bits - start_bits;
            let mut parser = cell.parser();
            let Ok(()) = parser.skip_bits(start_bits as usize) else {
                return "Slice(...)".to_string();
            };
            if length == 2 && parser.load_u8(2).unwrap_or(0) == 0 {
                return "addr_none".to_string();
            }
            if length != 267 {
                return "Slice(...)".to_string();
            }
            let Ok(address) = parser.load_address() else {
                return "Slice(...)".to_string();
            };
            address.to_string()
        }
        _ => format!("{}", item),
    }
}

impl fmt::Display for TupleItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TupleItem::Int(value) => {
                if *value == BigInt::from(18446744073709551615u64) {
                    write!(f, "-1")
                } else {
                    write!(f, "{}", value)
                }
            }
            TupleItem::Null => write!(f, "null"),
            TupleItem::Nan => write!(f, "NaN"),
            TupleItem::Cell(cell) => write!(f, "{:?}", cell),
            TupleItem::Slice(slice) => {
                if let Some(string) = crate::snake_string::snake_string_from_slice(slice) {
                    write!(f, "\"{}\"", string)
                } else {
                    write!(f, "Slice(...)")
                }
            }
            TupleItem::Builder(_) => write!(f, "Builder(...)"),
            TupleItem::Tuple(items) => {
                if items.len() == 1 {
                    write!(f, "{}", items[0])
                } else {
                    write!(f, "(")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", item)?;
                    }
                    write!(f, ")")
                }
            }
            TupleItem::TypedTuple {
                type_name,
                items,
                abi,
                contract_abi,
                accounts,
                build_cache,
                known_addresses,
            } => {
                if type_name == "address" && items.len() == 1 {
                    let addr = &items[0];
                    return write!(f, "{}", format_item_with_type(addr, type_name));
                }

                if type_name == "TransactionList" && items.len() == 1 {
                    return write!(
                        f,
                        "{}",
                        format_transaction_list(
                            &items,
                            contract_abi,
                            accounts,
                            build_cache,
                            known_addresses
                        )
                    );
                }

                if items.len() == 1 {
                    write!(f, "{}", items[0])
                } else {
                    if let Some(struct_desc) = abi {
                        if items.len() == struct_desc.fields.len() {
                            write!(f, "{} {{\n", type_name)?;
                            for (i, (field, item)) in
                                struct_desc.fields.iter().zip(items.iter()).enumerate()
                            {
                                write!(
                                    f,
                                    "    {}: {}",
                                    field.name,
                                    format_item_with_type(item, &field.type_info.human_readable)
                                )?;
                                if i < struct_desc.fields.len() - 1 {
                                    write!(f, ",")?;
                                }
                                write!(f, "\n")?;
                            }
                            write!(f, "}}")?;
                            return Ok(());
                        }
                    }

                    write!(
                        f,
                        "{}({})",
                        type_name,
                        items
                            .iter()
                            .map(|item| format!("{}", item))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
        }
    }
}

fn show_addr(addr: &IntAddr) -> String {
    let raw = addr.as_std().unwrap().display_base64(true).to_string();
    raw[..6].to_string() + ".." + &raw[raw.len() - 6..]
}

fn format_transaction_list(
    items: &&Vec<TupleItem>,
    contract_abi: &ContractAbi,
    accounts: &HashMap<String, ShardAccount>,
    build_cache: &BuildCache,
    known_addresses: &KnownAddresses,
) -> String {
    let item = &items[0];
    let TupleItem::Tuple(items) = item else {
        return format!("{}", items[0]);
    };

    let txs = items
        .iter()
        .filter_map(|el| match el {
            TupleItem::Cell(cell) => Some(cell),
            _ => None,
        })
        .map(|x| {
            let result = x.to_boc_b64(false).unwrap();
            let tx_cell: Cell = Boc::decode_base64(&result).unwrap();
            let mut tx_slice = tx_cell.as_slice().unwrap();
            Transaction::load_from(&mut tx_slice).unwrap()
        })
        .collect::<Vec<_>>();

    let mut builder = "".to_string();

    let mut known_contracts: Vec<IntAddr> = vec![];

    for tx in &txs {
        let in_msg = tx.load_in_msg().unwrap();
        if let Some(in_msg) = &in_msg
            && let MsgInfo::Int(info) = &in_msg.info
        {
            // It's O(N) but we need order, and we don't have many (thousands) transactions
            if !known_contracts.contains(&info.src) {
                known_contracts.push(info.src.clone());
            }
            if !known_contracts.contains(&info.dst) {
                known_contracts.push(info.dst.clone());
            }
        }
    }

    let mut contract_letters: HashMap<IntAddr, String> = HashMap::new();

    for (index, addr) in known_contracts.iter().enumerate() {
        let letter = char::from_u32('A' as u32 + index as u32)
            .unwrap_or_else(|| char::from_digit(index as u32, 10).unwrap());
        contract_letters.insert(addr.clone(), letter.to_string());
    }

    for tx in txs {
        let mut tx_builder = "\x1b[0m".to_string();

        tx_builder += "\x1b[0m";
        let in_msg = tx.load_in_msg().unwrap();
        if let Some(in_msg) = &in_msg
            && let MsgInfo::Int(info) = &in_msg.info
        {
            if info.bounced {
                tx_builder += "(!) ".red().to_string().as_str()
            }

            let mut body = in_msg.body.clone();
            let mut opcode = body.load_u32().unwrap_or(0);
            if opcode == 0xFFFFFFFF {
                // if bounce read another 32 bit to get actual opcode
                opcode = body.load_u32().unwrap_or(0);
            }

            let message_abi = contract_abi
                .messages
                .iter()
                .find(|msg| msg.opcode != Some(0) && msg.opcode == Some(opcode));

            let amount = info.value.tokens.into_inner() as f64 / 1e9;

            let src_contract_type =
                get_contract_type(accounts, build_cache, known_addresses, &info.src);
            if src_contract_type != "" {
                tx_builder += format!("{}", src_contract_type.cyan()).as_str();
            } else {
                tx_builder += show_addr(&info.src).dimmed().to_string().as_str();
            }

            let letter = contract_letters.get(&info.src);
            if let Some(letter) = letter {
                tx_builder += format!(" {}  ", letter.bold()).as_str();
            }

            tx_builder += " ";
            tx_builder += "-> ";

            if let Some(message_abi) = message_abi {
                tx_builder += message_abi
                    .name
                    .as_str()
                    .purple()
                    .bold()
                    .to_string()
                    .as_str();
            } else if opcode == 0 {
                tx_builder += "empty".purple().bold().to_string().as_str();
            } else {
                tx_builder += format!("0x{:x}", opcode)
                    .purple()
                    .bold()
                    .to_string()
                    .as_str();
            }
            tx_builder += " ";

            tx_builder += &format!("{} TON", amount.to_string()).green().to_string();
            tx_builder += " -> ";

            let dst_contract_type =
                get_contract_type(accounts, build_cache, known_addresses, &info.dst);
            if dst_contract_type != "" {
                tx_builder += format!("{}", dst_contract_type.cyan()).as_str();
            } else {
                tx_builder += show_addr(&info.dst).dimmed().to_string().as_str();
            }

            let letter = contract_letters.get(&info.dst);
            if let Some(letter) = letter {
                tx_builder += format!(" {}  ", letter.bold()).as_str();
            }
        }

        let TxInfo::Ordinary(info) = tx.load_info().unwrap() else {
            panic!("tick-tock message is unexpected")
        };

        if let ComputePhase::Executed(compute) = info.compute_phase {
            tx_builder += format!(" gas={}", compute.gas_used.to_string().as_str())
                .dimmed()
                .to_string()
                .as_str();

            if compute.exit_code != 0 {
                tx_builder += format!(" exit_code={}", compute.exit_code)
                    .red()
                    .to_string()
                    .as_str();
            }

            // tx_builder += format!("  lt: {} prev_lt: {}", tx.lt, tx.prev_trans_lt).as_str();

            if tx.orig_status == AccountStatus::NotExists && tx.end_status == AccountStatus::Active
            {
                tx_builder += "\n";
                tx_builder += "└─".dimmed().to_string().as_str();
                tx_builder += " account created";
            }
            if tx.orig_status == AccountStatus::Active && tx.end_status == AccountStatus::NotExists
            {
                tx_builder += "\n";
                tx_builder += "└─".dimmed().to_string().as_str();
                tx_builder += " account destroyed"
            }
        } else {
            tx_builder += format!(" {}", "compute phase skipped".dimmed()).as_str();
        }

        builder.push_str(&tx_builder);
        builder.push_str("\n");
    }

    builder
}

fn get_contract_type(
    accounts: &HashMap<String, ShardAccount>,
    build_cache: &BuildCache,
    known_addresses: &KnownAddresses,
    addr: &IntAddr,
) -> String {
    let known_address = known_addresses.addresses.iter().find(|(address, _info)| {
        let a1 = address.to_string();
        let s2 = addr.to_string();
        a1 == s2
    });
    if let Some(known_address) = known_address {
        return known_address.1.name.clone();
    }

    let account = accounts.get(&addr.to_string());
    let Some(account) = account else {
        return "".to_string();
    };

    let account_data = account.load_account();
    let Ok(Some(data)) = account_data else {
        return "".to_string();
    };

    let AccountState::Active(info) = data.state else {
        return "".to_string();
    };

    let Some(code) = &info.code else {
        return "".to_string();
    };

    let compilation_result = build_cache.built.iter().find(|(_name, result)| {
        result.code_hash.to_ascii_lowercase() == code.repr_hash().to_string()
    });

    if let Some(result) = compilation_result {
        return result.1.name.clone();
    }

    "".to_string()
}
