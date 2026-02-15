use crate::cfg::{ControlFlowGraph, EdgeKind, FlowNodeKind, NodeId};
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use tolk_resolver::file_index::AstNodeSpanExt;
use tolk_resolver::resolve_index::{FileResolveIndex, LocalDefId, Resolved};
use tolk_syntax::ast::Node;
use tolk_syntax::{
    Assert, Assign, AstNode, Call, DotAccess, Expr, FuncBody, FunctionLike, HasName, IfAlt,
    InstanceArg, Match, MatchArmBody, MatchPattern, Paren, SetAssign, Stmt, TopLevel,
    VarDeclPattern,
};

/// Builds CFG for supported top-level declarations (`fun`, `method`, `get fun`).
#[must_use]
pub fn build_cfg_for_top_level(
    top_level: &TopLevel<'_>,
    resolve_index: Option<&FileResolveIndex>,
) -> Option<ControlFlowGraph> {
    match top_level {
        TopLevel::Func(func) => build_cfg_for_function_like(func, resolve_index),
        TopLevel::Method(method) => build_cfg_for_function_like(method, resolve_index),
        TopLevel::GetMethod(get_method) => build_cfg_for_function_like(get_method, resolve_index),
        _ => None,
    }
}

/// Builds CFG for function-like declaration with block body.
#[must_use]
pub fn build_cfg_for_function_like<'tree, F: FunctionLike<'tree>>(
    function: &F,
    resolve_index: Option<&FileResolveIndex>,
) -> Option<ControlFlowGraph> {
    let body = function.body()?;
    let FuncBody::Block(block) = body else {
        return None;
    };

    let mut builder = CfgBuilder::new(resolve_index);
    let fragment = builder.build_block_fragment(block);

    builder
        .cfg
        .add_edge(builder.cfg.entry(), fragment.entry, EdgeKind::Unconditional);
    for exit in fragment.exits {
        builder
            .cfg
            .add_edge(exit, builder.cfg.exit(), EdgeKind::Unconditional);
    }

    Some(builder.cfg)
}

#[derive(Debug, Clone)]
struct Fragment {
    entry: NodeId,
    exits: Vec<NodeId>,
    nodes: Vec<NodeId>,
}

impl Fragment {
    fn single(node: NodeId) -> Self {
        Self {
            entry: node,
            exits: vec![node],
            nodes: vec![node],
        }
    }

