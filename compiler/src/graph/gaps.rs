//! Gap Detection Module
//!
//! Finds structural gaps in the behavior graph. Every gap is a question.
//! When zero gaps remain, the specification is complete.
//!
//! Gap types:
//! - Unsourced input: port needs data, nothing provides it
//! - Unconsumed output: port produces data, nothing uses it
//! - Unconsumed error: error output not handled
//! - Undefined behavior: referenced in requires but not found
//! - Stale dependency: dependency hash doesn't match current version

use super::{ProjectGraph, Gap, Direction, BehaviorNode};

/// Find all gaps in the project graph.
///
/// Iterates all ports on all local behavior nodes. For each port,
/// checks whether it's connected (has an edge) or explicitly discarded.
/// Unconnected, non-discarded ports become gaps with generated questions.
///
/// Also checks for undefined behaviors and stale dependency hashes.
pub fn find_gaps(graph: &ProjectGraph) -> Vec<Gap> {
    let mut gaps = Vec::new();

    for node in graph.nodes.values() {
        // Check each port on this behavior
        find_port_gaps(graph, node, &mut gaps);

        // Check dependency references
        find_dependency_gaps(graph, node, &mut gaps);
    }

    gaps
}

/// Find gaps from unconnected ports on a behavior node.
fn find_port_gaps(graph: &ProjectGraph, node: &BehaviorNode, gaps: &mut Vec<Gap>) {
    for port in &node.ports {
        match port.direction {
            Direction::In => {
                // Input ports must have a source (incoming edge) or be on the
                // root behavior (sourced externally). For now, check edges only.
                if !graph.port_has_source(node.id, &port.name) {
                    gaps.push(Gap {
                        node_id: node.id,
                        node_name: node.name.clone(),
                        port_name: port.name.clone(),
                        direction: Direction::In,
                        port_type: port.interpretation.clone(),
                        port_size: port.size,
                        question: format!(
                            "Behavior '{}' needs input '{}' ({:?}, {} bytes) \
                             — where does this data come from?",
                            node.name, port.name, port.interpretation, port.size
                        ),
                    });
                }
            }

            Direction::Out => {
                // Output ports must have a consumer (outgoing edge) or be
                // explicitly discarded.
                if !port.discarded && !graph.port_has_consumer(node.id, &port.name) {
                    gaps.push(Gap {
                        node_id: node.id,
                        node_name: node.name.clone(),
                        port_name: port.name.clone(),
                        direction: Direction::Out,
                        port_type: port.interpretation.clone(),
                        port_size: port.size,
                        question: format!(
                            "Behavior '{}' produces output '{}' ({:?}, {} bytes) \
                             — what should happen with this data?",
                            node.name, port.name, port.interpretation, port.size
                        ),
                    });
                }
            }

            Direction::Error => {
                // Error ports must be handled — consumed or explicitly discarded.
                // Unhandled errors are a particularly important gap.
                if !port.discarded && !graph.port_has_consumer(node.id, &port.name) {
                    gaps.push(Gap {
                        node_id: node.id,
                        node_name: node.name.clone(),
                        port_name: port.name.clone(),
                        direction: Direction::Error,
                        port_type: port.interpretation.clone(),
                        port_size: port.size,
                        question: format!(
                            "Behavior '{}' can produce error '{}' ({:?}, {} bytes) \
                             — how should this error be handled?",
                            node.name, port.name, port.interpretation, port.size
                        ),
                    });
                }
            }
        }
    }
}

