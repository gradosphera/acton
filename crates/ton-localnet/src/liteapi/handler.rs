use crate::liteapi::convert;
use crate::liteapi::proof;
use crate::localnet::{Localnet, LocalnetBlockId, LocalnetRunGetMethodResult, LocalnetTransaction};
use crate::types::{BocBytes, Hash256};
use crate::{LiteServerErrorCode, LocalnetError};
use anyhow::Context;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Instant, sleep};
use ton_liteapi::liteclient::types::LiteError;
use ton_liteapi::tl::common::{BlockIdExt, LibraryEntry, String as TlString, TransactionId3};
use ton_liteapi::tl::request::{
    GetAccountState, GetAllShardsInfo, GetBlock, GetBlockHeader, GetConfigAll, GetConfigParams,
    GetLibraries, GetLibrariesWithProof, GetMasterchainInfoExt, GetOneTransaction,
    GetShardBlockProof, GetShardInfo, GetTransactions, ListBlockTransactions, LookupBlock, Request,
    RunSmcMethod, SendMessage, WaitMasterchainSeqno, WrappedRequest,
};
use ton_liteapi::tl::response::{
    AccountState, AllShardsInfo, BlockData, BlockTransactions, BlockTransactionsExt, ConfigInfo,
    CurrentTime, Error as TlServerError, LibraryResult, LibraryResultWithProof, Response,
    RunMethodResult, SendMsgStatus, ShardBlockLink, ShardBlockProof, ShardInfo, TransactionId,
    TransactionInfo, TransactionList, Version,
};
use tvm_ffi::stack::Tuple;
use tycho_types::boc::Boc;
use tycho_types::boc::ser::BocHeader;

use super::{LITEAPI_CAPABILITIES, LITEAPI_VERSION};

const SEND_MESSAGE_ACCEPTED_STATUS: u32 = 1;
const RUN_SMC_METHOD_RESULT_MODE: u32 = 1 << 2;
const RUN_SMC_METHOD_SUPPORTED_BITS: u32 = RUN_SMC_METHOD_RESULT_MODE;
const RUN_SMC_METHOD_MAX_PARAMS_BYTES: usize = 65_535;
const LITESERVER_ACCOUNT_NOT_FOUND_EXIT_CODE: i32 = -0x100;
const LOCALNET_NO_CODE_EXIT_CODE: i32 = -13;

/// Handles one decoded `LiteServer` request against the localnet node.
///
/// The transport layer has already unwrapped ADNL and `liteServer.query` by the
/// time this function runs. The handler is responsible for honoring optional
/// `waitMasterchainSeqno`, mapping TL request variants to existing localnet
/// async APIs, and returning TL response variants that tonutils-go can parse.
pub(super) async fn handle(
    node: Arc<Localnet>,
    wrapped: WrappedRequest,
) -> Result<Response, LiteError> {
    if let Some(wait) = wrapped.wait_masterchain_seqno {
        wait_masterchain_seqno(&node, wait)
            .await
            .map_err(lite_error)?;
    }
    handle_request(node, wrapped.request)
        .await
        .map_err(lite_error)
}

