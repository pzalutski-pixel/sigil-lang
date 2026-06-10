//! Control Flow Graph Module
//!
//! Builds and analyzes Control Flow Graphs per Sigil Language Reference Section 14.
//! Used for "all paths" analysis required by:
//! - Section 14.1: Initialization (LOAD requires prior STORE on all paths)
//! - Section 14.1: Lifetime (handle valid only within owning scope)
//! - Section 14.1: No leaks (every ALLOC has FREE or is scoped)
//! - Section 14.4: Paths terminate (all paths reach END or loop)
//! - Section 14.6: writes_output (all paths write to outputs)

use std::collections::{HashMap, HashSet};
use crate::ast::{Node, NodeKind, Expr};

// =============================================================================
// CFG Types
// =============================================================================

/// Unique identifier for a basic block
pub type BlockId = usize;

/// A basic block is a sequence of statements with:
/// - No branches in the middle
/// - Single entry point (first statement)
/// - Single exit point (last statement, which may branch)
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique identifier
    pub id: BlockId,
    /// Statements in this block (indices into original node list)
    pub statements: Vec<usize>,
    /// Label name if this block starts with a LABEL
    pub label: Option<String>,
    /// Successor block IDs
    pub successors: Vec<BlockId>,
    /// Predecessor block IDs
    pub predecessors: Vec<BlockId>,
    /// Terminator type
    pub terminator: Terminator,
}

/// How a basic block ends
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    /// Falls through to next block
    Fallthrough,
    /// Unconditional jump to label
    Jump(String),
    /// Conditional branch
    Branch {
        true_label: String,
        false_label: String,
    },
    /// End of behavior (implicit return)
    Return,
}

/// Control Flow Graph
#[derive(Debug)]
pub struct CFG {
    /// All basic blocks
    pub blocks: Vec<BasicBlock>,
    /// Entry block ID
    pub entry: BlockId,
    /// Exit block IDs (blocks that terminate)
    pub exits: Vec<BlockId>,
    /// Label to block ID mapping
    #[allow(dead_code)]
    pub label_to_block: HashMap<String, BlockId>,
}

/// Flatten `SCOPE` blocks into the node stream so the CFG — and the path-based
/// analyses that index into the *same* node slice — can see the operations
/// **inside** a scope. Without this the CFG builder treats a `SCOPE` as one
/// opaque statement, and uninitialized-read / use-after-free / leak /
/// writes-output checks silently skip everything in the scope body.
///
/// A scope's directly-allocated handles are freed automatically at `END_SCOPE`,
/// so a synthetic `FREE` is appended for each scoped `ALLOC` (unless the body
/// already frees it explicitly), which keeps leak/ownership analysis honest
/// without false positives. Nested scopes are handled by recursion.
///
/// Limit: this models the common linear scope. A scope body that `JUMP`s out of
/// itself before `END_SCOPE` is not specially handled (the synthetic frees sit on
/// the fall-through path); Sigil leaves don't currently write that pattern.
pub fn flatten_scopes(nodes: &[Node]) -> Vec<Node> {
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        match &node.kind {
            NodeKind::Scope { nodes: inner } => {
                // This scope's directly-allocated handles (top-level ALLOC binds).
                let mut scoped_allocs: Vec<String> = inner.iter().filter_map(|n| {
                    if let NodeKind::Assignment { target, expr } = &n.kind {
                        if matches!(**expr, Expr::Alloc { .. }) {
                            return Some(target.clone());
                        }
                    }
                    None
                }).collect();
                // Don't double-free handles the body already frees explicitly.
                let freed: HashSet<&str> = inner.iter().filter_map(|n| {
                    if let NodeKind::Free(name) = &n.kind { Some(name.as_str()) } else { None }
                }).collect();
                scoped_allocs.retain(|a| !freed.contains(a.as_str()));

                // Recurse so deeper scopes are inlined (with their own frees).
                result.extend(flatten_scopes(inner));

                // Auto-free this scope's handles at scope exit (reverse order).
                for target in scoped_allocs.into_iter().rev() {
                    result.push(Node { span: node.span.clone(), kind: NodeKind::Free(target) });
                }
            }
            _ => result.push(node.clone()),
        }
    }
    result
}