/// Find gaps from dependency references that don't resolve or have stale hashes.
fn find_dependency_gaps(graph: &ProjectGraph, node: &BehaviorNode, gaps: &mut Vec<Gap>) {
    for dep in &node.requires {
        match graph.lookup(&dep.name) {
            Some(found) => {
                // Dependency exists — check hash matches (version pin)
                if !dep.hash.is_empty() && found.hash() != dep.hash {
                    gaps.push(Gap {
                        node_id: node.id,
                        node_name: node.name.clone(),
                        port_name: dep.name.clone(),
                        direction: Direction::In, // conceptually an input dependency
                        port_type: crate::ast::Type::Bytes, // placeholder
                        port_size: 0,
                        question: format!(
                            "Behavior '{}' expects '{}@{}' but '{}' is now @{} \
                             — the dependency's contract changed. Accept the update?",
                            node.name, dep.name, dep.hash, dep.name, found.hash()
                        ),
                    });
                }
            }
            None => {
                // Dependency not found — undefined behavior
                gaps.push(Gap {
                    node_id: node.id,
                    node_name: node.name.clone(),
                    port_name: dep.name.clone(),
                    direction: Direction::In,
                    port_type: crate::ast::Type::Bytes,
                    port_size: 0,
                    question: format!(
                        "Behavior '{}' requires '{}' but it's not defined \
                         — what should '{}' do?",
                        node.name, dep.name, dep.name
                    ),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast;
    use crate::graph::{BehaviorNode, BehaviorKind, Port};

    /// Helper: create a simple behavior node with given ports.
    fn make_node(graph: &mut ProjectGraph, name: &str, ports: Vec<Port>) -> u64 {
        let id = graph.next_node_id();
        let mut node = BehaviorNode::new(id, name, BehaviorKind::Leaf);
        node.ports = ports;
        graph.add_node(node);
        id
    }

    #[test]
    fn test_complete_graph_has_no_gaps() {
        let mut graph = ProjectGraph::new();

        // Behavior A: out result -> Behavior B: in data
        let a = make_node(&mut graph, "producer", vec![
            Port::output("result", ast::Type::Int, 8),
        ]);
        let b = make_node(&mut graph, "consumer", vec![
            Port::input("data", ast::Type::Int, 8),
        ]);
        graph.add_edge(a, "result", b, "data").unwrap();

        let gaps = graph.find_gaps();
        assert!(gaps.is_empty(), "Complete graph should have no gaps, found: {:?}",
            gaps.iter().map(|g| &g.question).collect::<Vec<_>>());
    }

    #[test]
    fn test_unsourced_input_is_gap() {
        let mut graph = ProjectGraph::new();

        make_node(&mut graph, "consumer", vec![
            Port::input("data", ast::Type::Int, 8),
        ]);

        let gaps = graph.find_gaps();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].port_name, "data");
        assert_eq!(gaps[0].direction, Direction::In);
        assert!(gaps[0].question.contains("where does this data come from"));
    }

    #[test]
    fn test_unconsumed_output_is_gap() {
        let mut graph = ProjectGraph::new();

        make_node(&mut graph, "producer", vec![
            Port::output("result", ast::Type::Int, 8),
        ]);

        let gaps = graph.find_gaps();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].port_name, "result");
        assert_eq!(gaps[0].direction, Direction::Out);
        assert!(gaps[0].question.contains("what should happen"));
    }

    #[test]
    fn test_unhandled_error_is_gap() {
        let mut graph = ProjectGraph::new();

        make_node(&mut graph, "parser", vec![
            Port::output("result", ast::Type::Bytes, 1024),
            Port::error("error", ast::Type::Int, 4),
        ]);

        let gaps = graph.find_gaps();
        // Both the output and the error are unconsumed
        assert_eq!(gaps.len(), 2);
        let error_gap = gaps.iter().find(|g| g.direction == Direction::Error).unwrap();
        assert!(error_gap.question.contains("how should this error be handled"));
    }

    #[test]
    fn test_discarded_output_is_not_gap() {
        let mut graph = ProjectGraph::new();

        let id = graph.next_node_id();
        let mut node = BehaviorNode::new(id, "producer", BehaviorKind::Leaf);
        let mut port = Port::output("written", ast::Type::Int, 8);
        port.discarded = true;
        node.ports.push(port);
        graph.add_node(node);

        let gaps = graph.find_gaps();
        assert!(gaps.is_empty(), "Discarded output should not be a gap");
    }

    #[test]
    fn test_undefined_dependency_is_gap() {
        let mut graph = ProjectGraph::new();

        let id = graph.next_node_id();
        let mut node = BehaviorNode::new(id, "caller", BehaviorKind::Composite);
        node.requires.push(super::super::DependencyRef {
            name: "nonexistent".to_string(),
            hash: "abcd1234".to_string(),
        });
        graph.add_node(node);

        let gaps = graph.find_gaps();
        assert!(gaps.iter().any(|g| g.question.contains("not defined")),
            "Missing dependency should generate a gap");
    }

    #[test]
    fn test_stale_dependency_is_gap() {
        let mut graph = ProjectGraph::new();

        // Create the dependency behavior
        let dep_id = graph.next_node_id();
        let mut dep = BehaviorNode::new(dep_id, "helper", BehaviorKind::Leaf);
        dep.ports.push(Port::input("x", ast::Type::Int, 8));
        dep.ports.push(Port::output("y", ast::Type::Int, 8));
        graph.add_node(dep);
        let actual_hash = graph.node(dep_id).unwrap().hash.clone();

        // Create caller that pins to a DIFFERENT hash
        let caller_id = graph.next_node_id();
        let mut caller = BehaviorNode::new(caller_id, "caller", BehaviorKind::Composite);
        caller.requires.push(super::super::DependencyRef {
            name: "helper".to_string(),
            hash: "00000000".to_string(), // wrong hash
        });
        graph.add_node(caller);

        let gaps = graph.find_gaps();
        assert!(gaps.iter().any(|g| g.question.contains("contract changed")),
            "Stale dependency hash should generate a gap. Actual hash: {}", actual_hash);
    }

    #[test]
    fn test_multiple_behaviors_with_mixed_gaps() {
        let mut graph = ProjectGraph::new();

        // A produces result (connected) and error (not connected)
        let a = make_node(&mut graph, "step-a", vec![
            Port::output("result", ast::Type::Bytes, 256),
            Port::error("error", ast::Type::Int, 4),
        ]);

        // B consumes A's result, needs another input (not connected), produces output (not connected)
        let b = make_node(&mut graph, "step-b", vec![
            Port::input("data", ast::Type::Bytes, 256),
            Port::input("config", ast::Type::Bytes, 64),
            Port::output("result", ast::Type::Bytes, 512),
        ]);

        // Wire A.result -> B.data
        graph.add_edge(a, "result", b, "data").unwrap();

        let gaps = graph.find_gaps();
        // Gaps: A.error (unhandled), B.config (unsourced), B.result (unconsumed)
        assert_eq!(gaps.len(), 3);

        let gap_names: Vec<&str> = gaps.iter().map(|g| g.port_name.as_str()).collect();
        assert!(gap_names.contains(&"error"));
        assert!(gap_names.contains(&"config"));
        assert!(gap_names.contains(&"result"));
    }
}
