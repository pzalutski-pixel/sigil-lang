//! Graph Validation Module
//!
//! Edge-level validation checks that run on the full ProjectGraph.
//! These complement node-level validation (semantic/mod.rs) which
//! checks individual behaviors. Graph validation checks the
//! relationships BETWEEN behaviors.

use std::collections::{HashMap, HashSet, VecDeque};
use crate::ast;
use super::{ProjectGraph, NodeId};

// =============================================================================
// Validation Result
// =============================================================================

/// A validation error at the graph level.
#[derive(Debug, Clone)]
pub struct GraphError {
    pub kind: GraphErrorKind,
    /// Name of the behavior where the error was detected
    pub behavior: String,
    /// Human-readable message
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum GraphErrorKind {
    /// Output port has no consumer and is not discarded
    UnconsumedOutput { port: String },
    /// Input port has no source
    UnsourcedInput { port: String },
    /// Error port has no handler and is not discarded
    UnhandledError { port: String },
    /// Required dependency not found (local or library)
    UndefinedDependency { name: String },
    /// Dependency hash doesn't match current version
    StaleDependency { name: String, expected: String, actual: String },
    /// Circular dependency detected
    CircularDependency { cycle: Vec<String> },
    /// Type mismatch on an edge
    EdgeTypeMismatch { from_behavior: String, from_port: String, to_port: String },
    /// Size mismatch on an edge
    EdgeSizeMismatch { from_behavior: String, from_port: String, to_port: String },
    /// Pure behavior calls impure dependency
    TransitivePurityViolation { impure_dep: String },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.behavior, self.message)
    }
}

// =============================================================================
// Full Validation
// =============================================================================

/// Run all graph-level validation checks.
/// Returns a list of errors (empty = valid).
pub fn validate_graph(graph: &ProjectGraph) -> Vec<GraphError> {
    let mut errors = Vec::new();

    check_dependencies_resolve(graph, &mut errors);
    check_hash_consistency(graph, &mut errors);
    check_circular_dependencies(graph, &mut errors);
    check_transitive_purity(graph, &mut errors);

    errors
}

// =============================================================================
// Individual Checks
// =============================================================================

/// Check that all dependencies in REQUIRES clauses resolve to known behaviors.
fn check_dependencies_resolve(graph: &ProjectGraph, errors: &mut Vec<GraphError>) {
    for node in graph.nodes.values() {
        for dep in &node.requires {
            if graph.lookup(&dep.name).is_none() {
                errors.push(GraphError {
                    kind: GraphErrorKind::UndefinedDependency { name: dep.name.clone() },
                    behavior: node.name.clone(),
                    message: format!(
                        "Required behavior '{}' not found in local behaviors or libraries",
                        dep.name
                    ),
                });
            }
        }
    }
}

/// Check that dependency hashes match current versions.
fn check_hash_consistency(graph: &ProjectGraph, errors: &mut Vec<GraphError>) {
    for node in graph.nodes.values() {
        for dep in &node.requires {
            if dep.hash.is_empty() {
                continue; // No hash pin — skip version check
            }

            if let Some(found) = graph.lookup(&dep.name) {
                let actual_hash = found.hash();
                if !actual_hash.is_empty() && dep.hash != actual_hash {
                    errors.push(GraphError {
                        kind: GraphErrorKind::StaleDependency {
                            name: dep.name.clone(),
                            expected: dep.hash.clone(),
                            actual: actual_hash.to_string(),
                        },
                        behavior: node.name.clone(),
                        message: format!(
                            "Dependency '{}' hash mismatch: expects @{} but found @{}. \
                             The dependency's contract has changed.",
                            dep.name, dep.hash, actual_hash
                        ),
                    });
                }
            }
        }
    }
}

