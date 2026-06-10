//! Diagnostic Module
//!
//! Rich error reporting with error codes, source locations, and suggestions.
//! Provides Rust-style error messages for the Sigil compiler.
//!
//! NOTE: this richer diagnostics subsystem is complete but not yet wired into
//! the compiler pipeline (which currently reports via CompilerError). It is
//! kept for future use.
#![allow(dead_code)]

use std::path::PathBuf;
use crate::lexer::Span;

// =============================================================================
// Error Codes
// =============================================================================

/// Error codes for categorized diagnostics
///
/// Numbering scheme:
/// - E01xx: Type errors
/// - E02xx: Syntax errors (reserved)
/// - E03xx: Memory/ownership errors
/// - E04xx: Label/control flow errors
/// - E05xx: Contract errors
/// - E06xx: Guarantee violations
/// - E07xx: Concurrency errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // Type errors (E01xx)
    /// Type mismatch between expected and found types
    E0101,
    /// Invalid integer size (must be 1, 2, 4, or 8)
    E0102,
    /// Invalid float size (must be 4 or 8)
    E0103,
    /// A handle (address) used where a value is expected (e.g. arithmetic on an
    /// un-LOADed INPUT) — must LOAD it to a value first
    E0104,

    // Memory/ownership errors (E03xx)
    /// Use of handle after FREE
    E0301,
    /// Double FREE of same handle
    E0302,
    /// Memory leak - ALLOC without FREE on all paths
    E0303,
    /// LOAD from uninitialized memory
    E0304,
    /// FREE of borrowed handle (from INPUT)
    E0305,
    /// FREE while handle is borrowed by SPAWN
    E0306,
    /// Possible bounds violation
    E0307,

    // Label/control flow errors (E04xx)
    /// Reference to undefined label
    E0401,
    /// Duplicate label definition
    E0402,
    /// Path does not terminate
    E0403,

    // Contract errors (E05xx)
    /// SYSCALL without declared capability
    E0501,
    /// Missing required dependency
    E0502,
    /// CALL argument count mismatch
    E0503,
    /// CALL argument type mismatch
    E0504,
    /// CALL argument size mismatch
    E0505,
    /// Output size mismatch in STORE
    E0506,
    /// Contract hash mismatch
    E0507,
    /// Dependency hash mismatch
    E0508,
    /// Composite (COMPOSITION/ENTRY) contains primitive computation
    E0509,
    /// Leaf (IMPLEMENTATION) contains a CALL
    E0510,
    /// Composition produces an output that is never consumed (Section 7.8)
    E0511,

    // Guarantee violations (E06xx)
    /// Pure guarantee violated
    E0601,
    /// NoAlloc guarantee violated
    E0602,
    /// NoSyscall guarantee violated
    E0603,
    /// Output not written on all paths
    E0604,

    // Concurrency errors (E07xx)
    /// Non-atomic access to SHARED memory
    E0701,
    /// Channel type mismatch
    E0702,
    /// SPAWN requires pattern target
    E0703,

    // General errors (E09xx)
    /// Undefined variable
    E0901,
}