impl CFG {
    /// Build CFG from AST nodes
    pub fn build(nodes: &[Node]) -> Self {
        let builder = CFGBuilder::new();
        builder.build(nodes)
    }

    /// Get all paths from entry to any exit
    /// Returns list of paths, where each path is a list of block IDs
    pub fn all_paths(&self) -> Vec<Vec<BlockId>> {
        let mut paths = Vec::new();
        let mut stack: Vec<(BlockId, Vec<BlockId>, HashSet<BlockId>)> = vec![(
            self.entry,
            vec![self.entry],
            HashSet::new(),
        )];

        while let Some((current, path, mut visited)) = stack.pop() {
            if visited.contains(&current) {
                // Loop detected - don't add as a path
                // Loops eventually exit through a non-loop path, which will be checked
                // Adding loop back-edges as "paths" causes false positive leak errors
                continue;
            }
            visited.insert(current);

            let block = &self.blocks[current];
            if block.successors.is_empty() || block.terminator == Terminator::Return {
                // Reached an exit
                paths.push(path);
            } else {
                for &succ in &block.successors {
                    let mut new_path = path.clone();
                    new_path.push(succ);
                    stack.push((succ, new_path, visited.clone()));
                }
            }
        }

        paths
    }

    /// Weakened, decidable termination check (Section 14.4): every block
    /// reachable from entry must be able to reach some exit block. This is the
    /// structural property from the theoretical foundations — it permits ordinary
    /// loops (which structurally connect to an exit) and rejects only control flow
    /// that can NEVER reach END (an exit-less infinite loop). It does NOT prove
    /// semantic termination (that's the halting problem); it proves the CFG has no
    /// dead-end with no path out. O(V+E).
    pub fn all_paths_terminate(&self) -> bool {
        // Forward reachability from entry.
        let mut reachable: HashSet<BlockId> = HashSet::new();
        let mut stack = vec![self.entry];
        while let Some(b) = stack.pop() {
            if !reachable.insert(b) {
                continue;
            }
            if let Some(block) = self.blocks.get(b) {
                for &s in &block.successors {
                    stack.push(s);
                }
            }
        }

        // Backward fixpoint: blocks from which an exit is reachable.
        let mut can_exit: HashSet<BlockId> = self.exits.iter().copied().collect();
        let mut changed = true;
        while changed {
            changed = false;
            for block in &self.blocks {
                if can_exit.contains(&block.id) {
                    continue;
                }
                if block.successors.iter().any(|s| can_exit.contains(s)) {
                    can_exit.insert(block.id);
                    changed = true;
                }
            }
        }

        // Every block reachable from entry must be able to reach an exit.
        reachable.iter().all(|b| can_exit.contains(b))
    }

    /// Get all statements on a path
    #[allow(dead_code)]
    pub fn statements_on_path<'a>(&'a self, path: &[BlockId], nodes: &'a [Node]) -> Vec<&'a Node> {
        let mut result = Vec::new();
        for &block_id in path {
            let block = &self.blocks[block_id];
            for &stmt_idx in &block.statements {
                if stmt_idx < nodes.len() {
                    result.push(&nodes[stmt_idx]);
                }
            }
        }
        result
    }
}

// =============================================================================
// CFG Builder
// =============================================================================

struct CFGBuilder {
    blocks: Vec<BasicBlock>,
    label_to_block: HashMap<String, BlockId>,
    current_block: BasicBlock,
    next_id: BlockId,
}

impl CFGBuilder {
    fn new() -> Self {
        CFGBuilder {
            blocks: Vec::new(),
            label_to_block: HashMap::new(),
            current_block: BasicBlock {
                id: 0,
                statements: Vec::new(),
                label: None,
                successors: Vec::new(),
                predecessors: Vec::new(),
                terminator: Terminator::Fallthrough,
            },
            next_id: 0,
        }
    }

