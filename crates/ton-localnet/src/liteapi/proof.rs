use crate::localnet::LocalnetBlockHeader;
use crate::types::{Addr, BocBytes};
use anyhow::Context;
use std::collections::BTreeMap;
use tycho_types::boc::Boc;
use tycho_types::cell::{Cell, CellBuilder, Lazy};
use tycho_types::dict::{AugDict, Dict};
use tycho_types::models::ShardAccount;
use tycho_types::models::block::{ShardDescription, ShardHashes, ShardIdent};
use tycho_types::models::config::{BlockchainConfig, BlockchainConfigParams};
use tycho_types::models::currency::CurrencyCollection;
use tycho_types::models::shard::{
    DepthBalanceInfo, McStateExtra, ShardAccounts, ShardStateUnsplit, ValidatorInfo,
};
use tycho_types::prelude::HashBytes;

const LOCALNET_GLOBAL_ID: i32 = 0;

/// Account-state payload expected by `liteServer.accountState`.
///
/// `proof` is a two-root `BoC` shaped for tonutils-go's unsafe proof path: the
/// second root references a full `ShardStateUnsplit`. `state` is the exact
/// `OptionalAccount` cell from localnet's stored `ShardAccount`.
pub(super) struct AccountStateCells {
    pub proof: Vec<u8>,
    pub state: Vec<u8>,
}

/// Builds the `liteServer.allShardsInfo.data` `BoC` for the localnet shard.
///
/// Anton asks the masterchain block for shard descriptions and then fetches
/// block bodies for every returned shard. Localnet has a single full basechain
/// shard, so the dictionary contains one `ShardDescription` that points back to
/// the block id already produced by localnet's block builder.
pub(super) fn all_shards_info_data(header: &LocalnetBlockHeader) -> anyhow::Result<Vec<u8>> {
    let shards = shard_hashes(header)?;
    let cell = CellBuilder::build_from(&shards).context("Failed to serialize shard hashes")?;
    Ok(Boc::encode(cell))
}

/// Builds account proof/state cells for `liteServer.accountState`.
///
/// The proof is intentionally minimal and is only meant for local indexers that
/// disable cryptographic proof checking. It still contains a real
/// `ShardAccounts` dictionary entry for the requested account, so tonutils-go
/// can verify the returned account cell hash and balance against the shard
/// state it parses from the proof.
pub(super) fn account_state_cells(
    address: &Addr,
    shard_account_boc: &BocBytes,
    header: &LocalnetBlockHeader,
) -> anyhow::Result<AccountStateCells> {
    let shard_account_cell =
        Boc::decode(shard_account_boc).context("Failed to decode ShardAccount BOC")?;
    let shard_account = shard_account_cell
        .parse::<ShardAccount>()
        .context("Failed to parse ShardAccount")?;
    let optional_account = shard_account
        .account
        .load()
        .context("Failed to load OptionalAccount")?;

    let (accounts, total_balance, state) = if let Some(account) = optional_account.0 {
        let state = Boc::encode(shard_account.account.inner().clone());
        let balance = account.balance;
        let mut entries = BTreeMap::new();
        entries.insert(
            HashBytes(address.addr),
            (
                DepthBalanceInfo {
                    split_depth: 0,
                    balance: balance.clone(),
                },
                shard_account,
            ),
        );
        let accounts =
            ShardAccounts::try_from_btree(&entries).context("Failed to build shard accounts")?;
        (accounts, balance, state)
    } else {
        (
            ShardAccounts::default(),
            CurrencyCollection::ZERO,
            Vec::new(),
        )
    };

    let state_cell = shard_state_cell(
        ShardIdent::new_full(address.workchain),
        header.id.seqno,
        header.gen_utime,
        header.end_lt,
        accounts,
        total_balance,
        None,
    )?;
    let proof = two_root_proof_with_state(state_cell)?;

    Ok(AccountStateCells { proof, state })
}

