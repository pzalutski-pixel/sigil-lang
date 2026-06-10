//! Semantic Analysis Module
//!
//! Validates AST against Sigil Language Reference Section 14 rules.
//! Runs after parsing, before code generation.
//!
//! Validation phases:
//! 1. Type validation (Section 14.2)
//! 2. Label validation (Section 14.4)
//! 3. Memory validation (Section 14.1)
//! 4. Contract validation (Section 14.3)
//! 5. Guarantee validation (Section 14.6)
//! 6. CFG-based data flow analysis (Section 14.1, 14.4, 14.6)

mod types;
mod context;

pub use types::{SemanticError, ErrorKind, SemanticWarning};
pub use context::{VarOrigin, HandleState};

use std::collections::{HashMap, HashSet};
use crate::ast::{Behavior, Node, NodeKind, Expr, Type, Parameter, Guarantee, Contract};
use crate::cfg::CFG;
use crate::hash;

use context::AnalysisContext;

// =============================================================================
// Semantic Analyzer
// =============================================================================

/// Main semantic analyzer
pub struct SemanticAnalyzer {
    errors: Vec<SemanticError>,
    warnings: Vec<SemanticWarning>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Analyze a behavior and return errors/warnings
    ///
    /// `dependencies` contains contracts of behaviors listed in REQUIRES,
    /// used for CALL argument validation per Section 14.3.
    /// `pattern_names` contains names of known patterns for SPAWN validation.
    pub fn analyze(
        &mut self,
        behavior: &Behavior,
        dependencies: &HashMap<String, Contract>,
        pattern_names: &HashSet<String>,
    ) -> Result<(), Vec<SemanticError>> {
        self.errors.clear();
        self.warnings.clear();

        let mut ctx = AnalysisContext::new(behavior, dependencies, pattern_names);

        // Phase 1: Validate contract
        self.validate_contract(behavior, &ctx);

        // Phase 2: Collect labels (first pass)
        self.collect_labels(behavior, &mut ctx);

        // Phase 3: Validate implementation/composition (linear pass)
        // NOTE: This pass now SKIPS double-free detection because it requires
        // CFG-aware analysis. We collect FREE operations and validate later.
        if let Some(ref impl_) = behavior.implementations.first() {
            self.validate_nodes(&impl_.nodes, &mut ctx);

            // Phase 3b: CFG-based ownership validation
            // This correctly handles branching (FREE in different paths is valid)
            self.validate_ownership_cfg(&impl_.nodes, &ctx);
        }
        if let Some(ref comp) = behavior.composition {
            self.validate_nodes(&comp.nodes, &mut ctx);
            self.validate_ownership_cfg(&comp.nodes, &ctx);
        }

        // Phase 3c: Termination (weakened, decidable) — Section 14.4. Every block
        // reachable from entry must be able to reach END; rejects exit-less loops.
        if let Some(ref impl_) = behavior.implementations.first() {
            self.check_termination(&impl_.nodes);
        }
        if let Some(ref comp) = behavior.composition {
            self.check_termination(&comp.nodes);
        }

        // Phase 3.5: Leaf/composite role separation (Section 6; DESIGN-MODEL 4.3).
        // A composite (COMPOSITION) only wires behaviors; it must not perform
        // primitive computation or own memory. A leaf (IMPLEMENTATION) is the
        // bottom of the call graph and must not CALL. NATIVE behaviors have no body.
        if let Some(ref comp) = behavior.composition {
            self.check_composite_purity(&comp.nodes);
        } else if let Some(ref impl_) = behavior.implementations.first() {
            self.check_leaf_no_call(&impl_.nodes);
        }

        // Phase 3.6: Output consumption (Section 7.8). In a composition every
        // output a CALL produces must be consumed — routed onward, branched on,
        // written to an OUTPUT, or explicitly DISCARDed. Leaves produce no
        // call-outputs, so this applies to compositions only.
        if let Some(ref comp) = behavior.composition {
            self.check_outputs_consumed(&comp.nodes, &ctx);
        }

        // Phase 4: Validate guarantees
        self.validate_guarantees(behavior, &ctx);

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// Get warnings (call after analyze)
    pub fn warnings(&self) -> &[SemanticWarning] {
        &self.warnings
    }

    // =========================================================================
    // Leaf/Composite Role Separation (Section 6; DESIGN-MODEL 4.3)
    // =========================================================================

    /// A composite (COMPOSITION / ENTRY) wires behaviors together; it must not
    /// perform primitive computation or touch memory. `ALLOC`/`FREE`/`STORE`/`LOAD`,
    /// `SCOPE`, and arithmetic/comparison/bitwise/atomic/conversion ops are all
    /// forbidden — to route on a result field, BRANCH on it directly
    /// (`BRANCH result.flag ...`). Computation belongs in a leaf or a pattern's
    /// `ON_CREATE`.
    fn check_composite_purity(&mut self, nodes: &[Node]) {
        for node in nodes {
            match &node.kind {
                NodeKind::Store { .. } => self.composite_violation("STORE", node.line()),
                NodeKind::Free(_) => self.composite_violation("FREE", node.line()),
                NodeKind::Scope { nodes } => {
                    self.composite_violation("SCOPE", node.line());
                    self.check_composite_purity(nodes);
                }
                NodeKind::Assignment { expr, .. } => {
                    if let Some(op) = forbidden_compute_op(expr) {
                        self.composite_violation(op, node.line());
                    }
                }
                NodeKind::Set { value, .. } => {
                    if let Some(op) = forbidden_compute_op(value) {
                        self.composite_violation(op, node.line());
                    }
                }
                NodeKind::Branch { condition, .. } => {
                    if let Some(op) = forbidden_compute_op(condition) {
                        self.composite_violation(op, node.line());
                    }
                }
                NodeKind::Call { args, .. } => {
                    for a in args {
                        if let Some(op) = forbidden_compute_op(a) {
                            self.composite_violation(op, node.line());
                        }
                    }
                }
                NodeKind::ChannelSend { value, .. } => {
                    if let Some(op) = forbidden_compute_op(value) {
                        self.composite_violation(op, node.line());
                    }
                }
                _ => {}
            }
        }
    }

    fn composite_violation(&mut self, op: &str, line: usize) {
        self.errors.push(SemanticError {
            kind: ErrorKind::CompositeComputes { op: op.to_string() },
            line,
            context: Some("composite behaviors wire; they don't compute".into()),
        });
    }

    /// A leaf (IMPLEMENTATION) is the bottom of the call graph; it must not CALL
    /// another behavior. A behavior that calls others is a composite.
    fn check_leaf_no_call(&mut self, nodes: &[Node]) {
        for node in nodes {
            match &node.kind {
                NodeKind::Call { behavior, .. } => {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::LeafCalls { behavior: behavior.clone() },
                        line: node.line(),
                        context: None,
                    });
                }
                NodeKind::Assignment { expr, .. } => {
                    if let Expr::Call { behavior, .. } = expr.as_ref() {
                        self.errors.push(SemanticError {
                            kind: ErrorKind::LeafCalls { behavior: behavior.clone() },
                            line: node.line(),
                            context: None,
                        });
                    }
                }
                NodeKind::Scope { nodes } => self.check_leaf_no_call(nodes),
                _ => {}
            }
        }
    }

    /// Section 7.8: in a COMPOSITION every output a CALL produces must be
    /// consumed — passed to another CALL, used as a BRANCH condition, written to
    /// an OUTPUT (SET or `-> output`), or explicitly DISCARDed. A produced output
    /// that reaches no such sink is an unconsumed wire (E0511). Consumption
    /// follows assignment chains: `x = result.field` consumes `result.field`
    /// only if `x` is itself consumed.
    fn check_outputs_consumed(&mut self, nodes: &[Node], ctx: &AnalysisContext) {
        // Places used where a value is genuinely consumed (a "sink").
        let mut live: HashSet<Place> = HashSet::new();
        // Propagation edges from plain assignments: (target, places in rhs).
        let mut edges: Vec<(String, Vec<Place>)> = Vec::new();
        // Outputs a CALL produces that must be consumed:
        // (place to check, display name, callee behavior, line).
        let mut obligations: Vec<(Place, String, String, usize)> = Vec::new();

        for node in nodes {
            match &node.kind {
                NodeKind::Call { behavior, args, outputs, .. } => {
                    for a in args {
                        add_sink_places(a, &mut live);
                    }
                    if outputs.is_empty() {
                        // A bare CALL binds nothing; if the callee produces outputs
                        // they are dropped with no way to consume them.
                        if let Some(c) = ctx.dependency_contracts.get(behavior) {
                            for o in &c.outputs {
                                self.errors.push(SemanticError {
                                    kind: ErrorKind::UnconsumedOutput {
                                        behavior: behavior.clone(),
                                        output: o.name.clone(),
                                    },
                                    line: node.line(),
                                    context: Some("bare CALL drops this output; bind it and consume it, or DISCARD it".into()),
                                });
                            }
                        }
                    } else {
                        for o in outputs {
                            // `CALL f -> o` where `o` is one of this composition's
                            // own OUTPUTs is itself a consumption (§7.8): the call's
                            // result is written into the output sink, so it imposes
                            // no further obligation. `-> o` into a local binds a
                            // produced value that must be consumed downstream like
                            // any other.
                            if ctx.outputs.contains_key(o) {
                                continue;
                            }
                            obligations.push(((o.clone(), None), o.clone(), behavior.clone(), node.line()));
                        }
                    }
                }
                NodeKind::Assignment { target, expr } => match expr.as_ref() {
                    Expr::Call { behavior, args } => {
                        for a in args {
                            add_sink_places(a, &mut live);
                        }
                        if let Some(c) = ctx.dependency_contracts.get(behavior) {
                            for o in &c.outputs {
                                obligations.push((
                                    (target.clone(), Some(o.name.clone())),
                                    format!("{}.{}", target, o.name),
                                    behavior.clone(),
                                    node.line(),
                                ));
                            }
                        }
                    }
                    Expr::Spawn { args, .. } => {
                        for a in args {
                            add_sink_places(a, &mut live);
                        }
                    }
                    other => {
                        let mut places = Vec::new();
                        collect_places_vec(other, &mut places);
                        edges.push((target.clone(), places));
                    }
                },
                NodeKind::Branch { condition, .. } => add_sink_places(condition, &mut live),
                NodeKind::Set { value, .. } => add_sink_places(value, &mut live),
                NodeKind::Discard(expr) => add_sink_places(expr, &mut live),
                NodeKind::Store { value, offset, .. } => {
                    add_sink_places(value, &mut live);
                    if let Some(o) = offset {
                        add_sink_places(o, &mut live);
                    }
                }
                NodeKind::ChannelSend { value, .. } => add_sink_places(value, &mut live),
                NodeKind::Spawn { args, .. } => {
                    for a in args {
                        add_sink_places(a, &mut live);
                    }
                }
                _ => {}
            }
        }

        // Propagate liveness backward through assignment chains to a fixpoint.
        let mut changed = true;
        while changed {
            changed = false;
            for (target, places) in &edges {
                let target_live = live.contains(&(target.clone(), None))
                    || live.iter().any(|(v, f)| v == target && f.is_some());
                if target_live {
                    for p in places {
                        if live.insert(p.clone()) {
                            changed = true;
                        }
                    }
                }
            }
        }

        for (place, display, behavior, line) in obligations {
            let consumed = live.contains(&place)
                || (place.1.is_some() && live.contains(&(place.0.clone(), None)));
            if !consumed {
                self.errors.push(SemanticError {
                    kind: ErrorKind::UnconsumedOutput { behavior, output: display },
                    line,
                    context: None,
                });
            }
        }
    }

    /// Section 14.4: reject control flow that can never reach END (an exit-less
    /// infinite loop). Ordinary loops pass — they structurally connect to an exit.
    fn check_termination(&mut self, nodes: &[Node]) {
        let flat = crate::cfg::flatten_scopes(nodes);
        let cfg = CFG::build(&flat);
        if !cfg.all_paths_terminate() {
            self.errors.push(SemanticError {
                kind: ErrorKind::PathDoesNotTerminate,
                line: 0,
                context: Some("control flow cannot reach END (exit-less loop)".into()),
            });
        }
    }

    // =========================================================================
    // CFG-based Ownership Validation (Section 14.1)
    // =========================================================================

    /// CFG-based validation per Section 14.1
    ///
    /// This pass builds a CFG and checks each path for:
    /// - Double-free (same variable freed twice on same path)
    /// - Use-after-free with proper control flow handling
    /// - Uninitialized LOAD (LOAD without prior STORE on this path)
    /// - Memory leaks (ALLOC without FREE on all paths)
    ///
    /// This is necessary because linear analysis incorrectly reports errors
    /// when operations are in different branches (mutually exclusive).
    fn validate_ownership_cfg(&mut self, nodes: &[Node], ctx: &AnalysisContext) {
        // Flatten SCOPE bodies so the path analysis sees operations *inside* scopes,
        // indexing into the same flattened slice the CFG is built from. (Without this
        // the CFG treats a SCOPE as one opaque statement and the checks skip it.)
        let flat = crate::cfg::flatten_scopes(nodes);
        let cfg = CFG::build(&flat);

        // Get all paths through the CFG
        let paths = cfg.all_paths();

        // Track reported errors to avoid duplicates
        let mut reported_errors: HashSet<(String, usize)> = HashSet::new();

        // Track allocations to check for leaks across all paths
        let mut allocations: HashMap<String, usize> = HashMap::new(); // var -> line allocated
        let mut not_freed_on_some_path: HashSet<String> = HashSet::new(); // allocations not freed on at least one path

        // Track outputs not written on some path (for writes_output CFG-aware check)
        let mut outputs_not_written_on_some_path: HashSet<String> = HashSet::new();
        let has_writes_output_guarantee = ctx.has_guarantee(&Guarantee::WritesOutput);

        // For each path, track state and check for violations
        for path in &paths {
            let mut freed_on_path: HashMap<String, usize> = HashMap::new(); // var -> line freed
            let mut stored_on_path: HashSet<String> = HashSet::new(); // vars that have been STORE'd to
            let mut alloc_on_path: HashSet<String> = HashSet::new(); // vars allocated on this path
            let mut written_outputs_on_path: HashSet<String> = HashSet::new(); // outputs written on this path

            // INPUT handles are considered "initialized" (caller stored to them)
            for name in ctx.inputs.keys() {
                stored_on_path.insert(name.clone());
            }

            for &block_id in path {
                if block_id >= cfg.blocks.len() {
                    continue;
                }
                let block = &cfg.blocks[block_id];

                for &stmt_idx in &block.statements {
                    if stmt_idx >= flat.len() {
                        continue;
                    }
                    let node = &flat[stmt_idx];

                    // Handle assignment - this creates a NEW value for the variable
                    if let NodeKind::Assignment { target, expr } = &node.kind {
                        // First check if expr uses any freed variables
                        self.check_expr_uses_freed_dedup(expr, &freed_on_path, node.line(), &mut reported_errors);

                        // Check for uninitialized LOAD in the expression
                        self.check_uninitialized_load(expr, &stored_on_path, node.line(), &mut reported_errors);

                        // Assignment resets the "freed" state
                        freed_on_path.remove(target);

                        // Track allocations
                        if matches!(expr.as_ref(), Expr::Alloc { .. }) {
                            alloc_on_path.insert(target.clone());
                            allocations.insert(target.clone(), node.line());
                        }
                    }

                    // Check FREE statements
                    if let NodeKind::Free(name) = &node.kind {
                        if let Some(&first_line) = freed_on_path.get(name) {
                            let error_key = (format!("DoubleFree:{}", name), node.line());
                            if !reported_errors.contains(&error_key) {
                                reported_errors.insert(error_key);
                                self.errors.push(SemanticError {
                                    kind: ErrorKind::DoubleFree {
                                        handle: name.clone(),
                                        first_free_at: first_line,
                                    },
                                    line: node.line(),
                                    context: Some("double FREE on same execution path".into()),
                                });
                            }
                        } else {
                            freed_on_path.insert(name.clone(), node.line());
                        }
                    }

                    // Check STORE - track initialization and use-after-free
                    if let NodeKind::Store { target, .. } = &node.kind {
                        if let Some(&freed_line) = freed_on_path.get(target) {
                            let error_key = (format!("UseAfterFree:{}", target), node.line());
                            if !reported_errors.contains(&error_key) {
                                reported_errors.insert(error_key);
                                self.errors.push(SemanticError {
                                    kind: ErrorKind::UseAfterFree {
                                        handle: target.clone(),
                                        freed_at: freed_line,
                                    },
                                    line: node.line(),
                                    context: Some("STORE to handle freed on this path".into()),
                                });
                            }
                        }
                        // Mark as stored/initialized
                        stored_on_path.insert(target.clone());
                        // Track output writes for writes_output guarantee
                        if ctx.outputs.contains_key(target) {
                            written_outputs_on_path.insert(target.clone());
                        }
                    }

                    // Check SET - track output writes
                    if let NodeKind::Set { target, .. } = &node.kind {
                        if ctx.outputs.contains_key(target) {
                            written_outputs_on_path.insert(target.clone());
                        }
                    }

                    // A CALL that receives an OUTPUT as an argument writes through
                    // it (the out-parameter idiom, e.g. `copy`'s dst, `read`'s
                    // buffer): credit that output as written. A composition can
                    // ONLY fill a bytes output this way (it cannot STORE), so
                    // without this a buffer-returning composition can never satisfy
                    // writes_output.
                    let call_args: Option<&Vec<Expr>> = match &node.kind {
                        NodeKind::Call { args, .. } => Some(args),
                        NodeKind::Assignment { expr, .. } => match expr.as_ref() {
                            Expr::Call { args, .. } => Some(args),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(args) = call_args {
                        for arg in args {
                            if let Expr::Var(name) = arg {
                                if ctx.outputs.contains_key(name) {
                                    written_outputs_on_path.insert(name.clone());
                                }
                            }
                        }
                    }

                    // A CALL that redirects its result into an OUTPUT via
                    // `-> out` (§7.1) writes that output, exactly like SET or the
                    // out-parameter idiom above. Without this a composition that
                    // fills its OUTPUT through the redirect form can never satisfy
                    // writes_output (false E0604).
                    if let NodeKind::Call { outputs, .. } = &node.kind {
                        for out_name in outputs {
                            if ctx.outputs.contains_key(out_name) {
                                written_outputs_on_path.insert(out_name.clone());
                            }
                        }
                    }
                }
            }

            // At end of path, check for leaks (alloc without free on THIS path)
            for alloc_var in &alloc_on_path {
                if !freed_on_path.contains_key(alloc_var) {
                    // This allocation was not freed on this path
                    not_freed_on_some_path.insert(alloc_var.clone());
                }
            }

            // At end of path, check for outputs not written (writes_output guarantee)
            if has_writes_output_guarantee {
                for output_name in ctx.outputs.keys() {
                    if !written_outputs_on_path.contains(output_name) {
                        // This output was not written on this path
                        outputs_not_written_on_some_path.insert(output_name.clone());
                    }
                }
            }
        }

        // Check for memory leaks: any allocation that wasn't freed on ALL paths
        // (i.e., it was not freed on at least one path)
        for var in &not_freed_on_some_path {
            if let Some(&line) = allocations.get(var) {
                let error_key = (format!("MemoryLeak:{}", var), line);
                if !reported_errors.contains(&error_key) {
                    reported_errors.insert(error_key);
                    self.errors.push(SemanticError {
                        kind: ErrorKind::MemoryLeak { handle: var.clone() },
                        line,
                        context: Some("allocation not freed on all execution paths".into()),
                    });
                }
            }
        }

        // Check for writes_output violation: outputs not written on ALL paths
        // Per Section 14.6: "writes_output: Path without OUTPUT write"
        if has_writes_output_guarantee && !outputs_not_written_on_some_path.is_empty() {
            let unwritten: Vec<String> = outputs_not_written_on_some_path.into_iter().collect();
            let error_key = (format!("OutputNotWritten:{:?}", unwritten), 0);
            if !reported_errors.contains(&error_key) {
                reported_errors.insert(error_key);
                self.errors.push(SemanticError {
                    kind: ErrorKind::OutputNotWritten { outputs: unwritten },
                    line: 0,
                    context: Some("output not written on all execution paths".into()),
                });
            }
        }
    }

    /// Check if an expression contains an uninitialized LOAD
    fn check_uninitialized_load(
        &mut self,
        expr: &Expr,
        stored: &HashSet<String>,
        line: usize,
        reported: &mut HashSet<(String, usize)>,
    ) {
        match expr {
            Expr::Load { source, .. } => {
                // Check if source is a variable that hasn't been stored to
                if let Expr::Var(name) = source.as_ref() {
                    if !stored.contains(name) {
                        let error_key = (format!("UninitializedLoad:{}", name), line);
                        if !reported.contains(&error_key) {
                            reported.insert(error_key);
                            self.errors.push(SemanticError {
                                kind: ErrorKind::UninitializedLoad { handle: name.clone() },
                                line,
                                context: Some("LOAD without prior STORE on this path".into()),
                            });
                        }
                    }
                }
                // Recursively check the source expression
                self.check_uninitialized_load(source, stored, line, reported);
            }
            Expr::Var(_) => {
                // Variable references are OK - we only care about LOAD
            }
            // Add other expression types as needed
            _ => {}
        }
    }

    /// Check if an expression uses any freed variables (with deduplication)
    fn check_expr_uses_freed_dedup(
        &mut self,
        expr: &Expr,
        freed: &HashMap<String, usize>,
        line: usize,
        reported: &mut HashSet<(String, usize)>,
    ) {
        match expr {
            Expr::Var(name) => {
                if let Some(&freed_line) = freed.get(name) {
                    let error_key = (format!("UseAfterFree:{}", name), line);
                    if !reported.contains(&error_key) {
                        reported.insert(error_key);
                        self.errors.push(SemanticError {
                            kind: ErrorKind::UseAfterFree {
                                handle: name.clone(),
                                freed_at: freed_line,
                            },
                            line,
                            context: Some("use of handle freed on this path".into()),
                        });
                    }
                }
            }
            Expr::Load { source, .. } => {
                self.check_expr_uses_freed_dedup(source, freed, line, reported);
            }
            Expr::Field { base, .. } => {
                self.check_expr_uses_freed_dedup(base, freed, line, reported);
            }
            // Add more cases as needed for other expression types
            _ => {}
        }
    }

    // =========================================================================
    // Contract Validation (Section 14.3)
    // =========================================================================

    fn validate_contract(&mut self, behavior: &Behavior, _ctx: &AnalysisContext) {
        // Validate input/output sizes
        for input in &behavior.contract.inputs {
            self.validate_param_size(input);
        }
        for output in &behavior.contract.outputs {
            self.validate_param_size(output);
        }

        // Section 14.3: Hash match - declared hash equals computed hash
        // Skip if behavior has no declared hash (empty string)
        if !behavior.hash.is_empty() {
            let computed = hash::compute_contract_hash(&behavior.contract);
            if behavior.hash != computed {
                self.errors.push(SemanticError {
                    kind: ErrorKind::HashMismatch {
                        declared: behavior.hash.clone(),
                        computed,
                    },
                    line: 0,
                    context: Some("update HASH to the computed value to fix".into()),
                });
            }
        }
    }

    fn validate_param_size(&mut self, param: &Parameter) {
        match param.typ {
            Type::Int => {
                if !matches!(param.size, 1 | 2 | 4 | 8) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::InvalidIntSize { size: param.size },
                        line: 0,
                        context: Some(format!("Parameter '{}'", param.name)),
                    });
                }
            }
            Type::Float => {
                if !matches!(param.size, 4 | 8) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::InvalidFloatSize { size: param.size },
                        line: 0,
                        context: Some(format!("Parameter '{}'", param.name)),
                    });
                }
            }
            Type::Bytes => {
                // Any positive size is valid
                if param.size == 0 {
                    self.warnings.push(SemanticWarning {
                        message: format!("Parameter '{}' has size 0", param.name),
                        line: 0,
                    });
                }
            }
            Type::String => {}
        }
    }

    // =========================================================================
    // Label Collection (First Pass)
    // =========================================================================

    fn collect_labels(&mut self, behavior: &Behavior, ctx: &mut AnalysisContext) {
        if let Some(ref impl_) = behavior.implementations.first() {
            self.collect_labels_from_nodes(&impl_.nodes, ctx);
        }
        if let Some(ref comp) = behavior.composition {
            self.collect_labels_from_nodes(&comp.nodes, ctx);
        }
    }

    fn collect_labels_from_nodes(&mut self, nodes: &[Node], ctx: &mut AnalysisContext) {
        for node in nodes {
            if let NodeKind::Label(name) = &node.kind {
                if let Some(first_line) = ctx.define_label(name, node.line()) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::DuplicateLabel {
                            name: name.clone(),
                            first_line,
                        },
                        line: node.line(),
                        context: None,
                    });
                }
            }
            // Recurse into scopes
            if let NodeKind::Scope { nodes: inner } = &node.kind {
                self.collect_labels_from_nodes(inner, ctx);
            }
        }
    }

    // =========================================================================
    // Node Validation
    // =========================================================================

    fn validate_nodes(&mut self, nodes: &[Node], ctx: &mut AnalysisContext) {
        for node in nodes {
            ctx.current_line = node.line();
            self.validate_node(node, ctx);
        }
    }

    fn validate_node(&mut self, node: &Node, ctx: &mut AnalysisContext) {
        match &node.kind {
            NodeKind::Label(_) => {
                // Already validated in collect_labels
            }

            NodeKind::Assignment { target, expr } => {
                self.validate_expr(expr, ctx);
                // Infer type, size, origin, ownership state, and shared flag from expression
                let (typ, size, origin, state, shared) = self.infer_expr_info(expr, ctx);
                // For channels, extract the element type for Section 14.5 type matching
                let channel_element_type = if let Expr::Channel { typ: elem_type, .. } = expr.as_ref() {
                    Some(elem_type.clone())
                } else {
                    None
                };
                ctx.define_var(target, typ, size, origin, state, shared, channel_element_type);

                // SPAWN borrow tracking per SAFETY-RULES Part 6
                // When handle passed to SPAWN, it's borrowed until WAIT
                if let Expr::Spawn { pattern, args } = expr.as_ref() {
                    for arg in args {
                        if let Expr::Var(arg_name) = arg {
                            // Track that this handle is borrowed by the spawned pattern
                            ctx.add_spawn_borrow(arg_name, pattern, node.line(), target);
                        }
                    }
                }
            }

            NodeKind::Store { target, value, size, offset } => {
                // Track non-output writes for pure guarantee (Section 14.6)
                if !ctx.outputs.contains_key(target) {
                    ctx.has_non_output_writes = true;
                }

                // Validate target exists
                if !ctx.var_exists(target) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: target.clone() },
                        line: node.line(),
                        context: Some("STORE target".into()),
                    });
                } else if let Some(info) = ctx.get_var(target) {
                    // Section 14.5: Non-atomic access to SHARED memory forbidden
                    if info.shared {
                        self.errors.push(SemanticError {
                            kind: ErrorKind::NonAtomicSharedAccess {
                                handle: target.clone(),
                                operation: "STORE".to_string(),
                            },
                            line: node.line(),
                            context: Some("use ATOMIC_STORE for SHARED handles".into()),
                        });
                    }
                }
                // NOTE: Use-after-free is checked in validate_ownership_cfg (CFG-aware)

                // Validate value expression
                self.validate_expr(value, ctx);

                // Validate offset if present. A dynamic offset is a value
                // position (it indexes into the buffer), so a handle there is
                // the same E0104 mistake as in arithmetic — LOAD it first.
                if let Some(off_expr) = offset {
                    self.validate_expr(off_expr, ctx);
                    self.check_value_operand(off_expr, "a dynamic offset", ctx);
                }

                // Validate size
                self.validate_int_size(*size, node.line(), "STORE");

                // BOUNDS CHECK per Sigil Reference Section 14.1:
                // Precondition: offset + size <= handle.allocated_size
                self.check_bounds(ctx, target, *size, offset.as_ref().map(|b| b.as_ref()), node.line(), "STORE");

                // Section 14.3: Output match - verify STORE size matches OUTPUT declaration
                // Only check when storing to offset 0 (full output write)
                if offset.is_none() {
                    if let Some(output_param) = ctx.outputs.get(target) {
                        if *size != output_param.size {
                            self.errors.push(SemanticError {
                                kind: ErrorKind::OutputSizeMismatch {
                                    output: target.clone(),
                                    expected: output_param.size,
                                    found: *size,
                                },
                                line: node.line(),
                                context: None,
                            });
                        }
                    }
                }

                // Mark output as written
                ctx.mark_output_written(target);
            }

            NodeKind::Set { target, value } => {
                // Validate target exists
                if !ctx.var_exists(target) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: target.clone() },
                        line: node.line(),
                        context: Some("SET target".into()),
                    });
                }

                self.validate_expr(value, ctx);

                // Section 14.3: Output match - verify value type matches OUTPUT declaration
                // Note: SET doesn't specify size (uses target's size), so only check type
                if let Some(output_param) = ctx.outputs.get(target) {
                    let (value_type, _, _, _, _) = self.infer_expr_info(value, ctx);

                    // Check type if inferrable (int to int, float to float, etc.)
                    if let Some(ref vt) = value_type {
                        // A `string` output may be SET from a `bytes` handle: a string is
                        // length-prefixed bytes ([len:8][content]), so a bytes buffer
                        // already holding that image IS a valid string. SET stores the
                        // buffer's pointer into the 8-byte string slot (codegen). This is
                        // how a string is constructed from COMPUTED data (e.g.
                        // bytes-to-string) rather than only from string literals; it is
                        // sound exactly while the buffer outlives the returned string —
                        // the same caller-ownership rule as collections (§2.5).
                        let bytes_as_string =
                            output_param.typ == Type::String && *vt == Type::Bytes;
                        if *vt != output_param.typ && !bytes_as_string {
                            self.errors.push(SemanticError {
                                kind: ErrorKind::CallArgTypeMismatch {
                                    behavior: "SET".to_string(),
                                    param: target.clone(),
                                    expected: Self::type_to_string(&output_param.typ).to_string(),
                                    found: Self::type_to_string(vt).to_string(),
                                },
                                line: node.line(),
                                context: Some(format!("output '{}' type mismatch", target)),
                            });
                        }
                    }
                }

                ctx.mark_output_written(target);
            }

            NodeKind::Branch { condition, true_label, false_label } => {
                self.validate_expr(condition, ctx);

                if !ctx.label_exists(true_label) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedLabel { name: true_label.clone() },
                        line: node.line(),
                        context: Some("BRANCH true target".into()),
                    });
                }
                if !ctx.label_exists(false_label) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedLabel { name: false_label.clone() },
                        line: node.line(),
                        context: Some("BRANCH false target".into()),
                    });
                }
            }

            NodeKind::Jump(label) => {
                if !ctx.label_exists(label) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedLabel { name: label.clone() },
                        line: node.line(),
                        context: Some("JUMP target".into()),
                    });
                }
            }

            NodeKind::Call { target, behavior, args, outputs: _ } => {
                for arg in args {
                    self.validate_expr(arg, ctx);
                }

                // Section 14.3: Input match - verify arguments against callee contract
                if let Some(callee_contract) = ctx.dependency_contracts.get(behavior) {
                    self.validate_call_args(behavior, args, callee_contract, node.line(), ctx);
                }

                if let Some(t) = target {
                    // CALL result is a structured output - not a handle we can FREE
                    let total_size = ctx.dependency_contracts.get(behavior)
                        .map(|c| c.outputs.iter().map(|o| o.size).sum());
                    ctx.define_var(t, None, 8, VarOrigin::CallResult { total_size }, HandleState::NotAHandle, false, None);
                }
            }

            NodeKind::Free(name) => {
                // Validate variable exists
                if !ctx.var_exists(name) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: name.clone() },
                        line: node.line(),
                        context: Some("FREE".into()),
                    });
                    return; // Can't check ownership on undefined variable
                }

                // OWNERSHIP ANALYSIS per SAFETY-RULES Part 2
                // Rule: Only owner can FREE
                // Rule: Borrowed handles cannot be freed
                // NOTE: Double-free is checked in validate_ownership_cfg (CFG-aware)
                if let Some(info) = ctx.get_var(name) {
                    match &info.state {
                        HandleState::Owned => {
                            // Check if borrowed by active SPAWN per Part 6
                            if let Some(borrow) = ctx.get_active_borrow(name) {
                                self.errors.push(SemanticError {
                                    kind: ErrorKind::FreeWhileBorrowed {
                                        handle: name.clone(),
                                        borrowed_by: borrow.pattern_name.clone(),
                                    },
                                    line: node.line(),
                                    context: Some(format!("spawned at line {}", borrow.spawn_line)),
                                });
                            }
                            // Don't mark as freed here - CFG analysis handles this
                        }
                        HandleState::Borrowed => {
                            // Per Section 3.3: "Borrowing: Input handles are borrowed;
                            // callee cannot free them"
                            self.errors.push(SemanticError {
                                kind: ErrorKind::FreeBorrowedHandle { handle: name.clone() },
                                line: node.line(),
                                context: Some("cannot FREE handle borrowed from caller".into()),
                            });
                        }
                        HandleState::Freed { .. } => {
                            // Double FREE - checked by CFG analysis, skip here
                            // The CFG analysis correctly handles branches
                        }
                        HandleState::NotAHandle => {
                            // Cannot FREE something that's not a handle (e.g., LOAD result)
                            self.errors.push(SemanticError {
                                kind: ErrorKind::TypeMismatch {
                                    expected: "handle".to_string(),
                                    found: "value".to_string(),
                                },
                                line: node.line(),
                                context: Some(format!("'{}' is not a handle", name)),
                            });
                        }
                    }
                }
            }

            NodeKind::Scope { nodes } => {
                // ALLOCs inside a SCOPE are freed at END_SCOPE, so they do not
                // count as "unscoped" for the no_alloc guarantee (reference 5.6).
                ctx.scope_depth += 1;
                self.validate_nodes(nodes, ctx);
                ctx.scope_depth -= 1;
            }

            // Section 14.5: Channel type matching - send type must match channel declaration
            NodeKind::ChannelSend { channel, value } => {
                // Track for pure guarantee (Section 14.6)
                ctx.has_channel_ops = true;

                // Validate channel exists
                if !ctx.var_exists(channel) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: channel.clone() },
                        line: node.line(),
                        context: Some("CHANNEL_SEND".into()),
                    });
                    return;
                }

                // Validate value expression
                self.validate_expr(value, ctx);

                // Type check: value type must match channel's element type
                if let Some(channel_info) = ctx.get_var(channel) {
                    if let Some(expected_type) = &channel_info.channel_element_type {
                        let (value_type, _, _, _, _) = self.infer_expr_info(value, ctx);
                        if let Some(actual_type) = value_type {
                            if &actual_type != expected_type {
                                self.errors.push(SemanticError {
                                    kind: ErrorKind::ChannelSendTypeMismatch {
                                        channel: channel.clone(),
                                        expected: format!("{:?}", expected_type),
                                        found: format!("{:?}", actual_type),
                                    },
                                    line: node.line(),
                                    context: Some("Per Section 14.5: Channel send type must match declaration".into()),
                                });
                            }
                        }
                    }
                }
            }

            NodeKind::ChannelClose(channel) => {
                // Track for pure guarantee (Section 14.6)
                ctx.has_channel_ops = true;

                // Validate channel exists
                if !ctx.var_exists(channel) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: channel.clone() },
                        line: node.line(),
                        context: Some("CHANNEL_CLOSE".into()),
                    });
                }
            }

            NodeKind::Discard(expr) => {
                // Section 7.8: validate the named value exists; whether it counts
                // as consumed is decided in check_outputs_consumed.
                self.validate_expr(expr, ctx);
            }

            _ => {}
        }
    }

    /// Validate CALL arguments against callee contract
    fn validate_call_args(&mut self, behavior: &str, args: &[Expr], callee_contract: &Contract, line: usize, ctx: &AnalysisContext) {
        let expected_count = callee_contract.inputs.len();
        let got_count = args.len();

        // Check argument count
        if expected_count != got_count {
            self.errors.push(SemanticError {
                kind: ErrorKind::CallArgCountMismatch {
                    behavior: behavior.to_string(),
                    expected: expected_count,
                    got: got_count,
                },
                line,
                context: None,
            });
        }

        // Check each argument's interpretation and size
        for (i, (arg, input_param)) in args.iter().zip(callee_contract.inputs.iter()).enumerate() {
            let (arg_type, arg_size, _, _, _) = self.infer_expr_info(arg, ctx);

            // Verify interpretation (type) matches
            if let Some(ref found_type) = arg_type {
                if *found_type != input_param.typ {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::CallArgTypeMismatch {
                            behavior: behavior.to_string(),
                            param: input_param.name.clone(),
                            expected: Self::type_to_string(&input_param.typ).to_string(),
                            found: Self::type_to_string(found_type).to_string(),
                        },
                        line,
                        context: Some(format!("argument {} ('{}')", i + 1, input_param.name)),
                    });
                }
            }

            // Verify size matches
            // For bytes: actual <= declared is OK (buffer fits within max)
            // For int/float: exact match required
            let size_mismatch = match input_param.typ {
                Type::Bytes => arg_size > input_param.size,
                Type::String => false, // self-describing: carries its own length
                Type::Int | Type::Float => arg_size != input_param.size,
            };
            if size_mismatch {
                self.errors.push(SemanticError {
                    kind: ErrorKind::CallArgSizeMismatch {
                        behavior: behavior.to_string(),
                        param: input_param.name.clone(),
                        expected: input_param.size,
                        found: arg_size,
                    },
                    line,
                    context: Some(format!("argument {} ('{}')", i + 1, input_param.name)),
                });
            }
        }
    }

    // =========================================================================
    // Expression Validation
    // =========================================================================

    /// If `expr` would compile to a handle (an address) and therefore cannot be
    /// used directly where a primitive expects a *value*, return a human label
    /// for what kind of handle it is. Mirrors codegen's `as_int`/`as_float`,
    /// which reject any `Handle` operand. Values — literals, LOAD results, and
    /// computed sub-expressions (arithmetic/comparison/etc.) — return None. An
    /// undefined name also returns None (its own error is raised separately).
    fn operand_handle_kind(expr: &Expr, ctx: &AnalysisContext) -> Option<&'static str> {
        match expr {
            Expr::Field { .. } => Some("CALL-result field"),
            Expr::Alloc { .. } => Some("ALLOC handle"),
            Expr::Var(name) => match ctx.variables.get(name).map(|i| &i.origin) {
                Some(VarOrigin::Input { .. }) => Some("INPUT"),
                Some(VarOrigin::Output { .. }) => Some("OUTPUT"),
                Some(VarOrigin::Alloc { .. }) => Some("ALLOC handle"),
                Some(VarOrigin::CallResult { .. }) => Some("CALL result"),
                // LoadResult / Computed are values; an unknown name is reported
                // elsewhere as UndefinedVariable.
                _ => None,
            },
            _ => None,
        }
    }

    /// Reject a handle used where a primitive `op` expects a value (E0104),
    /// with the source line and a LOAD hint — instead of letting it leak into
    /// codegen as the location-less `Expected int immediate, got handle`.
    fn check_value_operand(&mut self, expr: &Expr, op: &str, ctx: &AnalysisContext) {
        if let Some(kind) = Self::operand_handle_kind(expr, ctx) {
            let operand = match expr {
                Expr::Var(n) => n.clone(),
                Expr::Field { base, field } => match base.as_ref() {
                    Expr::Var(b) => format!("{}.{}", b, field),
                    _ => field.clone(),
                },
                _ => "operand".to_string(),
            };
            self.errors.push(SemanticError {
                kind: ErrorKind::HandleUsedAsValue {
                    operand,
                    kind: kind.to_string(),
                    op: op.to_string(),
                },
                line: ctx.current_line,
                context: None,
            });
        }
    }

    fn validate_expr(&mut self, expr: &Expr, ctx: &mut AnalysisContext) {
        match expr {
            Expr::Var(name) => {
                if !ctx.var_exists(name) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: name.clone() },
                        line: ctx.current_line,
                        context: None,
                    });
                }
                // NOTE: Use-after-free is checked in validate_ownership_cfg (CFG-aware)
            }

            Expr::Field { base, field: _ } => {
                self.validate_expr(base, ctx);
            }

            Expr::Load { source, size, offset } => {
                self.validate_expr(source, ctx);
                self.validate_int_size(*size, ctx.current_line, "LOAD");
                if let Some(off) = offset {
                    self.validate_expr(off, ctx);
                    // A dynamic offset is a value position; a handle there is the
                    // same E0104 mistake as in arithmetic.
                    self.check_value_operand(off, "a dynamic offset", ctx);
                }

                // BOUNDS CHECK per Sigil Reference Section 14.1:
                // Precondition: offset + size <= handle.allocated_size
                if let Expr::Var(name) = source.as_ref() {
                    self.check_bounds(ctx, name, *size, offset.as_ref().map(|b| b.as_ref()), ctx.current_line, "LOAD");

                    // Section 14.5: Non-atomic access to SHARED memory forbidden
                    if let Some(info) = ctx.get_var(name) {
                        if info.shared {
                            self.errors.push(SemanticError {
                                kind: ErrorKind::NonAtomicSharedAccess {
                                    handle: name.clone(),
                                    operation: "LOAD".to_string(),
                                },
                                line: ctx.current_line,
                                context: Some("use ATOMIC_LOAD for SHARED handles".into()),
                            });
                        }
                    }
                }
            }

            Expr::Alloc { size, shared: _, .. } => {
                self.validate_expr(size, ctx);
                // Only an ALLOC outside any SCOPE escapes; a scoped ALLOC is freed
                // at END_SCOPE and satisfies no_alloc (reference 5.6).
                if ctx.scope_depth == 0 {
                    ctx.has_unscoped_alloc = true;
                }
            }

            Expr::Call { behavior, args } => {
                for arg in args {
                    self.validate_expr(arg, ctx);
                }

                // Section 14.3: Input match - verify arguments against callee contract
                if let Some(callee_contract) = ctx.dependency_contracts.get(behavior) {
                    self.validate_call_args(behavior, args, callee_contract, ctx.current_line, ctx);
                }
            }

            // Binary operations
            Expr::Iadd(a, b, size) | Expr::Isub(a, b, size) | Expr::Imul(a, b, size) |
            Expr::Idiv(a, b, size) | Expr::Imod(a, b, size) => {
                self.validate_expr(a, ctx);
                self.validate_expr(b, ctx);
                self.check_value_operand(a, "integer arithmetic", ctx);
                self.check_value_operand(b, "integer arithmetic", ctx);
                self.validate_int_size(*size, ctx.current_line, "integer operation");
            }

            Expr::Ieq(a, b, size) | Expr::Ine(a, b, size) | Expr::Ilt(a, b, size) |
            Expr::Igt(a, b, size) | Expr::Ile(a, b, size) | Expr::Ige(a, b, size) => {
                self.validate_expr(a, ctx);
                self.validate_expr(b, ctx);
                self.check_value_operand(a, "integer comparison", ctx);
                self.check_value_operand(b, "integer comparison", ctx);
                self.validate_int_size(*size, ctx.current_line, "comparison");
            }

            Expr::Fadd(a, b, size) | Expr::Fsub(a, b, size) | Expr::Fmul(a, b, size) |
            Expr::Fdiv(a, b, size) => {
                self.validate_expr(a, ctx);
                self.validate_expr(b, ctx);
                self.check_value_operand(a, "float arithmetic", ctx);
                self.check_value_operand(b, "float arithmetic", ctx);
                self.validate_float_size(*size, ctx.current_line, "float operation");
            }

            Expr::Feq(a, b, size) | Expr::Fne(a, b, size) | Expr::Flt(a, b, size) |
            Expr::Fgt(a, b, size) | Expr::Fle(a, b, size) | Expr::Fge(a, b, size) => {
                self.validate_expr(a, ctx);
                self.validate_expr(b, ctx);
                self.check_value_operand(a, "float comparison", ctx);
                self.check_value_operand(b, "float comparison", ctx);
                self.validate_float_size(*size, ctx.current_line, "float comparison");
            }

            // Unary operations
            Expr::Ineg(a, size) | Expr::Not(a, size) => {
                self.validate_expr(a, ctx);
                self.check_value_operand(a, "this operation", ctx);
                self.validate_int_size(*size, ctx.current_line, "unary operation");
            }

            Expr::Fneg(a, size) => {
                self.validate_expr(a, ctx);
                self.check_value_operand(a, "float negation", ctx);
                self.validate_float_size(*size, ctx.current_line, "float negation");
            }

            // Bitwise
            Expr::And(a, b, size) | Expr::Or(a, b, size) | Expr::Xor(a, b, size) |
            Expr::Shl(a, b, size) | Expr::Shr(a, b, size) | Expr::Sar(a, b, size) => {
                self.validate_expr(a, ctx);
                self.validate_expr(b, ctx);
                self.check_value_operand(a, "bitwise op", ctx);
                self.check_value_operand(b, "bitwise op", ctx);
                self.validate_int_size(*size, ctx.current_line, "bitwise operation");
            }

            // Conversions
            Expr::Ftoi { value, float_size, int_size } => {
                self.validate_expr(value, ctx);
                self.check_value_operand(value, "FTOI", ctx);
                self.validate_float_size(*float_size, ctx.current_line, "FTOI source");
                self.validate_int_size(*int_size, ctx.current_line, "FTOI target");
            }

            Expr::Itof { value, int_size, float_size } => {
                self.validate_expr(value, ctx);
                self.check_value_operand(value, "ITOF", ctx);
                self.validate_int_size(*int_size, ctx.current_line, "ITOF source");
                self.validate_float_size(*float_size, ctx.current_line, "ITOF target");
            }

            // Atomics
            Expr::AtomicLoad(source, size) => {
                self.validate_expr(source, ctx);
                self.validate_int_size(*size, ctx.current_line, "ATOMIC_LOAD");
            }

            Expr::AtomicStore(target, value, size) => {
                self.validate_expr(target, ctx);
                self.validate_expr(value, ctx);
                self.validate_int_size(*size, ctx.current_line, "ATOMIC_STORE");
            }

            Expr::Cas { addr, expected, new, size } => {
                self.validate_expr(addr, ctx);
                self.validate_expr(expected, ctx);
                self.validate_expr(new, ctx);
                self.validate_int_size(*size, ctx.current_line, "CAS");
            }

            Expr::AtomicAdd(target, value, size) | Expr::AtomicSub(target, value, size) => {
                self.validate_expr(target, ctx);
                self.validate_expr(value, ctx);
                self.validate_int_size(*size, ctx.current_line, "atomic operation");
            }

            // Concurrency expressions (Section 14.5)
            Expr::Spawn { pattern, args } => {
                // Validate arguments
                for arg in args {
                    self.validate_expr(arg, ctx);
                }

                // Section 9.1: SPAWN creates "concurrent pattern instance"
                // Target must be a pattern, not a behavior
                // Check if it's a known pattern; if not and it's a dependency (behavior), error
                if !ctx.pattern_names.contains(pattern) {
                    // Check if it's a known behavior (from dependencies)
                    if ctx.dependency_contracts.contains_key(pattern) {
                        self.errors.push(SemanticError {
                            kind: ErrorKind::SpawnRequiresPattern { target: pattern.clone() },
                            line: ctx.current_line,
                            context: None,
                        });
                    }
                    // If neither pattern nor behavior, it's unknown - will fail at codegen
                }
            }

            Expr::Wait(handle_name) => {
                // Validate the handle variable exists
                if !ctx.var_exists(handle_name) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: handle_name.clone() },
                        line: ctx.current_line,
                        context: Some("WAIT handle".into()),
                    });
                } else {
                    // WAIT releases borrows associated with this spawn handle
                    // Per SAFETY-RULES Part 6: "Borrowed handle valid for spawned pattern lifetime"
                    // When WAIT completes, the pattern is done and borrows are released
                    ctx.release_spawn_borrows(handle_name);
                }
            }

            Expr::Channel { .. } => {
                // Channel creation - track for pure guarantee (Section 14.6)
                ctx.has_channel_ops = true;
            }

            Expr::ChannelReceive(channel_name) => {
                // Track for pure guarantee (Section 14.6)
                ctx.has_channel_ops = true;

                // Validate the channel variable exists
                if !ctx.var_exists(channel_name) {
                    self.errors.push(SemanticError {
                        kind: ErrorKind::UndefinedVariable { name: channel_name.clone() },
                        line: ctx.current_line,
                        context: Some("CHANNEL_RECEIVE channel".into()),
                    });
                }
            }

            // Literals and others need no validation
            _ => {}
        }
    }

    // =========================================================================
    // Guarantee Validation (Section 14.6)
    // =========================================================================

    fn validate_guarantees(&mut self, behavior: &Behavior, ctx: &AnalysisContext) {
        // Check 'pure' guarantee (Section 13.6)
        // Violation: pattern MEMORY access, CHANNEL ops, non-OUTPUT writes
        if ctx.has_guarantee(&Guarantee::Pure) {
            if ctx.has_channel_ops {
                self.errors.push(SemanticError {
                    kind: ErrorKind::PureViolation {
                        reason: "contains CHANNEL operations".into()
                    },
                    line: 0,
                    context: None,
                });
            }
            if ctx.has_non_output_writes {
                self.errors.push(SemanticError {
                    kind: ErrorKind::PureViolation {
                        reason: "contains writes to non-OUTPUT variables".into()
                    },
                    line: 0,
                    context: None,
                });
            }
            // A behavior that declares access to pattern MEMORY touches persistent
            // state and cannot be pure (Section 13.6). This closes the soundness
            // hole where a stateful behavior could pass as `pure`.
            if !behavior.contract.requires.memory.is_empty() {
                self.errors.push(SemanticError {
                    kind: ErrorKind::PureViolation {
                        reason: "accesses pattern MEMORY".into()
                    },
                    line: 0,
                    context: None,
                });
            }
        }

        // Check 'no_alloc' guarantee
        if ctx.has_guarantee(&Guarantee::NoAlloc) {
            if ctx.has_unscoped_alloc {
                self.errors.push(SemanticError {
                    kind: ErrorKind::NoAllocViolation {
                        reason: "contains ALLOC outside SCOPE".into()
                    },
                    line: 0,
                    context: None,
                });
            }
        }

        // NOTE: 'writes_output' guarantee is now checked in validate_ownership_cfg
        // which does CFG-aware analysis (checks all execution paths).
        // The old linear check here would miss cases like branching where
        // output is written on one path but not another.
    }

    // =========================================================================
    // Bounds Checking (Sigil Reference Section 14.1)
    // =========================================================================

    /// Check memory bounds per Sigil Reference Section 14.1:
    /// Precondition: offset + size <= handle.allocated_size
    ///
    /// This is a compile-time check. If offset is dynamic, we emit a warning
    /// since we cannot verify bounds statically.
    fn check_bounds(
        &mut self,
        ctx: &AnalysisContext,
        handle_name: &str,
        access_size: usize,
        offset_expr: Option<&Expr>,
        line: usize,
        operation: &str,
    ) {
        // Get handle info
        let var_info = match ctx.get_var(handle_name) {
            Some(info) => info,
            None => return, // Already reported as undefined variable
        };

        // Get allocated size if known
        let allocated_size = match var_info.allocated_size() {
            Some(size) => size,
            None => {
                // Cannot verify bounds - origin unknown (e.g., computed pointer)
                // This is expected for LoadResult or Computed origins
                return;
            }
        };

        // Determine offset value
        let offset = match offset_expr {
            None => 0usize,
            Some(expr) => {
                match self.try_eval_const(expr) {
                    Some(off) => off,
                    None => {
                        // Dynamic offset - cannot verify at compile time
                        // Per spec, this should still be checked, but we can only warn
                        self.warnings.push(SemanticWarning {
                            message: format!(
                                "{} with dynamic offset on '{}': bounds cannot be verified at compile time",
                                operation, handle_name
                            ),
                            line,
                        });
                        return;
                    }
                }
            }
        };

        // BOUNDS CHECK: offset + size <= allocated_size
        let end_offset = offset.saturating_add(access_size);
        if end_offset > allocated_size {
            self.errors.push(SemanticError {
                kind: ErrorKind::PossibleBoundsViolation {
                    handle: handle_name.to_string(),
                    offset,
                    size: access_size,
                },
                line,
                context: Some(format!(
                    "{}: access at [{}:{}] exceeds allocation size {}",
                    operation, offset, end_offset, allocated_size
                )),
            });
        }
    }

    // =========================================================================
    // Type Helpers
    // =========================================================================

    /// Convert Type to string representation for error messages
    fn type_to_string(typ: &Type) -> &'static str {
        match typ {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bytes => "bytes",
            Type::String => "string",
        }
    }

    // =========================================================================
    // Size Validation Helpers
    // =========================================================================

    fn validate_int_size(&mut self, size: usize, line: usize, context: &str) {
        if !matches!(size, 1 | 2 | 4 | 8) {
            self.errors.push(SemanticError {
                kind: ErrorKind::InvalidIntSize { size },
                line,
                context: Some(context.into()),
            });
        }
    }

    fn validate_float_size(&mut self, size: usize, line: usize, context: &str) {
        if !matches!(size, 4 | 8) {
            self.errors.push(SemanticError {
                kind: ErrorKind::InvalidFloatSize { size },
                line,
                context: Some(context.into()),
            });
        }
    }

    // =========================================================================
    // Constant Expression Evaluation
    // =========================================================================

    /// Try to evaluate an expression as a constant integer at compile time.
    /// Returns None if the expression cannot be evaluated statically.
    fn try_eval_const(&self, expr: &Expr) -> Option<usize> {
        match expr {
            Expr::IntLit(n) => Some(*n as usize),
            Expr::Iadd(a, b, _) => {
                let av = self.try_eval_const(a)?;
                let bv = self.try_eval_const(b)?;
                Some(av.wrapping_add(bv))
            }
            Expr::Isub(a, b, _) => {
                let av = self.try_eval_const(a)?;
                let bv = self.try_eval_const(b)?;
                Some(av.wrapping_sub(bv))
            }
            Expr::Imul(a, b, _) => {
                let av = self.try_eval_const(a)?;
                let bv = self.try_eval_const(b)?;
                Some(av.wrapping_mul(bv))
            }
            _ => None,
        }
    }

    // =========================================================================
    // Type and Origin Inference
    // =========================================================================

    /// Infer type, size, origin, ownership state, and shared flag for an expression.
    /// Per Sigil Reference Section 14.1, we track allocation size for bounds checking.
    /// Per SAFETY-RULES Part 2, we track ownership state for handle lifecycle.
    /// Per Section 14.5, we track SHARED flag for atomic access requirements.
    fn infer_expr_info(&self, expr: &Expr, ctx: &AnalysisContext) -> (Option<Type>, usize, VarOrigin, HandleState, bool) {
        match expr {
            Expr::IntLit(_) => (Some(Type::Int), 8, VarOrigin::Computed, HandleState::NotAHandle, false),
            Expr::FloatLit(_) => (Some(Type::Float), 8, VarOrigin::Computed, HandleState::NotAHandle, false),
            // A string literal is length-prefixed: 8-byte length + content.
            Expr::StringLit(s) => (Some(Type::String), s.len() + 8, VarOrigin::Computed, HandleState::NotAHandle, false),

            Expr::Var(name) => {
                if let Some(info) = ctx.variables.get(name) {
                    (info.typ.clone(), info.size, info.origin.clone(), info.state.clone(), info.shared)
                } else {
                    (None, 8, VarOrigin::Computed, HandleState::NotAHandle, false)
                }
            }

            Expr::Alloc { size, typ, shared } => {
                // Try to evaluate allocation size at compile time
                let sz = self.try_eval_const(size);
                let actual_size = sz.unwrap_or(8);
                // ALLOC creates an owned handle
                (Some(typ.clone()), actual_size, VarOrigin::Alloc { allocated_size: sz }, HandleState::Owned, *shared)
            }

            Expr::Load { size, .. } => {
                // LOAD returns an immediate value, not a handle
                (Some(Type::Int), *size, VarOrigin::LoadResult, HandleState::NotAHandle, false)
            }

            Expr::Call { .. } => {
                // Call results have structured output - not a handle we can FREE
                (None, 8, VarOrigin::CallResult { total_size: None }, HandleState::NotAHandle, false)
            }

            // Arithmetic operations return computed values (not handles)
            Expr::Iadd(_, _, size) | Expr::Isub(_, _, size) |
            Expr::Imul(_, _, size) | Expr::Idiv(_, _, size) |
            Expr::Imod(_, _, size) | Expr::Ineg(_, size) => {
                (Some(Type::Int), *size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Comparison operations return computed values
            Expr::Ieq(_, _, _) | Expr::Ine(_, _, _) |
            Expr::Ilt(_, _, _) | Expr::Igt(_, _, _) |
            Expr::Ile(_, _, _) | Expr::Ige(_, _, _) => {
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Float operations return computed values
            Expr::Fadd(_, _, size) | Expr::Fsub(_, _, size) |
            Expr::Fmul(_, _, size) | Expr::Fdiv(_, _, size) |
            Expr::Fneg(_, size) => {
                (Some(Type::Float), *size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Float comparisons return int
            Expr::Feq(_, _, _) | Expr::Fne(_, _, _) |
            Expr::Flt(_, _, _) | Expr::Fgt(_, _, _) |
            Expr::Fle(_, _, _) | Expr::Fge(_, _, _) => {
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Bitwise operations return computed values
            Expr::And(_, _, size) | Expr::Or(_, _, size) |
            Expr::Xor(_, _, size) | Expr::Shl(_, _, size) |
            Expr::Shr(_, _, size) | Expr::Sar(_, _, size) |
            Expr::Not(_, size) => {
                (Some(Type::Int), *size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Conversions
            Expr::Ftoi { int_size, .. } => {
                (Some(Type::Int), *int_size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }
            Expr::Itof { float_size, .. } => {
                (Some(Type::Float), *float_size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // Atomics return computed values
            Expr::AtomicLoad(_, size) => (Some(Type::Int), *size, VarOrigin::LoadResult, HandleState::NotAHandle, false),
            Expr::AtomicStore(_, _, size) => (Some(Type::Int), *size, VarOrigin::Computed, HandleState::NotAHandle, false),
            Expr::Cas { size, .. } => (Some(Type::Int), *size, VarOrigin::Computed, HandleState::NotAHandle, false),
            Expr::AtomicAdd(_, _, size) | Expr::AtomicSub(_, _, size) => {
                (Some(Type::Int), *size, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // SPAWN returns a spawn handle (owned by this behavior, for use with WAIT)
            Expr::Spawn { .. } => {
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::Owned, false)
            }

            // WAIT returns a result value (not a handle)
            Expr::Wait(_) => {
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            // CHANNEL creates an owned channel handle
            Expr::Channel { .. } => {
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::Owned, false)
            }

            // CHANNEL_RECEIVE returns a value with the channel's element type
            Expr::ChannelReceive(channel_name) => {
                // Look up the channel's element type for proper type inference
                if let Some(info) = ctx.get_var(channel_name) {
                    if let Some(elem_type) = &info.channel_element_type {
                        let size = match elem_type {
                            Type::Int => 8,
                            Type::Float => 8,
                            Type::Bytes => 8, // pointer size
                            Type::String => 8,
                        };
                        return (Some(elem_type.clone()), size, VarOrigin::Computed, HandleState::NotAHandle, false);
                    }
                }
                // Fallback if channel not found or no element type
                (Some(Type::Int), 8, VarOrigin::Computed, HandleState::NotAHandle, false)
            }

            _ => (None, 8, VarOrigin::Computed, HandleState::NotAHandle, false),
        }
    }
}

// =============================================================================
// Leaf/Composite Helpers (module-level)
// =============================================================================

/// Returns the name of a forbidden computation/memory primitive if `expr`
/// (or any subexpression) contains one, for the composite-purity check.
///
/// Permitted in a composite: literals, variables, field access (route a result
/// field, including directly into BRANCH), `CALL`, and concurrency expressions.
/// Forbidden: `ALLOC`, `LOAD`, and any arithmetic / comparison / bitwise /
/// atomic / conversion op — composites wire and route; they do not read or compute.
fn forbidden_compute_op(expr: &Expr) -> Option<&'static str> {
    match expr {
        Expr::Alloc { .. } => Some("ALLOC"),
        Expr::Iadd(..) | Expr::Isub(..) | Expr::Imul(..) | Expr::Idiv(..)
            | Expr::Imod(..) | Expr::Ineg(..) => Some("integer arithmetic"),
        Expr::Ieq(..) | Expr::Ine(..) | Expr::Ilt(..) | Expr::Igt(..)
            | Expr::Ile(..) | Expr::Ige(..) => Some("integer comparison"),
        Expr::Fadd(..) | Expr::Fsub(..) | Expr::Fmul(..) | Expr::Fdiv(..)
            | Expr::Fneg(..) => Some("float arithmetic"),
        Expr::Feq(..) | Expr::Fne(..) | Expr::Flt(..) | Expr::Fgt(..)
            | Expr::Fle(..) | Expr::Fge(..) => Some("float comparison"),
        Expr::Ftoi { .. } | Expr::Itof { .. } => Some("conversion"),
        Expr::And(..) | Expr::Or(..) | Expr::Xor(..) | Expr::Not(..)
            | Expr::Shl(..) | Expr::Shr(..) | Expr::Sar(..) => Some("bitwise op"),
        Expr::AtomicLoad(..) | Expr::AtomicStore(..) | Expr::Cas { .. }
            | Expr::AtomicAdd(..) | Expr::AtomicSub(..) => Some("atomic op"),
        Expr::Load { .. } => Some("LOAD"),
        // Permitted wrappers — scan their children for nested forbidden ops.
        Expr::Field { base, .. } => forbidden_compute_op(base),
        Expr::Call { args, .. } => args.iter().find_map(|a| forbidden_compute_op(a)),
        Expr::Spawn { args, .. } => args.iter().find_map(|a| forbidden_compute_op(a)),
        // Literals, variables, WAIT, CHANNEL, CHANNEL_RECEIVE: permitted leaves.
        _ => None,
    }
}

// =============================================================================
// Output-consumption helpers (Section 7.8)
// =============================================================================

/// A produced value referenced in a composition: a variable, optionally a field.
type Place = (String, Option<String>);

/// Collect the places (`var` or `var.field`) a composition expression references.
fn collect_places_vec(expr: &Expr, out: &mut Vec<Place>) {
    match expr {
        Expr::Var(x) => out.push((x.clone(), None)),
        Expr::Field { base, field } => {
            if let Expr::Var(x) = base.as_ref() {
                out.push((x.clone(), Some(field.clone())));
            } else {
                collect_places_vec(base, out);
            }
        }
        Expr::Call { args, .. } | Expr::Spawn { args, .. } => {
            for a in args {
                collect_places_vec(a, out);
            }
        }
        _ => {}
    }
}

/// Add the places an expression references to the live (consumed) set.
fn add_sink_places(expr: &Expr, set: &mut HashSet<Place>) {
    let mut v = Vec::new();
    collect_places_vec(expr, &mut v);
    for p in v {
        set.insert(p);
    }
}

/// Enforce that an executable's `ENTRY` block obeys the composite rule
/// (Section 6): it wires behaviors and must not perform primitive computation.
/// Returns the violations (empty = ok). `ENTRY` is otherwise not run through the
/// per-behavior analyzer (it has no contract), so this is its dedicated check.
pub fn check_entry_is_composite(nodes: &[Node]) -> Vec<SemanticError> {
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.check_composite_purity(nodes);
    analyzer.errors
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    /// Helper: parse and analyze behavior
    fn analyze_source(source: &str) -> Result<(), Vec<SemanticError>> {
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let deps = HashMap::new();
        let patterns = HashSet::new();
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.analyze(&behavior, &deps, &patterns)
    }

    /// Helper: analyze and expect errors
    fn expect_errors(source: &str) -> Vec<SemanticError> {
        match analyze_source(source) {
            Ok(()) => panic!("Expected errors but analysis passed"),
            Err(errors) => errors,
        }
    }

    /// Helper: analyze and expect success
    fn expect_success(source: &str) {
        if let Err(errors) = analyze_source(source) {
            panic!("Expected success but got errors: {:?}", errors);
        }
    }

    #[test]
    fn test_arith_on_unloaded_input_handle_fails() {
        // `IADD x 1` where x is an INPUT int is arithmetic on a handle (address),
        // not a value — it must be LOADed first. Caught in semantic (E0104) with
        // a source line instead of leaking into codegen as `got handle`.
        let source = r#"
BEHAVIOR addone

CONTRACT
  INPUT x int 8
  OUTPUT y int 8
  GUARANTEES writes_output

HASH 00000000

IMPLEMENTATION
  r = IADD x 1 8
  STORE y r 8 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::HandleUsedAsValue { .. })),
            "arithmetic on an un-LOADed INPUT handle must be rejected (E0104), got: {:?}", errors);
    }

    #[test]
    fn test_arith_on_loaded_value_ok() {
        // The correct form — LOAD the input to a value, then compute on it —
        // must NOT trip the handle-as-value check.
        let source = r#"
BEHAVIOR addone

CONTRACT
  INPUT x int 8
  OUTPUT y int 8
  GUARANTEES writes_output

HASH 00000000

IMPLEMENTATION
  xv = LOAD x 8
  r = IADD xv 1 8
  STORE y r 8 0
END
"#;
        let errors = analyze_source(source).err().unwrap_or_default();
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::HandleUsedAsValue { .. })),
            "arithmetic on a LOADed value must be accepted, got: {:?}", errors);
    }

    #[test]
    fn test_handle_as_dynamic_offset_fails() {
        // A handle used as a dynamic LOAD/STORE offset is the same E0104 mistake
        // as in arithmetic — the offset indexes the buffer, so it must be a
        // value. (This is what noex-t3 mistook for "dynamic offsets don't work":
        // the stdlib uses dynamic offsets fine, with *computed* values.)
        let source = r#"
BEHAVIOR offh

CONTRACT
  INPUT idx int 8
  OUTPUT buf bytes 16
  GUARANTEES writes_output

HASH 00000000

IMPLEMENTATION
  STORE buf 65 1 0
  STORE buf 66 1 idx
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::HandleUsedAsValue { .. })),
            "a handle used as a dynamic offset must be rejected (E0104), got: {:?}", errors);
    }

    // =========================================================================
    // Valid Behaviors
    // =========================================================================

    #[test]
    fn test_valid_behavior_passes() {
        let source = r#"
BEHAVIOR valid

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  SET status 0
END
"#;
        expect_success(source);
    }

    #[test]
    fn test_valid_with_inputs_outputs() {
        let source = r#"
BEHAVIOR valid-io

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH e612e39d

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  sum = IADD va vb 8
  STORE result sum 8
END
"#;
        expect_success(source);
    }

    // =========================================================================
    // Invalid Int/Float Sizes (Section 2.2)
    // =========================================================================

    #[test]
    fn test_invalid_int_size() {
        let source = r#"
BEHAVIOR invalid-int

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH invi1234

IMPLEMENTATION
  v = IADD 1 2 3
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(e.kind, ErrorKind::InvalidIntSize { size: 3 })));
    }

    #[test]
    fn test_invalid_float_size() {
        let source = r#"
BEHAVIOR invalid-float

CONTRACT
  OUTPUT status float 8
  GUARANTEES writes_output

HASH invf1234

IMPLEMENTATION
  v = FADD 1.0 2.0 2
  STORE status v 8
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(e.kind, ErrorKind::InvalidFloatSize { size: 2 })));
    }

    // =========================================================================
    // Undefined Labels (Section 14.4)
    // =========================================================================

    #[test]
    fn test_undefined_label() {
        let source = r#"
BEHAVIOR undef-label

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH unde1234

IMPLEMENTATION
  JUMP nonexistent
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UndefinedLabel { name } if name == "nonexistent")));
    }

    #[test]
    fn test_undefined_branch_target() {
        let source = r#"
BEHAVIOR undef-branch

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH undb1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c exists missing

  LABEL exists
    SET status 1
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UndefinedLabel { name } if name == "missing")));
    }

    // =========================================================================
    // Duplicate Labels (Section 14.4)
    // =========================================================================

    #[test]
    fn test_duplicate_label() {
        let source = r#"
BEHAVIOR dup-label

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH dupl1234

IMPLEMENTATION
  LABEL same
    SET status 0
  LABEL same
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::DuplicateLabel { name, .. } if name == "same")));
    }

    // =========================================================================
    // Guarantee Violations (Section 13.6)
    // =========================================================================

    #[test]
    fn test_no_alloc_violation() {
        let source = r#"
BEHAVIOR no-alloc-viol

CONTRACT
  OUTPUT status int 8
  GUARANTEES no_alloc writes_output

HASH noal1234

IMPLEMENTATION
  h = ALLOC 64 bytes
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::NoAllocViolation { .. })));
    }

    // =========================================================================
    // Leaf/Composite Role Separation (Section 6; DESIGN-MODEL 4.3)
    // =========================================================================

    #[test]
    fn test_composite_may_not_alloc() {
        let source = r#"
BEHAVIOR comp-alloc

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  buf = ALLOC 8 int
  CALL helper
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::CompositeComputes { .. })));
    }

    #[test]
    fn test_composite_may_not_store() {
        let source = r#"
BEHAVIOR comp-store

CONTRACT
  OUTPUT out bytes 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  CALL helper
  STORE out 42 8
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::CompositeComputes { .. })));
    }

    #[test]
    fn test_leaf_may_not_call() {
        let source = r#"
BEHAVIOR leaf-call

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

IMPLEMENTATION
  CALL helper
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::LeafCalls { .. })));
    }

    #[test]
    fn test_composite_may_not_load() {
        // A composite must not LOAD; to route on a result field, BRANCH on it
        // directly (BRANCH result.flag ...).
        let source = r#"
BEHAVIOR comp-load

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  result = CALL helper
  flag = LOAD result.flag 8
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::CompositeComputes { .. })),
            "LOAD in a composite must be rejected");
    }

    // =========================================================================
    // Output Consumption (Section 7.8)
    // =========================================================================

    /// A dependency that produces a single `status int 8` output.
    fn producer_contract() -> Contract {
        let mut c = Contract {
            inputs: Vec::new(),
            outputs: Vec::new(),
            requires: Default::default(),
            guarantees: Vec::new(),
        };
        c.outputs.push(Parameter { name: "status".to_string(), typ: Type::Int, size: 8 });
        c
    }

    /// Analyze a composition that depends on `helper` (the producer above).
    fn analyze_with_helper(source: &str) -> Result<(), Vec<SemanticError>> {
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let mut deps = HashMap::new();
        deps.insert("helper".to_string(), producer_contract());
        let patterns = HashSet::new();
        let mut analyzer = SemanticAnalyzer::new();
        analyzer.analyze(&behavior, &deps, &patterns)
    }

    #[test]
    fn test_unconsumed_output_fails() {
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  r = CALL helper
  SET done 0
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "an unconsumed CALL output must be rejected (E0511)");
    }

    #[test]
    fn test_discard_consumes_output() {
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  r = CALL helper
  DISCARD r
  SET done 0
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "DISCARD must satisfy output consumption");
    }

    #[test]
    fn test_output_consumed_by_branch_ok() {
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  r = CALL helper
  BRANCH r.status yes no

  LABEL yes
    SET done 1
    JUMP end

  LABEL no
    SET done 0
    JUMP end

  LABEL end
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "an output used as a BRANCH condition is consumed");
    }

    #[test]
    fn test_bare_call_with_outputs_fails() {
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  CALL helper
  SET done 0
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "a bare CALL that drops a non-sink output must be rejected");
    }

    #[test]
    fn test_redirect_into_output_ok() {
        // `CALL helper -> done` writes the composition's own OUTPUT (§7.1, §7.8):
        // it satisfies writes_output (no E0604) and consumes helper's result by
        // routing it into the output sink (no E0511). This is the `-> out`
        // redirect form the grammar defines but the checker used to reject.
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  CALL helper -> done
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "redirect into a declared OUTPUT is a consumption (no E0511)");
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::OutputNotWritten { .. })),
            "redirect into a declared OUTPUT satisfies writes_output (no E0604)");
    }

    #[test]
    fn test_redirect_into_local_must_be_consumed() {
        // `-> tmp` where tmp is NOT a declared OUTPUT produces a value that must
        // still be consumed downstream; unconsumed, it is E0511. Guards the
        // narrowing in the redirect fix to OUTPUT targets only.
        let source = r#"
BEHAVIOR comp

CONTRACT
  OUTPUT done int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH 00000000

COMPOSITION
  CALL helper -> tmp
  SET done 0
END
"#;
        let errors = analyze_with_helper(source).err().unwrap_or_default();
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UnconsumedOutput { .. })),
            "redirect into an unconsumed local must be rejected (E0511)");
    }

    // =========================================================================
    // no_alloc SCOPE handling + pure MEMORY soundness
    // =========================================================================

    #[test]
    fn test_no_alloc_scoped_is_ok() {
        // An ALLOC inside a SCOPE is freed at END_SCOPE, so no_alloc holds.
        let source = r#"
BEHAVIOR scoped-alloc

CONTRACT
  OUTPUT status int 8
  GUARANTEES no_alloc writes_output

HASH 00000000

IMPLEMENTATION
  SCOPE
    tmp = ALLOC 8 int
    FREE tmp
  END_SCOPE
  SET status 0
END
"#;
        let errors = analyze_source(source).err().unwrap_or_default();
        assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::NoAllocViolation { .. })),
            "ALLOC inside SCOPE must NOT violate no_alloc");
    }

    #[test]
    fn test_pure_with_memory_access_fails() {
        // A behavior that accesses pattern MEMORY cannot be pure.
        let source = r#"
BEHAVIOR pure-mem

CONTRACT
  OUTPUT status int 8
  MEMORY cache
  GUARANTEES pure writes_output

HASH 00000000

IMPLEMENTATION
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::PureViolation { .. })),
            "pure + pattern MEMORY access must be rejected");
    }

    #[test]
    fn test_output_not_written() {
        let source = r#"
BEHAVIOR no-write

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH nwrt1234

IMPLEMENTATION
  v = 42
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::OutputNotWritten { .. })));
    }

    // =========================================================================
    // Undefined Variables
    // =========================================================================

    #[test]
    fn test_undefined_variable() {
        let source = r#"
BEHAVIOR undef-var

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH undv1234

IMPLEMENTATION
  v = LOAD nonexistent 8
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UndefinedVariable { .. })));
    }

    // =========================================================================
    // Bounds Violations (Section 14.1)
    // =========================================================================

    #[test]
    fn test_bounds_violation() {
        let source = r#"
BEHAVIOR bounds-viol

CONTRACT
  INPUT data bytes 16
  OUTPUT status int 8
  GUARANTEES writes_output

HASH boun1234

IMPLEMENTATION
  v = LOAD data 8 20
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::PossibleBoundsViolation { .. })));
    }

    // =========================================================================
    // Output Size Mismatch (Section 14.3)
    // =========================================================================

    #[test]
    fn test_output_size_mismatch() {
        let source = r#"
BEHAVIOR out-size

CONTRACT
  OUTPUT result int 4
  GUARANTEES writes_output

HASH outs1234

IMPLEMENTATION
  STORE result 42 8
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::OutputSizeMismatch { expected: 4, found: 8, .. })));
    }

    // =========================================================================
    // Bytes Size Flexibility
    // =========================================================================

    #[test]
    fn test_bytes_smaller_than_declared_ok() {
        // Per Section 14.3: For bytes, passing smaller buffer is OK
        let source = r#"
BEHAVIOR bytes-flex

CONTRACT
  INPUT data bytes 256
  OUTPUT status int 8
  GUARANTEES writes_output

HASH 2065e68b

IMPLEMENTATION
  v = LOAD data 8
  SET status 0
END
"#;
        expect_success(source);
    }

    // =========================================================================
    // Valid Sizes
    // =========================================================================

    #[test]
    fn test_valid_int_sizes() {
        // Hash is 72fb80bd for OUTPUT status int 8, GUARANTEES writes_output
        for size in [1, 2, 4, 8] {
            let source = format!(r#"
BEHAVIOR valid-int-{}

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  v = IADD 1 2 {}
  SET status 0
END
"#, size, size);
            expect_success(&source);
        }
    }

    #[test]
    fn test_valid_float_sizes() {
        // Hash is 0a8b26fa for OUTPUT status float 8, GUARANTEES writes_output
        for size in [4, 8] {
            let source = format!(r#"
BEHAVIOR valid-float-{}

CONTRACT
  OUTPUT status float 8
  GUARANTEES writes_output

HASH ae461411

IMPLEMENTATION
  v = FADD 1.0 2.0 {}
  STORE status v 8
END
"#, size, size);
            expect_success(&source);
        }
    }

    // =========================================================================
    // Ownership Analysis Tests (SAFETY-RULES Part 2)
    // =========================================================================

    #[test]
    fn test_valid_alloc_and_free() {
        // Valid: ALLOC creates owned handle, owner can FREE
        let source = r#"
BEHAVIOR valid-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  h = ALLOC 64 bytes
  STORE h 42 8
  FREE h
  SET status 0
END
"#;
        expect_success(source);
    }

    #[test]
    fn test_double_free() {
        // Error: Cannot FREE twice
        let source = r#"
BEHAVIOR double-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH dblf1234

IMPLEMENTATION
  h = ALLOC 64 bytes
  FREE h
  FREE h
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::DoubleFree { .. })));
    }

    #[test]
    fn test_use_after_free() {
        // Error: Cannot use handle after FREE
        let source = r#"
BEHAVIOR use-after-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH uaf01234

IMPLEMENTATION
  h = ALLOC 64 bytes
  STORE h 42 8
  FREE h
  v = LOAD h 8
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UseAfterFree { .. })));
    }

    #[test]
    fn test_free_borrowed_input() {
        // Error: Cannot FREE borrowed INPUT handle
        let source = r#"
BEHAVIOR free-input

CONTRACT
  INPUT data bytes 64
  OUTPUT status int 8
  GUARANTEES writes_output

HASH frin1234

IMPLEMENTATION
  FREE data
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeBorrowedHandle { .. })));
    }

    #[test]
    fn test_free_borrowed_output() {
        // Error: Cannot FREE OUTPUT handle (caller-allocated, borrowed)
        let source = r#"
BEHAVIOR free-output

CONTRACT
  OUTPUT result int 8
  GUARANTEES writes_output

HASH frou1234

IMPLEMENTATION
  SET result 42
  FREE result
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeBorrowedHandle { .. })));
    }

    #[test]
    fn test_free_non_handle() {
        // Error: Cannot FREE something that's not a handle
        let source = r#"
BEHAVIOR free-value

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH frva1234

IMPLEMENTATION
  h = ALLOC 8 int
  v = LOAD h 8
  FREE h
  FREE v
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::TypeMismatch { expected, .. } if expected == "handle")));
    }

    #[test]
    fn test_store_after_free() {
        // Error: Cannot STORE to freed handle (use-after-free)
        let source = r#"
BEHAVIOR store-after-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH staf1234

IMPLEMENTATION
  h = ALLOC 64 bytes
  FREE h
  STORE h 42 8
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UseAfterFree { .. })));
    }

    // =========================================================================
    // SPAWN Borrow Tracking Tests (SAFETY-RULES Part 6)
    // =========================================================================

    #[test]
    fn test_free_while_borrowed_by_spawn() {
        // Per SAFETY-RULES Part 6: Cannot FREE handle while borrowed by SPAWN
        // "FREE buffer" while worker is using it should error
        let source = r#"
BEHAVIOR spawn-borrow

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH spbr1234

IMPLEMENTATION
  buffer = ALLOC 1024 bytes
  worker = SPAWN processor buffer
  FREE buffer
  SET status 0
END
"#;
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let deps = HashMap::new();
        let mut patterns = HashSet::new();
        patterns.insert("processor".to_string());
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &deps, &patterns);

        match result {
            Ok(()) => panic!("Expected FreeWhileBorrowed error"),
            Err(errors) => {
                assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeWhileBorrowed { .. })),
                    "Expected FreeWhileBorrowed error, got: {:?}", errors);
            }
        }
    }

    #[test]
    fn test_free_after_wait_ok() {
        // Per SAFETY-RULES Part 6: After WAIT, borrows are released
        // FREE buffer after WAIT should succeed
        let source = r#"
BEHAVIOR spawn-wait-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH swfr1234

IMPLEMENTATION
  buffer = ALLOC 1024 bytes
  worker = SPAWN processor buffer
  result = WAIT worker
  FREE buffer
  SET status 0
END
"#;
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let deps = HashMap::new();
        let mut patterns = HashSet::new();
        patterns.insert("processor".to_string());
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &deps, &patterns);

        if let Err(errors) = result {
            // Should not have FreeWhileBorrowed error
            assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeWhileBorrowed { .. })),
                "Should not have FreeWhileBorrowed after WAIT, got: {:?}", errors);
        }
    }

    #[test]
    fn test_multiple_borrows_same_handle() {
        // Multiple SPAWNs borrowing same handle - need all WAITs to FREE
        let source = r#"
BEHAVIOR multi-spawn

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH musp1234

IMPLEMENTATION
  buffer = ALLOC 1024 bytes
  w1 = SPAWN processor buffer
  w2 = SPAWN processor buffer
  r1 = WAIT w1
  FREE buffer
  SET status 0
END
"#;
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let deps = HashMap::new();
        let mut patterns = HashSet::new();
        patterns.insert("processor".to_string());
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &deps, &patterns);

        // Should error because w2 still holds borrow
        match result {
            Ok(()) => panic!("Expected FreeWhileBorrowed error (w2 still active)"),
            Err(errors) => {
                assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeWhileBorrowed { .. })),
                    "Expected FreeWhileBorrowed error, got: {:?}", errors);
            }
        }
    }

    #[test]
    fn test_all_waits_then_free_ok() {
        // All SPAWNs waited - FREE should succeed
        let source = r#"
BEHAVIOR all-wait-free

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH awfr1234

IMPLEMENTATION
  buffer = ALLOC 1024 bytes
  w1 = SPAWN processor buffer
  w2 = SPAWN processor buffer
  r1 = WAIT w1
  r2 = WAIT w2
  FREE buffer
  SET status 0
END
"#;
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let deps = HashMap::new();
        let mut patterns = HashSet::new();
        patterns.insert("processor".to_string());
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &deps, &patterns);

        if let Err(errors) = result {
            assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::FreeWhileBorrowed { .. })),
                "Should not error after all WAITs, got: {:?}", errors);
        }
    }

    // =========================================================================
    // SHARED Atomics Tests (Section 14.5)
    // =========================================================================

    #[test]
    fn test_non_atomic_load_shared() {
        // Error: Non-atomic LOAD on SHARED handle
        let source = r#"
BEHAVIOR shared-load

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH shld1234

IMPLEMENTATION
  h = ALLOC 8 int SHARED
  STORE h 42 8
  v = LOAD h 8
  FREE h
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::NonAtomicSharedAccess { operation, .. } if operation == "LOAD")),
            "Expected NonAtomicSharedAccess LOAD error, got: {:?}", errors);
    }

    #[test]
    fn test_non_atomic_store_shared() {
        // Error: Non-atomic STORE on SHARED handle
        let source = r#"
BEHAVIOR shared-store

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH shst1234

IMPLEMENTATION
  h = ALLOC 8 int SHARED
  STORE h 42 8
  FREE h
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::NonAtomicSharedAccess { operation, .. } if operation == "STORE")),
            "Expected NonAtomicSharedAccess STORE error, got: {:?}", errors);
    }

    #[test]
    fn test_non_shared_load_store_ok() {
        // OK: Regular LOAD/STORE on non-SHARED handle
        let source = r#"
BEHAVIOR non-shared

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH nosh1234

IMPLEMENTATION
  h = ALLOC 8 int
  STORE h 42 8
  v = LOAD h 8
  FREE h
  SET status 0
END
"#;
        // Should not error for NonAtomicSharedAccess
        let tokens = lex(source).expect("Lex failed");
        let behavior = parse(&tokens, source).expect("Parse failed");
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &HashMap::new(), &HashSet::new());

        if let Err(errors) = result {
            assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::NonAtomicSharedAccess { .. })),
                "Should not have NonAtomicSharedAccess error on non-SHARED handle, got: {:?}", errors);
        }
    }

    // =========================================================================
    // TDD Tests for Remaining Section 14 Rules
    // These tests are written BEFORE implementation per TDD methodology
    // =========================================================================

    // --- Section 14.1: Initialization (LOAD requires prior STORE) ---

    #[test]
    fn test_uninitialized_load_error() {
        // Error: LOAD without prior STORE (Section 14.1)
        let source = r#"
BEHAVIOR uninit-load

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH unil1234

IMPLEMENTATION
  h = ALLOC 8 int
  v = LOAD h 8
  FREE h
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UninitializedLoad { .. })),
            "Expected UninitializedLoad error, got: {:?}", errors);
    }

    #[test]
    fn test_initialized_load_ok() {
        // OK: LOAD after STORE
        let source = r#"
BEHAVIOR init-load

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  h = ALLOC 8 int
  STORE h 42 8
  v = LOAD h 8
  FREE h
  SET status 0
END
"#;
        expect_success(source);
    }

    #[test]
    fn test_uninitialized_load_on_some_path() {
        // Error: LOAD without STORE on one branch (Section 14.1 "on all paths")
        let source = r#"
BEHAVIOR uninit-path

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH unip1234

IMPLEMENTATION
  c = LOAD cond 8
  h = ALLOC 8 int
  BRANCH c do_store skip_store

  LABEL do_store
    STORE h 42 8
    JUMP read_it

  LABEL skip_store
    JUMP read_it

  LABEL read_it
    v = LOAD h 8
    FREE h
    SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UninitializedLoad { .. })),
            "Expected UninitializedLoad error (not stored on all paths), got: {:?}", errors);
    }

    // --- Section 14.1: No Leaks (Every ALLOC has FREE or is scoped) ---

    #[test]
    fn test_memory_leak_error() {
        // Error: ALLOC without FREE (Section 14.1)
        let source = r#"
BEHAVIOR leak

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH leak1234

IMPLEMENTATION
  h = ALLOC 64 bytes
  STORE h 42 8
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::MemoryLeak { .. })),
            "Expected MemoryLeak error, got: {:?}", errors);
    }

    #[test]
    fn test_no_leak_with_free() {
        // OK: ALLOC with FREE
        let source = r#"
BEHAVIOR no-leak

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  h = ALLOC 64 bytes
  STORE h 42 8
  FREE h
  SET status 0
END
"#;
        expect_success(source);
    }

    #[test]
    fn test_leak_on_some_path() {
        // Error: FREE on only one branch (Section 14.1 "every ALLOC has FREE")
        let source = r#"
BEHAVIOR leak-path

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH lkpt1234

IMPLEMENTATION
  c = LOAD cond 8
  h = ALLOC 64 bytes
  STORE h 42 8
  BRANCH c do_free skip_free

  LABEL do_free
    FREE h
    JUMP done

  LABEL skip_free
    JUMP done

  LABEL done
    SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::MemoryLeak { .. })),
            "Expected MemoryLeak error (not freed on all paths), got: {:?}", errors);
    }

    // --- Section 14.6: writes_output CFG-aware (all paths must write) ---

    #[test]
    fn test_output_not_written_on_some_path() {
        // Error: OUTPUT not written on one branch
        let source = r#"
BEHAVIOR output-path

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH oupt1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c write_output skip_output

  LABEL write_output
    SET status 0
    JUMP done

  LABEL skip_output
    JUMP done

  LABEL done
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::OutputNotWritten { .. })),
            "Expected OutputNotWritten error (not written on all paths), got: {:?}", errors);
    }

    #[test]
    fn test_output_written_on_all_paths() {
        // OK: OUTPUT written on all branches
        let source = r#"
BEHAVIOR output-all

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH d3a019bc

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c path_a path_b

  LABEL path_a
    SET status 1
    JUMP done

  LABEL path_b
    SET status 0
    JUMP done

  LABEL done
END
"#;
        expect_success(source);
    }

    // --- Section 14.5: Channel type matching ---

    #[test]
    fn test_channel_send_type_match() {
        // OK: Send int to int channel
        let source = r#"
BEHAVIOR chan-ok

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  ch = CHANNEL int 10
  CHANNEL_SEND ch 42
  CHANNEL_CLOSE ch
  SET status 0
END
"#;
        expect_success(source);
    }

    #[test]
    fn test_channel_send_type_mismatch() {
        // Error: Send float to int channel
        let source = r#"
BEHAVIOR chan-mismatch

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH chmm1234

IMPLEMENTATION
  ch = CHANNEL int 10
  f = FADD 1.0 2.0 8
  CHANNEL_SEND ch f
  CHANNEL_CLOSE ch
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::ChannelSendTypeMismatch { .. })),
            "Expected ChannelSendTypeMismatch error, got: {:?}", errors);
    }

    #[test]
    fn test_channel_undefined() {
        // Error: Channel not defined
        let source = r#"
BEHAVIOR chan-undef

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH chun1234

IMPLEMENTATION
  CHANNEL_SEND undefined_ch 42
  SET status 0
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::UndefinedVariable { .. })),
            "Expected UndefinedVariable error, got: {:?}", errors);
    }

    // =========================================================================
    // Diagnostic Conversion Tests
    // =========================================================================

    #[test]
    fn test_to_diagnostic_use_after_free() {
        let err = SemanticError {
            kind: ErrorKind::UseAfterFree {
                handle: "buffer".to_string(),
                freed_at: 10,
            },
            line: 15,
            context: Some("in LOAD expression".to_string()),
        };

        let diag = err.to_diagnostic();

        // Verify error code
        assert_eq!(diag.code, crate::diagnostic::ErrorCode::E0301);
        // Verify message contains handle name
        assert!(diag.message.contains("buffer"));
        assert!(diag.message.contains("after free"));
        // Verify help is provided
        assert!(diag.help.is_some());
        // Verify context is preserved as note
        assert!(diag.notes.iter().any(|n| n.contains("LOAD")));
    }

    #[test]
    fn test_to_diagnostic_double_free() {
        let err = SemanticError {
            kind: ErrorKind::DoubleFree {
                handle: "h1".to_string(),
                first_free_at: 20,
            },
            line: 25,
            context: None,
        };

        let diag = err.to_diagnostic();

        assert_eq!(diag.code, crate::diagnostic::ErrorCode::E0302);
        assert!(diag.message.contains("double free"));
        assert!(diag.message.contains("h1"));
        assert!(diag.help.is_some());
    }

    #[test]
    fn test_to_diagnostic_undefined_label() {
        let err = SemanticError {
            kind: ErrorKind::UndefinedLabel {
                name: "loop_end".to_string(),
            },
            line: 42,
            context: Some("BRANCH target".to_string()),
        };

        let diag = err.to_diagnostic();

        assert_eq!(diag.code, crate::diagnostic::ErrorCode::E0401);
        assert!(diag.message.contains("loop_end"));
        assert!(diag.labels.len() >= 1);
        assert_eq!(diag.labels[0].span.line, 42);
    }

    #[test]
    fn test_to_diagnostic_pure_violation() {
        let err = SemanticError {
            kind: ErrorKind::PureViolation {
                reason: "contains SYSCALL".to_string(),
            },
            line: 8,
            context: None,
        };

        let diag = err.to_diagnostic();

        assert_eq!(diag.code, crate::diagnostic::ErrorCode::E0601);
        assert!(diag.message.contains("pure"));
        assert!(diag.message.contains("SYSCALL"));
        assert!(diag.help.is_some());
    }

    #[test]
    fn test_to_diagnostic_output_not_written() {
        let err = SemanticError {
            kind: ErrorKind::OutputNotWritten {
                outputs: vec!["status".to_string(), "result".to_string()],
            },
            line: 0,
            context: None,
        };

        let diag = err.to_diagnostic();

        assert_eq!(diag.code, crate::diagnostic::ErrorCode::E0604);
        assert!(diag.message.contains("status"));
        assert!(diag.message.contains("result"));
    }

    // =========================================================================
    // Hash Verification Tests (Section 14.3)
    // =========================================================================

    #[test]
    fn test_hash_mismatch_error() {
        // Error: Declared hash doesn't match computed hash
        let source = r#"
BEHAVIOR hash-test

CONTRACT
  INPUT a int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH wronghash

IMPLEMENTATION
  v = LOAD a 8
  STORE result v 8
END
"#;
        let errors = expect_errors(source);
        assert!(errors.iter().any(|e| matches!(&e.kind, ErrorKind::HashMismatch { .. })),
            "Expected HashMismatch error, got: {:?}", errors);

        // Verify the error contains both declared and computed hash
        let hash_error = errors.iter().find(|e| matches!(&e.kind, ErrorKind::HashMismatch { .. })).unwrap();
        if let ErrorKind::HashMismatch { declared, computed } = &hash_error.kind {
            assert_eq!(declared, "wronghash");
            assert!(!computed.is_empty(), "Computed hash should not be empty");
            assert_ne!(declared, computed, "Declared and computed should differ");
        }
    }

    #[test]
    fn test_correct_hash_passes() {
        // First, compute the correct hash for this contract
        use crate::hash::compute_contract_hash;
        use crate::ast::{Contract, Parameter, Type, Guarantee, Requirements};

        let contract = Contract {
            inputs: vec![
                Parameter { name: "a".into(), typ: Type::Int, size: 8 },
            ],
            outputs: vec![
                Parameter { name: "result".into(), typ: Type::Int, size: 8 },
            ],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![Guarantee::WritesOutput],
        };
        let correct_hash = compute_contract_hash(&contract);

        // Now test with correct hash
        let source = format!(r#"
BEHAVIOR hash-correct

CONTRACT
  INPUT a int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH {}

IMPLEMENTATION
  v = LOAD a 8
  STORE result v 8
END
"#, correct_hash);

        // Should pass without HashMismatch error
        let tokens = lex(&source).expect("Lex failed");
        let behavior = parse(&tokens, &source).expect("Parse failed");
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &HashMap::new(), &HashSet::new());

        if let Err(errors) = result {
            assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::HashMismatch { .. })),
                "Should not have HashMismatch error with correct hash, got: {:?}", errors);
        }
    }

    #[test]
    fn test_empty_hash_skipped() {
        // Empty hash should not trigger validation (placeholder support)
        let source = r#"
BEHAVIOR empty-hash

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH

IMPLEMENTATION
  SET status 0
END
"#;
        // This might fail parsing or pass - either way, no HashMismatch
        // since empty hash is skipped
        let tokens = lex(source);
        if tokens.is_err() {
            return; // Empty hash might not parse, that's OK
        }
        let tokens = tokens.unwrap();
        let behavior = parse(&tokens, source);
        if behavior.is_err() {
            return; // Parse error is fine
        }
        let behavior = behavior.unwrap();

        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&behavior, &HashMap::new(), &HashSet::new());

        if let Err(errors) = result {
            assert!(!errors.iter().any(|e| matches!(&e.kind, ErrorKind::HashMismatch { .. })),
                "Empty hash should skip validation, got: {:?}", errors);
        }
    }
}