    fn terminal(node: NodeId) -> Self {
        Self {
            entry: node,
            exits: Vec::new(),
            nodes: vec![node],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

#[derive(Debug)]
struct CfgBuilder<'idx> {
    cfg: ControlFlowGraph,
    loops: Vec<LoopContext>,
    exception_targets: Vec<NodeId>,
    collector: Option<UseDefCollector<'idx>>,
}

impl<'idx> CfgBuilder<'idx> {
    fn new(resolve_index: Option<&'idx FileResolveIndex>) -> Self {
        Self {
            cfg: ControlFlowGraph::new(),
            loops: Vec::new(),
            exception_targets: Vec::new(),
            collector: resolve_index.map(UseDefCollector::new),
        }
    }

    fn make_nop_fragment(&mut self, span: Option<tolk_resolver::Span>) -> Fragment {
        let node = self.cfg.add_node(FlowNodeKind::Nop, span);
        Fragment::single(node)
    }

    fn append_fragments(&mut self, mut left: Fragment, right: Fragment) -> Fragment {
        for exit in &left.exits {
            self.cfg
                .add_edge(*exit, right.entry, EdgeKind::Unconditional);
        }
        left.nodes.extend(right.nodes.iter().copied());
        left.exits = right.exits;
        left
    }

    fn build_block_fragment(&mut self, block: tolk_syntax::Block<'_>) -> Fragment {
        let mut stmt_iter = block.stmts();
        let Some(first_stmt) = stmt_iter.next() else {
            return self.make_nop_fragment(Some(block.span()));
        };

        let mut fragment = self.build_stmt_fragment(first_stmt);

        for stmt in stmt_iter {
            let next = self.build_stmt_fragment(stmt);
            fragment = self.append_fragments(fragment, next);
        }

        fragment
    }

    fn build_stmt_fragment(&mut self, stmt: Stmt<'_>) -> Fragment {
        match stmt {
            Stmt::ExprStmt(expr_stmt) => {
                if let Some(expr) = expr_stmt.expr() {
                    let node = self.cfg.add_node(FlowNodeKind::Expr, Some(expr.span()));
                    self.collect_expr_into_node(node, expr, AccessMode::Read);
                    Fragment::single(node)
                } else {
                    self.make_nop_fragment(Some(expr_stmt.span()))
                }
            }
            Stmt::Block(block_stmt) => self.build_block_fragment(block_stmt),
            Stmt::If(if_stmt) => self.build_if_fragment(if_stmt),
            Stmt::While(while_stmt) => self.build_while_fragment(while_stmt),
            Stmt::Repeat(repeat_stmt) => self.build_repeat_fragment(repeat_stmt),
            Stmt::TryCatch(try_catch_stmt) => self.build_try_catch_fragment(try_catch_stmt),
            Stmt::Return(return_stmt) => self.build_return_fragment(return_stmt),
            Stmt::DoWhile(do_while_stmt) => self.build_do_while_fragment(do_while_stmt),
            Stmt::Break(break_stmt) => self.build_break_fragment(break_stmt),
            Stmt::Continue(continue_stmt) => self.build_continue_fragment(continue_stmt),
            Stmt::Throw(throw_stmt) => self.build_throw_fragment(throw_stmt),
            Stmt::Assert(assert_stmt) => self.build_assert_fragment(assert_stmt),
            Stmt::Match(match_stmt) => self.build_match_stmt_fragment(match_stmt),
            Stmt::EmptyStmt(empty) => self.make_nop_fragment(Some(empty.span())),
            Stmt::Unmapped(unmapped) => {
                self.make_nop_fragment(Some(tolk_resolver::Span::from_syntax(&unmapped.0)))
            }
        }
    }

    fn build_if_fragment(&mut self, if_stmt: tolk_syntax::If<'_>) -> Fragment {
        let cond_span = if_stmt.condition().map(|cond| cond.span());
        let cond_node = self.cfg.add_node(
            FlowNodeKind::Condition,
            cond_span.or_else(|| Some(if_stmt.span())),
        );

        if let Some(condition) = if_stmt.condition() {
            self.collect_expr_into_node(cond_node, condition, AccessMode::Read);
        }

        let then_fragment = if let Some(body) = if_stmt.body() {
            self.build_block_fragment(body)
        } else {
            self.make_nop_fragment(Some(if_stmt.span()))
        };

        let else_fragment = match if_stmt.alternative() {
            Some(IfAlt::If(else_if)) => self.build_stmt_fragment(Stmt::If(else_if)),
            Some(IfAlt::Block(else_block)) => self.build_block_fragment(else_block),
            None => self.make_nop_fragment(Some(if_stmt.span())),
        };

        self.cfg
            .add_edge(cond_node, then_fragment.entry, EdgeKind::TrueBranch);
        self.cfg
            .add_edge(cond_node, else_fragment.entry, EdgeKind::FalseBranch);

        let mut nodes = vec![cond_node];
        nodes.extend(then_fragment.nodes.iter().copied());
        nodes.extend(else_fragment.nodes.iter().copied());

        let mut exits = then_fragment.exits;
        exits.extend(else_fragment.exits);

        Fragment {
            entry: cond_node,
            exits,
            nodes,
        }
    }

    fn build_while_fragment(&mut self, while_stmt: tolk_syntax::While<'_>) -> Fragment {
        let cond_span = while_stmt.condition().map(|cond| cond.span());
        let cond_node = self.cfg.add_node(
            FlowNodeKind::Condition,
            cond_span.or_else(|| Some(while_stmt.span())),
        );

        if let Some(condition) = while_stmt.condition() {
            self.collect_expr_into_node(cond_node, condition, AccessMode::Read);
        }

        let after_loop = self
            .cfg
            .add_node(FlowNodeKind::Join, Some(while_stmt.span()));

        self.loops.push(LoopContext {
            break_target: after_loop,
            continue_target: cond_node,
        });

        let body_fragment = if let Some(body) = while_stmt.body() {
            self.build_block_fragment(body)
        } else {
            self.make_nop_fragment(Some(while_stmt.span()))
        };

        self.loops.pop();

        self.cfg
            .add_edge(cond_node, body_fragment.entry, EdgeKind::TrueBranch);
        self.cfg
            .add_edge(cond_node, after_loop, EdgeKind::FalseBranch);

        for exit in &body_fragment.exits {
            self.cfg.add_edge(*exit, cond_node, EdgeKind::LoopBack);
        }

        let mut nodes = vec![cond_node, after_loop];
        nodes.extend(body_fragment.nodes.iter().copied());

        Fragment {
            entry: cond_node,
            exits: vec![after_loop],
            nodes,
        }
    }

    fn build_repeat_fragment(&mut self, repeat_stmt: tolk_syntax::Repeat<'_>) -> Fragment {
        let count_span = repeat_stmt.count().map(|count| count.span());
        let count_node = self.cfg.add_node(
            FlowNodeKind::Condition,
            count_span.or_else(|| Some(repeat_stmt.span())),
        );

        if let Some(count) = repeat_stmt.count() {
            self.collect_expr_into_node(count_node, count, AccessMode::Read);
        }

        let after_loop = self
            .cfg
            .add_node(FlowNodeKind::Join, Some(repeat_stmt.span()));

        self.loops.push(LoopContext {
            break_target: after_loop,
            continue_target: count_node,
        });

        let body_fragment = if let Some(body) = repeat_stmt.body() {
            self.build_block_fragment(body)
        } else {
            self.make_nop_fragment(Some(repeat_stmt.span()))
        };

        self.loops.pop();

        self.cfg
            .add_edge(count_node, body_fragment.entry, EdgeKind::TrueBranch);
        self.cfg
            .add_edge(count_node, after_loop, EdgeKind::FalseBranch);

        for exit in &body_fragment.exits {
            self.cfg.add_edge(*exit, count_node, EdgeKind::LoopBack);
        }

        let mut nodes = vec![count_node, after_loop];
        nodes.extend(body_fragment.nodes.iter().copied());

        Fragment {
            entry: count_node,
            exits: vec![after_loop],
            nodes,
        }
    }

    fn build_do_while_fragment(&mut self, do_while_stmt: tolk_syntax::DoWhile<'_>) -> Fragment {
        let cond_span = do_while_stmt.condition().map(|cond| cond.span());
        let cond_node = self.cfg.add_node(
            FlowNodeKind::Condition,
            cond_span.or_else(|| Some(do_while_stmt.span())),
        );

        if let Some(condition) = do_while_stmt.condition() {
            self.collect_expr_into_node(cond_node, condition, AccessMode::Read);
        }

        let after_loop = self
            .cfg
            .add_node(FlowNodeKind::Join, Some(do_while_stmt.span()));

        self.loops.push(LoopContext {
            break_target: after_loop,
            continue_target: cond_node,
        });

        let body_fragment = if let Some(body) = do_while_stmt.body() {
            self.build_block_fragment(body)
        } else {
            self.make_nop_fragment(Some(do_while_stmt.span()))
        };

        self.loops.pop();

        for exit in &body_fragment.exits {
            self.cfg.add_edge(*exit, cond_node, EdgeKind::Unconditional);
        }

        self.cfg
            .add_edge(cond_node, body_fragment.entry, EdgeKind::LoopBack);
        self.cfg
            .add_edge(cond_node, after_loop, EdgeKind::FalseBranch);

        let mut nodes = vec![cond_node, after_loop];
        nodes.extend(body_fragment.nodes.iter().copied());

        Fragment {
            entry: body_fragment.entry,
            exits: vec![after_loop],
            nodes,
        }
    }

    fn build_try_catch_fragment(&mut self, try_catch_stmt: tolk_syntax::TryCatch<'_>) -> Fragment {
        let catch_fragment = if let Some(catch_clause) = try_catch_stmt.catch() {
            self.build_catch_fragment(catch_clause)
        } else {
            self.make_nop_fragment(Some(try_catch_stmt.span()))
        };

        self.exception_targets.push(catch_fragment.entry);
        let try_fragment = if let Some(try_body) = try_catch_stmt.body() {
            self.build_block_fragment(try_body)
        } else {
            self.make_nop_fragment(Some(try_catch_stmt.span()))
        };
        self.exception_targets.pop();

        for node in &try_fragment.nodes {
            if self.node_may_throw(*node) {
                self.cfg
                    .add_edge(*node, catch_fragment.entry, EdgeKind::Exceptional);
            }
        }

        let join = self
            .cfg
            .add_node(FlowNodeKind::Join, Some(try_catch_stmt.span()));

        for exit in &try_fragment.exits {
            self.cfg.add_edge(*exit, join, EdgeKind::Unconditional);
        }
        for exit in &catch_fragment.exits {
            self.cfg.add_edge(*exit, join, EdgeKind::Unconditional);
        }

        let mut nodes = vec![join];
        nodes.extend(try_fragment.nodes.iter().copied());
        nodes.extend(catch_fragment.nodes.iter().copied());

        Fragment {
            entry: try_fragment.entry,
            exits: vec![join],
            nodes,
        }
    }

    fn build_catch_fragment(&mut self, catch_clause: tolk_syntax::CatchClause<'_>) -> Fragment {
        let catch_binding = self
            .cfg
            .add_node(FlowNodeKind::CatchBinding, Some(catch_clause.span()));

        if let Some(collector) = &self.collector {
            let writes = &mut self.cfg.node_mut(catch_binding).writes;
            if let Some(var1) = catch_clause.catch_var1() {
                collector.collect_definition_ident(var1.syntax(), writes);
            }
            if let Some(var2) = catch_clause.catch_var2() {
                collector.collect_definition_ident(var2.syntax(), writes);
            }
        }

        let body_fragment = if let Some(catch_body) = catch_clause.body() {
            self.build_block_fragment(catch_body)
        } else {
            self.make_nop_fragment(Some(catch_clause.span()))
        };

        self.cfg
            .add_edge(catch_binding, body_fragment.entry, EdgeKind::Unconditional);

        let mut nodes = vec![catch_binding];
        nodes.extend(body_fragment.nodes.iter().copied());

        Fragment {
            entry: catch_binding,
            exits: body_fragment.exits,
            nodes,
        }
    }

    fn build_return_fragment(&mut self, return_stmt: tolk_syntax::Return<'_>) -> Fragment {
        let node = self
            .cfg
            .add_node(FlowNodeKind::Return, Some(return_stmt.span()));

        if let Some(expr) = return_stmt.expr() {
            self.collect_expr_into_node(node, expr, AccessMode::Read);
        }

        self.cfg.add_edge(node, self.cfg.exit(), EdgeKind::Return);
        Fragment::terminal(node)
    }

    fn build_throw_fragment(&mut self, throw_stmt: tolk_syntax::Throw<'_>) -> Fragment {
        let node = self
            .cfg
            .add_node(FlowNodeKind::Throw, Some(throw_stmt.span()));

        if let Some(expr) = throw_stmt.expr() {
            self.collect_expr_into_node(node, expr, AccessMode::Read);
        }

        if let Some(catch_target) = self.exception_targets.last().copied() {
            self.cfg.add_edge(node, catch_target, EdgeKind::Exceptional);
        } else {
            self.cfg.add_edge(node, self.cfg.exit(), EdgeKind::Throw);
        }

        Fragment::terminal(node)
    }

    fn build_assert_fragment(&mut self, assert_stmt: Assert<'_>) -> Fragment {
        let node = self
            .cfg
            .add_node(FlowNodeKind::Assert, Some(assert_stmt.span()));

        if let Some(condition) = assert_stmt.condition() {
            self.collect_expr_into_node(node, condition, AccessMode::Read);
        }
        if let Some(exc) = assert_stmt.expr() {
            self.collect_expr_into_node(node, exc, AccessMode::Read);
        }

        if let Some(catch_target) = self.exception_targets.last().copied() {
            self.cfg.add_edge(node, catch_target, EdgeKind::Exceptional);
        } else {
            self.cfg.add_edge(node, self.cfg.exit(), EdgeKind::Throw);
        }

        Fragment::single(node)
    }

    fn build_break_fragment(&mut self, break_stmt: tolk_syntax::Break<'_>) -> Fragment {
        let node = self
            .cfg
            .add_node(FlowNodeKind::Break, Some(break_stmt.span()));

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg
                .add_edge(node, loop_ctx.break_target, EdgeKind::Break);
        } else {
            self.cfg.add_edge(node, self.cfg.exit(), EdgeKind::Break);
        }

        Fragment::terminal(node)
    }

    fn build_continue_fragment(&mut self, continue_stmt: tolk_syntax::Continue<'_>) -> Fragment {
        let node = self
            .cfg
            .add_node(FlowNodeKind::Continue, Some(continue_stmt.span()));

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg
                .add_edge(node, loop_ctx.continue_target, EdgeKind::Continue);
        } else {
            self.cfg.add_edge(node, self.cfg.exit(), EdgeKind::Continue);
        }

        Fragment::terminal(node)
    }

