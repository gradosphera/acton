use crate::spec::SpecInstruction;
use num_bigint::{BigInt, BigUint};
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

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for instruction in &self.instructions {
            writeln!(f, "{}", instruction.print(0))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Method {
    pub id: u64,
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
    Code(Box<Code>),
    CodeDictionary(CodeDictionary),
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.print(0))
    }
}
