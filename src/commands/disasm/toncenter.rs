use anyhow::{Context, anyhow};
use serde::Deserialize;

#[derive(Deserialize)]
struct TonCenterResponse {
    accounts: Vec<AccountState>,
}

#[derive(Deserialize)]
struct AccountState {
    code_boc: Option<String>,
    status: String,
}

pub fn fetch_contract_boc(address: &str, api_key: Option<&str>) -> anyhow::Result<String> {
    let mainnet_url = format!(
        "https://toncenter.com/api/v3/accountStates?address={}",
        urlencoding::encode(address)
    );

    match fetch_from_toncenter(&mainnet_url, api_key) {
        Ok(boc) => Ok(boc),
        Err(_) => {
            let testnet_url = format!(
                "https://testnet.toncenter.com/api/v3/accountStates?address={}",
                urlencoding::encode(address)
            );

            fetch_from_toncenter(&testnet_url, api_key)
                .context("Contract not found on both mainnet and testnet")
        }
    }
}

fn fetch_from_toncenter(url: &str, api_key: Option<&str>) -> anyhow::Result<String> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.get(url).header("User-Agent", "acton-cli");

    if let Some(key) = api_key {
        request = request.header("X-API-Key", key);
    }

    let response = request
        .send()
        .context("Failed to send request to TonCenter")?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "TonCenter API returned status: {}",
            response.status()
        ));
    }

    let data: TonCenterResponse = response
        .json()
        .context("Failed to parse TonCenter response")?;

    if data.accounts.is_empty() {
        return Err(anyhow!("Contract not found"));
    }

    let account = &data.accounts[0];
    if account.status != "active" {
        return Err(anyhow!(
            "Contract is not active (status: {})",
            account.status
        ));
    }

    account
        .code_boc
        .clone()
        .ok_or_else(|| anyhow!("Contract has no code"))
}