    fn build_match_stmt_fragment(&mut self, match_stmt: tolk_syntax::MatchStmt<'_>) -> Fragment {
        let Some(match_expr) = match_stmt.expr() else {
            return self.make_nop_fragment(Some(match_stmt.span()));
        };
        self.build_match_expr_fragment(match_expr)
    }

    fn build_match_expr_fragment(&mut self, match_expr: Match<'_>) -> Fragment {
        let dispatch = self
            .cfg
            .add_node(FlowNodeKind::Condition, Some(match_expr.span()));

        if let Some(subject) = match_expr.expr() {
            self.collect_expr_into_node(dispatch, subject, AccessMode::Read);
        }

        let join = self
            .cfg
            .add_node(FlowNodeKind::Join, Some(match_expr.span()));

        let mut pending_fail = Some(dispatch);
        let mut nodes = vec![dispatch, join];
        let mut arm_exits = Vec::new();

        for arm in match_expr.arms() {
            let pattern = arm.pattern();
            let pattern_span = match pattern {
                MatchPattern::Type(ty) => Some(ty.span()),
                MatchPattern::Expr(expr) => Some(expr.span()),
                MatchPattern::Else => Some(arm.span()),
            };

            let pattern_node = self.cfg.add_node(FlowNodeKind::MatchPattern, pattern_span);
            self.collect_match_pattern_into_node(pattern_node, pattern);

            if let Some(prev) = pending_fail {
                let edge_kind = if prev == dispatch {
                    EdgeKind::Unconditional
                } else {
                    EdgeKind::FalseBranch
                };
                self.cfg.add_edge(prev, pattern_node, edge_kind);
            }

            let body_fragment = self.build_match_arm_body_fragment(arm.body(), arm.span());
            let to_body = if matches!(pattern, MatchPattern::Else) {
                EdgeKind::Unconditional
            } else {
                EdgeKind::TrueBranch
            };
            self.cfg
                .add_edge(pattern_node, body_fragment.entry, to_body);

            arm_exits.extend(body_fragment.exits);
            nodes.push(pattern_node);
            nodes.extend(body_fragment.nodes);

            pending_fail = if matches!(pattern, MatchPattern::Else) {
                None
            } else {
                Some(pattern_node)
            };
        }

        if let Some(last_fail) = pending_fail {
            self.cfg.add_edge(last_fail, join, EdgeKind::FalseBranch);
        }

        for exit in arm_exits {
            self.cfg.add_edge(exit, join, EdgeKind::Unconditional);
        }

        Fragment {
            entry: dispatch,
            exits: vec![join],
            nodes,
        }
    }