/// Builds `liteServer.configInfo.config_proof` for Anton's config loader.
///
/// Anton follows the common tonutils-go path of reading the first reference from
/// `config_proof`, parsing it as `ShardStateUnsplit`, then loading
/// `McStateExtra.config`. The returned `BoC` is a wrapper cell with exactly that
/// reference.
pub(super) fn config_proof(
    config_boc: &BocBytes,
    header: &LocalnetBlockHeader,
) -> anyhow::Result<Vec<u8>> {
    let config_root = Boc::decode(config_boc).context("Failed to decode config dictionary BOC")?;
    let shards = shard_hashes(header)?;
    let config = BlockchainConfig {
        address: HashBytes::ZERO,
        params: BlockchainConfigParams::from_raw(config_root),
    };
    let custom = McStateExtra {
        shards,
        config,
        validator_info: ValidatorInfo {
            validator_list_hash_short: 0,
            catchain_seqno: 0,
            nx_cc_updated: false,
        },
        prev_blocks: AugDict::new(),
        after_key_block: false,
        last_key_block: None,
        block_create_stats: None,
        global_balance: CurrencyCollection::ZERO,
    };
    let state_cell = shard_state_cell(
        ShardIdent::MASTERCHAIN,
        header.id.seqno,
        header.gen_utime,
        header.end_lt,
        ShardAccounts::default(),
        CurrencyCollection::ZERO,
        Some(custom),
    )?;
    wrapper_with_ref(state_cell).map(Boc::encode)
}

/// Builds the `liteServer.shardInfo.shard_descr` `BoC` for localnet's shard.
///
/// The descriptor mirrors the shard entry exposed through
/// `liteServer.allShardsInfo`: it points to the real block root/file hashes that
/// localnet already generated, while merge/split and validator accounting fields
/// stay empty because the localnet model has a single full shard.
pub(super) fn shard_description_data(header: &LocalnetBlockHeader) -> anyhow::Result<Vec<u8>> {
    let shard_description = localnet_shard_description(header);
    let cell = CellBuilder::build_from(&shard_description)
        .context("Failed to serialize shard description")?;
    Ok(Boc::encode(cell))
}

/// Minimal valid proof placeholder for local `LiteAPI` responses.
///
/// Several tonutils-go response structs decode proof `bytes` fields as `BoC`
/// cells during TL parsing, even when proof checking is disabled. Returning an
/// empty byte vector therefore breaks clients before they can ignore the proof.
pub(super) fn empty_cell_boc() -> Vec<u8> {
    Boc::encode(Cell::default())
}

fn shard_hashes(header: &LocalnetBlockHeader) -> anyhow::Result<ShardHashes> {
    let shard_ident = ShardIdent::new_full(header.id.workchain);
    let shard_description = localnet_shard_description(header);
    ShardHashes::from_shards([(&shard_ident, &shard_description)])
        .context("Failed to build shard hashes")
}

const fn localnet_shard_description(header: &LocalnetBlockHeader) -> ShardDescription {
    ShardDescription {
        seqno: header.id.seqno,
        reg_mc_seqno: header.id.seqno,
        start_lt: header.start_lt,
        end_lt: header.end_lt,
        root_hash: HashBytes(header.id.root_hash.0),
        file_hash: HashBytes(header.id.file_hash.0),
        before_split: false,
        before_merge: false,
        want_split: false,
        want_merge: false,
        nx_cc_updated: false,
        next_catchain_seqno: 0,
        next_validator_shard: header.id.shard as u64,
        min_ref_mc_seqno: 0,
        gen_utime: header.gen_utime,
        split_merge_at: None,
        fees_collected: CurrencyCollection::ZERO,
        funds_created: CurrencyCollection::ZERO,
    }
}

fn shard_state_cell(
    shard_ident: ShardIdent,
    seqno: u32,
    gen_utime: u32,
    gen_lt: u64,
    accounts: ShardAccounts,
    total_balance: CurrencyCollection,
    custom: Option<McStateExtra>,
) -> anyhow::Result<Cell> {
    let custom = custom
        .as_ref()
        .map(Lazy::new)
        .transpose()
        .context("Failed to wrap masterchain state extra")?;
    let state = ShardStateUnsplit {
        global_id: LOCALNET_GLOBAL_ID,
        shard_ident,
        seqno,
        vert_seqno: 0,
        gen_utime,
        gen_lt,
        min_ref_mc_seqno: 0,
        out_msg_queue_info: Cell::default(),
        before_split: false,
        accounts: Lazy::new(&accounts).context("Failed to wrap shard accounts")?,
        overload_history: 0,
        underload_history: 0,
        total_balance,
        total_validator_fees: CurrencyCollection::ZERO,
        libraries: Dict::new(),
        master_ref: None,
        custom,
    };
    CellBuilder::build_from(&state).context("Failed to serialize shard state")
}

fn two_root_proof_with_state(state_cell: Cell) -> anyhow::Result<Vec<u8>> {
    let wrapper = wrapper_with_ref(state_cell)?;
    Ok(Boc::encode_pair((Cell::default(), wrapper)))
}

fn wrapper_with_ref(cell: Cell) -> anyhow::Result<Cell> {
    let mut builder = CellBuilder::new();
    builder
        .store_reference(cell)
        .context("Failed to store state reference")?;
    builder.build().context("Failed to build proof wrapper")
}