    fn build(mut self, nodes: &[Node]) -> CFG {
        // First pass: identify labels and create blocks
        self.first_pass(nodes);

        // Second pass: connect blocks
        self.second_pass();

        // Identify entry and exits
        let entry = 0;
        let exits: Vec<BlockId> = self.blocks
            .iter()
            .filter(|b| b.successors.is_empty() || b.terminator == Terminator::Return)
            .map(|b| b.id)
            .collect();

        CFG {
            blocks: self.blocks,
            entry,
            exits,
            label_to_block: self.label_to_block,
        }
    }

    fn first_pass(&mut self, nodes: &[Node]) {
        for (idx, node) in nodes.iter().enumerate() {
            match &node.kind {
                NodeKind::Label(name) => {
                    // Start a new block for this label
                    if !self.current_block.statements.is_empty() {
                        // Finish current block
                        self.finish_block(Terminator::Fallthrough);
                    }
                    self.current_block.label = Some(name.clone());
                    self.label_to_block.insert(name.clone(), self.next_id);
                    self.current_block.statements.push(idx);
                }
                NodeKind::Jump(target) => {
                    self.current_block.statements.push(idx);
                    self.finish_block(Terminator::Jump(target.clone()));
                }
                NodeKind::Branch { true_label, false_label, .. } => {
                    self.current_block.statements.push(idx);
                    self.finish_block(Terminator::Branch {
                        true_label: true_label.clone(),
                        false_label: false_label.clone(),
                    });
                }
                NodeKind::Scope { nodes: _inner } => {
                    // For now, treat scope as inline statements
                    // A more complete implementation would track scope boundaries
                    self.current_block.statements.push(idx);
                    // Recursively process inner nodes would require flattening
                }
                _ => {
                    self.current_block.statements.push(idx);
                }
            }
        }

        // Finish the last block
        if !self.current_block.statements.is_empty() || self.blocks.is_empty() {
            self.finish_block(Terminator::Return);
        }
    }

    fn second_pass(&mut self) {
        // Connect blocks based on terminators
        for i in 0..self.blocks.len() {
            let terminator = self.blocks[i].terminator.clone();
            let block_id = self.blocks[i].id;

            match terminator {
                Terminator::Fallthrough => {
                    // Connect to next block if exists
                    if i + 1 < self.blocks.len() {
                        let next_id = self.blocks[i + 1].id;
                        self.blocks[i].successors.push(next_id);
                        self.blocks[i + 1].predecessors.push(block_id);
                    }
                }
                Terminator::Jump(ref target) => {
                    if let Some(&target_id) = self.label_to_block.get(target) {
                        self.blocks[i].successors.push(target_id);
                        if target_id < self.blocks.len() {
                            self.blocks[target_id].predecessors.push(block_id);
                        }
                    }
                }
                Terminator::Branch { ref true_label, ref false_label } => {
                    if let Some(&true_id) = self.label_to_block.get(true_label) {
                        self.blocks[i].successors.push(true_id);
                        if true_id < self.blocks.len() {
                            self.blocks[true_id].predecessors.push(block_id);
                        }
                    }
                    if let Some(&false_id) = self.label_to_block.get(false_label) {
                        self.blocks[i].successors.push(false_id);
                        if false_id < self.blocks.len() {
                            self.blocks[false_id].predecessors.push(block_id);
                        }
                    }
                }
                Terminator::Return => {
                    // No successors
                }
            }
        }
    }

    fn finish_block(&mut self, terminator: Terminator) {
        self.current_block.id = self.next_id;
        self.current_block.terminator = terminator;

        // Register label if present
        if let Some(ref label) = self.current_block.label {
            self.label_to_block.insert(label.clone(), self.next_id);
        }

        self.blocks.push(std::mem::replace(&mut self.current_block, BasicBlock {
            id: 0,
            statements: Vec::new(),
            label: None,
            successors: Vec::new(),
            predecessors: Vec::new(),
            terminator: Terminator::Fallthrough,
        }));

        self.next_id += 1;
    }
}

