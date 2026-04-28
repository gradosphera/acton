use rustc_hash::FxHashSet;
use tolk_dataflow::cfg::{ControlFlowGraph, EdgeKind, NodeId};
use tolk_resolver::Span;
use tolk_resolver::file_index::{FileId, SymbolId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitializationSite {
    pub file_id: FileId,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InitializationSummary {
    pub is_guaranteed: bool,
    pub has_any_initialization: bool,
    pub sample_site: Option<InitializationSite>,
}

pub trait SummaryProvider {
    fn summary_for(&mut self, symbol_id: SymbolId) -> InitializationSummary;
}

/// Single sink report for random value usage potentially reachable
/// without guaranteed random initialization.
#[derive(Debug, Clone)]
pub struct UninitializedRandomUsage {
    pub node: NodeId,
    pub span: Option<Span>,
    pub conditional_initialization_site: Option<InitializationSite>,
}

#[derive(Debug, Clone)]
pub struct RandomInitializationReport {
    pub issues: Vec<UninitializedRandomUsage>,
}

fn node_guarantees_initialization(
    cfg: &ControlFlowGraph,
    node_id: NodeId,
    summary_provider: &mut impl SummaryProvider,
) -> bool {
    let node = cfg.node(node_id);
    if node.taint.has_random_initialize_call {
        return true;
    }

    for symbol_id in &node.taint.called_global_symbols {
        if summary_provider.summary_for(*symbol_id).is_guaranteed {
            return true;
        }
    }

    false
}

fn node_initialization_site(
    cfg: &ControlFlowGraph,
    current_file_id: FileId,
    node_id: NodeId,
    summary_provider: &mut impl SummaryProvider,
) -> Option<InitializationSite> {
    let node = cfg.node(node_id);
    if node.taint.has_random_initialize_call
        && let Some(span) = node.span
    {
        return Some(InitializationSite {
            file_id: current_file_id,
            span,
        });
    }

    for symbol_id in &node.taint.called_global_symbols {
        if let Some(site) = summary_provider.summary_for(*symbol_id).sample_site {
            return Some(site);
        }
    }

    None
}

fn path_without_initialization_exists(
    cfg: &ControlFlowGraph,
    start_nodes: &[NodeId],
    summary_provider: &mut impl SummaryProvider,
) -> bool {
    let mut stack = Vec::from(start_nodes);
    let mut visited = FxHashSet::default();

    while let Some(node_id) = stack.pop() {
        if !visited.insert(node_id) {
            continue;
        }

        if node_id == cfg.entry() {
            return true;
        }

        if node_guarantees_initialization(cfg, node_id, summary_provider) {
            continue;
        }

        for edge in cfg.predecessors(node_id) {
            stack.push(edge.from);
        }
    }

    false
}

fn find_initialization_site_on_any_path(
    cfg: &ControlFlowGraph,
    current_file_id: FileId,
    start_nodes: &[NodeId],
    summary_provider: &mut impl SummaryProvider,
) -> Option<InitializationSite> {
    let mut stack = start_nodes
        .iter()
        .copied()
        .map(|node_id| (node_id, None))
        .collect::<Vec<_>>();
    let mut visited = FxHashSet::default();

    while let Some((node_id, seen_site)) = stack.pop() {
        let seen_site = seen_site
            .or_else(|| node_initialization_site(cfg, current_file_id, node_id, summary_provider));
        let seen_initialization = seen_site.is_some();

        if !visited.insert((node_id, seen_initialization)) {
            continue;
        }

        if node_id == cfg.entry() {
            if let Some(site) = seen_site {
                return Some(site);
            }
            continue;
        }

        for edge in cfg.predecessors(node_id) {
            stack.push((edge.from, seen_site));
        }
    }

    None
}

fn normal_exit_predecessors(cfg: &ControlFlowGraph) -> Vec<NodeId> {
    cfg.predecessors(cfg.exit())
        .filter(|edge| matches!(edge.kind, EdgeKind::Unconditional | EdgeKind::Return))
        .map(|edge| edge.from)
        .collect()
}

fn sink_predecessors(cfg: &ControlFlowGraph, sink: NodeId) -> Vec<NodeId> {
    cfg.predecessors(sink).map(|edge| edge.from).collect()
}

pub fn function_summary(
    cfg: &ControlFlowGraph,
    current_file_id: FileId,
    summary_provider: &mut impl SummaryProvider,
) -> InitializationSummary {
    let starts = normal_exit_predecessors(cfg);
    if starts.is_empty() {
        return InitializationSummary {
            is_guaranteed: true,
            has_any_initialization: false,
            sample_site: None,
        };
    }

    let sample_site =
        find_initialization_site_on_any_path(cfg, current_file_id, &starts, summary_provider);

    InitializationSummary {
        is_guaranteed: !path_without_initialization_exists(cfg, &starts, summary_provider),
        has_any_initialization: sample_site.is_some(),
        sample_site,
    }
}

/// Finds random value sinks that are reachable without guaranteed initialization.
#[must_use]
pub fn find_uninitialized_random_usage(
    cfg: &ControlFlowGraph,
    current_file_id: FileId,
    summary_provider: &mut impl SummaryProvider,
) -> Vec<UninitializedRandomUsage> {
    let mut issues = Vec::new();

    for node in cfg.nodes() {
        if !node.taint.has_random_value_sink {
            continue;
        }

        let starts = sink_predecessors(cfg, node.id);
        if starts.is_empty() {
            if node.id == cfg.entry() {
                issues.push(UninitializedRandomUsage {
                    node: node.id,
                    span: node.span,
                    conditional_initialization_site: None,
                });
            }
            continue;
        }

        if path_without_initialization_exists(cfg, &starts, summary_provider) {
            issues.push(UninitializedRandomUsage {
                node: node.id,
                span: node.span,
                conditional_initialization_site: find_initialization_site_on_any_path(
                    cfg,
                    current_file_id,
                    &starts,
                    summary_provider,
                ),
            });
        }
    }

    issues
}

#[must_use]
pub fn run(
    cfg: &ControlFlowGraph,
    current_file_id: FileId,
    summary_provider: &mut impl SummaryProvider,
) -> RandomInitializationReport {
    let issues = find_uninitialized_random_usage(cfg, current_file_id, summary_provider);
    RandomInitializationReport { issues }
}
