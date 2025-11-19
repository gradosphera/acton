use crate::types::{ArgValue, Instruction};
use tycho_types::boc::Boc;

const OFFSET_PADDING: &str = "    │ ";

#[derive(Clone, Copy, Default)]
pub struct FormatOptions {
    pub show_hashes: bool,
    pub show_offsets: bool,
}

impl FormatOptions {
    pub fn with_show_hashes(mut self, show_hashes: bool) -> Self {
        self.show_hashes = show_hashes;
        self
    }

    pub fn with_show_offset(mut self, show_offset: bool) -> Self {
        self.show_offsets = show_offset;
        self
    }
}

impl Instruction {
    pub fn print(&self, depth: usize, opts: FormatOptions, offset: Option<u16>) -> String {
        let indent = "    ".repeat(depth);
        let mut builder = String::new();

        if opts.show_offsets {
            if let Some(off) = offset {
                builder.push_str(&format!("{:<4}│ ", off));
            } else {
                builder.push_str("     │");
            }
        }

        builder.push_str(&indent);
        builder.push_str(&normalize_name(&self.name));
        builder.push(' ');

        for (i, arg) in self.args.iter().enumerate() {
            builder.push_str(&format_arg(arg, depth, opts));
            if i < self.args.len() - 1 {
                builder.push(' ');
            }
        }

        builder.trim_end().to_string()
    }
}

impl ArgValue {
    pub fn string(&self) -> String {
        match self {
            ArgValue::Control(c) => format!("{c}"),
            ArgValue::StackRegister(s) => format!("{s}"),
            ArgValue::Int(b) => format!("{b}"),
            _ => panic!("unhandled value: {self:?}"),
        }
    }
}

fn normalize_name(name: &str) -> String {
    if let Some(stripped) = name.strip_prefix('2') {
        format!("{stripped}2")
    } else {
        name.replace('#', "_")
    }
}

fn format_arg(arg: &ArgValue, depth: usize, opts: FormatOptions) -> String {
    let indent = "    ".repeat(depth);
    match arg {
        ArgValue::Control(c) => format!("{c}"),
        ArgValue::StackRegister(s) => format!("{s}"),
        ArgValue::Int(b) => format!("{b}"),
        ArgValue::Cell(s) => {
            let slice = s.as_slice().unwrap();
            if slice.size_refs() == 0 {
                format!("x{{{}}}", slice.display_data().to_string())
            } else {
                format!("boc{{{}}}", Boc::encode_hex(s))
            }
        }
        ArgValue::Code {
            code,
            source,
            offset,
        } => {
            let mut builder = String::new();
            builder.push('{');
            if opts.show_hashes {
                builder.push_str(&format!(" // {} offset {}", source.repr_hash(), offset));
            }
            builder.push('\n');
            for (i, instruction) in code.instructions.iter().enumerate() {
                let instr_offset = code.offsets.as_ref().and_then(|offs| offs.get(i).copied());
                builder.push_str(&instruction.print(depth + 1, opts, instr_offset));
                builder.push('\n');
            }

            if opts.show_offsets {
                builder.push_str("    │ ");
            }

            builder.push_str(&indent);
            builder.push('}');
            builder
        }
        ArgValue::CodeDictionary(dict) => {
            let mut builder = String::new();
            builder.push_str("[\n");
            for method in &dict.methods {
                if opts.show_offsets {
                    builder.push_str(OFFSET_PADDING);
                }

                builder.push_str(&indent);
                builder.push_str(&format!("    {} => ", method.id));
                builder.push_str("{");
                if opts.show_hashes {
                    builder.push_str(&format!(" // {}", method.source.repr_hash()));
                }
                builder.push('\n');
                for (i, instruction) in method.instructions.iter().enumerate() {
                    let instr_offset = method
                        .offsets
                        .as_ref()
                        .and_then(|offs| offs.get(i).copied());
                    builder.push_str(&instruction.print(depth + 2, opts, instr_offset));
                    builder.push('\n');
                }

                if opts.show_offsets {
                    builder.push_str(OFFSET_PADDING);
                }

                builder.push_str("    ");
                builder.push_str(&indent);
                builder.push_str("}\n");
            }

            if opts.show_offsets {
                builder.push_str(OFFSET_PADDING);
            }

            builder.push_str(&indent);
            builder.push(']');
            builder
        }
        ArgValue::UInt(v) => format!("{v}"),
    }
}
