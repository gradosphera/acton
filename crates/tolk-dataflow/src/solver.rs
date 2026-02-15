use crate::cfg::{ControlFlowGraph, NodeId};
use std::collections::VecDeque;

/// Direction of dataflow propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// User-extensible dataflow analysis contract.
pub trait DataflowAnalysis {
    type State: Clone + Eq;

    /// Forward analyses consume predecessors and produce successors.
    /// Backward analyses consume successors and produce predecessors.
    fn direction(&self) -> Direction;

    /// Bottom element of the lattice.
    fn bottom(&self, cfg: &ControlFlowGraph) -> Self::State;

    /// Boundary value on CFG entry (forward) or exit (backward).
    fn boundary(&self, cfg: &ControlFlowGraph) -> Self::State;

    /// Join `other` into `into`. Returns `true` if `into` changed.
    fn merge(&self, into: &mut Self::State, other: &Self::State) -> bool;

    /// Transfer function for a single node.
    fn transfer(&self, cfg: &ControlFlowGraph, node: NodeId, state: &Self::State) -> Self::State;
}

/// Optional solver knobs.
#[derive(Debug, Clone, Copy)]
pub struct SolverConfig {
    pub max_iterations: usize,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100_000,
        }
    }
}

/// Per-node in/out lattice values.
#[derive(Debug, Clone)]
pub struct DataflowResult<State> {
    pub in_state: Vec<State>,
    pub out_state: Vec<State>,
    pub iterations: usize,
    pub converged: bool,
}

impl<State> DataflowResult<State> {
    #[must_use]
    pub fn in_at(&self, node: NodeId) -> &State {
        &self.in_state[node.index()]
    }

    #[must_use]
    pub fn out_at(&self, node: NodeId) -> &State {
        &self.out_state[node.index()]
    }
}

/// Runs worklist fixed-point analysis with default settings.
pub fn solve<A: DataflowAnalysis>(
    cfg: &ControlFlowGraph,
    analysis: &A,
) -> DataflowResult<A::State> {
    solve_with_config(cfg, analysis, SolverConfig::default())
}

/// Runs worklist fixed-point analysis with custom settings.
pub fn solve_with_config<A: DataflowAnalysis>(
    cfg: &ControlFlowGraph,
    analysis: &A,
    config: SolverConfig,
) -> DataflowResult<A::State> {
    let node_count = cfg.node_count();
    let bottom = analysis.bottom(cfg);

    let mut in_state = vec![bottom.clone(); node_count];
    let mut out_state = vec![bottom; node_count];

    match analysis.direction() {
        Direction::Forward => {
            in_state[cfg.entry().index()] = analysis.boundary(cfg);
        }
        Direction::Backward => {
            out_state[cfg.exit().index()] = analysis.boundary(cfg);
        }
    }

    let mut queue = VecDeque::new();
    let mut in_queue = vec![false; node_count];

    for node in initial_order(cfg, analysis.direction()) {
        queue.push_back(node);
        in_queue[node.index()] = true;
    }

    let mut iterations = 0usize;
    while let Some(node) = queue.pop_front() {
        in_queue[node.index()] = false;
        iterations += 1;

        if iterations > config.max_iterations {
            return DataflowResult {
                in_state,
                out_state,
                iterations,
                converged: false,
            };
        }

        match analysis.direction() {
            Direction::Forward => {
                let mut merged_in = if node == cfg.entry() {
                    analysis.boundary(cfg)
                } else {
                    analysis.bottom(cfg)
                };

                for pred in cfg.predecessors(node) {
                    analysis.merge(&mut merged_in, &out_state[pred.from.index()]);
                }

                let in_changed = merged_in != in_state[node.index()];
                if in_changed {
                    in_state[node.index()] = merged_in;
                }

                let next_out = analysis.transfer(cfg, node, &in_state[node.index()]);
                let out_changed = next_out != out_state[node.index()];
                if out_changed {
                    out_state[node.index()] = next_out;
                }

                if out_changed {
                    for succ in cfg.successors(node) {
                        if !in_queue[succ.to.index()] {
                            queue.push_back(succ.to);
                            in_queue[succ.to.index()] = true;
                        }
                    }
                }
            }
            Direction::Backward => {
                let mut merged_out = if node == cfg.exit() {
                    analysis.boundary(cfg)
                } else {
                    analysis.bottom(cfg)
                };

                for succ in cfg.successors(node) {
                    analysis.merge(&mut merged_out, &in_state[succ.to.index()]);
                }

                let out_changed = merged_out != out_state[node.index()];
                if out_changed {
                    out_state[node.index()] = merged_out;
                }

                let next_in = analysis.transfer(cfg, node, &out_state[node.index()]);
                let in_changed = next_in != in_state[node.index()];
                if in_changed {
                    in_state[node.index()] = next_in;
                }

                if in_changed {
                    for pred in cfg.predecessors(node) {
                        if !in_queue[pred.from.index()] {
                            queue.push_back(pred.from);
                            in_queue[pred.from.index()] = true;
                        }
                    }
                }
            }
        }
    }

    DataflowResult {
        in_state,
        out_state,
        iterations,
        converged: true,
    }
}

fn initial_order(cfg: &ControlFlowGraph, direction: Direction) -> Vec<NodeId> {
    let mut order = Vec::with_capacity(cfg.node_count());
    match direction {
        Direction::Forward => {
            for index in 0..cfg.node_count() {
                order.push(NodeId(index));
            }
        }
        Direction::Backward => {
            for index in (0..cfg.node_count()).rev() {
                order.push(NodeId(index));
            }
        }
    }
    order
}
