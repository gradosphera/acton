use crate::LocalnetError;
use crate::localnet::{LocalnetBlockHeader, LocalnetBlockId, LocalnetMasterchainInfo};
use crate::types::Addr;
use sha2::{Digest, Sha256};
use ton_liteapi::tl::common::{AccountId, BlockIdExt, Int256, ZeroStateIdExt};
use ton_liteapi::tl::response::{BlockHeader, MasterchainInfo, MasterchainInfoExt};

use super::{LITEAPI_CAPABILITIES, LITEAPI_VERSION, proof};

/// `LiteServer` reserves workchain `-1` for the masterchain.
///
/// Localnet stores only one real block stream today, and that stream is the
/// basechain (`workchain=0`). The `LiteAPI` adapter still has to expose a
/// masterchain anchor because tonutils-go and indexers discover basechain shard
/// blocks from masterchain responses.
pub(super) const MASTERCHAIN_WORKCHAIN: i32 = -1;

/// Converts localnet's hash wrapper into the `int256` type used by TL.
///
/// `LiteServer` TL objects carry hashes as fixed-width integer byte arrays. The
/// localnet representation already stores canonical 32-byte hashes, so the
/// conversion is a zero-copy shape change at the value level.
pub(super) const fn int256(bytes: [u8; 32]) -> Int256 {
    Int256(bytes)
}

/// Converts a localnet block id into the `LiteServer` `tonNode.blockIdExt`.
///
/// Localnet currently models a single basechain shard and stores the same
/// workchain/shard pair in every block id. `LiteServer` clients still require the
/// full extended block id because block, transaction, and shard calls all use it
/// as their consistency anchor.
pub(super) fn block_id_ext(id: &LocalnetBlockId) -> BlockIdExt {
    BlockIdExt {
        workchain: id.workchain,
        shard: id.shard,
        seqno: i32::try_from(id.seqno).unwrap_or(i32::MAX),
        root_hash: int256(id.root_hash.0),
        file_hash: int256(id.file_hash.0),
    }
}

/// Presents a localnet block as a synthetic masterchain block id.
///
/// Keep the seqno and hashes identical to the stored localnet block so follow-up
/// queries can resolve the same local history entry, but switch the workchain to
/// `-1` so clients do not classify the returned basechain shard as another
/// masterchain block.
pub(super) fn masterchain_block_id_ext(id: &LocalnetBlockId) -> BlockIdExt {
    BlockIdExt {
        workchain: MASTERCHAIN_WORKCHAIN,
        shard: id.shard,
        seqno: i32::try_from(id.seqno).unwrap_or(i32::MAX),
        root_hash: synthetic_masterchain_hash(b"master-root", id.root_hash.0),
        file_hash: synthetic_masterchain_hash(b"master-file", id.file_hash.0),
    }
}

pub(super) const fn is_masterchain_workchain(workchain: i32) -> bool {
    workchain == MASTERCHAIN_WORKCHAIN
}

/// Converts a `LiteServer` account id into the localnet address type.
///
/// `LiteServer` splits an account into workchain and 256-bit account id, while
/// localnet uses the same pair as one `Addr` value. This helper is used for
/// account state, transaction, and library-adjacent lookups.
pub(super) const fn addr_from_account_id(id: &AccountId) -> Addr {
    Addr {
        workchain: id.workchain,
        addr: id.id.0,
    }
}

/// Reads a non-negative TL sequence number as the local `u32` seqno type.
///
/// TL schema uses signed `int` for block sequence numbers. Localnet block
/// history is indexed by unsigned sequence numbers, so negative values are
/// rejected before they can accidentally wrap.
pub(super) fn seqno_from_i32(seqno: i32) -> anyhow::Result<u32> {
    u32::try_from(seqno).map_err(|_| {
        LocalnetError::protocol_violation(format!("Invalid negative block seqno {seqno}")).into()
    })
}

/// Builds the `LiteServer` masterchain info response from localnet metadata.
///
/// Localnet exposes one synthetic chain as the masterchain anchor. The response
/// keeps the local block id and state hash intact so clients can feed it back
/// into follow-up `LiteServer` calls.
pub(super) fn masterchain_info(info: LocalnetMasterchainInfo) -> MasterchainInfo {
    MasterchainInfo {
        last: masterchain_block_id_ext(&info.last),
        state_root_hash: int256(info.state_root_hash.0),
        init: masterchain_zero_state_id(&info.init),
    }
}

