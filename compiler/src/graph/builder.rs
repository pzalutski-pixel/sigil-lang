//! Graph Builder Module
//!
//! Constructs a ProjectGraph from parsed behavior files.
//! Converts V1 ast::Behavior into graph BehaviorNodes,
//! derives edges from composite wiring (CALL statements),
//! and loads library ContractRefs from registries.

use std::path::{Path, PathBuf};
use std::collections::HashMap;

use crate::ast;
use super::{
    ProjectGraph, BehaviorNode, BehaviorKind, Port,
    DependencyRef, ContractRef, Implementation,
};

// =============================================================================
// Building from AST behaviors
// =============================================================================

/// Convert a parsed ast::Behavior into a graph BehaviorNode.
///
/// This is the bridge between V1 file format and the graph model.
/// Ports are derived from contract INPUT/OUTPUT declarations.
/// Implementation is preserved as V1 AST nodes for codegen compatibility.
pub fn behavior_to_node(
    graph: &mut ProjectGraph,
    behavior: &ast::Behavior,
    source_path: Option<PathBuf>,
) -> BehaviorNode {
    let id = graph.next_node_id();

    let kind = if behavior.is_native {
        BehaviorKind::Native
    } else if behavior.composition.is_some() {
        BehaviorKind::Composite
    } else {
        BehaviorKind::Leaf
    };

    let mut node = BehaviorNode::new(id, &behavior.name, kind);
    node.description = behavior.description.clone();
    node.source_path = source_path;

    // Convert contract inputs to input ports
    for input in &behavior.contract.inputs {
        node.ports.push(Port::input(
            &input.name,
            input.typ.clone(),
            input.size,
        ));
    }

    // Convert contract outputs to output ports
    // NOTE: V1 doesn't distinguish error ports from output ports.
    // V2 grammar will add explicit error ports. For now, all outputs
    // are Direction::Out.
    for output in &behavior.contract.outputs {
        node.ports.push(Port::output(
            &output.name,
            output.typ.clone(),
            output.size,
        ));
    }

    // Convert guarantees
    node.guarantees = behavior.contract.guarantees.clone();

    // Convert requires to dependency refs
    for req in &behavior.contract.requires.behaviors {
        node.requires.push(DependencyRef {
            name: req.name.clone(),
            hash: req.hash.clone(),
        });
    }

    // Preserve implementation
    if let Some(ref comp) = behavior.composition {
        node.implementation = Some(Implementation::Composite(comp.nodes.clone()));
    } else if let Some(impl_) = behavior.implementations.first() {
        node.implementation = Some(Implementation::Leaf(impl_.nodes.clone()));
    }

    // Hash will be computed when added to graph via add_node()
    node
}

/// Convert a parsed ast::Contract (from library registry) into a ContractRef.
pub fn contract_to_ref(
    name: &str,
    contract: &ast::Contract,
    hash: &str,
    library_path: PathBuf,
) -> ContractRef {
    let mut ports = Vec::new();

    for input in &contract.inputs {
        ports.push(Port::input(&input.name, input.typ.clone(), input.size));
    }
    for output in &contract.outputs {
        ports.push(Port::output(&output.name, output.typ.clone(), output.size));
    }

    ContractRef {
        name: name.to_string(),
        hash: hash.to_string(),
        ports,
        guarantees: contract.guarantees.clone(),
        library_path,
    }
}

// =============================================================================
// Deriving edges from composite wiring
// =============================================================================

