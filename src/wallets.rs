use crate::config::ActonConfig;
use crate::context::Wallet;
use std::collections::BTreeMap;
use std::fs;
use tonlib_core::wallet::ton_wallet::TonWallet;

pub fn open_wallets(
    config: &ActonConfig,
    broadcast: bool,
) -> anyhow::Result<BTreeMap<String, Wallet>> {
    if !broadcast {
        return Ok(BTreeMap::new());
    }

    let wallets = config
        .wallets
        .as_ref()
        .map(|w| w.wallets.clone())
        .unwrap_or_default();

    let mut open_wallets: BTreeMap<String, Wallet> = BTreeMap::new();

    for (name, wallet) in wallets {
        let mnemonic = if let Some(env) = wallet.keys.mnemonic_env {
            std::env::var(env).ok()
        } else if let Some(file) = wallet.keys.mnemonic_file {
            Some(fs::read_to_string(file)?.trim().to_string())
        } else {
            None
        };

        let Some(mnemonic) = mnemonic else {
            anyhow::bail!("No mnemonic found for '{name}' wallet")
        };

        let mnemonic = tonlib_core::wallet::mnemonic::Mnemonic::from_str(&mnemonic, &None)?;

        let wallet = TonWallet::new_with_params(
            tonlib_core::wallet::wallet_version::WalletVersion::V5R1,
            mnemonic.to_key_pair()?,
            wallet.workchain.unwrap_or(0),
            0x7FFFFFFD,
        )?;

        open_wallets.insert(
            name.clone(),
            Wallet {
                wallet,
                name,
                seqno: None,
            },
        );
    }

    Ok(open_wallets)
}
