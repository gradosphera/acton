use abi::ContractAbi;
use emulator::blockchain::Blockchain;
use emulator::emulator::Emulator;
use emulator::tuple::stack::Tuple;
use num_bigint::BigInt;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AssertBinFailure {
    pub operator: String,
    pub left: Tuple,
    pub left_type: String,
    pub right: Tuple,
    pub right_type: String,
    pub message: Option<String>,
    pub location: Option<String>,
}

impl AssertBinFailure {
    pub fn is_ord(&self) -> bool {
        self.operator == "<"
            || self.operator == ">"
            || self.operator == "<="
            || self.operator == ">="
    }
}

#[derive(Debug, Clone)]
pub struct FailAssertFailure {
    pub message: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AssertFailure {
    Bin(AssertBinFailure),
    Fail(FailAssertFailure),
}

impl AssertFailure {
    pub fn message(&self) -> Option<String> {
        match self {
            AssertFailure::Bin(arg) => arg.message.clone(),
            AssertFailure::Fail(arg) => arg.message.clone(),
        }
    }

    pub fn location(&self) -> Option<String> {
        match self {
            AssertFailure::Bin(arg) => arg.location.clone(),
            AssertFailure::Fail(arg) => arg.location.clone(),
        }
    }
}

pub struct BuildCache {
    pub built: HashMap<String, CompilationResult>,
}

impl BuildCache {
    pub fn new() -> Self {
        Self {
            built: HashMap::new(),
        }
    }

    pub fn memoize(&mut self, name: &String, path: &String, code: &String, code_hash: &String) {
        self.built.insert(
            path.clone(),
            CompilationResult {
                name: name.clone(),
                code_boc64: code.clone(),
                code_hash: code_hash.clone(),
            },
        );
    }

    pub fn to_tuple_build_cache(&self) -> emulator::tuple::stack::BuildCache {
        emulator::tuple::stack::BuildCache {
            built: self
                .built
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        emulator::tuple::stack::CompilationResult {
                            name: v.name.clone(),
                            code_boc64: v.code_boc64.clone(),
                            code_hash: v.code_hash.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
}

pub struct CompilationResult {
    name: String,
    code_boc64: String,
    code_hash: String,
}

pub struct Context<'a> {
    pub stdout_buffer: String,
    pub stderr_buffer: String,
    pub capture_test_output: bool,
    pub assert_failure: &'a mut Option<AssertFailure>,
    pub expected_exit_code: &'a mut Option<BigInt>,
    pub blockchain: &'a mut Blockchain,
    pub emulator: &'a mut Emulator,
    pub build_cache: &'a mut BuildCache,
    pub abi: ContractAbi,
}