impl ErrorCode {
    /// Get the numeric code as a string (e.g., "E0301")
    pub fn as_str(&self) -> &'static str {
        match self {
            // Type errors
            ErrorCode::E0101 => "E0101",
            ErrorCode::E0102 => "E0102",
            ErrorCode::E0103 => "E0103",
            ErrorCode::E0104 => "E0104",

            // Memory errors
            ErrorCode::E0301 => "E0301",
            ErrorCode::E0302 => "E0302",
            ErrorCode::E0303 => "E0303",
            ErrorCode::E0304 => "E0304",
            ErrorCode::E0305 => "E0305",
            ErrorCode::E0306 => "E0306",
            ErrorCode::E0307 => "E0307",

            // Label errors
            ErrorCode::E0401 => "E0401",
            ErrorCode::E0402 => "E0402",
            ErrorCode::E0403 => "E0403",

            // Contract errors
            ErrorCode::E0501 => "E0501",
            ErrorCode::E0502 => "E0502",
            ErrorCode::E0503 => "E0503",
            ErrorCode::E0504 => "E0504",
            ErrorCode::E0505 => "E0505",
            ErrorCode::E0506 => "E0506",
            ErrorCode::E0507 => "E0507",
            ErrorCode::E0508 => "E0508",
            ErrorCode::E0509 => "E0509",
            ErrorCode::E0510 => "E0510",
            ErrorCode::E0511 => "E0511",

            // Guarantee errors
            ErrorCode::E0601 => "E0601",
            ErrorCode::E0602 => "E0602",
            ErrorCode::E0603 => "E0603",
            ErrorCode::E0604 => "E0604",

            // Concurrency errors
            ErrorCode::E0701 => "E0701",
            ErrorCode::E0702 => "E0702",
            ErrorCode::E0703 => "E0703",

            // General errors
            ErrorCode::E0901 => "E0901",
        }
    }

    /// Get a short description of the error category
    pub fn category(&self) -> &'static str {
        match self {
            ErrorCode::E0101 | ErrorCode::E0102 | ErrorCode::E0103 |
            ErrorCode::E0104 => "type error",
            ErrorCode::E0301 | ErrorCode::E0302 | ErrorCode::E0303 |
            ErrorCode::E0304 | ErrorCode::E0305 | ErrorCode::E0306 |
            ErrorCode::E0307 => "memory error",
            ErrorCode::E0401 | ErrorCode::E0402 | ErrorCode::E0403 => "control flow error",
            ErrorCode::E0501 | ErrorCode::E0502 | ErrorCode::E0503 |
            ErrorCode::E0504 | ErrorCode::E0505 | ErrorCode::E0506 |
            ErrorCode::E0507 | ErrorCode::E0508 => "contract error",
            ErrorCode::E0509 | ErrorCode::E0510 => "behavior structure error",
            ErrorCode::E0511 => "composition completeness error",
            ErrorCode::E0601 | ErrorCode::E0602 | ErrorCode::E0603 |
            ErrorCode::E0604 => "guarantee violation",
            ErrorCode::E0701 | ErrorCode::E0702 | ErrorCode::E0703 => "concurrency error",
            ErrorCode::E0901 => "error",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// Severity
// =============================================================================

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational note
    Note,
    /// Warning - compilation continues
    Warning,
    /// Error - compilation fails
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Note => write!(f, "note"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

// =============================================================================
// Label
// =============================================================================

/// A label attached to a span in source code
#[derive(Debug, Clone)]
pub struct Label {
    /// The span this label points to
    pub span: Span,
    /// The message for this label
    pub message: String,
    /// Whether this is the primary label (vs secondary/related)
    pub primary: bool,
}

impl Label {
    /// Create a primary label (the main error location)
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Label {
            span,
            message: message.into(),
            primary: true,
        }
    }

    /// Create a secondary label (related location)
    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Label {
            span,
            message: message.into(),
            primary: false,
        }
    }
}

// =============================================================================
// Diagnostic
// =============================================================================

/// A rich diagnostic message with source locations and suggestions
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Error code for categorization
    pub code: ErrorCode,
    /// Severity level
    pub severity: Severity,
    /// Main error message
    pub message: String,
    /// Source file path (if known)
    pub file: Option<PathBuf>,
    /// Labels pointing to source locations
    pub labels: Vec<Label>,
    /// Help text with suggestions
    pub help: Option<String>,
    /// Additional notes
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Create a new error diagnostic
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            message: message.into(),
            file: None,
            labels: Vec::new(),
            help: None,
            notes: Vec::new(),
        }
    }

    /// Create a new warning diagnostic
    pub fn warning(code: ErrorCode, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Warning,
            message: message.into(),
            file: None,
            labels: Vec::new(),
            help: None,
            notes: Vec::new(),
        }
    }

    /// Set the source file
    pub fn with_file(mut self, file: impl Into<PathBuf>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Add a primary label (the main error location)
    pub fn with_primary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::primary(span, message));
        self
    }

    /// Add a secondary label (related location)
    pub fn with_secondary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::secondary(span, message));
        self
    }

    /// Add a label
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Add help text
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add a note
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Get the primary span (first primary label's span)
    pub fn primary_span(&self) -> Option<Span> {
        self.labels.iter()
            .find(|l| l.primary)
            .map(|l| l.span)
    }

    /// Get the line number of the primary span
    pub fn line(&self) -> Option<usize> {
        self.primary_span().map(|s| s.line)
    }
}