    fn build_match_arm_body_fragment(
        &mut self,
        body: Option<MatchArmBody<'_>>,
        fallback_span: tolk_resolver::Span,
    ) -> Fragment {
        let Some(body) = body else {
            return self.make_nop_fragment(Some(fallback_span));
        };

        match body {
            MatchArmBody::Block(block) => self.build_block_fragment(block),
            MatchArmBody::Return(ret) => self.build_return_fragment(ret),
            MatchArmBody::Throw(throw) => self.build_throw_fragment(throw),
            MatchArmBody::Expr(expr) => {
                let node = self.cfg.add_node(FlowNodeKind::Expr, Some(expr.span()));
                self.collect_expr_into_node(node, expr, AccessMode::Read);
                Fragment::single(node)
            }
        }
    }

    fn collect_expr_into_node(&mut self, node_id: NodeId, expr: Expr<'_>, mode: AccessMode) {
        if let Some(collector) = &self.collector {
            let node = self.cfg.node_mut(node_id);
            collector.collect_expr(expr, mode, &mut node.reads, &mut node.writes);
        }
    }

    fn collect_match_pattern_into_node(&mut self, node_id: NodeId, pattern: MatchPattern<'_>) {
        let Some(collector) = &self.collector else {
            return;
        };

        if let MatchPattern::Expr(expr) = pattern {
            let node = self.cfg.node_mut(node_id);
            collector.collect_expr(expr, AccessMode::Read, &mut node.reads, &mut node.writes);
        }
    }