/// Check for circular dependencies in the behavior graph.
///
/// Uses topological sort — if it fails, there's a cycle.
fn check_circular_dependencies(graph: &ProjectGraph, errors: &mut Vec<GraphError>) {
    // Build adjacency list: behavior name → set of dependency names
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();

    for node in graph.nodes.values() {
        adj.entry(node.name.as_str()).or_default();
        in_degree.entry(node.name.as_str()).or_insert(0);

        for dep in &node.requires {
            // Only check deps that are local (library deps can't form cycles)
            if graph.find_node(&dep.name).is_some() {
                adj.entry(dep.name.as_str()).or_default();
                adj.get_mut(node.name.as_str()).unwrap().push(dep.name.as_str());
                *in_degree.entry(dep.name.as_str()).or_insert(0) += 1;
            }
        }
    }

    // Kahn's algorithm for topological sort
    let mut queue: VecDeque<&str> = VecDeque::new();
    for (name, &degree) in &in_degree {
        if degree == 0 {
            queue.push_back(name);
        }
    }

    let mut sorted_count = 0;
    while let Some(name) = queue.pop_front() {
        sorted_count += 1;
        if let Some(deps) = adj.get(name) {
            for dep in deps {
                if let Some(degree) = in_degree.get_mut(dep) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if sorted_count < in_degree.len() {
        // Cycle detected — find which nodes are in the cycle
        let in_cycle: Vec<String> = in_degree.iter()
            .filter(|(_, &d)| d > 0)
            .map(|(name, _)| name.to_string())
            .collect();

        errors.push(GraphError {
            kind: GraphErrorKind::CircularDependency { cycle: in_cycle.clone() },
            behavior: in_cycle.first().cloned().unwrap_or_default(),
            message: format!(
                "Circular dependency detected involving: {}",
                in_cycle.join(" → ")
            ),
        });
    }
}

/// Check transitive purity: if a behavior declares `pure`, all its
/// dependencies (transitively) must also be pure.
fn check_transitive_purity(graph: &ProjectGraph, errors: &mut Vec<GraphError>) {
    for node in graph.nodes.values() {
        if !node.guarantees.contains(&ast::Guarantee::Pure) {
            continue;
        }

        // This behavior claims purity — check all dependencies
        let mut visited = HashSet::new();
        let mut to_check: Vec<&str> = node.requires.iter().map(|r| r.name.as_str()).collect();

        while let Some(dep_name) = to_check.pop() {
            if !visited.insert(dep_name) {
                continue;
            }

            // Look up dependency (local or library)
            let dep_guarantees = if let Some(dep_node) = graph.find_node(dep_name) {
                // Check this dep's deps too (transitive)
                for sub_dep in &dep_node.requires {
                    to_check.push(&sub_dep.name);
                }
                &dep_node.guarantees
            } else if let Some(lib_node) = graph.library_nodes.get(dep_name) {
                &lib_node.guarantees
            } else {
                continue; // Undefined dep — caught by check_dependencies_resolve
            };

            if !dep_guarantees.contains(&ast::Guarantee::Pure) {
                errors.push(GraphError {
                    kind: GraphErrorKind::TransitivePurityViolation {
                        impure_dep: dep_name.to_string(),
                    },
                    behavior: node.name.clone(),
                    message: format!(
                        "Behavior '{}' declares 'pure' but depends on '{}' which is not pure. \
                         Pure behaviors cannot transitively depend on impure behaviors.",
                        node.name, dep_name
                    ),
                });
                break; // One violation is enough
            }
        }
    }
}

// =============================================================================
// Topological Sort (for compilation order)
// =============================================================================

/// Produce a topological ordering of local behaviors for compilation.
/// Dependencies come before dependents. Returns Err if circular.
pub fn topological_sort(graph: &ProjectGraph) -> Result<Vec<NodeId>, Vec<String>> {
    let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
    let mut dependents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

    // Initialize all nodes with zero in-degree
    for &id in graph.nodes.keys() {
        in_degree.insert(id, 0);
    }

    // Build dependency edges (only local → local)
    for node in graph.nodes.values() {
        for dep in &node.requires {
            if let Some(dep_id) = graph.find_node_id(&dep.name) {
                *in_degree.entry(node.id).or_insert(0) += 1;
                dependents.entry(dep_id).or_default().push(node.id);
            }
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<NodeId> = VecDeque::new();
    for (&id, &degree) in &in_degree {
        if degree == 0 {
            queue.push_back(id);
        }
    }

    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id);
        if let Some(deps) = dependents.get(&id) {
            for &dep_id in deps {
                if let Some(degree) = in_degree.get_mut(&dep_id) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dep_id);
                    }
                }
            }
        }
    }

    if order.len() < in_degree.len() {
        let in_cycle: Vec<String> = in_degree.iter()
            .filter(|(_, &d)| d > 0)
            .filter_map(|(id, _)| graph.nodes.get(id).map(|n| n.name.clone()))
            .collect();
        Err(in_cycle)
    } else {
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{BehaviorNode, BehaviorKind, Port, DependencyRef};

    fn make_pure_node(graph: &mut ProjectGraph, name: &str, deps: Vec<(&str, &str)>) -> NodeId {
        let id = graph.next_node_id();
        let mut node = BehaviorNode::new(id, name, BehaviorKind::Leaf);
        node.guarantees.push(ast::Guarantee::Pure);
        for (dep_name, dep_hash) in deps {
            node.requires.push(DependencyRef {
                name: dep_name.to_string(),
                hash: dep_hash.to_string(),
            });
        }
        graph.add_node(node);
        id
    }

    fn make_impure_node(graph: &mut ProjectGraph, name: &str) -> NodeId {
        let id = graph.next_node_id();
        let node = BehaviorNode::new(id, name, BehaviorKind::Leaf);
        // No pure guarantee
        graph.add_node(node);
        id
    }

    #[test]
    fn test_valid_graph_has_no_errors() {
        let mut graph = ProjectGraph::new();

        let a = graph.next_node_id();
        let mut node_a = BehaviorNode::new(a, "a", BehaviorKind::Leaf);
        node_a.ports.push(Port::output("x", ast::Type::Int, 8));
        graph.add_node(node_a);

        let b = graph.next_node_id();
        let mut node_b = BehaviorNode::new(b, "b", BehaviorKind::Leaf);
        node_b.requires.push(DependencyRef {
            name: "a".to_string(),
            hash: graph.node(a).unwrap().hash.clone(),
        });
        graph.add_node(node_b);

        let errors = validate_graph(&graph);
        assert!(errors.is_empty(), "Valid graph should have no errors: {:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>());
    }

    #[test]
    fn test_undefined_dependency_detected() {
        let mut graph = ProjectGraph::new();

        let id = graph.next_node_id();
        let mut node = BehaviorNode::new(id, "caller", BehaviorKind::Composite);
        node.requires.push(DependencyRef {
            name: "nonexistent".to_string(),
            hash: "abcd1234".to_string(),
        });
        graph.add_node(node);

        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| matches!(e.kind, GraphErrorKind::UndefinedDependency { .. })));
    }

    #[test]
    fn test_stale_dependency_detected() {
        let mut graph = ProjectGraph::new();

        let dep_id = graph.next_node_id();
        let mut dep = BehaviorNode::new(dep_id, "dep", BehaviorKind::Leaf);
        dep.ports.push(Port::input("x", ast::Type::Int, 8));
        graph.add_node(dep);

        let caller_id = graph.next_node_id();
        let mut caller = BehaviorNode::new(caller_id, "caller", BehaviorKind::Composite);
        caller.requires.push(DependencyRef {
            name: "dep".to_string(),
            hash: "00000000".to_string(), // Wrong hash
        });
        graph.add_node(caller);

        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| matches!(e.kind, GraphErrorKind::StaleDependency { .. })));
    }

    #[test]
    fn test_circular_dependency_detected() {
        let mut graph = ProjectGraph::new();

        let a = graph.next_node_id();
        let mut node_a = BehaviorNode::new(a, "a", BehaviorKind::Leaf);
        node_a.requires.push(DependencyRef { name: "b".to_string(), hash: String::new() });
        graph.add_node(node_a);

        let b = graph.next_node_id();
        let mut node_b = BehaviorNode::new(b, "b", BehaviorKind::Leaf);
        node_b.requires.push(DependencyRef { name: "a".to_string(), hash: String::new() });
        graph.add_node(node_b);

        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| matches!(e.kind, GraphErrorKind::CircularDependency { .. })));
    }

    #[test]
    fn test_transitive_purity_violation() {
        let mut graph = ProjectGraph::new();

        let _impure = make_impure_node(&mut graph, "impure-dep");
        let _pure = make_pure_node(&mut graph, "pure-caller", vec![("impure-dep", "")]);

        let errors = validate_graph(&graph);
        assert!(errors.iter().any(|e| matches!(e.kind, GraphErrorKind::TransitivePurityViolation { .. })),
            "Pure behavior depending on impure should be flagged. Errors: {:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>());
    }

    #[test]
    fn test_transitive_purity_ok_when_all_pure() {
        let mut graph = ProjectGraph::new();

        let _dep = make_pure_node(&mut graph, "pure-dep", vec![]);
        let dep_hash = graph.find_node("pure-dep").unwrap().hash.clone();
        let _caller = make_pure_node(&mut graph, "pure-caller", vec![("pure-dep", &dep_hash)]);

        let errors = validate_graph(&graph);
        let purity_errors: Vec<_> = errors.iter()
            .filter(|e| matches!(e.kind, GraphErrorKind::TransitivePurityViolation { .. }))
            .collect();
        assert!(purity_errors.is_empty(), "All-pure chain should not flag purity violation");
    }

    #[test]
    fn test_topological_sort_simple() {
        let mut graph = ProjectGraph::new();

        let a = graph.next_node_id();
        let node_a = BehaviorNode::new(a, "a", BehaviorKind::Leaf);
        graph.add_node(node_a);

        let b = graph.next_node_id();
        let mut node_b = BehaviorNode::new(b, "b", BehaviorKind::Leaf);
        node_b.requires.push(DependencyRef { name: "a".to_string(), hash: String::new() });
        graph.add_node(node_b);

        let order = topological_sort(&graph).unwrap();
        let a_pos = order.iter().position(|&id| id == a).unwrap();
        let b_pos = order.iter().position(|&id| id == b).unwrap();
        assert!(a_pos < b_pos, "Dependency 'a' should come before 'b'");
    }

    #[test]
    fn test_topological_sort_cycle_fails() {
        let mut graph = ProjectGraph::new();

        let a = graph.next_node_id();
        let mut node_a = BehaviorNode::new(a, "a", BehaviorKind::Leaf);
        node_a.requires.push(DependencyRef { name: "b".to_string(), hash: String::new() });
        graph.add_node(node_a);

        let b = graph.next_node_id();
        let mut node_b = BehaviorNode::new(b, "b", BehaviorKind::Leaf);
        node_b.requires.push(DependencyRef { name: "a".to_string(), hash: String::new() });
        graph.add_node(node_b);

        assert!(topological_sort(&graph).is_err());
    }
}