// =============================================================================
// DiagnosticBuilder - Convenience constructors for common errors
// =============================================================================

impl Diagnostic {
    // --- Type Errors ---

    pub fn type_mismatch(span: Span, expected: &str, found: &str) -> Self {
        Diagnostic::error(ErrorCode::E0101, format!("type mismatch: expected {}, found {}", expected, found))
            .with_primary_label(span, format!("expected {}", expected))
    }

    pub fn invalid_int_size(span: Span, size: usize) -> Self {
        Diagnostic::error(ErrorCode::E0102, format!("invalid int size {}", size))
            .with_primary_label(span, "must be 1, 2, 4, or 8")
            .with_help("valid integer sizes are 1, 2, 4, or 8 bytes")
    }

    pub fn invalid_float_size(span: Span, size: usize) -> Self {
        Diagnostic::error(ErrorCode::E0103, format!("invalid float size {}", size))
            .with_primary_label(span, "must be 4 or 8")
            .with_help("valid float sizes are 4 (single) or 8 (double) bytes")
    }

    // --- Memory Errors ---

    pub fn use_after_free(use_span: Span, free_span: Span, handle: &str) -> Self {
        Diagnostic::error(ErrorCode::E0301, format!("use of handle '{}' after free", handle))
            .with_primary_label(use_span, "use of freed handle")
            .with_secondary_label(free_span, "handle freed here")
            .with_help("move this use before the FREE statement")
    }

    pub fn double_free(second_span: Span, first_span: Span, handle: &str) -> Self {
        Diagnostic::error(ErrorCode::E0302, format!("double free of handle '{}'", handle))
            .with_primary_label(second_span, "second FREE here")
            .with_secondary_label(first_span, "first FREE here")
            .with_help("remove one of the FREE statements")
    }

    pub fn memory_leak(alloc_span: Span, handle: &str) -> Self {
        Diagnostic::error(ErrorCode::E0303, format!("memory leak: '{}' not freed on all paths", handle))
            .with_primary_label(alloc_span, "allocated here but not freed")
            .with_help("add FREE before all exit paths, or use SCOPE for automatic cleanup")
    }

    pub fn uninitialized_load(span: Span, handle: &str) -> Self {
        Diagnostic::error(ErrorCode::E0304, format!("load from uninitialized handle '{}'", handle))
            .with_primary_label(span, "LOAD without prior STORE")
            .with_help("add STORE to initialize the handle before LOAD")
    }

    pub fn free_borrowed_handle(span: Span, handle: &str) -> Self {
        Diagnostic::error(ErrorCode::E0305, format!("cannot free borrowed handle '{}'", handle))
            .with_primary_label(span, "handle borrowed from caller")
            .with_help("INPUT handles are borrowed and cannot be freed")
    }

    pub fn free_while_borrowed(free_span: Span, spawn_span: Span, handle: &str, pattern: &str) -> Self {
        Diagnostic::error(ErrorCode::E0306, format!("cannot free '{}' while borrowed by '{}'", handle, pattern))
            .with_primary_label(free_span, "FREE while borrowed")
            .with_secondary_label(spawn_span, format!("borrowed by SPAWN here"))
            .with_help("call WAIT on the spawn handle before FREE")
    }

    // --- Label Errors ---

    pub fn undefined_label(span: Span, name: &str) -> Self {
        Diagnostic::error(ErrorCode::E0401, format!("undefined label '{}'", name))
            .with_primary_label(span, "label not found")
    }

    pub fn duplicate_label(second_span: Span, first_span: Span, name: &str) -> Self {
        Diagnostic::error(ErrorCode::E0402, format!("duplicate label '{}'", name))
            .with_primary_label(second_span, "duplicate definition")
            .with_secondary_label(first_span, "first defined here")
    }

    // --- Contract Errors ---

    pub fn undeclared_capability(span: Span, syscall: &str) -> Self {
        Diagnostic::error(ErrorCode::E0501, format!("syscall '{}' not declared in CAPABILITIES", syscall))
            .with_primary_label(span, "undeclared syscall")
            .with_help(format!("add 'CAPABILITIES syscall:{}' to the contract", syscall))
    }

