//! Behavior Graph Module
//!
//! The graph is the primary representation of a Sigil program.
//! Behaviors are nodes with typed ports. Connections between ports are edges.
//! Unconnected ports are gaps — each gap generates a question.
//!
//! The graph model supports incomplete programs as first-class state.
//! A complete graph (zero gaps) can be compiled to a binary.
//! An incomplete graph is a specification in progress.
//!
//! The graph is the program's representation, and it is built and verified on
//! every compile — not a future direction. `builder::behavior_to_node` /
//! `contract_to_ref` construct a `ProjectGraph` (behaviors = nodes identified by
//! their content-addressed contract hash, contract inputs/outputs = typed ports,
//! `REQUIRES@hash` + composition wiring = edges), and `validate::validate_graph`
//! enforces transitive purity, circular-dependency rejection, and
//! dependency-hash-pin consistency over it — wired into the compile pipeline in
//! `main.rs` (Phase 5.5).
//!
//! What is *not* yet wired into V1 is the **explicit edge** model (`add_edge`,
//! the `edges` vector) and the gap detector built on it (`gaps::find_gaps`,
//! `topological_sort`, `completeness_pct`). V1 derives a composition's wiring
//! implicitly from its CALL references rather than materializing explicit `Edge`
//! values, so this machinery is built and unit-tested here but unused by the V1
//! binary — hence the `allow(dead_code)` below. Materializing the wiring as edges
//! so the gap detector runs over them is the next increment on a working
//! foundation, not a missing one.
#![allow(dead_code)]

pub mod gaps;
pub mod validate;
pub mod builder;

use std::collections::HashMap;
use std::path::PathBuf;
use crate::ast;
use crate::hash;

// =============================================================================
// Identity Types
// =============================================================================

/// Unique identifier for a behavior node within a project graph.
pub type NodeId = u64;

/// Unique identifier for an edge within a project graph.
pub type EdgeId = u64;

// =============================================================================
// Port — Typed endpoint on a behavior
// =============================================================================

/// Direction of data flow through a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Input: data flows into the behavior
    In,
    /// Output: data flows out of the behavior on success
    Out,
    /// Error: data flows out on failure (V2 structured error handling)
    Error,
}

/// A typed port on a behavior node.
///
/// Ports are the connection points. An input port must be sourced by an edge
/// or a constant. An output/error port must be consumed by an edge or
/// explicitly discarded. Unconnected ports are gaps.
#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub direction: Direction,
    pub interpretation: ast::Type,
    pub size: usize,
    /// Whether this output has been explicitly discarded (-> _)
    pub discarded: bool,
}

impl Port {
    pub fn input(name: impl Into<String>, typ: ast::Type, size: usize) -> Self {
        Port {
            name: name.into(),
            direction: Direction::In,
            interpretation: typ,
            size,
            discarded: false,
        }
    }

    pub fn output(name: impl Into<String>, typ: ast::Type, size: usize) -> Self {
        Port {
            name: name.into(),
            direction: Direction::Out,
            interpretation: typ,
            size,
            discarded: false,
        }
    }

    pub fn error(name: impl Into<String>, typ: ast::Type, size: usize) -> Self {
        Port {
            name: name.into(),
            direction: Direction::Error,
            interpretation: typ,
            size,
            discarded: false,
        }
    }
}

// =============================================================================
// Edge — Connection between ports
// =============================================================================

/// A connection from one port to another.
///
/// Edges carry data from an output/error port of one behavior to an input
/// port of another. Type and size must match across the edge.
#[derive(Debug, Clone)]
pub struct Edge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub from_port: String,
    pub to_node: NodeId,
    pub to_port: String,
}

// =============================================================================
// Gap — Unconnected port (a question)
// =============================================================================

/// A structural gap in the behavior graph.
///
/// Every gap is a question. When zero gaps remain, the specification
/// is complete — not by judgment but by structure.
#[derive(Debug, Clone)]
pub struct Gap {
    pub node_id: NodeId,
    pub node_name: String,
    pub port_name: String,
    pub direction: Direction,
    pub port_type: ast::Type,
    pub port_size: usize,
    /// Human-readable question generated from the gap context
    pub question: String,
}

