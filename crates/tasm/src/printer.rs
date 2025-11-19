use crate::types::{ArgValue, Instruction};
use tycho_types::boc::Boc;

#[derive(Clone, Copy, Default)]
pub struct FormatOptions {
    pub show_hashes: bool,
}

impl FormatOptions {
    pub fn with_show_hashes(mut self, show_hashes: bool) -> Self {
        self.show_hashes = show_hashes;
        self
    }
}

impl Instruction {
    pub fn print(&self, depth: usize, opts: FormatOptions) -> String {
        let indent = "    ".repeat(depth);
        let mut builder = String::new();
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
            for instruction in &code.instructions {
                builder.push_str(&instruction.print(depth + 1, opts));
                builder.push('\n');
            }
            builder.push_str(&indent);
            builder.push('}');
            builder
        }
        ArgValue::CodeDictionary(dict) => {
            let mut builder = String::new();
            builder.push_str("[\n");
            for method in &dict.methods {
                builder.push_str(&indent);
                builder.push_str(&format!("    {} => ", method.id));
                builder.push_str("{");
                if opts.show_hashes {
                    builder.push_str(&format!(" // {}", method.source.repr_hash()));
                }
                builder.push('\n');
                for instruction in &method.instructions {
                    builder.push_str(&instruction.print(depth + 2, opts));
                    builder.push('\n');
                }
                builder.push_str("    ");
                builder.push_str(&indent);
                builder.push_str("}\n");
            }
            builder.push_str(&indent);
            builder.push(']');
            builder
        }
        ArgValue::UInt(v) => format!("{v}"),
    }
}