// =============================================================================
// Data Flow Analysis Framework
// =============================================================================

/// State at a program point for data flow analysis
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFlowState {
    /// Variables that have been initialized (STORE'd to) on this path
    pub initialized: HashSet<String>,
    /// Variables that have been freed on this path
    pub freed: HashSet<String>,
    /// Variables that are currently allocated (ALLOC'd but not FREE'd)
    pub allocated: HashSet<String>,
    /// Outputs that have been written on this path
    pub written_outputs: HashSet<String>,
}

impl DataFlowState {
    pub fn new() -> Self {
        DataFlowState {
            initialized: HashSet::new(),
            freed: HashSet::new(),
            allocated: HashSet::new(),
            written_outputs: HashSet::new(),
        }
    }

    /// Merge two states (union for "any path" properties)
    pub fn merge(&self, other: &DataFlowState) -> DataFlowState {
        DataFlowState {
            // For "must be initialized on all paths", we use intersection
            initialized: self.initialized.intersection(&other.initialized).cloned().collect(),
            // For freed, we track what's freed on any path (conservative)
            freed: self.freed.union(&other.freed).cloned().collect(),
            // For allocated, we track what might still be allocated
            allocated: self.allocated.union(&other.allocated).cloned().collect(),
            // For writes_output, we use intersection (must write on all paths)
            written_outputs: self.written_outputs.intersection(&other.written_outputs).cloned().collect(),
        }
    }

    /// Apply a node's effects to this state
    pub fn apply_node(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::Assignment { target, expr } => {
                // Check if this is an ALLOC
                if matches!(expr.as_ref(), Expr::Alloc { .. }) {
                    self.allocated.insert(target.clone());
                }
                self.initialized.insert(target.clone());
            }
            NodeKind::Store { target, .. } => {
                self.initialized.insert(target.clone());
                self.written_outputs.insert(target.clone());
            }
            NodeKind::Set { target, .. } => {
                self.written_outputs.insert(target.clone());
            }
            NodeKind::Free(name) => {
                self.freed.insert(name.clone());
                self.allocated.remove(name);
            }
            _ => {}
        }
    }
}

/// Perform data flow analysis on a CFG.
///
/// RESERVED — implemented but not yet wired in. This is the polynomial-time
/// dataflow pass intended to replace the exponential `all_paths()` enumeration
/// used by `validate_ownership_cfg`, once a behavior's branch count makes the
/// exponential pass a problem. Today's behaviors are small enough that the
/// exponential pass is fine, so this is kept as the basis
/// for that future migration rather than removed.
#[allow(dead_code)]
pub fn analyze_data_flow(cfg: &CFG, nodes: &[Node], outputs: &HashSet<String>) -> DataFlowAnalysisResult {
    let mut block_states: HashMap<BlockId, DataFlowState> = HashMap::new();
    let mut result = DataFlowAnalysisResult {
        uninitialized_reads: Vec::new(),
        leaked_allocations: Vec::new(),
        unwritten_outputs_paths: Vec::new(),
    };

    // Initialize entry block state
    block_states.insert(cfg.entry, DataFlowState::new());

    // Fixed-point iteration
    let mut changed = true;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 1000;

    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;

        for block in &cfg.blocks {
            // Compute input state from predecessors
            let input_state = if block.predecessors.is_empty() {
                DataFlowState::new()
            } else {
                let pred_states: Vec<&DataFlowState> = block.predecessors
                    .iter()
                    .filter_map(|&pred_id| block_states.get(&pred_id))
                    .collect();

                if pred_states.is_empty() {
                    DataFlowState::new()
                } else {
                    let mut merged = pred_states[0].clone();
                    for state in &pred_states[1..] {
                        merged = merged.merge(state);
                    }
                    merged
                }
            };

            // Apply statements in this block
            let mut current_state = input_state;
            for &stmt_idx in &block.statements {
                if stmt_idx < nodes.len() {
                    current_state.apply_node(&nodes[stmt_idx]);
                }
            }

            // Check if state changed
            if let Some(old_state) = block_states.get(&block.id) {
                if *old_state != current_state {
                    changed = true;
                }
            } else {
                changed = true;
            }

            block_states.insert(block.id, current_state);
        }
    }

    // Collect results from exit blocks
    for &exit_id in &cfg.exits {
        if let Some(state) = block_states.get(&exit_id) {
            // Check for leaked allocations
            for alloc in &state.allocated {
                result.leaked_allocations.push(alloc.clone());
            }

            // Check for unwritten outputs
            let unwritten: Vec<String> = outputs
                .iter()
                .filter(|o| !state.written_outputs.contains(*o))
                .cloned()
                .collect();
            if !unwritten.is_empty() {
                result.unwritten_outputs_paths.push(unwritten);
            }
        }
    }

    result
}