    pub fn call_arg_count_mismatch(span: Span, behavior: &str, expected: usize, got: usize) -> Self {
        Diagnostic::error(ErrorCode::E0503, format!("wrong number of arguments to '{}'", behavior))
            .with_primary_label(span, format!("expected {} arguments, found {}", expected, got))
    }

    // --- Guarantee Errors ---

    pub fn pure_violation(span: Span, reason: &str) -> Self {
        Diagnostic::error(ErrorCode::E0601, format!("pure guarantee violated: {}", reason))
            .with_primary_label(span, "violates pure")
            .with_note("pure behaviors cannot have side effects")
    }

    pub fn output_not_written(outputs: &[String]) -> Self {
        let list = outputs.join(", ");
        Diagnostic::error(ErrorCode::E0604, format!("outputs not written on all paths: {}", list))
            .with_help("ensure all outputs are written before every exit path")
    }

    // --- Concurrency Errors ---

    pub fn non_atomic_shared_access(span: Span, handle: &str, operation: &str) -> Self {
        Diagnostic::error(ErrorCode::E0701, format!("non-atomic {} on SHARED handle '{}'", operation, handle))
            .with_primary_label(span, "non-atomic access")
            .with_help(format!("use ATOMIC_{} for SHARED handles", operation))
    }

    pub fn spawn_requires_pattern(span: Span, target: &str) -> Self {
        Diagnostic::error(ErrorCode::E0703, format!("SPAWN requires a pattern, '{}' is a behavior", target))
            .with_primary_label(span, "not a pattern")
            .with_note("SPAWN creates concurrent pattern instances, use CALL for behaviors")
    }

    // --- General Errors ---

    pub fn undefined_variable(span: Span, name: &str) -> Self {
        Diagnostic::error(ErrorCode::E0901, format!("undefined variable '{}'", name))
            .with_primary_label(span, "not found in this scope")
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::E0301.to_string(), "E0301");
        assert_eq!(ErrorCode::E0101.to_string(), "E0101");
    }

    #[test]
    fn test_error_code_category() {
        assert_eq!(ErrorCode::E0301.category(), "memory error");
        assert_eq!(ErrorCode::E0101.category(), "type error");
        assert_eq!(ErrorCode::E0601.category(), "guarantee violation");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Note < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn test_diagnostic_builder() {
        let span = Span::new(10, 5, 4);
        let diag = Diagnostic::error(ErrorCode::E0301, "use after free")
            .with_primary_label(span, "used here")
            .with_help("move use before FREE");

        assert_eq!(diag.code, ErrorCode::E0301);
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.labels.len(), 1);
        assert!(diag.labels[0].primary);
        assert!(diag.help.is_some());
    }

    #[test]
    fn test_use_after_free_diagnostic() {
        let use_span = Span::new(20, 3, 6);
        let free_span = Span::new(15, 3, 4);

        let diag = Diagnostic::use_after_free(use_span, free_span, "buffer");

        assert_eq!(diag.code, ErrorCode::E0301);
        assert_eq!(diag.labels.len(), 2);
        assert!(diag.labels[0].primary);
        assert!(!diag.labels[1].primary);
        assert_eq!(diag.line(), Some(20));
    }

    #[test]
    fn test_double_free_diagnostic() {
        let first = Span::new(10, 3, 4);
        let second = Span::new(20, 3, 4);

        let diag = Diagnostic::double_free(second, first, "buf");

        assert_eq!(diag.code, ErrorCode::E0302);
        assert_eq!(diag.labels.len(), 2);
        assert!(diag.help.is_some());
    }

    #[test]
    fn test_diagnostic_with_file() {
        let span = Span::new(1, 1, 1);
        let diag = Diagnostic::error(ErrorCode::E0901, "test")
            .with_file("test.beh")
            .with_primary_label(span, "here");

        assert_eq!(diag.file, Some(PathBuf::from("test.beh")));
    }

    #[test]
    fn test_diagnostic_with_notes() {
        let span = Span::new(1, 1, 1);
        let diag = Diagnostic::error(ErrorCode::E0601, "pure violation")
            .with_primary_label(span, "here")
            .with_note("first note")
            .with_note("second note");

        assert_eq!(diag.notes.len(), 2);
    }
}