/// Extract edges from composite behavior wiring (CALL statements).
///
/// When a composite behavior calls another behavior, the arguments
/// and outputs create implicit edges in the graph. This function
/// examines CALL nodes and creates edges between the caller's
/// available variables (ports/call results) and the callee's ports.
///
/// NOTE: This is a simplified version that tracks CALL-level dependencies.
/// Full edge derivation requires type-aware analysis of how outputs
/// flow into subsequent inputs, which depends on the specific wiring
/// in the composition. For V2, the graph server builds edges
/// incrementally via tool calls. This function provides a best-effort
/// derivation from V1 composite syntax.
pub fn derive_edges_from_composites(graph: &mut ProjectGraph) {
    // Collect edges to add (can't mutate graph while iterating)
    let mut edges_to_add: Vec<(u64, String, u64, String)> = Vec::new();

    // For each composite node, examine its CALL statements
    let node_ids: Vec<u64> = graph.nodes.keys().cloned().collect();
    for node_id in &node_ids {
        let node = match graph.nodes.get(node_id) {
            Some(n) => n,
            None => continue,
        };

        if node.kind != BehaviorKind::Composite {
            continue;
        }

        let nodes = match &node.implementation {
            Some(Implementation::Composite(nodes)) => nodes.clone(),
            _ => continue,
        };

        // Walk CALL statements and create dependency-level edges
        // This connects the caller to each callee at the behavior level
        for ast_node in &nodes {
            if let ast::NodeKind::Call { behavior: callee_name, .. } = &ast_node.kind {
                // Find callee node ID
                if let Some(callee_id) = graph.find_node_id(callee_name) {
                    // Create edges from caller's matching ports to callee's inputs
                    // This is approximate — full wiring analysis needs the V2 graph server
                    let callee_inputs: Vec<(String, ast::Type, usize)> = graph.nodes.get(&callee_id)
                        .map(|n| n.inputs().map(|p| (p.name.clone(), p.interpretation.clone(), p.size)).collect())
                        .unwrap_or_default();

                    for (port_name, _port_type, _port_size) in &callee_inputs {
                        // Check if the caller has a matching port name
                        if graph.nodes.get(node_id)
                            .and_then(|n| n.port(port_name))
                            .is_some()
                        {
                            edges_to_add.push((*node_id, port_name.clone(), callee_id, port_name.clone()));
                        }
                    }
                }
            }

            // Also handle Assignment with Call expression
            if let ast::NodeKind::Assignment { expr, .. } = &ast_node.kind {
                if let ast::Expr::Call { behavior: callee_name, .. } = expr.as_ref() {
                    // Similar logic — record the dependency
                    if let Some(_callee_id) = graph.find_node_id(callee_name) {
                        // Dependency exists — edges at port level need detailed analysis
                        // The graph server will handle this properly in Phase 3
                    }
                }
            }
        }
    }

    // Apply collected edges (ignore errors — best effort)
    for (from, from_port, to, to_port) in edges_to_add {
        let _ = graph.add_edge(from, &from_port, to, &to_port);
    }
}

// =============================================================================
// Loading library contracts
// =============================================================================

/// Load library contracts from a contracts.registry file.
///
/// The registry format is the V1 format generated by the compiler's
/// `--lib` mode. Each behavior block has CONTRACT + HASH.
///
/// Returns a map of behavior name → (Contract, hash).
pub fn load_library_contracts(
    registry_path: &Path,
) -> Result<HashMap<String, (ast::Contract, String)>, String> {
    let content = std::fs::read_to_string(registry_path)
        .map_err(|e| format!("Failed to read {}: {}", registry_path.display(), e))?;

    let mut contracts = HashMap::new();

    // Parse behavior blocks from registry
    // Format: BEHAVIOR name\nCONTRACT\n  ...\nHASH xxxx\nEND\n
    let mut start = 0;
    while let Some(beh_pos) = content[start..].find("BEHAVIOR ") {
        let abs_beh = start + beh_pos;
        if let Some(end_pos) = content[abs_beh..].find("\nEND") {
            let block = &content[abs_beh..abs_beh + end_pos + 4];

            // Parse using existing V1 lexer and parser
            match crate::lexer::lex(block) {
                Ok(tokens) => {
                    match crate::parser::parse(&tokens, block) {
                        Ok(behavior) => {
                            contracts.insert(
                                behavior.name.clone(),
                                (behavior.contract, behavior.hash),
                            );
                        }
                        Err(_) => { /* Skip unparseable blocks */ }
                    }
                }
                Err(_) => { /* Skip unlexable blocks */ }
            }

            start = abs_beh + end_pos + 4;
        } else {
            break;
        }
    }

    Ok(contracts)
}