    fn node_may_throw(&self, node: NodeId) -> bool {
        matches!(
            self.cfg.node(node).kind,
            FlowNodeKind::Expr
                | FlowNodeKind::Condition
                | FlowNodeKind::Assert
                | FlowNodeKind::Return
                | FlowNodeKind::Throw
                | FlowNodeKind::MatchPattern
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug)]
struct UseDefCollector<'idx> {
    uses_by_start: FxHashMap<u32, LocalDefId>,
    defs_by_start: FxHashMap<u32, LocalDefId>,
    _resolve_index: &'idx FileResolveIndex,
}

impl<'idx> UseDefCollector<'idx> {
    fn new(resolve_index: &'idx FileResolveIndex) -> Self {
        let mut uses_by_start = FxHashMap::default();
        for usage in &resolve_index.uses {
            if let Resolved::Local(local_id) = usage.resolved {
                uses_by_start.insert(usage.span.start, local_id);
            }
        }

        let mut defs_by_start = FxHashMap::default();
        for local in &resolve_index.locals {
            defs_by_start.insert(local.def_span.start, local.id);
        }

        Self {
            uses_by_start,
            defs_by_start,
            _resolve_index: resolve_index,
        }
    }

    fn collect_definition_ident(&self, ident: Node<'_>, writes: &mut FxHashSet<LocalDefId>) {
        let start = ident.start_byte() as u32;
        if let Some(local) = self
            .defs_by_start
            .get(&start)
            .copied()
            .or_else(|| self.uses_by_start.get(&start).copied())
        {
            writes.insert(local);
        }
    }

