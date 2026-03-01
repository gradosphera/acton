use super::ast::{ExprAst, StmtAst, UnaryOp, Var};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
struct UseOccurrence {
    stmt_idx: usize,
    ident: String,
}

#[derive(Debug, Default)]
struct BlockDefUseIndex {
    uses: Vec<UseOccurrence>,
    def_to_uses: BTreeMap<usize, Vec<usize>>,
    use_to_def: Vec<Option<usize>>,
}

pub(crate) fn simplify_method_body(stmts: &mut Vec<StmtAst>) {
    simplify_stmt_list(stmts);
}

fn simplify_stmt_list(stmts: &mut Vec<StmtAst>) {
    for stmt in stmts.iter_mut() {
        simplify_stmt(stmt);
    }

    loop {
        let index = build_block_def_use_index(stmts);
        let mut changed = false;

        for def_idx in 0..stmts.len().saturating_sub(1) {
            let (var_name, init_expr) = match &stmts[def_idx] {
                StmtAst::VarDecl {
                    binding: Var::Name(name),
                    expr,
                } => (name.clone(), expr.clone()),
                _ => continue,
            };

            let use_ids = match index.def_to_uses.get(&def_idx) {
                Some(ids) if ids.len() == 1 => ids,
                _ => continue,
            };
            let use_id = use_ids[0];
            let use_occurrence = match index.uses.get(use_id) {
                Some(occ) => occ,
                None => continue,
            };

            if use_occurrence.stmt_idx != def_idx + 1
                || use_occurrence.ident != var_name
                || index.use_to_def.get(use_id).copied().flatten() != Some(def_idx)
            {
                continue;
            }

            let next_stmt = match stmts.get_mut(def_idx + 1) {
                Some(stmt) => stmt,
                None => continue,
            };
            let changed_next = rewrite_next_stmt_condition_site(next_stmt, &var_name, &init_expr);
            if !changed_next {
                continue;
            }
            stmts.remove(def_idx);
            changed = true;
            break;
        }

        if !changed {
            break;
        }
    }
}

fn condition_inline_replacement(
    condition: &ExprAst,
    var_name: &str,
    init_expr: &ExprAst,
) -> Option<ExprAst> {
    match condition {
        ExprAst::Ident(name) if name == var_name => Some(init_expr.clone()),
        ExprAst::Unary {
            op: UnaryOp::BitNot,
            expr,
        } => match expr.as_ref() {
            ExprAst::Ident(name) if name == var_name => Some(ExprAst::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(init_expr.clone()),
            }),
            _ => None,
        },
        _ => None,
    }
}

fn rewrite_next_stmt_condition_site(
    stmt: &mut StmtAst,
    var_name: &str,
    init_expr: &ExprAst,
) -> bool {
    match stmt {
        StmtAst::If { condition, .. } => {
            if let Some(replacement) = condition_inline_replacement(condition, var_name, init_expr)
            {
                *condition = replacement;
                return true;
            }
            false
        }
        StmtAst::VarDecl {
            expr:
                ExprAst::Ternary {
                    condition,
                    ..
                },
            ..
        }
        | StmtAst::Assign {
            expr:
                ExprAst::Ternary {
                    condition,
                    ..
                },
            ..
        } => {
            if let Some(replacement) = condition_inline_replacement(condition, var_name, init_expr)
            {
                *condition = Box::new(replacement);
                return true;
            }
            false
        }
        _ => false,
    }
}

fn simplify_stmt(stmt: &mut StmtAst) {
    match stmt {
        StmtAst::If {
            then_body,
            else_body,
            ..
        } => {
            simplify_stmt_list(then_body);
            if let Some(else_body) = else_body.as_mut() {
                simplify_stmt_list(else_body);
            }
        }
        StmtAst::Repeat { body, .. } => simplify_stmt_list(body),
        StmtAst::DoUntil { body, .. } => simplify_stmt_list(body),
        StmtAst::Comment(_)
        | StmtAst::VarDecl { .. }
        | StmtAst::Assign { .. }
        | StmtAst::Return(_)
        | StmtAst::Call { .. } => {}
    }
}