async fn handle_request(node: Arc<Localnet>, request: Request) -> anyhow::Result<Response> {
    match request {
        Request::GetMasterchainInfo => {
            let info = node.get_masterchain_info().await?;
            Ok(Response::MasterchainInfo(convert::masterchain_info(info)))
        }
        Request::GetMasterchainInfoExt(request) => get_masterchain_info_ext(&node, request).await,
        Request::GetTime => Ok(Response::CurrentTime(CurrentTime { now: now() })),
        Request::GetVersion => Ok(Response::Version(Version {
            mode: 0,
            version: LITEAPI_VERSION,
            capabilities: LITEAPI_CAPABILITIES,
            now: now(),
        })),
        Request::GetBlock(request) => get_block(&node, request).await,
        Request::GetBlockHeader(request) => get_block_header(&node, request).await,
        Request::SendMessage(request) => send_message(&node, request).await,
        Request::GetAccountState(request) | Request::GetAccountStatePrunned(request) => {
            get_account_state(&node, request).await
        }
        Request::GetShardInfo(request) => get_shard_info(&node, request).await,
        Request::GetAllShardsInfo(request) => get_all_shards_info(&node, request).await,
        Request::GetOneTransaction(request) => get_one_transaction(&node, request).await,
        Request::GetTransactions(request) => get_transactions(&node, request).await,
        Request::LookupBlock(request) => lookup_block(&node, request).await,
        Request::ListBlockTransactions(request) => list_block_transactions(&node, request).await,
        Request::ListBlockTransactionsExt(request) => {
            list_block_transactions_ext(&node, request).await
        }
        Request::RunSmcMethod(request) => run_smc_method(&node, request).await,
        Request::GetConfigAll(request) => get_config(&node, ConfigRequest::from(request)).await,
        Request::GetConfigParams(request) => get_config(&node, ConfigRequest::from(request)).await,
        Request::GetLibraries(request) => get_libraries(&node, request).await,
        Request::GetLibrariesWithProof(request) => get_libraries_with_proof(&node, request).await,
        Request::GetShardBlockProof(request) => get_shard_block_proof(&node, request).await,
        unsupported => Err(LocalnetError::protocol_violation(format!(
            "LiteAPI request is not implemented in localnet: {unsupported:?}"
        ))
        .into()),
    }
}

/// Returns the TL block id in the same chain namespace as the incoming request.
///
/// The local database has one real block per seqno, stored as basechain
/// `workchain=0`. `LiteAPI` clients, however, first address that seqno through a
/// synthetic masterchain block (`workchain=-1`) and then discover the real
/// basechain shard from `getAllShardsInfo`. Keeping this mapping explicit
/// prevents indexers from mistaking the shard block for the masterchain block.
fn block_id_for_request(request_workchain: i32, local_id: &LocalnetBlockId) -> BlockIdExt {
    if convert::is_masterchain_workchain(request_workchain) {
        convert::masterchain_block_id_ext(local_id)
    } else {
        convert::block_id_ext(local_id)
    }
}

async fn get_masterchain_info_ext(
    node: &Localnet,
    _request: GetMasterchainInfoExt,
) -> anyhow::Result<Response> {
    let info = node.get_masterchain_info().await?;
    let header = if info.last.seqno == 0 {
        None
    } else {
        Some(node.get_block_header(info.last.seqno).await?)
    };
    Ok(Response::MasterchainInfoExt(convert::masterchain_info_ext(
        info,
        header.as_ref(),
        now(),
    )))
}

async fn get_block(node: &Localnet, request: GetBlock) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    let data = node.get_block_data(seqno).await?;
    Ok(Response::BlockData(BlockData {
        id: block_id_for_request(request.id.workchain, &header.id),
        data: data.0,
    }))
}

async fn get_block_header(node: &Localnet, request: GetBlockHeader) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    let response = if convert::is_masterchain_workchain(request.id.workchain) {
        convert::masterchain_block_header(
            header,
            request.with_state_update,
            request.with_value_flow,
            request.with_extra,
            request.with_shard_hashes,
            request.with_prev_blk_signatures,
        )
    } else {
        convert::block_header(
            header,
            request.with_state_update,
            request.with_value_flow,
            request.with_extra,
            request.with_shard_hashes,
            request.with_prev_blk_signatures,
        )
    };
    Ok(Response::BlockHeader(response))
}

/// Handles `liteServer.sendMessage` by queueing a raw external-in message.
///
/// TL already carries the body as decoded bytes, so this path skips the
/// toncenter base64 layer and forwards the `BoC` into the same localnet queue
/// used by HTTP `sendBoc`. A successful enqueue returns status `1`, matching the
/// upstream liteserver accepted-message status used by `LiteAPI` clients.
async fn send_message(node: &Localnet, request: SendMessage) -> anyhow::Result<Response> {
    node.send_boc_bytes(BocBytes::from(request.body)).await?;
    Ok(Response::SendMsgStatus(SendMsgStatus {
        status: SEND_MESSAGE_ACCEPTED_STATUS,
    }))
}

