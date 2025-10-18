pub mod config;
pub mod exit_codes;
pub mod exts;
pub mod exts_lib;
pub mod tolk_parser;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
