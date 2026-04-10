use crate::DebugExecutorHandle;
use std::sync::Arc;
use tolkc::TolkSourceMap;
use tolkc::abi::ContractABI;

#[derive(Clone)]
pub struct ChildDebugContextSpec {
    pub thread_id: i64,
    pub name: String,
    pub executor: DebugExecutorHandle,
    pub source_map: Option<Arc<TolkSourceMap>>,
    pub compiler_abi: Option<Arc<ContractABI>>,
    pub stop_on_entry: bool,
}