/// Builds the extended masterchain info response with wall-clock metadata.
///
/// Anton and tonutils-go use this variant for node capability discovery. The
/// local implementation reports only the `LiteServer` capabilities backed by the
/// current localnet implementation, rather than mirroring a full validator
/// liteserver.
pub(super) fn masterchain_info_ext(
    info: LocalnetMasterchainInfo,
    header: Option<&LocalnetBlockHeader>,
    now: u32,
) -> MasterchainInfoExt {
    MasterchainInfoExt {
        mode: (),
        version: LITEAPI_VERSION,
        capabilities: LITEAPI_CAPABILITIES,
        last: masterchain_block_id_ext(&info.last),
        last_utime: header.map_or(0, |header| header.gen_utime),
        now,
        state_root_hash: int256(info.state_root_hash.0),
        init: masterchain_zero_state_id(&info.init),
    }
}

/// Builds a `LiteServer` block header shell for lookup/header requests.
///
/// The minimal interface returns an empty `header_proof`; clients running with
/// unsafe proof policy still use the block id and fetch the real block body via
/// `getBlock`.
pub(super) fn block_header(
    header: LocalnetBlockHeader,
    with_state_update: Option<()>,
    with_value_flow: Option<()>,
    with_extra: Option<()>,
    with_shard_hashes: Option<()>,
    with_prev_blk_signatures: Option<()>,
) -> BlockHeader {
    block_header_with_id(
        block_id_ext(&header.id),
        with_state_update,
        with_value_flow,
        with_extra,
        with_shard_hashes,
        with_prev_blk_signatures,
    )
}

/// Builds a block header for the synthetic masterchain view.
///
/// The proof bytes are intentionally the same minimal localnet proof stub as the
/// shard header; only the TL block id changes so clients keep masterchain and
/// shard identities separate.
pub(super) fn masterchain_block_header(
    header: LocalnetBlockHeader,
    with_state_update: Option<()>,
    with_value_flow: Option<()>,
    with_extra: Option<()>,
    with_shard_hashes: Option<()>,
    with_prev_blk_signatures: Option<()>,
) -> BlockHeader {
    block_header_with_id(
        masterchain_block_id_ext(&header.id),
        with_state_update,
        with_value_flow,
        with_extra,
        with_shard_hashes,
        with_prev_blk_signatures,
    )
}

fn block_header_with_id(
    id: BlockIdExt,
    with_state_update: Option<()>,
    with_value_flow: Option<()>,
    with_extra: Option<()>,
    with_shard_hashes: Option<()>,
    with_prev_blk_signatures: Option<()>,
) -> BlockHeader {
    BlockHeader {
        id,
        mode: (),
        with_state_update,
        with_value_flow,
        with_extra,
        with_shard_hashes,
        with_prev_blk_signatures,
        header_proof: proof::empty_cell_boc(),
    }
}

const fn masterchain_zero_state_id(id: &LocalnetBlockId) -> ZeroStateIdExt {
    ZeroStateIdExt {
        workchain: MASTERCHAIN_WORKCHAIN,
        root_hash: Int256(id.root_hash.0),
        file_hash: Int256(id.file_hash.0),
    }
}

fn synthetic_masterchain_hash(domain: &[u8], hash: [u8; 32]) -> Int256 {
    // The synthetic masterchain id must not reuse the real shard root/file
    // hashes. Anton stores blocks by unique file hash, and real liteservers also
    // expose distinct masterchain and shard block ids even when they share a
    // seqno. Domain-separated hashes keep the mapping stable without pretending
    // localnet has built a separate masterchain block BoC.
    let digest = Sha256::new()
        .chain_update(b"acton-localnet-liteapi-synthetic-masterchain")
        .chain_update(domain)
        .chain_update(hash)
        .finalize();
    let mut bytes = [0; 32];
    bytes.copy_from_slice(&digest);
    Int256(bytes)
}