async fn get_account_state(node: &Localnet, request: GetAccountState) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    let address = convert::addr_from_account_id(&request.account);
    let shard_account = node
        .get_shard_account_cell(address.to_string(), Some(seqno))
        .await?;
    let cells = proof::account_state_cells(&address, &shard_account, &header)?;
    let id = block_id_for_request(request.id.workchain, &header.id);
    let shardblk = convert::block_id_ext(&header.id);

    Ok(Response::AccountState(AccountState {
        id,
        shardblk,
        // tonutils-go decodes AccountState.shard_proof as `tl:"cell optional 2"`.
        // Empty bytes mean "proof omitted"; a one-cell placeholder fails during
        // TL parsing before `ProofCheckPolicyUnsafe` can skip proof validation.
        shard_proof: Vec::new(),
        proof: cells.proof,
        state: cells.state,
    }))
}

/// Handles `liteServer.getShardInfo` for localnet's single full shard.
///
/// Real liteservers read a shard descriptor from the masterchain state. Localnet
/// already stores one canonical block stream, so the descriptor is synthesized
/// from the requested block header and points at that real block root/file hash.
/// With `exact=false`, any shard inside the same workchain resolves to the full
/// localnet shard; with `exact=true`, the requested shard id must match exactly.
async fn get_shard_info(node: &Localnet, request: GetShardInfo) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    let local_shard = header.id.shard as u64;

    if request.workchain != header.id.workchain || (request.exact && request.shard != local_shard) {
        return Err(LocalnetError::protocol_violation(format!(
            "Shard {}:{} is not available in localnet block {}",
            request.workchain, request.shard, header.id.seqno
        ))
        .into());
    }

    let id = block_id_for_request(request.id.workchain, &header.id);
    let shardblk = convert::block_id_ext(&header.id);
    Ok(Response::ShardInfo(ShardInfo {
        id,
        shardblk,
        shard_proof: proof::empty_cell_boc(),
        shard_descr: proof::shard_description_data(&header)?,
    }))
}

async fn get_all_shards_info(
    node: &Localnet,
    request: GetAllShardsInfo,
) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    Ok(Response::AllShardsInfo(AllShardsInfo {
        id: block_id_for_request(request.id.workchain, &header.id),
        proof: proof::empty_cell_boc(),
        data: proof::all_shards_info_data(&header)?,
    }))
}

async fn get_one_transaction(
    node: &Localnet,
    request: GetOneTransaction,
) -> anyhow::Result<Response> {
    let address = convert::addr_from_account_id(&request.account);
    let transactions = node
        .get_transactions(address.to_string(), 1, Some(request.lt), None, None)
        .await?;
    let transaction = transactions
        .into_iter()
        .find(|tx| tx.transaction_id.lt == request.lt)
        .map(|tx| tx.data.0)
        .unwrap_or_default();

    Ok(Response::TransactionInfo(TransactionInfo {
        id: request.id,
        proof: proof::empty_cell_boc(),
        transaction,
    }))
}

/// Handles `liteServer.getTransactions` using localnet's account transaction index.
///
/// The `LiteAPI` response pairs each transaction with the block id that contained
/// it and stores all transaction cells in a single multi-root `BoC`, matching the
/// upstream liteserver shape produced by `std_boc_serialize_multi`.
async fn get_transactions(node: &Localnet, request: GetTransactions) -> anyhow::Result<Response> {
    let address = convert::addr_from_account_id(&request.account);
    let requested = usize::try_from(request.count).unwrap_or(usize::MAX);
    let transactions = node
        .get_transactions_by_address(
            address,
            requested,
            Some(request.lt),
            Some(Hash256(request.hash.0)),
            None,
        )
        .await?;
    let ids = transaction_block_ids(node, &transactions).await?;
    let transactions = transaction_roots_boc(&transactions)?;

    Ok(Response::TransactionList(TransactionList {
        ids,
        transactions,
    }))
}

