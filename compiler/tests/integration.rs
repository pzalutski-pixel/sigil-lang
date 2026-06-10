//! Integration tests for the Sigil compiler
//!
//! Tests end-to-end compilation pipeline per Sigil Language Reference.

use std::collections::{HashMap, HashSet};

// Import compiler modules
use sigil_compiler::lexer::lex;
use sigil_compiler::parser::parse;
use sigil_compiler::semantic::SemanticAnalyzer;
use sigil_compiler::ast::Contract;

/// Helper: Full pipeline - lex, parse, analyze
fn compile_source(source: &str) -> Result<(), String> {
    let tokens = lex(source).map_err(|e| format!("Lexer error: {}", e.message))?;
    let behavior = parse(&tokens, source).map_err(|e| format!("Parser error: {}", e))?;

    let deps: HashMap<String, Contract> = HashMap::new();
    let patterns: HashSet<String> = HashSet::new();
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze(&behavior, &deps, &patterns).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();
        format!("Semantic errors:\n{}", msgs.join("\n"))
    })?;

    Ok(())
}

// =============================================================================
// Valid Program Tests
// =============================================================================

#[test]
fn test_minimal_valid_program() {
    let source = r#"
BEHAVIOR minimal

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  SET status 0
END
"#;
    assert!(compile_source(source).is_ok());
}

#[test]
fn test_program_with_inputs_outputs() {
    let source = r#"
BEHAVIOR adder

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
    assert!(compile_source(source).is_ok());
}

#[test]
fn test_program_with_control_flow() {
    let source = r#"
BEHAVIOR conditional

CONTRACT
  INPUT cond int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH ad9e284f

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c positive negative

  LABEL positive
    SET result 1
    JUMP done

  LABEL negative
    SET result 0
    JUMP done

  LABEL done
END
"#;
    assert!(compile_source(source).is_ok());
}

#[test]
fn test_program_with_memory_ops() {
    let source = r#"
BEHAVIOR memory

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH a222ae31

IMPLEMENTATION
  buf = ALLOC 64 bytes
  STORE buf 42 8
  v = LOAD buf 8
  FREE buf
  SET status 0
END
"#;
    assert!(compile_source(source).is_ok());
}

// =============================================================================
// Invalid Program Tests (Semantic Errors)
// =============================================================================

#[test]
fn test_undefined_label_fails() {
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
    let result = compile_source(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Undefined label"));
}

#[test]
fn test_output_not_written_fails() {
    let source = r#"
BEHAVIOR no-write

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH nowr1234

IMPLEMENTATION
  v = 42
END
"#;
    let result = compile_source(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not written"));
}

// =============================================================================
// Parser Error Tests
// =============================================================================

#[test]
fn test_missing_end_fails() {
    let source = r#"
BEHAVIOR no-end

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH noen1234

IMPLEMENTATION
  SET status 0
"#;
    let result = compile_source(source);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("end of file") || err.contains("END"));
}

#[test]
fn test_invalid_type_fails() {
    let source = r#"
BEHAVIOR bad-type

CONTRACT
  INPUT x invalid 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH badt1234

IMPLEMENTATION
  SET status 0
END
"#;
    let result = compile_source(source);
    assert!(result.is_err());
}
