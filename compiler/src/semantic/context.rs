//! Analysis context for semantic validation
//!
//! Contains state tracked during semantic analysis including variable info,
//! ownership tracking, and scope management.

use std::collections::{HashMap, HashSet};
use crate::ast::{Behavior, Type, Parameter, Guarantee, Contract};

// =============================================================================
// Variable Origin and State
// =============================================================================

/// Origin of a variable - tracks where it came from for bounds checking
#[derive(Debug, Clone)]
pub enum VarOrigin {
    /// From INPUT declaration - size from contract
    Input { declared_size: usize },
    /// From OUTPUT declaration - size from contract
    Output { declared_size: usize },
    /// From ALLOC expression - size may be known at compile time
    Alloc { allocated_size: Option<usize> },
    /// From LOAD result - no allocation, just a value
    LoadResult,
    /// From computation (arithmetic, comparison, etc.)
    Computed,
    /// From CALL result - structured output
    CallResult { total_size: Option<usize> },
}

/// Handle ownership state per SAFETY-RULES Part 2
///
/// Tracks the lifecycle of handles for ownership analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleState {
    /// Handle is owned by current behavior (can FREE, can use)
    /// Created by ALLOC within this behavior
    Owned,
    /// Handle was borrowed from caller via INPUT (cannot FREE, can use)
    /// Per Section 3.3: "Borrowing: Input handles are borrowed; callee cannot free them"
    Borrowed,
    /// Handle has been freed (cannot use, cannot FREE again)
    /// Per Section 4.1 FREE: "Postcondition: Handle is invalid; further use is a compile error"
    Freed { at_line: usize },
    /// Not a handle (computed value, immediate, etc.) - ownership not applicable
    NotAHandle,
}

/// Active borrow tracking for SPAWN per SAFETY-RULES Part 6
///
/// When a handle is passed to SPAWN, it is borrowed until WAIT.
/// Per Section 14.5: "Borrow across spawn: Borrowed handle valid for spawn lifetime"
#[derive(Debug, Clone)]
pub struct ActiveBorrow {
    /// The pattern that borrowed this handle
    pub pattern_name: String,
    /// Line where SPAWN occurred
    pub spawn_line: usize,
    /// The spawn handle variable name (for tracking WAIT)
    pub spawn_handle: String,
}

/// Variable information for semantic analysis
///
/// Per Sigil Reference Section 14.1, we must verify:
/// - Bounds: offset + size <= allocated_size
/// Per SAFETY-RULES Part 2, we must verify:
/// - Ownership: Only owner can FREE
/// - Lifetime: Handle valid only until FREE
/// Per Section 14.5, we must verify:
/// - SHARED handles require atomic operations
/// - Channel send/receive types match declaration
#[derive(Debug, Clone)]
pub struct VarInfo {
    /// Type interpretation (int, float, bytes)
    pub typ: Option<Type>,
    /// Size of the data
    pub size: usize,
    /// Whether the variable has been initialized
    #[allow(dead_code)]
    pub initialized: bool,
    /// Origin of this variable - determines allocated_size for bounds checking
    pub origin: VarOrigin,
    /// Ownership state for handle lifecycle tracking
    pub state: HandleState,
    /// Whether this is a SHARED allocation (requires atomic access)
    pub shared: bool,
    /// For channels: the element type (Section 14.5 channel type matching)
    pub channel_element_type: Option<Type>,
}

impl VarInfo {
    /// Get the allocated size if known, for bounds checking
    pub fn allocated_size(&self) -> Option<usize> {
        match &self.origin {
            VarOrigin::Input { declared_size } => Some(*declared_size),
            VarOrigin::Output { declared_size } => Some(*declared_size),
            VarOrigin::Alloc { allocated_size } => *allocated_size,
            VarOrigin::CallResult { total_size } => *total_size,
            VarOrigin::LoadResult | VarOrigin::Computed => None,
        }
    }
}

// =============================================================================
// Analysis Context
// =============================================================================

/// Tracks state during semantic analysis
pub struct AnalysisContext {
    /// Defined labels: name -> line number
    pub labels: HashMap<String, usize>,

    /// Variables in scope: name -> (type, size, initialized)
    pub variables: HashMap<String, VarInfo>,

    /// Input parameters
    pub inputs: HashMap<String, Parameter>,

    /// Output parameters
    pub outputs: HashMap<String, Parameter>,

    /// Outputs that have been written
    pub written_outputs: HashSet<String>,

    /// Declared guarantees
    pub guarantees: Vec<Guarantee>,

    /// Current line (approximate)
    pub current_line: usize,

    /// Has ALLOC outside SCOPE
    pub has_unscoped_alloc: bool,

    /// Current SCOPE nesting depth (0 = not inside any SCOPE). An ALLOC inside a
    /// SCOPE is freed at END_SCOPE, so it does NOT count as unscoped for no_alloc.
    pub scope_depth: usize,

    /// Has CHANNEL operations (for pure guarantee)
    pub has_channel_ops: bool,

    /// Has writes to non-OUTPUT variables (for pure guarantee)
    pub has_non_output_writes: bool,

    /// Dependency contracts for CALL validation (Section 14.3)
    pub dependency_contracts: HashMap<String, Contract>,

