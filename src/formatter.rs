use crate::context::{BuildCache, KnownAddresses};
use abi::ContractAbi;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use tonlib_core::tlb_types::tlb::TLB;
use tvmffi::stack::{Tuple, TupleItem, TupleSlice};
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, Load};
use tycho_types::models::{
    AccountState, AccountStatus, ComputePhase, IntAddr, MsgInfo, ShardAccount, Transaction, TxInfo,
};

/// Context for formatting TupleItems with rich information
#[derive(Debug, Clone)]
pub struct FormatterContext {
    pub contract_abi: ContractAbi,
    pub accounts: HashMap<String, ShardAccount>,
    pub build_cache: BuildCache,
    pub known_addresses: KnownAddresses,
}

impl FormatterContext {
    /// Create formatter context from the main Context
    pub fn from_context(ctx: &crate::context::Context) -> Self {
        Self {
            contract_abi: ctx.abi.clone(),
            accounts: ctx.blockchain.get_accounts().clone(),
            build_cache: ctx.build_cache.clone(),
            known_addresses: ctx.known_addresses.clone(),
        }
    }

    /// Format a tuple item with the given type name
    pub fn format_item_with_type(&self, item: &TupleItem, type_name: &str) -> String {
        match item {
            TupleItem::Int(value) if type_name == "bool" => {
                if value == &num_bigint::BigInt::from(0) {
                    "false".to_string()
                } else if value == &num_bigint::BigInt::from(18446744073709551615u64) {
                    "true".to_string()
                } else {
                    format!("{}", value)
                }
            }
            TupleItem::Slice(slice) if type_name == "address" => {
                self.format_address_from_slice(slice)
            }
            _ => format!("{}", item),
        }
    }

    /// Format address from slice
    fn format_address_from_slice(&self, slice: &TupleSlice) -> String {
        let length = slice.end_bits - slice.start_bits;
        let mut parser = slice.cell.parser();
        let Ok(()) = parser.skip_bits(slice.start_bits as usize) else {
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

    /// Format transaction list
    pub fn format_transaction_list(&self, items: &[TupleItem]) -> String {
        let item = &items[0];
        let TupleItem::Tuple(tx_items) = item else {
            return format!("{}", items[0]);
        };

        let txs = tx_items
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

        let mut builder = String::new();

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

                let message_abi = self
                    .contract_abi
                    .messages
                    .iter()
                    .find(|msg| msg.opcode != Some(0) && msg.opcode == Some(opcode));

                let amount = info.value.tokens.into_inner() as f64 / 1e9;

                let src_contract_type = self.get_contract_type(&info.src);
                if src_contract_type != "" {
                    tx_builder += format!("{}", src_contract_type.cyan()).as_str();
                } else {
                    tx_builder += Self::show_addr(&info.src).dimmed().to_string().as_str();
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

                let dst_contract_type = self.get_contract_type(&info.dst);
                if dst_contract_type != "" {
                    tx_builder += format!("{}", dst_contract_type.cyan()).as_str();
                } else {
                    tx_builder += Self::show_addr(&info.dst).dimmed().to_string().as_str();
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

                if tx.orig_status == AccountStatus::NotExists
                    && tx.end_status == AccountStatus::Active
                {
                    tx_builder += "\n";
                    tx_builder += "└─".dimmed().to_string().as_str();
                    tx_builder += " account created";
                }
                if tx.orig_status == AccountStatus::Active
                    && tx.end_status == AccountStatus::NotExists
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

    /// Get contract type for address
    fn get_contract_type(&self, addr: &IntAddr) -> String {
        let known_address = self
            .known_addresses
            .addresses
            .iter()
            .find(|(address, _info)| {
                let a1 = address.to_string();
                let s2 = addr.to_string();
                a1 == s2
            });
        if let Some(known_address) = known_address {
            return known_address.1.name.clone();
        }

        let account = self.accounts.get(&addr.to_string());
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

        let compilation_result = self.build_cache.built.iter().find(|(_name, result)| {
            result.code_hash.to_ascii_lowercase() == code.repr_hash().to_string()
        });

        if let Some(result) = compilation_result {
            return result.1.name.clone();
        }

        "".to_string()
    }

    /// Format any TupleItem with rich formatting
    pub fn format(&self, item: &TupleItem) -> String {
        let formatted = match item {
            TupleItem::TypedTuple {
                type_name, items, ..
            } => {
                if type_name == "TransactionList" && items.len() == 1 {
                    self.format_transaction_list(items)
                } else {
                    // For other TypedTuple, use basic Display formatting
                    // TODO: Add more rich formatting for other types as needed
                    format!("{}", item)
                }
            }
            TupleItem::Slice(slice) => {
                if let Some(string) = Tuple::parse_snake_string(slice) {
                    format!("\"{}\"", string)
                } else {
                    self.format_address_from_slice(slice)
                }
            }
            _ => format!("{}", item),
        };

        // Remove quotes from strings if it's the root element
        if formatted.starts_with("\"") && formatted.ends_with("\"") {
            formatted[1..formatted.len() - 1].to_string()
        } else {
            formatted
        }
    }

    /// Show address in short format
    fn show_addr(addr: &IntAddr) -> String {
        let raw = addr.as_std().unwrap().display_base64(true).to_string();
        raw[..6].to_string() + ".." + &raw[raw.len() - 6..]
    }
}
