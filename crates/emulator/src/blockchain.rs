use num_bigint::BigInt;
use num_traits::cast::ToPrimitive;
use std::collections::HashMap;
use tycho_types::cell::{Cell, HashBytes, Lazy};
use tycho_types::models::{AccountState, OptionalAccount, ShardAccount};

pub fn account_code(accounts: &HashMap<String, ShardAccount>, addr: String) -> Option<Cell> {
    let account = accounts.get(&addr);
    let state = account?.account.load().ok()?.0?.state;
    match state {
        AccountState::Uninit => None,
        AccountState::Active(state) => state.code,
        AccountState::Frozen(_) => None,
    }
}

pub struct Blockchain {
    accounts: HashMap<String, ShardAccount>,
    current_lt: BigInt,
    libraries: Vec<Cell>,
}

impl Blockchain {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            current_lt: BigInt::from(0),
            libraries: vec![],
        }
    }

    pub fn get_accounts(&self) -> &HashMap<String, ShardAccount> {
        &self.accounts
    }

    pub fn is_deployed(&self, raw_addr: &String) -> bool {
        self.accounts.contains_key(raw_addr)
    }

    pub fn get_account(&mut self, raw_addr: &String) -> ShardAccount {
        let account = self.accounts.get(raw_addr);

        match account {
            Some(arg) => arg.clone(),
            None => {
                let acc = ShardAccount {
                    account: Lazy::new(&OptionalAccount(None)).unwrap(),
                    last_trans_hash: HashBytes::ZERO,
                    last_trans_lt: self.current_lt.to_u64().unwrap_or(0),
                };
                self.accounts.insert(raw_addr.to_string(), acc.clone());
                acc
            }
        }
    }

    pub fn update_account(&mut self, addr: &String, account: &ShardAccount) {
        self.accounts.insert(addr.clone(), account.clone());
    }

    pub fn get_lt(&mut self) -> BigInt {
        self.current_lt += BigInt::from(1_000_000);
        self.current_lt.clone()
    }

    pub fn libs(&self) -> Vec<Cell> {
        self.libraries.clone()
    }

    pub fn register_lib(&mut self, lib: Cell) {
        self.libraries.push(lib);
    }
}