    /// Known pattern names for SPAWN validation (Section 14.5)
    pub pattern_names: HashSet<String>,

    /// Active spawn borrows: handle_name -> list of active borrows
    /// Per SAFETY-RULES Part 6: Track handles borrowed by SPAWN until WAIT
    pub spawn_borrows: HashMap<String, Vec<ActiveBorrow>>,
}

impl AnalysisContext {
    pub fn new(behavior: &Behavior, dependencies: &HashMap<String, Contract>, patterns: &HashSet<String>) -> Self {
        let mut ctx = AnalysisContext {
            labels: HashMap::new(),
            variables: HashMap::new(),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            written_outputs: HashSet::new(),
            guarantees: behavior.contract.guarantees.clone(),
            current_line: 1,
            has_unscoped_alloc: false,
            scope_depth: 0,
            has_channel_ops: false,
            has_non_output_writes: false,
            dependency_contracts: dependencies.clone(),
            pattern_names: patterns.clone(),
            spawn_borrows: HashMap::new(),
        };

        // Register inputs as initialized variables with known allocation size
        // Per Section 3.3: "Borrowing: Input handles are borrowed; callee cannot free them"
        for input in &behavior.contract.inputs {
            ctx.inputs.insert(input.name.clone(), input.clone());
            ctx.variables.insert(input.name.clone(), VarInfo {
                typ: Some(input.typ.clone()),
                size: input.size,
                initialized: true,
                origin: VarOrigin::Input { declared_size: input.size },
                state: HandleState::Borrowed, // INPUTs are borrowed from caller
                shared: false, // INPUTs are not SHARED by default
                channel_element_type: None, // INPUTs are not channels
            });
        }

        // Register outputs as uninitialized variables with known allocation size
        // OUTPUTs are caller-allocated handles passed to behavior - also borrowed
        for output in &behavior.contract.outputs {
            ctx.outputs.insert(output.name.clone(), output.clone());
            ctx.variables.insert(output.name.clone(), VarInfo {
                typ: Some(output.typ.clone()),
                size: output.size,
                initialized: false,
                origin: VarOrigin::Output { declared_size: output.size },
                state: HandleState::Borrowed, // OUTPUTs are also borrowed from caller
                shared: false, // OUTPUTs are not SHARED by default
                channel_element_type: None, // OUTPUTs are not channels
            });
        }

        ctx
    }

    pub fn define_label(&mut self, name: &str, line: usize) -> Option<usize> {
        if let Some(&first) = self.labels.get(name) {
            Some(first) // Return first definition line for error
        } else {
            self.labels.insert(name.to_string(), line);
            None
        }
    }

    pub fn label_exists(&self, name: &str) -> bool {
        self.labels.contains_key(name)
    }

    pub fn define_var(&mut self, name: &str, typ: Option<Type>, size: usize, origin: VarOrigin, state: HandleState, shared: bool, channel_element_type: Option<Type>) {
        self.variables.insert(name.to_string(), VarInfo {
            typ,
            size,
            initialized: true,
            origin,
            state,
            shared,
            channel_element_type,
        });
    }

    /// Get variable info for bounds checking
    pub fn get_var(&self, name: &str) -> Option<&VarInfo> {
        self.variables.get(name)
    }

    /// Get mutable variable info for state updates
    #[allow(dead_code)]
    pub fn get_var_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        self.variables.get_mut(name)
    }

    pub fn var_exists(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Mark a handle as freed per SAFETY-RULES Part 2
    #[allow(dead_code)]
    pub fn mark_freed(&mut self, name: &str, line: usize) {
        if let Some(info) = self.variables.get_mut(name) {
            info.state = HandleState::Freed { at_line: line };
        }
    }

    /// Check if a handle is currently borrowed by any active spawn
    pub fn get_active_borrow(&self, handle_name: &str) -> Option<&ActiveBorrow> {
        self.spawn_borrows.get(handle_name).and_then(|borrows| borrows.first())
    }

    /// Record that a handle is borrowed by a SPAWN
    pub fn add_spawn_borrow(&mut self, handle_name: &str, pattern_name: &str, spawn_line: usize, spawn_handle: &str) {
        self.spawn_borrows
            .entry(handle_name.to_string())
            .or_insert_with(Vec::new)
            .push(ActiveBorrow {
                pattern_name: pattern_name.to_string(),
                spawn_line,
                spawn_handle: spawn_handle.to_string(),
            });
    }

    /// Release borrows associated with a spawn handle (when WAIT is called)
    pub fn release_spawn_borrows(&mut self, spawn_handle: &str) {
        for borrows in self.spawn_borrows.values_mut() {
            borrows.retain(|b| b.spawn_handle != spawn_handle);
        }
    }

    pub fn mark_output_written(&mut self, name: &str) {
        if self.outputs.contains_key(name) {
            self.written_outputs.insert(name.to_string());
        }
    }

    #[allow(dead_code)]
    pub fn unwritten_outputs(&self) -> Vec<String> {
        self.outputs.keys()
            .filter(|k| !self.written_outputs.contains(*k))
            .cloned()
            .collect()
    }

    pub fn has_guarantee(&self, g: &Guarantee) -> bool {
        self.guarantees.contains(g)
    }
}