// =============================================================================
// DependencyRef — Pinned version reference
// =============================================================================

/// A reference to a dependency behavior, pinned by contract hash.
///
/// The hash IS the version. If the dependency's contract changes,
/// the hash changes, and this reference becomes stale — a gap.
#[derive(Debug, Clone)]
pub struct DependencyRef {
    pub name: String,
    pub hash: String,
}

// =============================================================================
// ExamplePair — Semantic test case
// =============================================================================

/// An input/output example pair for semantic testing.
///
/// Examples are part of the contract. They serve triple duty:
/// documentation (what the behavior does), testing (verify
/// implementation), and elicitation (AI asks for examples).
#[derive(Debug, Clone)]
pub struct ExamplePair {
    pub inputs: Vec<ExampleValue>,
    pub outputs: Vec<ExampleValue>,
}

/// A value in an example pair.
#[derive(Debug, Clone)]
pub enum ExampleValue {
    Int(i64),
    Float(f64),
    Bytes(Vec<u8>),
    Str(String),
}

// =============================================================================
// BehaviorKind — What type of behavior this is
// =============================================================================

/// The kind of behavior determines how it fulfills its contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BehaviorKind {
    /// Leaf: implements computation using primitives (expressions, control flow).
    /// Contains actual algorithmic logic.
    Leaf,
    /// Composite: wires other behaviors together. No new computation.
    /// The wiring IS the implementation.
    Composite,
    /// Native: implemented in C runtime. Contract only, no Sigil implementation.
    Native,
}

// =============================================================================
// Implementation — The body of a behavior
// =============================================================================

/// The implementation body of a behavior.
///
/// For now, stores V1 AST nodes. Phase 2 adds V2 expression AST
/// and a desugarer that converts V2 → V1 AST for the existing codegen.
#[derive(Debug, Clone)]
pub enum Implementation {
    /// Leaf implementation: primitive operations
    Leaf(Vec<ast::Node>),
    /// Composite implementation: wiring (CALL, BRANCH, SET, etc.)
    Composite(Vec<ast::Node>),
}

// =============================================================================
// BehaviorNode — A behavior in the graph
// =============================================================================

/// A behavior node in the project graph.
///
/// A behavior is a contract that specifies a unit of computation.
/// The contract IS the behavior. Implementation merely fulfills it.
#[derive(Debug, Clone)]
pub struct BehaviorNode {
    pub id: NodeId,
    pub name: String,
    /// Contract hash — auto-computed from ports + guarantees + requires.
    /// Written to files by graph server, never by AI.
    pub hash: String,
    pub kind: BehaviorKind,
    pub ports: Vec<Port>,
    pub guarantees: Vec<ast::Guarantee>,
    pub requires: Vec<DependencyRef>,
    pub implementation: Option<Implementation>,
    pub examples: Vec<ExamplePair>,
    pub description: Option<String>,
    /// Source file path (for error reporting and file sync)
    pub source_path: Option<PathBuf>,
}

impl BehaviorNode {
    /// Create a new empty behavior node.
    pub fn new(id: NodeId, name: impl Into<String>, kind: BehaviorKind) -> Self {
        BehaviorNode {
            id,
            name: name.into(),
            hash: String::new(),
            kind,
            ports: Vec::new(),
            guarantees: Vec::new(),
            requires: Vec::new(),
            implementation: None,
            examples: Vec::new(),
            description: None,
            source_path: None,
        }
    }

