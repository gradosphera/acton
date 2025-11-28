use anyhow::Context;
use ton_api::{Network, TonApiClient};

pub fn fetch_contract_boc(address: &str, api_key: Option<&str>) -> anyhow::Result<String> {
    let mainnet_client = TonApiClient::new(Network::Mainnet, api_key.map(|s| s.to_string()));
    match mainnet_client.get_contract_boc(address) {
        Ok(boc) => Ok(boc),
        Err(_) => {
            let testnet_client =
                TonApiClient::new(Network::Testnet, api_key.map(|s| s.to_string()));
            testnet_client
                .get_contract_boc(address)
                .context("Contract not found on both mainnet and testnet")
        }
    }
}
