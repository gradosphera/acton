use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParamAst {
    pub ty: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MethodSignatureAst {
    pub return_type: String,
    pub name: String,
    pub params: Vec<ParamAst>,
    pub qualifiers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UnaryOp {
    Negate,
    BitNot,
}

impl UnaryOp {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Negate => "-",
            Self::BitNot => "~",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    DivR,
    DivC,
    Mod,
    ModR,
    ModC,
    LShift,
    RShift,
    RShiftR,
    RShiftC,
    And,
    Or,
    Xor,
    Greater,
    Less,
    Equal,
    NotEqual,
    LessOrEqual,
    GreaterOrEqual,
    Cmp,
}

impl BinaryOp {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::DivR => "~/",
            Self::DivC => "^/",
            Self::Mod => "%",
            Self::ModR => "~%",
            Self::ModC => "^%",
            Self::LShift => "<<",
            Self::RShift => ">>",
            Self::RShiftR => "~>>",
            Self::RShiftC => "^>>",
            Self::And => "&",
            Self::Or => "|",
            Self::Xor => "^",
            Self::Greater => ">",
            Self::Less => "<",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::LessOrEqual => "<=",
            Self::GreaterOrEqual => ">=",
            Self::Cmp => "<=>",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ExprAst {
    Ident(String),
    Number(String),
    StringLiteral(String),
    CellLiteral(String),
    NullLiteral,
    Unary {
        op: UnaryOp,
        expr: Box<ExprAst>,
    },
    Binary {
        lhs: Box<ExprAst>,
        op: BinaryOp,
        rhs: Box<ExprAst>,
        wrap_lhs: bool,
        wrap_rhs: bool,
    },
    Ternary {
        condition: Box<ExprAst>,
        then_expr: Box<ExprAst>,
        else_expr: Box<ExprAst>,
    },
    Tuple(Vec<ExprAst>),
    Call {
        callee: String,
        args: Vec<ExprAst>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Var {
    Name(String),
    Tensor(Vec<Var>),
}

impl Var {
    #[must_use]
    pub(crate) fn name(name: impl Into<String>) -> Self {
        Self::Name(name.into())
    }

    #[must_use]
    pub(crate) fn tensor(items: Vec<Var>) -> Self {
        Self::Tensor(items)
    }
}

impl From<String> for Var {
    fn from(value: String) -> Self {
        Self::Name(value)
    }
}

impl From<&str> for Var {
    fn from(value: &str) -> Self {
        Self::Name(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StmtAst {
    Comment(String),
    VarDecl {
        binding: Var,
        expr: ExprAst,
    },
    Assign {
        target: String,
        expr: ExprAst,
    },
    Return(Option<ExprAst>),
    Call {
        callee: String,
        args: Vec<ExprAst>,
    },
    If {
        negated: bool,
        condition: ExprAst,
        then_body: Vec<StmtAst>,
        else_body: Option<Vec<StmtAst>>,
    },
    Repeat {
        count: ExprAst,
        body: Vec<StmtAst>,
    },
    DoUntil {
        body: Vec<StmtAst>,
        condition: ExprAst,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MethodAst {
    pub signature: MethodSignatureAst,
    pub leading_comments: Vec<String>,
    pub body: Vec<StmtAst>,
}

impl MethodSignatureAst {
    #[must_use]
    pub(crate) fn render(&self) -> String {
        let params = self
            .params
            .iter()
            .map(|p| format!("{} {}", p.ty, p.name))
            .collect::<Vec<_>>()
            .join(", ");
        let mut out = format!("{} {}({})", self.return_type, self.name, params);
        if !self.qualifiers.is_empty() {
            out.push(' ');
            out.push_str(&self.qualifiers.join(" "));
        }
        out.push_str(" {");
        out
    }
}

impl StmtAst {
    #[must_use]
    #[cfg(test)]
    pub(crate) fn var(name: impl Into<String>, expr: ExprAst) -> Self {
        Self::VarDecl {
            binding: Var::Name(name.into()),
            expr,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn assign(target: impl Into<String>, expr: ExprAst) -> Self {
        Self::Assign {
            target: target.into(),
            expr,
        }
    }
}

pub(crate) fn render_method_ast(ast: &MethodAst, out: &mut String) {
    let _ = writeln!(out, "{}", ast.signature.render());
    for comment in &ast.leading_comments {
        let _ = writeln!(out, "    {}", comment.trim());
    }
    render_stmt_list(&ast.body, 1, out);
    out.push_str("}\n\n");
}

fn render_stmt_list(stmts: &[StmtAst], depth: usize, out: &mut String) {
    for stmt in stmts {
        render_stmt(stmt, depth, out);
    }
}

fn render_stmt(stmt: &StmtAst, depth: usize, out: &mut String) {
    let indent = "    ".repeat(depth);
    match stmt {
        StmtAst::Comment(line) => {
            let _ = writeln!(out, "{indent}{line}");
        }
        StmtAst::VarDecl { binding, expr } => {
            let _ = writeln!(
                out,
                "{indent}var {} = {};",
                render_tensor_expr(binding),
                render_expr(expr)
            );
        }
        StmtAst::Assign { target, expr } => {
            let _ = writeln!(out, "{indent}{target} = {};", render_expr(expr));
        }
        StmtAst::Return(Some(expr)) => {
            let _ = writeln!(out, "{indent}return {};", render_expr(expr));
        }
        StmtAst::Return(None) => {
            let _ = writeln!(out, "{indent}return ();");
        }
        StmtAst::Call { callee, args } => {
            let rendered_args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            let _ = writeln!(out, "{indent}{callee}({rendered_args});");
        }
        StmtAst::If {
            negated,
            condition,
            then_body,
            else_body,
        } => {
            let keyword = if *negated { "ifnot" } else { "if" };
            let _ = writeln!(out, "{indent}{keyword} ({}) {{", render_expr(condition));
            render_stmt_list(then_body, depth + 1, out);
            if let Some(else_body) = else_body {
                let _ = writeln!(out, "{indent}}} else {{");
                render_stmt_list(else_body, depth + 1, out);
            }
            let _ = writeln!(out, "{indent}}}");
        }
        StmtAst::Repeat { count, body } => {
            let _ = writeln!(out, "{indent}repeat ({}) {{", render_expr(count));
            render_stmt_list(body, depth + 1, out);
            let _ = writeln!(out, "{indent}}}");
        }
        StmtAst::DoUntil { body, condition } => {
            let _ = writeln!(out, "{indent}do {{");
            render_stmt_list(body, depth + 1, out);
            let _ = writeln!(out, "{indent}}} until ({});", render_expr(condition));
        }
    }
}

fn render_expr(expr: &ExprAst) -> String {
    match expr {
        ExprAst::Ident(s) => s.clone(),
        ExprAst::Number(s) => s.clone(),
        ExprAst::StringLiteral(s) => s.clone(),
        ExprAst::CellLiteral(s) => s.clone(),
        ExprAst::NullLiteral => "null()".to_string(),
        ExprAst::Unary { op, expr } => {
            format!("{}({})", op.as_str(), render_expr(expr))
        }
        ExprAst::Binary {
            lhs,
            op,
            rhs,
            wrap_lhs,
            wrap_rhs,
        } => {
            let lhs = render_expr(lhs);
            let rhs = render_expr(rhs);
            let lhs = if *wrap_lhs { format!("({lhs})") } else { lhs };
            let rhs = if *wrap_rhs { format!("({rhs})") } else { rhs };
            format!("{lhs} {} {rhs}", op.as_str())
        }
        ExprAst::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            format!(
                "({}) ? ({}) : ({})",
                render_expr(condition),
                render_expr(then_expr),
                render_expr(else_expr)
            )
        }
        ExprAst::Tuple(items) => {
            let rendered = items.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("({rendered})")
        }
        ExprAst::Call { callee, args } => {
            let rendered_args = args.iter().map(render_expr).collect::<Vec<_>>().join(", ");
            format!("{callee}({rendered_args})")
        }
    }
}

fn render_tensor_expr(tensor: &Var) -> String {
    match tensor {
        Var::Name(name) => name.clone(),
        Var::Tensor(items) => {
            let rendered = items
                .iter()
                .map(render_tensor_expr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({rendered})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BinaryOp, ExprAst, MethodAst, MethodSignatureAst, ParamAst, StmtAst, render_method_ast,
    };

    #[test]
    fn renders_structured_ast() {
        let ast = MethodAst {
            signature: MethodSignatureAst {
                return_type: "int".to_string(),
                name: "method_1".to_string(),
                params: vec![ParamAst {
                    ty: "int".to_string(),
                    name: "arg0".to_string(),
                }],
                qualifiers: vec!["impure".to_string(), "method_id(1)".to_string()],
            },
            leading_comments: vec![";; dict_method_id: 1".to_string()],
            body: vec![
                StmtAst::var("v0", ExprAst::Number("0".to_string())),
                StmtAst::If {
                    negated: false,
                    condition: ExprAst::Ident("arg0".to_string()),
                    then_body: vec![StmtAst::assign("v0", ExprAst::Number("1".to_string()))],
                    else_body: Some(vec![StmtAst::assign(
                        "v0",
                        ExprAst::Number("2".to_string()),
                    )]),
                },
                StmtAst::DoUntil {
                    body: vec![StmtAst::assign(
                        "v0",
                        ExprAst::Binary {
                            lhs: Box::new(ExprAst::Ident("v0".to_string())),
                            op: BinaryOp::Add,
                            rhs: Box::new(ExprAst::Number("1".to_string())),
                            wrap_lhs: false,
                            wrap_rhs: false,
                        },
                    )],
                    condition: ExprAst::Binary {
                        lhs: Box::new(ExprAst::Ident("v0".to_string())),
                        op: BinaryOp::Greater,
                        rhs: Box::new(ExprAst::Number("10".to_string())),
                        wrap_lhs: false,
                        wrap_rhs: false,
                    },
                },
                StmtAst::Return(Some(ExprAst::Ident("v0".to_string()))),
            ],
        };

        let mut out = String::new();
        render_method_ast(&ast, &mut out);

        assert!(out.contains("int method_1(int arg0) impure method_id(1) {"));
        assert!(out.contains("if (arg0) {"));
        assert!(out.contains("} else {"));
        assert!(out.contains("do {"));
        assert!(out.contains("} until (v0 > 10);"));
        assert!(out.contains("return v0;"));
    }
}