    /// Get all input ports.
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.direction == Direction::In)
    }

    /// Get all output ports (not including error ports).
    pub fn outputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.direction == Direction::Out)
    }

    /// Get all error ports.
    pub fn errors(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.direction == Direction::Error)
    }

    /// Get a port by name.
    pub fn port(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.name == name)
    }

    /// Compute the contract hash from current ports, guarantees, and requires.
    /// Uses the existing hash module for SHA-256 computation.
    pub fn compute_hash(&self) -> String {
        let contract = self.to_ast_contract();
        hash::compute_contract_hash(&contract)
    }

    /// Recompute and update the stored hash. Returns true if hash changed.
    pub fn update_hash(&mut self) -> bool {
        let new_hash = self.compute_hash();
        if new_hash != self.hash {
            self.hash = new_hash;
            true
        } else {
            false
        }
    }

    /// Convert ports/guarantees/requires to V1 ast::Contract for hash computation
    /// and compatibility with existing compiler pipeline.
    pub fn to_ast_contract(&self) -> ast::Contract {
        let inputs = self.inputs().map(|p| ast::Parameter {
            name: p.name.clone(),
            typ: p.interpretation.clone(),
            size: p.size,
        }).collect();

        let outputs = self.outputs().chain(self.errors()).map(|p| ast::Parameter {
            name: p.name.clone(),
            typ: p.interpretation.clone(),
            size: p.size,
        }).collect();

        let behaviors = self.requires.iter().map(|r| ast::BehaviorRef {
            name: r.name.clone(),
            hash: r.hash.clone(),
        }).collect();

        ast::Contract {
            inputs,
            outputs,
            requires: ast::Requirements {
                behaviors,
                memory: Vec::new(),
            },
            guarantees: self.guarantees.clone(),
        }
    }

    /// Convert to a V1 ast::Behavior for compatibility with existing codegen.
    pub fn to_ast_behavior(&self) -> ast::Behavior {
        let mut behavior = ast::Behavior::new(self.name.clone());
        behavior.contract = self.to_ast_contract();
        behavior.hash = self.hash.clone();
        behavior.description = self.description.clone();
        behavior.is_native = self.kind == BehaviorKind::Native;

        match &self.implementation {
            Some(Implementation::Leaf(nodes)) => {
                behavior.implementations.push(ast::PlatformImpl {
                    platform: None,
                    nodes: nodes.clone(),
                });
            }
            Some(Implementation::Composite(nodes)) => {
                behavior.composition = Some(ast::Composition {
                    nodes: nodes.clone(),
                });
            }
            None => {}
        }

        behavior
    }
}

// =============================================================================
// ContractRef — Read-only library behavior reference
// =============================================================================

/// A read-only reference to a library behavior.
///
/// Contains only the contract (ports, guarantees, hash). No implementation.
/// The implementation is already compiled in the library .lib file.
#[derive(Debug, Clone)]
pub struct ContractRef {
    pub name: String,
    pub hash: String,
    pub ports: Vec<Port>,
    pub guarantees: Vec<ast::Guarantee>,
    pub library_path: PathBuf,
}

impl ContractRef {
    /// Get a port by name.
    pub fn port(&self, name: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.name == name)
    }

    /// Get all input ports.
    pub fn inputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.direction == Direction::In)
    }

    /// Get all output ports.
    pub fn outputs(&self) -> impl Iterator<Item = &Port> {
        self.ports.iter().filter(|p| p.direction == Direction::Out)
    }
}

// =============================================================================
// ProjectGraph — The full program graph
// =============================================================================

/// The project-level behavior graph.
///
/// Contains local (editable) behaviors and library (read-only) references.
/// Edges connect ports between behaviors. Gaps are computed on demand.
pub struct ProjectGraph {
    /// Local behaviors — full detail, mutable
    pub nodes: HashMap<NodeId, BehaviorNode>,
    /// Library behaviors — contract only, read-only, already compiled
    pub library_nodes: HashMap<String, ContractRef>,
    /// Edges between ports
    pub edges: Vec<Edge>,
    /// Counter for generating unique node IDs
    next_node_id: u64,
    /// Counter for generating unique edge IDs
    next_edge_id: u64,
}

impl ProjectGraph {
    /// Create an empty project graph.
    pub fn new() -> Self {
        ProjectGraph {
            nodes: HashMap::new(),
            library_nodes: HashMap::new(),
            edges: Vec::new(),
            next_node_id: 1,
            next_edge_id: 1,
        }
    }

