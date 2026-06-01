pub mod handlers;
pub mod models;
pub mod router;

use crate::localnet::Localnet;
use acton_config::color::OwoColorize;
use axum::extract::FromRef;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize)]
pub struct StartupWallet {
    pub name: String,
    pub mnemonic: Vec<String>,
    pub version: String,
    pub network: String,
    pub address: String,
    pub public_key: String,
    pub wallet_id: i32,
}

#[derive(Clone)]
pub struct ServerState {
    pub node: Arc<Localnet>,
    pub startup_wallets: Arc<Vec<StartupWallet>>,
    pub state_source: Arc<StateSourceInfo>,
    pub shutdown: ShutdownSignal,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateSourceInfo {
    pub state_source: &'static str,
    pub fork_network: Option<String>,
    pub fork_block_number: Option<u64>,
}

#[derive(Clone)]
pub struct ShutdownSignal {
    tx: broadcast::Sender<()>,
}

impl ShutdownSignal {
    fn new() -> Self {
        // Streaming handlers are long-lived; graceful shutdown waits until they exit.
        let (tx, _) = broadcast::channel(1);
        Self { tx }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }

    fn notify(&self) {
        let _ = self.tx.send(());
    }
}

impl FromRef<ServerState> for Arc<Localnet> {
    fn from_ref(state: &ServerState) -> Self {
        state.node.clone()
    }
}

impl FromRef<ServerState> for Arc<Vec<StartupWallet>> {
    fn from_ref(state: &ServerState) -> Self {
        state.startup_wallets.clone()
    }
}

impl FromRef<ServerState> for Arc<StateSourceInfo> {
    fn from_ref(state: &ServerState) -> Self {
        state.state_source.clone()
    }
}

impl FromRef<ServerState> for ShutdownSignal {
    fn from_ref(state: &ServerState) -> Self {
        state.shutdown.clone()
    }
}

pub struct ServerArgs {
    pub port: u16,
    pub db_path: Option<String>,
    pub fork_network: Option<String>,
    pub fork_block_number: Option<u64>,
    pub rate_limit_rps: Option<u32>,
    pub response_delay_ms: Option<u64>,
    pub startup_wallets: Vec<StartupWallet>,
}

pub async fn run_server(node: Arc<Localnet>, args: ServerArgs) -> anyhow::Result<()> {
    let ServerArgs {
        port,
        db_path: _,
        fork_network,
        fork_block_number,
        rate_limit_rps,
        response_delay_ms,
        startup_wallets,
    } = args;

    seed_startup_wallet_names(&node, &startup_wallets).await?;

    let state_source = StateSourceInfo {
        state_source: if fork_network.is_some() {
            "remote"
        } else {
            "local"
        },
        fork_network: fork_network.clone(),
        fork_block_number,
    };
    let shutdown = ShutdownSignal::new();
    let app = router::create_router(
        ServerState {
            node,
            startup_wallets: Arc::new(startup_wallets),
            state_source: Arc::new(state_source),
            shutdown: shutdown.clone(),
        },
        rate_limit_rps,
        response_delay_ms,
    );

    let address = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!(
        "    {} Localnet server and UI on http://{address}",
        "Starting".green().bold(),
    );
    if let Some(fork_network) = fork_network {
        let fork_source = fork_block_number
            .map(|seqno| format!("{fork_network} at seqno {seqno}"))
            .unwrap_or(fork_network);
        println!("     {} from {}", "Forking".green().bold(), fork_source);
    }
    if let Some(limit) = rate_limit_rps {
        println!(
            "    {} API requests to {} req/s",
            "Limiting".yellow().bold(),
            limit
        );
    }
    if let Some(delay_ms) = response_delay_ms.filter(|delay_ms| *delay_ms > 0) {
        println!(
            "    {} API v2/v3/emulate responses by {}ms",
            "Delaying".yellow().bold(),
            delay_ms
        );
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                println!("  {} Localnet server", "Stopping".yellow().bold());
                shutdown.notify();
            }
        })
        .await?;
    Ok(())
}

async fn seed_startup_wallet_names(
    node: &Localnet,
    startup_wallets: &[StartupWallet],
) -> anyhow::Result<()> {
    let mut seen_addresses = HashSet::new();
    let mut named_wallets = Vec::new();

    for wallet in startup_wallets {
        let address = wallet.address.trim();
        let name = wallet.name.trim();
        if address.is_empty() || name.is_empty() || !seen_addresses.insert(address.to_string()) {
            continue;
        }
        named_wallets.push((address.to_string(), name.to_string()));
    }

    if named_wallets.is_empty() {
        return Ok(());
    }

    let existing_names = node
        .get_address_names(
            named_wallets
                .iter()
                .map(|(address, _)| address.clone())
                .collect(),
        )
        .await?;

    for ((address, name), (_, existing_name)) in named_wallets.into_iter().zip(existing_names) {
        if existing_name.is_none() {
            node.set_address_name(address, name).await?;
        }
    }

    Ok(())
}
