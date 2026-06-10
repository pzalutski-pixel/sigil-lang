//! Contract Hash Computation
//!
//! Per Sigil Language Reference Section 5.7, contract hashes are used for:
//! - Dependency version verification (REQUIRES behavior@hash)
//! - Contract change detection across sessions
//!
//! Hash is computed from CONTRACT contents: inputs, outputs, requires,
//! and guarantees.

use sha2::{Sha256, Digest};
use crate::ast::{Contract, Type, Guarantee};

/// Compute contract hash (8 hex chars from SHA-256)
///
/// The hash includes:
/// - INPUT definitions (name, type, size)
/// - OUTPUT definitions (name, type, size)
/// - REQUIRES behavior references (name, hash)
/// - GUARANTEES list
///
/// Returns lowercase 8-character hex string.
pub fn compute_contract_hash(contract: &Contract) -> String {
    let mut hasher = Sha256::new();

    // Hash inputs in order
    hasher.update(b"INPUTS:");
    for input in &contract.inputs {
        hasher.update(input.name.as_bytes());
        hasher.update(&[type_to_byte(&input.typ)]);
        hasher.update(&input.size.to_le_bytes());
    }

    // Hash outputs in order
    hasher.update(b"OUTPUTS:");
    for output in &contract.outputs {
        hasher.update(output.name.as_bytes());
        hasher.update(&[type_to_byte(&output.typ)]);
        hasher.update(&output.size.to_le_bytes());
    }

    // Hash requires (behavior references)
    hasher.update(b"REQUIRES:");
    for req in &contract.requires.behaviors {
        hasher.update(req.name.as_bytes());
        hasher.update(b"@");
        hasher.update(req.hash.as_bytes());
    }

    // Hash guarantees
    hasher.update(b"GUARANTEES:");
    for guarantee in &contract.guarantees {
        hasher.update(&[guarantee_to_byte(guarantee)]);
    }

    // Take first 4 bytes = 8 hex characters
    let result = hasher.finalize();
    hex::encode(&result[..4])
}

/// Convert type to single byte for hashing
fn type_to_byte(typ: &Type) -> u8 {
    match typ {
        Type::Int => 1,
        Type::Float => 2,
        Type::Bytes => 3,
        Type::String => 4,
    }
}

/// Convert guarantee to single byte for hashing
fn guarantee_to_byte(g: &Guarantee) -> u8 {
    match g {
        Guarantee::Pure => 1,
        Guarantee::NoAlloc => 2,
        Guarantee::WritesOutput => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Parameter, Requirements, BehaviorRef};

    #[test]
    fn test_empty_contract_hash() {
        let contract = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash = compute_contract_hash(&contract);
        assert_eq!(hash.len(), 8);
    }

    #[test]
    fn test_contract_hash_deterministic() {
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
            guarantees: vec![Guarantee::Pure],
        };
        let hash1 = compute_contract_hash(&contract);
        let hash2 = compute_contract_hash(&contract);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_changes_with_input() {
        let contract1 = Contract {
            inputs: vec![
                Parameter { name: "a".into(), typ: Type::Int, size: 8 },
            ],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let contract2 = Contract {
            inputs: vec![
                Parameter { name: "b".into(), typ: Type::Int, size: 8 },
            ],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash1 = compute_contract_hash(&contract1);
        let hash2 = compute_contract_hash(&contract2);
        assert_ne!(hash1, hash2, "Different input names should produce different hashes");
    }

    #[test]
    fn test_hash_changes_with_output() {
        let contract1 = Contract {
            inputs: vec![],
            outputs: vec![
                Parameter { name: "result".into(), typ: Type::Int, size: 8 },
            ],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let contract2 = Contract {
            inputs: vec![],
            outputs: vec![
                Parameter { name: "result".into(), typ: Type::Int, size: 4 },
            ],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash1 = compute_contract_hash(&contract1);
        let hash2 = compute_contract_hash(&contract2);
        assert_ne!(hash1, hash2, "Different output sizes should produce different hashes");
    }

    #[test]
    fn test_hash_changes_with_type() {
        let contract1 = Contract {
            inputs: vec![
                Parameter { name: "x".into(), typ: Type::Int, size: 8 },
            ],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let contract2 = Contract {
            inputs: vec![
                Parameter { name: "x".into(), typ: Type::Float, size: 8 },
            ],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash1 = compute_contract_hash(&contract1);
        let hash2 = compute_contract_hash(&contract2);
        assert_ne!(hash1, hash2, "Different types should produce different hashes");
    }

    #[test]
    fn test_hash_changes_with_requires() {
        let contract1 = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![
                    BehaviorRef { name: "foo".into(), hash: "abc123".into() },
                ],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let contract2 = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![
                    BehaviorRef { name: "bar".into(), hash: "abc123".into() },
                ],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash1 = compute_contract_hash(&contract1);
        let hash2 = compute_contract_hash(&contract2);
        assert_ne!(hash1, hash2, "Different required behaviors should produce different hashes");
    }

    #[test]
    fn test_hash_changes_with_guarantees() {
        let contract1 = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![Guarantee::Pure],
        };
        let contract2 = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![Guarantee::NoAlloc],
        };
        let hash1 = compute_contract_hash(&contract1);
        let hash2 = compute_contract_hash(&contract2);
        assert_ne!(hash1, hash2, "Different guarantees should produce different hashes");
    }

    #[test]
    fn test_hash_hex_format() {
        let contract = Contract {
            inputs: vec![],
            outputs: vec![],
            requires: Requirements {
                behaviors: vec![],
                memory: vec![],
            },
            guarantees: vec![],
        };
        let hash = compute_contract_hash(&contract);
        assert_eq!(hash.len(), 8, "Hash should be 8 hex characters");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "Hash should only contain hex digits");
    }
}