    fn collect_expr(
        &self,
        expr: Expr<'_>,
        mode: AccessMode,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        match expr {
            Expr::Ident(ident) => self.collect_ident_access(ident.syntax(), mode, reads, writes),
            Expr::Paren(Paren(paren)) => {
                let node = paren;
                if let Some(inner) = node.field::<Expr<'_>>("inner") {
                    self.collect_expr(inner, mode, reads, writes);
                }
            }
            Expr::Assign(assign) => self.collect_assign(assign, reads, writes),
            Expr::SetAssign(set_assign) => self.collect_set_assign(set_assign, reads, writes),
            Expr::VarDeclLhs(var_decl_lhs) => {
                if let Some(pattern) = var_decl_lhs.pattern() {
                    self.collect_var_pattern_writes(pattern, writes);
                }
            }
            Expr::DotAccess(dot_access) => self.collect_dot_access(dot_access, mode, reads, writes),
            Expr::Call(call) => self.collect_call(call, reads, writes),
            Expr::Instantiation(instantiation) => {
                if let Some(inner) = instantiation.expr() {
                    self.collect_expr(inner, AccessMode::Read, reads, writes);
                }
            }
            Expr::Ternary(ternary) => {
                if let Some(condition) = ternary.condition() {
                    self.collect_expr(condition, AccessMode::Read, reads, writes);
                }
                if let Some(consequence) = ternary.consequence() {
                    self.collect_expr(consequence, AccessMode::Read, reads, writes);
                }
                if let Some(alternative) = ternary.alternative() {
                    self.collect_expr(alternative, AccessMode::Read, reads, writes);
                }
            }
            Expr::Bin(bin) => {
                if let Some(left) = bin.left() {
                    self.collect_expr(left, AccessMode::Read, reads, writes);
                }
                if let Some(right) = bin.right() {
                    self.collect_expr(right, AccessMode::Read, reads, writes);
                }
            }
            Expr::Unary(unary) => {
                if let Some(argument) = unary.argument() {
                    self.collect_expr(argument, AccessMode::Read, reads, writes);
                }
            }
            Expr::Lazy(lazy) => {
                if let Some(inner) = lazy.expr() {
                    self.collect_expr(inner, AccessMode::Read, reads, writes);
                }
            }
            Expr::AsCast(as_cast) => {
                if let Some(inner) = as_cast.expr() {
                    self.collect_expr(inner, AccessMode::Read, reads, writes);
                }
            }
            Expr::IsType(is_type) => {
                if let Some(inner) = is_type.expr() {
                    self.collect_expr(inner, AccessMode::Read, reads, writes);
                }
            }
            Expr::NotNull(not_null) => {
                if let Some(inner) = not_null.inner() {
                    self.collect_expr(inner, AccessMode::Read, reads, writes);
                }
            }
            Expr::ObjectLit(object_lit) => {
                for arg in object_lit.arguments() {
                    self.collect_instance_arg(arg, reads, writes);
                }
            }
            Expr::Tensor(tensor) => {
                for element in tensor.elements() {
                    self.collect_expr(element, AccessMode::Read, reads, writes);
                }
            }
            Expr::Tuple(tuple) => {
                for element in tuple.elements() {
                    self.collect_expr(element, AccessMode::Read, reads, writes);
                }
            }
            Expr::Match(match_expr) => self.collect_match(match_expr, reads, writes),
            Expr::Lambda(_) => {
                // Lambda body is not executed eagerly and should not influence enclosing CFG node.
            }
            Expr::NumberLit(_)
            | Expr::StringLit(_)
            | Expr::BoolLit(_)
            | Expr::NullLit(_)
            | Expr::Underscore(_)
            | Expr::Unmapped(_) => {}
        }
    }

    fn collect_assign(
        &self,
        assign: Assign<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        if let Some(left) = assign.left() {
            self.collect_expr(left, AccessMode::Write, reads, writes);
        }
        if let Some(right) = assign.right() {
            self.collect_expr(right, AccessMode::Read, reads, writes);
        }
    }

    fn collect_set_assign(
        &self,
        assign: SetAssign<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        if let Some(left) = assign.left() {
            self.collect_expr(left, AccessMode::ReadWrite, reads, writes);
        }
        if let Some(right) = assign.right() {
            self.collect_expr(right, AccessMode::Read, reads, writes);
        }
    }

    fn collect_dot_access(
        &self,
        dot_access: DotAccess<'_>,
        mode: AccessMode,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        let obj_mode = match mode {
            AccessMode::Read => AccessMode::Read,
            AccessMode::Write | AccessMode::ReadWrite => AccessMode::ReadWrite,
        };

        if let Some(obj) = dot_access.obj() {
            self.collect_expr(obj, obj_mode, reads, writes);
        }
    }

    fn collect_call(
        &self,
        call: Call<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        if let Some(callee) = call.callee() {
            self.collect_expr(callee, AccessMode::Read, reads, writes);
        }

        for argument in call.arguments() {
            if let Some(expr) = argument.expr() {
                let mode = if argument.mutate() {
                    AccessMode::ReadWrite
                } else {
                    AccessMode::Read
                };
                self.collect_expr(expr, mode, reads, writes);
            }
        }
    }

    fn collect_instance_arg(
        &self,
        arg: InstanceArg<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        if let Some(value) = arg.value() {
            self.collect_expr(value, AccessMode::Read, reads, writes);
            return;
        }

        if let Some(name) = arg.name() {
            self.collect_ident_access(name.syntax(), AccessMode::Read, reads, writes);
        }
    }

    fn collect_match(
        &self,
        match_expr: Match<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        if let Some(subject) = match_expr.expr() {
            self.collect_expr(subject, AccessMode::Read, reads, writes);
        }

        for arm in match_expr.arms() {
            match arm.pattern() {
                MatchPattern::Type(_) => {}
                MatchPattern::Expr(expr) => {
                    self.collect_expr(expr, AccessMode::Read, reads, writes)
                }
                MatchPattern::Else => {}
            }

            if let Some(body) = arm.body() {
                self.collect_match_arm_body(body, reads, writes);
            }
        }
    }

    fn collect_match_arm_body(
        &self,
        body: MatchArmBody<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        match body {
            MatchArmBody::Block(block) => {
                for stmt in block.stmts() {
                    self.collect_stmt_inline(stmt, reads, writes);
                }
            }
            MatchArmBody::Return(ret) => {
                if let Some(expr) = ret.expr() {
                    self.collect_expr(expr, AccessMode::Read, reads, writes);
                }
            }
            MatchArmBody::Throw(throw_stmt) => {
                if let Some(expr) = throw_stmt.expr() {
                    self.collect_expr(expr, AccessMode::Read, reads, writes);
                }
            }
            MatchArmBody::Expr(expr) => self.collect_expr(expr, AccessMode::Read, reads, writes),
        }
    }

    fn collect_stmt_inline(
        &self,
        stmt: Stmt<'_>,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        match stmt {
            Stmt::Block(block) => {
                for nested in block.stmts() {
                    self.collect_stmt_inline(nested, reads, writes);
                }
            }
            Stmt::If(if_stmt) => {
                if let Some(cond) = if_stmt.condition() {
                    self.collect_expr(cond, AccessMode::Read, reads, writes);
                }
                if let Some(body) = if_stmt.body() {
                    for nested in body.stmts() {
                        self.collect_stmt_inline(nested, reads, writes);
                    }
                }
                if let Some(alt) = if_stmt.alternative() {
                    match alt {
                        IfAlt::If(else_if) => {
                            self.collect_stmt_inline(Stmt::If(else_if), reads, writes)
                        }
                        IfAlt::Block(else_block) => {
                            for nested in else_block.stmts() {
                                self.collect_stmt_inline(nested, reads, writes);
                            }
                        }
                    }
                }
            }
            Stmt::While(while_stmt) => {
                if let Some(cond) = while_stmt.condition() {
                    self.collect_expr(cond, AccessMode::Read, reads, writes);
                }
                if let Some(body) = while_stmt.body() {
                    for nested in body.stmts() {
                        self.collect_stmt_inline(nested, reads, writes);
                    }
                }
            }
            Stmt::Repeat(repeat_stmt) => {
                if let Some(count) = repeat_stmt.count() {
                    self.collect_expr(count, AccessMode::Read, reads, writes);
                }
                if let Some(body) = repeat_stmt.body() {
                    for nested in body.stmts() {
                        self.collect_stmt_inline(nested, reads, writes);
                    }
                }
            }
            Stmt::DoWhile(do_while) => {
                if let Some(body) = do_while.body() {
                    for nested in body.stmts() {
                        self.collect_stmt_inline(nested, reads, writes);
                    }
                }
                if let Some(cond) = do_while.condition() {
                    self.collect_expr(cond, AccessMode::Read, reads, writes);
                }
            }
            Stmt::TryCatch(try_catch) => {
                if let Some(body) = try_catch.body() {
                    for nested in body.stmts() {
                        self.collect_stmt_inline(nested, reads, writes);
                    }
                }
                if let Some(catch_clause) = try_catch.catch() {
                    if let Some(var1) = catch_clause.catch_var1() {
                        self.collect_definition_ident(var1.syntax(), writes);
                    }
                    if let Some(var2) = catch_clause.catch_var2() {
                        self.collect_definition_ident(var2.syntax(), writes);
                    }
                    if let Some(catch_body) = catch_clause.body() {
                        for nested in catch_body.stmts() {
                            self.collect_stmt_inline(nested, reads, writes);
                        }
                    }
                }
            }
            Stmt::Return(ret) => {
                if let Some(expr) = ret.expr() {
                    self.collect_expr(expr, AccessMode::Read, reads, writes);
                }
            }
            Stmt::Throw(throw_stmt) => {
                if let Some(expr) = throw_stmt.expr() {
                    self.collect_expr(expr, AccessMode::Read, reads, writes);
                }
            }
            Stmt::Assert(assert_stmt) => {
                if let Some(cond) = assert_stmt.condition() {
                    self.collect_expr(cond, AccessMode::Read, reads, writes);
                }
                if let Some(exc) = assert_stmt.expr() {
                    self.collect_expr(exc, AccessMode::Read, reads, writes);
                }
            }
            Stmt::Match(match_stmt) => {
                if let Some(match_expr) = match_stmt.expr() {
                    self.collect_match(match_expr, reads, writes);
                }
            }
            Stmt::ExprStmt(expr_stmt) => {
                if let Some(expr) = expr_stmt.expr() {
                    self.collect_expr(expr, AccessMode::Read, reads, writes);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::EmptyStmt(_) | Stmt::Unmapped(_) => {}
        }
    }

    fn collect_var_pattern_writes(
        &self,
        pattern: VarDeclPattern<'_>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        match pattern {
            VarDeclPattern::TupleVars(tuple_vars) => {
                for nested in tuple_vars.vars() {
                    self.collect_var_pattern_writes(nested, writes);
                }
            }
            VarDeclPattern::TensorVars(tensor_vars) => {
                for nested in tensor_vars.vars() {
                    self.collect_var_pattern_writes(nested, writes);
                }
            }
            VarDeclPattern::VarDecl(var_decl) => {
                if let Some(name) = var_decl.name() {
                    if var_decl.is_redefinition() {
                        let start = name.syntax().start_byte() as u32;
                        if let Some(local) = self.uses_by_start.get(&start).copied() {
                            writes.insert(local);
                        } else if let Some(local) = self.defs_by_start.get(&start).copied() {
                            writes.insert(local);
                        }
                    } else {
                        self.collect_definition_ident(name.syntax(), writes);
                    }
                }
            }
        }
    }

    fn collect_ident_access(
        &self,
        ident: Node<'_>,
        mode: AccessMode,
        reads: &mut FxHashSet<LocalDefId>,
        writes: &mut FxHashSet<LocalDefId>,
    ) {
        let start = ident.start_byte() as u32;
        let local = self.uses_by_start.get(&start).copied();
        let Some(local) = local else {
            return;
        };

        match mode {
            AccessMode::Read => {
                reads.insert(local);
            }
            AccessMode::Write => {
                writes.insert(local);
            }
            AccessMode::ReadWrite => {
                reads.insert(local);
                writes.insert(local);
            }
        }
    }
}