fn build_block_def_use_index(stmts: &[StmtAst]) -> BlockDefUseIndex {
    let mut defs_by_ident: HashMap<String, Vec<usize>> = HashMap::new();
    for (stmt_idx, stmt) in stmts.iter().enumerate() {
        if let StmtAst::VarDecl {
            binding: Var::Name(name),
            ..
        } = stmt
        {
            defs_by_ident.entry(name.clone()).or_default().push(stmt_idx);
        }
    }

    let mut index = BlockDefUseIndex::default();
    for (stmt_idx, stmt) in stmts.iter().enumerate() {
        let mut idents = Vec::new();
        collect_stmt_idents(stmt, &mut idents);
        for ident in idents {
            let use_id = index.uses.len();
            index.uses.push(UseOccurrence {
                stmt_idx,
                ident: ident.clone(),
            });

            let def = defs_by_ident
                .get(&ident)
                .and_then(|defs| defs.iter().copied().filter(|d| *d < stmt_idx).max());
            index.use_to_def.push(def);
            if let Some(def_idx) = def {
                index.def_to_uses.entry(def_idx).or_default().push(use_id);
            }
        }
    }

    index
}

fn collect_stmt_idents(stmt: &StmtAst, out: &mut Vec<String>) {
    match stmt {
        StmtAst::Comment(_) => {}
        StmtAst::VarDecl { expr, .. } => collect_expr_idents(expr, out),
        StmtAst::Assign { target, expr } => {
            out.push(target.clone());
            collect_expr_idents(expr, out);
        }
        StmtAst::Return(Some(expr)) => collect_expr_idents(expr, out),
        StmtAst::Return(None) => {}
        StmtAst::Call { args, .. } => {
            for arg in args {
                collect_expr_idents(arg, out);
            }
        }
        StmtAst::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            collect_expr_idents(condition, out);
            for nested in then_body {
                collect_stmt_idents(nested, out);
            }
            if let Some(else_body) = else_body {
                for nested in else_body {
                    collect_stmt_idents(nested, out);
                }
            }
        }
        StmtAst::Repeat { count, body } => {
            collect_expr_idents(count, out);
            for nested in body {
                collect_stmt_idents(nested, out);
            }
        }
        StmtAst::DoUntil { body, condition } => {
            for nested in body {
                collect_stmt_idents(nested, out);
            }
            collect_expr_idents(condition, out);
        }
    }
}

