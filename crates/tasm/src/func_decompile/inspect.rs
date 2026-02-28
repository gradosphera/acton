use crate::types::{ArgValue, Instruction, PlainInstruction};

pub(crate) fn flatten_plain_instructions(instructions: &[Instruction]) -> Vec<&PlainInstruction> {
    let mut flattened = Vec::new();
    for instruction in instructions {
        flatten_instruction(instruction, &mut flattened);
    }
    flattened
}

fn flatten_instruction<'a>(
    instruction: &'a Instruction,
    flattened: &mut Vec<&'a PlainInstruction>,
) {
    match instruction {
        Instruction::Plain(plain) => {
            flattened.push(plain);
            for arg in &plain.args {
                flatten_arg(arg, flattened);
            }
        }
        Instruction::Ref(reference) => flatten_arg(&reference.code, flattened),
        Instruction::ExoticCell(_) => {}
    }
}

fn flatten_arg<'a>(arg: &'a ArgValue, flattened: &mut Vec<&'a PlainInstruction>) {
    match arg {
        ArgValue::Code { code, .. } => {
            for instruction in &code.instructions {
                flatten_instruction(instruction, flattened);
            }
        }
        ArgValue::CodeDictionary(dict) => {
            for method in &dict.methods {
                for instruction in &method.instructions {
                    flatten_instruction(instruction, flattened);
                }
            }
        }
        ArgValue::Int(_)
        | ArgValue::UInt(_)
        | ArgValue::Control(_)
        | ArgValue::StackRegister(_)
        | ArgValue::Cell(_) => {}
    }
}

pub(crate) fn as_plain(instruction: &Instruction) -> Option<&PlainInstruction> {
    match instruction {
        Instruction::Plain(plain) => Some(plain),
        Instruction::Ref(_) | Instruction::ExoticCell(_) => None,
    }
}