/// Results of data flow analysis
#[allow(dead_code)]
#[derive(Debug)]
pub struct DataFlowAnalysisResult {
    /// Variables read before being initialized on some path
    pub uninitialized_reads: Vec<String>,
    /// Variables allocated but not freed on some path
    pub leaked_allocations: Vec<String>,
    /// Outputs not written on some path (each inner Vec is outputs missing on that path)
    pub unwritten_outputs_paths: Vec<Vec<String>>,
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn build_cfg_from_source(source: &str) -> CFG {
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let nodes = &behavior.implementations.first().unwrap().nodes;
        CFG::build(nodes)
    }

    #[test]
    fn test_cfg_linear() {
        let source = r#"
BEHAVIOR linear

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH line1234

IMPLEMENTATION
  a = 1
  b = 2
  SET status 0
END
"#;
        let cfg = build_cfg_from_source(source);
        assert_eq!(cfg.blocks.len(), 1);
        assert_eq!(cfg.blocks[0].statements.len(), 3);
    }

    #[test]
    fn test_cfg_branch() {
        let source = r#"
BEHAVIOR branch-test

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH bran1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c true_path false_path

  LABEL true_path
    SET status 1
    JUMP done

  LABEL false_path
    SET status 0
    JUMP done

  LABEL done
END
"#;
        let cfg = build_cfg_from_source(source);
        // Should have 4 blocks: entry, true_path, false_path, done
        assert!(cfg.blocks.len() >= 4);

        // Entry block should have 2 successors
        assert_eq!(cfg.blocks[0].successors.len(), 2);
    }

    #[test]
    fn test_cfg_all_paths() {
        let source = r#"
BEHAVIOR paths-test

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH path1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c true_path false_path

  LABEL true_path
    SET status 1
    JUMP done

  LABEL false_path
    SET status 0
    JUMP done

  LABEL done
END
"#;
        let cfg = build_cfg_from_source(source);
        let paths = cfg.all_paths();

        // Should have 2 paths: entry->true_path->done, entry->false_path->done
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_terminate_linear() {
        let cfg = build_cfg_from_source(r#"
BEHAVIOR lin
CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output
HASH 00000000
IMPLEMENTATION
  SET status 0
END
"#);
        assert!(cfg.all_paths_terminate());
    }

    #[test]
    fn test_terminate_loop_with_exit() {
        // A loop that can branch out to END terminates structurally.
        let cfg = build_cfg_from_source(r#"
BEHAVIOR loopy
CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output
HASH 00000000
IMPLEMENTATION
  c = LOAD cond 8
  LABEL loop
    BRANCH c done body
  LABEL body
    SET status 1
    JUMP loop
  LABEL done
    SET status 0
END
"#);
        assert!(cfg.all_paths_terminate());
    }

    #[test]
    fn test_no_terminate_exitless_loop() {
        // An infinite loop with no way out does NOT terminate.
        let cfg = build_cfg_from_source(r#"
BEHAVIOR spin
CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output
HASH 00000000
IMPLEMENTATION
  LABEL loop
    SET status 0
    JUMP loop
END
"#);
        assert!(!cfg.all_paths_terminate());
    }
}