    /// Allocate a new unique node ID.
    pub fn next_node_id(&mut self) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Allocate a new unique edge ID.
    fn next_edge_id(&mut self) -> EdgeId {
        let id = self.next_edge_id;
        self.next_edge_id += 1;
        id
    }

    /// Add a behavior node to the graph. Returns the node ID.
    pub fn add_node(&mut self, mut node: BehaviorNode) -> NodeId {
        let id = node.id;
        node.update_hash();
        self.nodes.insert(id, node);
        id
    }

    /// Remove a behavior node and all its edges.
    pub fn remove_node(&mut self, id: NodeId) -> Option<BehaviorNode> {
        self.edges.retain(|e| e.from_node != id && e.to_node != id);
        self.nodes.remove(&id)
    }

    /// Get a behavior node by ID.
    pub fn node(&self, id: NodeId) -> Option<&BehaviorNode> {
        self.nodes.get(&id)
    }

    /// Get a mutable behavior node by ID.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut BehaviorNode> {
        self.nodes.get_mut(&id)
    }

    /// Find a node by name (local nodes only).
    pub fn find_node(&self, name: &str) -> Option<&BehaviorNode> {
        self.nodes.values().find(|n| n.name == name)
    }

    /// Find a node ID by name.
    pub fn find_node_id(&self, name: &str) -> Option<NodeId> {
        self.nodes.values().find(|n| n.name == name).map(|n| n.id)
    }

    /// Add a library contract reference.
    pub fn add_library_node(&mut self, contract: ContractRef) {
        self.library_nodes.insert(contract.name.clone(), contract);
    }

    /// Look up any behavior by name — local first, then libraries.
    pub fn lookup(&self, name: &str) -> Option<BehaviorLookup<'_>> {
        if let Some(node) = self.find_node(name) {
            Some(BehaviorLookup::Local(node))
        } else if let Some(contract) = self.library_nodes.get(name) {
            Some(BehaviorLookup::Library(contract))
        } else {
            None
        }
    }

    /// Add an edge between two ports. Returns Ok(edge_id) or Err with reason.
    pub fn add_edge(
        &mut self,
        from_node: NodeId,
        from_port: &str,
        to_node: NodeId,
        to_port: &str,
    ) -> Result<EdgeId, EdgeError> {
        // Validate source port exists and is Out or Error
        let from_p = self.resolve_port(from_node, from_port)?;
        if from_p.direction == Direction::In {
            return Err(EdgeError::WrongDirection {
                node_name: self.node_name(from_node),
                port: from_port.to_string(),
                expected: "Out or Error",
            });
        }
        let from_type = from_p.interpretation.clone();
        let from_size = from_p.size;

        // Validate target port exists and is In
        let to_p = self.resolve_port(to_node, to_port)?;
        if to_p.direction != Direction::In {
            return Err(EdgeError::WrongDirection {
                node_name: self.node_name(to_node),
                port: to_port.to_string(),
                expected: "In",
            });
        }
        let to_type = to_p.interpretation.clone();
        let to_size = to_p.size;

        // Type match
        if from_type != to_type {
            return Err(EdgeError::TypeMismatch {
                from: format!("{:?}", from_type),
                to: format!("{:?}", to_type),
            });
        }

        // Size match
        if from_size != to_size {
            return Err(EdgeError::SizeMismatch {
                from: from_size,
                to: to_size,
            });
        }

        // Check target port isn't already sourced (single-source rule)
        if self.edges.iter().any(|e| e.to_node == to_node && e.to_port == to_port) {
            return Err(EdgeError::AlreadyConnected {
                node_name: self.node_name(to_node),
                port: to_port.to_string(),
            });
        }

        let id = self.next_edge_id();
        self.edges.push(Edge {
            id,
            from_node,
            from_port: from_port.to_string(),
            to_node,
            to_port: to_port.to_string(),
        });

        Ok(id)
    }

    /// Remove an edge by ID.
    pub fn remove_edge(&mut self, id: EdgeId) -> bool {
        let len_before = self.edges.len();
        self.edges.retain(|e| e.id != id);
        self.edges.len() < len_before
    }

    /// Check if a port has an outgoing edge (for output/error ports).
    pub fn port_has_consumer(&self, node_id: NodeId, port_name: &str) -> bool {
        self.edges.iter().any(|e| e.from_node == node_id && e.from_port == port_name)
    }

    /// Check if a port has an incoming edge (for input ports).
    pub fn port_has_source(&self, node_id: NodeId, port_name: &str) -> bool {
        self.edges.iter().any(|e| e.to_node == node_id && e.to_port == port_name)
    }

    /// Get the number of local behaviors.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Compute completeness as a percentage (connected ports / total ports).
    pub fn completeness_pct(&self) -> f64 {
        let gaps = self.find_gaps();
        let total_ports: usize = self.nodes.values()
            .map(|n| n.ports.len())
            .sum();
        if total_ports == 0 {
            return 100.0;
        }
        let connected = total_ports - gaps.len();
        (connected as f64 / total_ports as f64) * 100.0
    }

    /// Find all gaps in the graph. Delegates to gaps module.
    pub fn find_gaps(&self) -> Vec<Gap> {
        gaps::find_gaps(self)
    }

    /// Check if the graph is complete (no gaps).
    pub fn is_complete(&self) -> bool {
        self.find_gaps().is_empty()
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    /// Resolve a port on a node (local or library).
    fn resolve_port(&self, node_id: NodeId, port_name: &str) -> Result<Port, EdgeError> {
        if let Some(node) = self.nodes.get(&node_id) {
            node.port(port_name).cloned().ok_or_else(|| EdgeError::PortNotFound {
                node_name: node.name.clone(),
                port: port_name.to_string(),
            })
        } else {
            Err(EdgeError::NodeNotFound(node_id))
        }
    }

    /// Get a node's name by ID (for error messages).
    fn node_name(&self, id: NodeId) -> String {
        self.nodes.get(&id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("<node {}>", id))
    }
}