/// Executes `liteServer.runSmcMethod` through localnet's existing get-method engine.
///
/// The request format is already binary and carries both the numeric method id
/// and a serialized TVM stack, so this adapter only validates the `LiteServer`
/// mode bits, decodes the stack `BoC`, and forwards the typed values to the
/// localnet actor. Proof, c7, and library-extra response modes are rejected for
/// now because localnet does not yet build the corresponding verified payloads.
async fn run_smc_method(node: &Localnet, request: RunSmcMethod) -> anyhow::Result<Response> {
    if request.mode & !RUN_SMC_METHOD_SUPPORTED_BITS != 0 {
        return Err(LocalnetError::protocol_violation(format!(
            "Unsupported liteServer.runSmcMethod mode {}: localnet currently supports only result bit {}",
            request.mode,
            RUN_SMC_METHOD_RESULT_MODE
        ))
        .into());
    }

    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let method_id = i32::try_from(request.method_id).map_err(|_| {
        LocalnetError::protocol_violation(format!(
            "runSmcMethod method_id {} exceeds i32 range",
            request.method_id
        ))
    })?;
    let stack = run_smc_method_params(request.params)?;
    let result = node
        .run_get_method_by_id(
            convert::addr_from_account_id(&request.account),
            method_id,
            stack,
            Some(seqno),
        )
        .await?;

    let id = block_id_for_request(request.id.workchain, &result.block_id);
    let shardblk = convert::block_id_ext(&result.block_id);
    let include_result = request.mode & RUN_SMC_METHOD_RESULT_MODE != 0;
    let account_not_found = run_smc_method_account_not_found(&result);
    let exit_code = if account_not_found {
        LITESERVER_ACCOUNT_NOT_FOUND_EXIT_CODE
    } else {
        result.exit_code
    };
    let result_stack = include_result.then(|| {
        if account_not_found {
            Vec::new()
        } else {
            result.stack.0
        }
    });

    Ok(Response::RunMethodResult(RunMethodResult {
        mode: (),
        id,
        shardblk,
        shard_proof: None,
        proof: None,
        state_proof: None,
        init_c7: None,
        lib_extras: None,
        exit_code,
        result: result_stack,
    }))
}

async fn lookup_block(node: &Localnet, request: LookupBlock) -> anyhow::Result<Response> {
    let requested_workchain = request.id.workchain;
    let block_id = node
        .lookup_block(
            requested_workchain,
            request.id.shard.to_string(),
            request
                .seqno
                .map(|()| convert::seqno_from_i32(request.id.seqno))
                .transpose()?,
            request.lt,
            request.utime,
        )
        .await?;
    let header = node.get_block_header(block_id.seqno).await?;
    let response = if convert::is_masterchain_workchain(requested_workchain) {
        convert::masterchain_block_header(
            header,
            request.with_state_update,
            request.with_value_flow,
            request.with_extra,
            request.with_shard_hashes,
            request.with_prev_blk_signatures,
        )
    } else {
        convert::block_header(
            header,
            request.with_state_update,
            request.with_value_flow,
            request.with_extra,
            request.with_shard_hashes,
            request.with_prev_blk_signatures,
        )
    };
    Ok(Response::BlockHeader(response))
}

/// Decodes the `params` field from `liteServer.runSmcMethod` into a TVM stack.
///
/// `LiteServer` clients send method arguments as a `BoC` containing a serialized
/// `VmStack`. Empty params are accepted as an empty stack, matching upstream
/// liteserver behavior for get-methods without arguments.
fn run_smc_method_params(params: Vec<u8>) -> anyhow::Result<Tuple> {
    if params.len() > RUN_SMC_METHOD_MAX_PARAMS_BYTES {
        return Err(LocalnetError::protocol_violation(format!(
            "runSmcMethod params are too large: {} bytes, maximum is {}",
            params.len(),
            RUN_SMC_METHOD_MAX_PARAMS_BYTES
        ))
        .into());
    }

    if params.is_empty() {
        return Ok(Tuple::empty());
    }

    let cell = Boc::decode(&params).map_err(|error| {
        LocalnetError::protocol_violation(format!(
            "Failed to decode runSmcMethod params BoC: {error}"
        ))
    })?;
    Tuple::deserialize(&cell).map_err(|error| {
        LocalnetError::protocol_violation(format!(
            "Failed to deserialize runSmcMethod params as TVM stack: {error}"
        ))
        .into()
    })
}

