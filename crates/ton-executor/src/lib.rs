//! # ton-executor
//!
//! `ton-executor` is a thin Rust wrapper around the C++ TON transaction and TVM emulators.
//! It provides two specialized executors for different use cases:
//!
//! - [`message::Executor`]: Used for full transaction emulation, including account state
//!   updates, gas calculation, and action processing.
//! - [`get::GetExecutor`]: Optimized for executing "get-methods" of smart contracts,
//!   allowing off-chain state inspection.
//!
//! ## Key Concepts
//!
//! ### Data Format
//! Most data (messages, account states, stacks) is exchanged as **Base64-encoded Bag of Cells (BoC)** strings.
//!
//! ### Concurrency and Thread Safety
//! **Important:** The underlying C++ implementation relies on **global variables**.
//! - Only one instance of any executor should be active at a time.
//! - All operations must be performed within a **single thread**.
//! - When running tests, use `cargo test -- --test-threads=1`.
//!
//! ### Extension Methods
//! Both executors support registering custom extension methods (external opcodes) using
//! `register_ext_method`. These are triggered by the `EXTCALL <ID>` instruction in the TVM.

mod common;
mod config;

pub mod get;
pub mod message;

pub use common::*;