impl Default for ProjectGraph {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Lookup result — local or library
// =============================================================================

/// Result of looking up a behavior by name.
pub enum BehaviorLookup<'a> {
    Local(&'a BehaviorNode),
    Library(&'a ContractRef),
}

impl<'a> BehaviorLookup<'a> {
    /// Get the contract hash.
    pub fn hash(&self) -> &str {
        match self {
            BehaviorLookup::Local(n) => &n.hash,
            BehaviorLookup::Library(c) => &c.hash,
        }
    }

    /// Get a port by name.
    pub fn port(&self, name: &str) -> Option<&Port> {
        match self {
            BehaviorLookup::Local(n) => n.port(name),
            BehaviorLookup::Library(c) => c.port(name),
        }
    }
}

// =============================================================================
// Edge errors
// =============================================================================

/// Errors that can occur when adding an edge.
#[derive(Debug, Clone)]
pub enum EdgeError {
    NodeNotFound(NodeId),
    PortNotFound { node_name: String, port: String },
    WrongDirection { node_name: String, port: String, expected: &'static str },
    TypeMismatch { from: String, to: String },
    SizeMismatch { from: usize, to: usize },
    AlreadyConnected { node_name: String, port: String },
}

impl std::fmt::Display for EdgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeError::NodeNotFound(id) => write!(f, "Node {} not found", id),
            EdgeError::PortNotFound { node_name, port } =>
                write!(f, "Port '{}' not found on '{}'", port, node_name),
            EdgeError::WrongDirection { node_name, port, expected } =>
                write!(f, "Port '{}' on '{}' must be {} direction", port, node_name, expected),
            EdgeError::TypeMismatch { from, to } =>
                write!(f, "Type mismatch: {} -> {}", from, to),
            EdgeError::SizeMismatch { from, to } =>
                write!(f, "Size mismatch: {} bytes -> {} bytes", from, to),
            EdgeError::AlreadyConnected { node_name, port } =>
                write!(f, "Input port '{}' on '{}' already has a source", port, node_name),
        }
    }
}