/// Maps localnet's no-code sentinel to the exit code used by upstream `LiteServer`.
///
/// The HTTP toncenter-compatible API historically reports local no-code
/// get-method calls as `-13`. `LiteServer` returns `-256` for an absent,
/// uninitialized, frozen, or otherwise non-runnable account, and Go `LiteAPI`
/// clients rely on that value for account-not-found handling.
fn run_smc_method_account_not_found(result: &LocalnetRunGetMethodResult) -> bool {
    result.exit_code == LOCALNET_NO_CODE_EXIT_CODE
        && result.gas_used == 0
        && result.vm_log.is_empty()
}

async fn list_block_transactions(
    node: &Localnet,
    request: ListBlockTransactions,
) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    // The synthetic masterchain is only an anchor for shard discovery; localnet
    // stores executable transactions on the real basechain shard block.
    if convert::is_masterchain_workchain(request.id.workchain) {
        let header = node.get_block_header(seqno).await?;
        return Ok(Response::BlockTransactions(BlockTransactions {
            id: convert::masterchain_block_id_ext(&header.id),
            req_count: request.count,
            incomplete: false,
            ids: Vec::new(),
            proof: proof::empty_cell_boc(),
        }));
    }

    let block = node.get_block_transactions(seqno).await?;
    let id = convert::block_id_ext(&block.id);
    let (transactions, incomplete) =
        limit_block_transactions(block.transactions, request.after, request.count);
    let ids = transactions
        .into_iter()
        .map(transaction_id)
        .collect::<Vec<_>>();

    Ok(Response::BlockTransactions(BlockTransactions {
        id,
        req_count: request.count,
        incomplete,
        ids,
        proof: proof::empty_cell_boc(),
    }))
}

/// Handles `liteServer.listBlockTransactionsExt` for localnet blocks.
///
/// This mirrors `liteServer.listBlockTransactions` pagination but returns the
/// actual transaction cells as a multi-root `BoC`. Proof bytes stay minimal for
/// now, consistent with the other localnet `LiteAPI` methods that expose data for
/// indexers without building a full cryptographic proof chain.
async fn list_block_transactions_ext(
    node: &Localnet,
    request: ListBlockTransactions,
) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    // See `list_block_transactions`: masterchain transaction lists are empty by
    // construction, while the same seqno's basechain shard carries real txs.
    if convert::is_masterchain_workchain(request.id.workchain) {
        let header = node.get_block_header(seqno).await?;
        return Ok(Response::BlockTransactionsExt(BlockTransactionsExt {
            id: convert::masterchain_block_id_ext(&header.id),
            req_count: request.count,
            incomplete: false,
            transactions: transaction_roots_boc(&[])?,
            proof: proof::empty_cell_boc(),
        }));
    }

    let block = node.get_block_transactions(seqno).await?;
    let id = convert::block_id_ext(&block.id);
    let (transactions, incomplete) =
        limit_block_transactions(block.transactions, request.after, request.count);
    let transactions = transaction_roots_boc(&transactions)?;

    Ok(Response::BlockTransactionsExt(BlockTransactionsExt {
        id,
        req_count: request.count,
        incomplete,
        transactions,
        proof: proof::empty_cell_boc(),
    }))
}

/// Applies `LiteAPI` block-transaction pagination to an in-memory localnet block.
///
/// The `after` cursor removes transactions through the cursor transaction, then
/// `count` caps the returned slice. The boolean mirrors liteserver's
/// `incomplete` flag and tells callers whether more transactions remain after
/// the returned page.
fn limit_block_transactions(
    mut transactions: Vec<LocalnetTransaction>,
    after: Option<TransactionId3>,
    count: u32,
) -> (Vec<LocalnetTransaction>, bool) {
    if let Some(after) = after
        && let Some(index) = transactions
            .iter()
            .position(|tx| tx.address.addr == after.account.0 && tx.transaction_id.lt == after.lt)
    {
        transactions.drain(..=index);
    }

    let requested = usize::try_from(count).unwrap_or(usize::MAX);
    let incomplete = transactions.len() > requested;
    transactions.truncate(requested);
    (transactions, incomplete)
}