fn collect_expr_idents(expr: &ExprAst, out: &mut Vec<String>) {
    match expr {
        ExprAst::Ident(name) => out.push(name.clone()),
        ExprAst::Number(_)
        | ExprAst::StringLiteral(_)
        | ExprAst::CellLiteral(_)
        | ExprAst::NullLiteral => {}
        ExprAst::Unary { expr, .. } => collect_expr_idents(expr, out),
        ExprAst::Binary { lhs, rhs, .. } => {
            collect_expr_idents(lhs, out);
            collect_expr_idents(rhs, out);
        }
        ExprAst::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_idents(condition, out);
            collect_expr_idents(then_expr, out);
            collect_expr_idents(else_expr, out);
        }
        ExprAst::Tuple(items) => {
            for item in items {
                collect_expr_idents(item, out);
            }
        }
        ExprAst::Call { args, .. } => {
            for arg in args {
                collect_expr_idents(arg, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::simplify_method_body;
    use crate::func_decompile::ast::{BinaryOp, ExprAst, StmtAst, Var};

    fn ident(name: &str) -> ExprAst {
        ExprAst::Ident(name.to_string())
    }

    fn num(n: &str) -> ExprAst {
        ExprAst::Number(n.to_string())
    }

    #[test]
    fn inlines_single_use_if_condition() {
        let mut body = vec![
            StmtAst::VarDecl {
                binding: Var::name("v247"),
                expr: ExprAst::Binary {
                    lhs: Box::new(ident("v42")),
                    op: BinaryOp::Equal,
                    rhs: Box::new(num("621336170")),
                    wrap_lhs: true,
                    wrap_rhs: true,
                },
            },
            StmtAst::If {
                negated: false,
                condition: ident("v247"),
                then_body: vec![StmtAst::Return(None)],
                else_body: None,
            },
        ];

        simplify_method_body(&mut body);

        assert_eq!(body.len(), 1);
        match &body[0] {
            StmtAst::If { condition, .. } => {
                assert_ne!(condition, &ident("v247"));
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn does_not_inline_when_multiple_uses() {
        let mut body = vec![
            StmtAst::VarDecl {
                binding: Var::name("v247"),
                expr: ExprAst::Binary {
                    lhs: Box::new(ident("v42")),
                    op: BinaryOp::Equal,
                    rhs: Box::new(num("621336170")),
                    wrap_lhs: true,
                    wrap_rhs: true,
                },
            },
            StmtAst::If {
                negated: false,
                condition: ident("v247"),
                then_body: vec![],
                else_body: None,
            },
            StmtAst::Call {
                callee: "touch".to_string(),
                args: vec![ident("v247")],
            },
        ];

        simplify_method_body(&mut body);

        assert_eq!(body.len(), 3);
        match &body[1] {
            StmtAst::If { condition, .. } => assert_eq!(condition, &ident("v247")),
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn inlines_single_use_if_condition_under_bitnot() {
        let mut body = vec![
            StmtAst::VarDecl {
                binding: Var::name("v165"),
                expr: ExprAst::Binary {
                    lhs: Box::new(ident("v164")),
                    op: BinaryOp::Equal,
                    rhs: Box::new(num("0")),
                    wrap_lhs: true,
                    wrap_rhs: true,
                },
            },
            StmtAst::If {
                negated: false,
                condition: ExprAst::Unary {
                    op: crate::func_decompile::ast::UnaryOp::BitNot,
                    expr: Box::new(ident("v165")),
                },
                then_body: vec![StmtAst::Return(None)],
                else_body: None,
            },
        ];

        simplify_method_body(&mut body);

        assert_eq!(body.len(), 1);
        match &body[0] {
            StmtAst::If { condition, .. } => match condition {
                ExprAst::Unary { op, expr } => {
                    assert_eq!(*op, crate::func_decompile::ast::UnaryOp::BitNot);
                    assert_ne!(expr.as_ref(), &ident("v165"));
                }
                _ => panic!("expected unary bitnot condition"),
            },
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn inlines_single_use_ternary_condition() {
        let mut body = vec![
            StmtAst::VarDecl {
                binding: Var::name("v88"),
                expr: ExprAst::Call {
                    callee: "null?".to_string(),
                    args: vec![ident("v87")],
                },
            },
            StmtAst::VarDecl {
                binding: Var::name("v89"),
                expr: ExprAst::Ternary {
                    condition: Box::new(ident("v88")),
                    then_expr: Box::new(num("10065")),
                    else_expr: Box::new(ident("v87")),
                },
            },
        ];

        simplify_method_body(&mut body);

        assert_eq!(body.len(), 1);
        match &body[0] {
            StmtAst::VarDecl { expr, .. } => match expr {
                ExprAst::Ternary { condition, .. } => {
                    assert_ne!(condition.as_ref(), &ident("v88"));
                }
                _ => panic!("expected ternary expr"),
            },
            _ => panic!("expected var decl"),
        }
    }

    #[test]
    fn inlines_each_adjacent_ternary_condition_pair() {
        let mut body = vec![
            StmtAst::VarDecl {
                binding: Var::name("v88"),
                expr: ExprAst::Call {
                    callee: "null?".to_string(),
                    args: vec![ident("v87")],
                },
            },
            StmtAst::VarDecl {
                binding: Var::name("v89"),
                expr: ExprAst::Ternary {
                    condition: Box::new(ident("v88")),
                    then_expr: Box::new(num("10065")),
                    else_expr: Box::new(ident("v87")),
                },
            },
            StmtAst::VarDecl {
                binding: Var::name("v90"),
                expr: ExprAst::Call {
                    callee: "null?".to_string(),
                    args: vec![ident("v87")],
                },
            },
            StmtAst::VarDecl {
                binding: Var::name("v91"),
                expr: ExprAst::Ternary {
                    condition: Box::new(ident("v90")),
                    then_expr: Box::new(num("10435")),
                    else_expr: Box::new(ident("v87")),
                },
            },
        ];

        simplify_method_body(&mut body);

        assert_eq!(body.len(), 2);
        match &body[0] {
            StmtAst::VarDecl { expr, .. } => match expr {
                ExprAst::Ternary { condition, .. } => {
                    assert_ne!(condition.as_ref(), &ident("v88"));
                }
                _ => panic!("expected ternary expr"),
            },
            _ => panic!("expected var decl"),
        }
        match &body[1] {
            StmtAst::VarDecl { expr, .. } => match expr {
                ExprAst::Ternary { condition, .. } => {
                    assert_ne!(condition.as_ref(), &ident("v90"));
                }
                _ => panic!("expected ternary expr"),
            },
            _ => panic!("expected var decl"),
        }
    }
}
