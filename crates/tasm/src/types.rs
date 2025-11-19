use crate::printer::FormatOptions;
use crate::spec::SpecInstruction;
use num_bigint::{BigInt, BigUint};
use std::fmt::Write;
use tycho_types::cell::Cell;

#[derive(Debug, Clone)]
pub struct Instruction {
    pub name: String,
    pub instr: Option<Box<SpecInstruction>>,
    pub args: smallvec::SmallVec<[ArgValue; 3]>,
}

#[derive(Debug, Clone)]
pub struct Control {
    pub idx: u64,
}

impl std::fmt::Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "c{}", self.idx)
    }
}

#[derive(Debug, Clone)]
pub struct StackRegister {
    pub idx: i64,
}

impl std::fmt::Display for StackRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "s{}", self.idx)
    }
}

#[derive(Debug, Clone)]
pub struct Code {
    pub instructions: Vec<Instruction>,
}

impl Code {
    pub fn print(&self, options: FormatOptions) -> String {
        let mut s = String::new();
        for instruction in &self.instructions {
            s.write_str(instruction.print(0, options).as_str()).ok();
            s.write_str("\n").ok();
        }
        s
    }
}

#[derive(Debug, Clone)]
pub struct Method {
    pub id: u64,
    pub source: Cell,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct CodeDictionary {
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone)]
pub enum ArgValue {
    Int(BigInt),
    UInt(BigUint),
    Control(Control),
    StackRegister(StackRegister),
    Cell(Cell),
    Code {
        code: Box<Code>,
        source: Cell,
        offset: u16,
    },
    CodeDictionary(CodeDictionary),
}