/// Resolves the containing block id for each transaction in a `LiteAPI` response.
///
/// `liteServer.transactionList` does not embed transaction ids; instead, it
/// carries a vector of block ids aligned with the multi-root transaction `BoC`.
/// Localnet stores each transaction's masterchain block seqno, so this helper
/// loads the corresponding real block headers and caches repeated seqnos within
/// one response.
async fn transaction_block_ids(
    node: &Localnet,
    transactions: &[LocalnetTransaction],
) -> anyhow::Result<Vec<BlockIdExt>> {
    let mut by_seqno: BTreeMap<u32, BlockIdExt> = BTreeMap::new();
    let mut ids = Vec::with_capacity(transactions.len());

    for transaction in transactions {
        let id = if let Some(id) = by_seqno.get(&transaction.mc_block_seqno) {
            id.clone()
        } else {
            let header = node.get_block_header(transaction.mc_block_seqno).await?;
            let id = convert::block_id_ext(&header.id);
            by_seqno.insert(transaction.mc_block_seqno, id.clone());
            id
        };
        ids.push(id);
    }

    Ok(ids)
}

/// Serializes transaction cells as one multi-root `BoC`.
///
/// Upstream liteserver uses the same shape for `liteServer.transactionList` and
/// `liteServer.blockTransactionsExt`: every returned transaction is a separate
/// root cell in one `BoC`, preserving the original transaction serialization.
fn transaction_roots_boc(transactions: &[LocalnetTransaction]) -> anyhow::Result<Vec<u8>> {
    let cells = transactions
        .iter()
        .map(|tx| {
            Boc::decode(&tx.data)
                .with_context(|| format!("Failed to decode transaction {} BoC", tx.hash.to_hex()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut header: BocHeader<'_> = BocHeader::with_capacity(cells.len());

    for cell in &cells {
        header.add_root(cell.as_ref());
    }

    let mut result = Vec::new();
    header.encode(&mut result);
    Ok(result)
}

async fn get_config(node: &Localnet, request: ConfigRequest) -> anyhow::Result<Response> {
    let seqno = convert::seqno_from_i32(request.id.seqno)?;
    let header = node.get_block_header(seqno).await?;
    let config_boc = node.get_config_all(Some(seqno)).await?;

    Ok(Response::ConfigInfo(ConfigInfo {
        mode: (),
        id: block_id_for_request(request.id.workchain, &header.id),
        state_proof: proof::empty_cell_boc(),
        config_proof: proof::config_proof(&config_boc, &header)?,
        with_state_root: request.with_state_root,
        with_libraries: request.with_libraries,
        with_state_extra_root: request.with_state_extra_root,
        with_shard_hashes: request.with_shard_hashes,
        with_validator_set: request.with_validator_set,
        with_special_smc: request.with_special_smc,
        with_accounts_root: request.with_accounts_root,
        with_prev_blocks: request.with_prev_blocks,
        with_workchain_info: request.with_workchain_info,
        with_capabilities: request.with_capabilities,
        extract_from_key_block: request.extract_from_key_block,
    }))
}

struct ConfigRequest {
    id: BlockIdExt,
    with_state_root: Option<()>,
    with_libraries: Option<()>,
    with_state_extra_root: Option<()>,
    with_shard_hashes: Option<()>,
    with_validator_set: Option<()>,
    with_special_smc: Option<()>,
    with_accounts_root: Option<()>,
    with_prev_blocks: Option<()>,
    with_workchain_info: Option<()>,
    with_capabilities: Option<()>,
    extract_from_key_block: Option<()>,
}

impl From<GetConfigAll> for ConfigRequest {
    fn from(value: GetConfigAll) -> Self {
        Self {
            id: value.id,
            with_state_root: value.with_state_root,
            with_libraries: value.with_libraries,
            with_state_extra_root: value.with_state_extra_root,
            with_shard_hashes: value.with_shard_hashes,
            with_validator_set: value.with_validator_set,
            with_special_smc: value.with_special_smc,
            with_accounts_root: value.with_accounts_root,
            with_prev_blocks: value.with_prev_blocks,
            with_workchain_info: value.with_workchain_info,
            with_capabilities: value.with_capabilities,
            extract_from_key_block: value.extract_from_key_block,
        }
    }
}

impl From<GetConfigParams> for ConfigRequest {
    fn from(value: GetConfigParams) -> Self {
        Self {
            id: value.id,
            with_state_root: value.with_state_root,
            with_libraries: value.with_libraries,
            with_state_extra_root: value.with_state_extra_root,
            with_shard_hashes: value.with_shard_hashes,
            with_validator_set: value.with_validator_set,
            with_special_smc: value.with_special_smc,
            with_accounts_root: value.with_accounts_root,
            with_prev_blocks: value.with_prev_blocks,
            with_workchain_info: value.with_workchain_info,
            with_capabilities: value.with_capabilities,
            extract_from_key_block: value.extract_from_key_block,
        }
    }
}

async fn get_libraries(node: &Localnet, request: GetLibraries) -> anyhow::Result<Response> {
    let hashes = request
        .library_list
        .into_iter()
        .map(|hash| Hash256(hash.0))
        .collect::<Vec<_>>();
    let libraries = node.get_libraries(hashes).await?;
    let result = libraries
        .into_iter()
        .filter_map(|library| {
            library.found.then(|| LibraryEntry {
                hash: convert::int256(library.hash.0),
                data: library.data.map_or_else(Vec::new, |data| data.0),
            })
        })
        .collect();

    Ok(Response::LibraryResult(LibraryResult { result }))
}

async fn get_libraries_with_proof(
    node: &Localnet,
    request: GetLibrariesWithProof,
) -> anyhow::Result<Response> {
    let hashes = request
        .library_list
        .into_iter()
        .map(|hash| Hash256(hash.0))
        .collect::<Vec<_>>();
    let libraries = node.get_libraries(hashes).await?;
    let result = libraries
        .into_iter()
        .filter_map(|library| {
            library.found.then(|| LibraryEntry {
                hash: convert::int256(library.hash.0),
                data: library.data.map_or_else(Vec::new, |data| data.0),
            })
        })
        .collect();

    Ok(Response::LibraryResultWithProof(LibraryResultWithProof {
        id: request.id,
        mode: (),
        result,
        state_proof: proof::empty_cell_boc(),
        data_proof: proof::empty_cell_boc(),
    }))
}

async fn get_shard_block_proof(
    node: &Localnet,
    request: GetShardBlockProof,
) -> anyhow::Result<Response> {
    let masterchain_id = convert::block_id_ext(&node.get_masterchain_info().await?.last);
    Ok(Response::ShardBlockProof(ShardBlockProof {
        masterchain_id,
        links: vec![ShardBlockLink {
            id: request.id,
            proof: proof::empty_cell_boc(),
        }],
    }))
}

async fn wait_masterchain_seqno(node: &Localnet, wait: WaitMasterchainSeqno) -> anyhow::Result<()> {
    let timeout = Duration::from_millis(u64::from(wait.timeout_ms));
    let deadline = Instant::now() + timeout;

    loop {
        let info = node.get_masterchain_info().await?;
        if info.last.seqno >= wait.seqno {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(LocalnetError::MasterchainWaitTimeout { seqno: wait.seqno }.into());
        }
        sleep(Duration::from_millis(50)).await;
    }
}

fn transaction_id(transaction: LocalnetTransaction) -> TransactionId {
    TransactionId {
        mode: (),
        account: Some(convert::int256(transaction.address.addr)),
        lt: Some(transaction.transaction_id.lt),
        hash: Some(convert::int256(transaction.hash.0)),
        metadata: None,
    }
}

fn now() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as u32)
}

fn lite_error(error: anyhow::Error) -> LiteError {
    let code = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<LocalnetError>())
        .map_or(LiteServerErrorCode::Error, LocalnetError::lite_server_code);
    let message = error.to_string();

    LiteError::ServerError(TlServerError {
        code: i32::from(code),
        message: TlString::new(message),
    })
}