/// Load all library contracts and add them to the graph as ContractRefs.
pub fn load_library_into_graph(
    graph: &mut ProjectGraph,
    registry_path: &Path,
    library_path: &Path,
) -> Result<usize, String> {
    let contracts = load_library_contracts(registry_path)?;
    let count = contracts.len();

    for (name, (contract, hash)) in &contracts {
        let contract_ref = contract_to_ref(name, contract, hash, library_path.to_path_buf());
        graph.add_library_node(contract_ref);
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behavior_to_node_leaf() {
        let mut behavior = ast::Behavior::new("add".to_string());
        behavior.contract.inputs.push(ast::Parameter {
            name: "a".to_string(),
            typ: ast::Type::Int,
            size: 4,
        });
        behavior.contract.inputs.push(ast::Parameter {
            name: "b".to_string(),
            typ: ast::Type::Int,
            size: 4,
        });
        behavior.contract.outputs.push(ast::Parameter {
            name: "result".to_string(),
            typ: ast::Type::Int,
            size: 4,
        });
        behavior.contract.guarantees.push(ast::Guarantee::Pure);
        behavior.contract.guarantees.push(ast::Guarantee::WritesOutput);
        behavior.implementations.push(ast::PlatformImpl {
            platform: None,
            nodes: vec![],
        });

        let mut graph = ProjectGraph::new();
        let node = behavior_to_node(&mut graph, &behavior, None);

        assert_eq!(node.name, "add");
        assert_eq!(node.kind, BehaviorKind::Leaf);
        assert_eq!(node.ports.len(), 3);
        assert_eq!(node.inputs().count(), 2);
        assert_eq!(node.outputs().count(), 1);
        assert_eq!(node.guarantees.len(), 2);
    }

    #[test]
    fn test_behavior_to_node_native() {
        let mut behavior = ast::Behavior::new("println".to_string());
        behavior.is_native = true;
        behavior.contract.inputs.push(ast::Parameter {
            name: "data".to_string(),
            typ: ast::Type::Bytes,
            size: 1024,
        });
        behavior.contract.outputs.push(ast::Parameter {
            name: "written".to_string(),
            typ: ast::Type::Int,
            size: 8,
        });

        let mut graph = ProjectGraph::new();
        let node = behavior_to_node(&mut graph, &behavior, None);

        assert_eq!(node.kind, BehaviorKind::Native);
        assert_eq!(node.inputs().count(), 1);
        assert_eq!(node.outputs().count(), 1);
    }

    #[test]
    fn test_behavior_to_node_preserves_requires() {
        let mut behavior = ast::Behavior::new("caller".to_string());
        behavior.contract.requires.behaviors.push(ast::BehaviorRef {
            name: "helper".to_string(),
            hash: "abcd1234".to_string(),
        });

        let mut graph = ProjectGraph::new();
        let node = behavior_to_node(&mut graph, &behavior, None);

        assert_eq!(node.requires.len(), 1);
        assert_eq!(node.requires[0].name, "helper");
        assert_eq!(node.requires[0].hash, "abcd1234");
    }

    #[test]
    fn test_node_hash_computed_on_add() {
        let mut behavior = ast::Behavior::new("test".to_string());
        behavior.contract.inputs.push(ast::Parameter {
            name: "x".to_string(),
            typ: ast::Type::Int,
            size: 8,
        });

        let mut graph = ProjectGraph::new();
        let node = behavior_to_node(&mut graph, &behavior, None);
        let id = graph.add_node(node);

        let stored_hash = &graph.node(id).unwrap().hash;
        assert!(!stored_hash.is_empty(), "Hash should be computed on add");
        assert_eq!(stored_hash.len(), 8, "Hash should be 8 hex chars");
    }

    #[test]
    fn test_roundtrip_to_ast_behavior() {
        let mut behavior = ast::Behavior::new("round".to_string());
        behavior.contract.inputs.push(ast::Parameter {
            name: "a".to_string(),
            typ: ast::Type::Int,
            size: 4,
        });
        behavior.contract.outputs.push(ast::Parameter {
            name: "b".to_string(),
            typ: ast::Type::Int,
            size: 4,
        });
        behavior.contract.guarantees.push(ast::Guarantee::WritesOutput);

        let mut graph = ProjectGraph::new();
        let node = behavior_to_node(&mut graph, &behavior, None);
        let id = graph.add_node(node);

        let ast_beh = graph.node(id).unwrap().to_ast_behavior();
        assert_eq!(ast_beh.name, "round");
        assert_eq!(ast_beh.contract.inputs.len(), 1);
        assert_eq!(ast_beh.contract.outputs.len(), 1);
        assert_eq!(ast_beh.contract.guarantees.len(), 1);
    }
}
