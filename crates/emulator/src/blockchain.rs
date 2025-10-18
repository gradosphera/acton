use crate::executor::Executor;
use std::collections::HashMap;
use tycho_types::cell::{HashBytes, Lazy};
use tycho_types::models::{OptionalAccount, ShardAccount};

pub struct Blockchain {
    pub executor: Executor,
    accounts: HashMap<String, ShardAccount>,
}

impl Blockchain {
    pub fn new(executor: Executor) -> Self {
        Self {
            executor,
            accounts: HashMap::new(),
        }
    }

    pub fn get_account(&mut self, addr: String) -> ShardAccount {
        let account = self.accounts.get(&addr);

        match account {
            Some(arg) => arg.clone(),
            None => {
                let acc = ShardAccount {
                    account: Lazy::new(&OptionalAccount(None)).unwrap(),
                    last_trans_hash: HashBytes::ZERO,
                    last_trans_lt: 0,
                };
                self.accounts.insert(addr.to_string(), acc.clone());
                acc
            }
        }
    }

    pub fn update_account(&mut self, addr: String, account: ShardAccount) {
        self.accounts.insert(addr, account);
    }
}
