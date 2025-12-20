use crate::context::Context;
use adnl::AdnlPeer;
use base64::prelude::*;
use emulator::traits::BaseExecutor;
use emulator::{extension, register_ext_methods};
use futures::{SinkExt, StreamExt};
use std::net::Ipv4Addr;
use tl_proto::{TlRead, TlWrite};
use tokio::time::Instant;
use ton_liteapi::tl::adnl::Message;
use ton_liteapi::tl::common::Int256;
use ton_liteapi::tl::response::Response;
use tonlib_core::cell::{ArcCell, CellBuilder};
use tonlib_core::tlb_types::tlb::TLB;
use tvmffi::stack::{Tuple, TupleItem};

#[derive(TlRead, TlWrite, Debug, Clone, PartialEq)]
#[tl(
    boxed,
    id = "liteServer.query",
    scheme_inline = r##"liteServer.query data:bytes = Object;"##
)]
pub struct MyLiteQuery {
    pub data: Vec<u8>,
}

#[derive(tl_proto::TlWrite)]
#[tl(boxed, id = 0xb48bf97a)] // adnl.message.query
struct MyAdnlQuery {
    query_id: Int256,
    query: Vec<u8>,
}

#[derive(TlRead)]
#[tl(boxed)]
enum MyAdnlResponse {
    #[tl(id = 0x0fac8416)]
    Answer { _query_id: Int256, answer: Vec<u8> },
}

extension!(liteserver_query in (Context) with (data: ArcCell) using liteserver_query_impl);
fn liteserver_query_impl(ctx: &mut Context, stack: &mut Tuple, data: ArcCell) {
    let ip_i32: i32 = 1844203589;
    let ip_u32: u32 = ip_i32 as u32;
    let ip = Ipv4Addr::from(ip_u32);
    let port = 49913;
    let server_pubkey_b64 = "AxFZRHVD1qIO9Fyva52P4vC3tRvk8ac1KKOG0c6IVio=";

    let server_public = match BASE64_STANDARD.decode(server_pubkey_b64) {
        Ok(p) => p,
        Err(e) => {
            ctx.asserts
                .fail(format!("Failed to decode server public key: {}", e));
            stack.push(TupleItem::Null);
            return;
        }
    };

    let now = Instant::now();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            ctx.asserts
                .fail(format!("Failed to create tokio runtime: {}", e));
            stack.push(TupleItem::Null);
            return;
        }
    };

    let result: anyhow::Result<Vec<u8>> = rt.block_on(async {
        let mut peer = AdnlPeer::connect(server_public, (ip, port))
            .await
            .map_err(|e| anyhow::anyhow!("ADNL connection error: {:?}", e))?;

        let mut data_slice = data.parser();
        let data = data_slice
            .load_snake_format_aligned(false)
            .map_err(|e| anyhow::anyhow!("Cannot load cell data: {:?}", e))?;

        let lite_query_data = tl_proto::serialize(MyLiteQuery { data });

        let adnl_query = MyAdnlQuery {
            query_id: Int256::random(),
            query: lite_query_data,
        };

        let send_data = tl_proto::serialize(adnl_query);
        peer.send(send_data.into())
            .await
            .map_err(|e| anyhow::anyhow!("Send error: {:?}", e))?;

        let resp_bytes = peer
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("No response from server"))?
            .map_err(|e| anyhow::anyhow!("ADNL receive error: {:?}", e))?;

        let resp_message: MyAdnlResponse = tl_proto::deserialize(&resp_bytes)
            .map_err(|e| anyhow::anyhow!("TL deserialization error: {:?}", e))?;

        match resp_message {
            MyAdnlResponse::Answer { answer, .. } => Ok(answer),
        }

        // i32::from_le_bytes();
        // let workchain = match <i32 as tl_proto::TlRead<'tl>>::read_from(__packet, __offset) {
        //     Ok(value) => value,
        //     Err(e) => return Err(e),
        // };
        // let resp_message: Message = tl_proto::deserialize(&resp_bytes)
        //     .map_err(|e| anyhow::anyhow!("TL deserialization error: {:?}", e))?;
        // println!("Get response in {:?}", now.elapsed());
        //
        // if let Message::Answer { answer, .. } = resp_message {
        //     if let Response::MasterchainInfo(info) = answer {
        //         println!("Success! Masterchain seqno: {}", info.last.seqno);
        //         println!("Full info: {:?}", info);
        //     } else {
        //         anyhow::bail!("Unexpected response type: {:?}", answer);
        //     }
        // } else {
        //     anyhow::bail!("Unexpected ADNL message: {:?}", resp_message);
        // }

        // Ok(vec![])
    });

    match result {
        Ok(answer_bytes) => stack.push(TupleItem::Cell(to_snake_cell(&answer_bytes).unwrap())),
        Err(e) => {
            println!("{e}");
            ctx.asserts.fail(format!("LiteServer query failed: {}", e));
            stack.push(TupleItem::Null);
        }
    }
}

pub fn to_snake_cell(bytes: &[u8]) -> anyhow::Result<ArcCell> {
    let total_bits = bytes.len() * 8;

    // We leave 8 bits free in each cell for prefixes
    if total_bits <= 1015 {
        // Fast path, the string fits in one cell
        let mut b = CellBuilder::new();
        b.store_bits(total_bits, bytes)?;
        return Ok(b.build()?.to_arc());
    }

    let mut remaining_bytes = bytes;
    let mut cell_data = Vec::new();

    while !remaining_bytes.is_empty() {
        let chunk_size = std::cmp::min(remaining_bytes.len(), 126); // 126 bytes = 1008 bits < 1015
        let chunk = &remaining_bytes[..chunk_size];
        cell_data.push((chunk, chunk.len() * 8));
        remaining_bytes = &remaining_bytes[chunk_size..];
    }

    // build cells from last to first
    let mut next_cell: Option<ArcCell> = None;

    for (chunk, bits) in cell_data.into_iter().rev() {
        let mut b = CellBuilder::new();
        b.store_bits(bits, chunk).unwrap();

        if let Some(next) = next_cell {
            b.store_reference(&next).unwrap();
        }

        next_cell = Some(ArcCell::from(b.build().unwrap()));
    }

    let root_cell = next_cell.unwrap();
    Ok(root_cell)
}

pub fn register_extensions<T: BaseExecutor>(executor: &mut T, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        31 => liteserver_query,
    });
}
