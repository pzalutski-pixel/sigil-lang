//! Semantic analysis error types
//!
//! Contains error and warning types for semantic validation per Sigil Reference Section 14.

use crate::diagnostic::{Diagnostic, ErrorCode};
use crate::lexer::Span;

// =============================================================================
// Error Types
// =============================================================================

/// Semantic validation error with location and context
#[derive(Debug, Clone)]
pub struct SemanticError {
    pub kind: ErrorKind,
    pub line: usize,
    pub context: Option<String>,
}

/// Error kinds for semantic validation
/// Some variants are reserved for future validation phases
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ErrorKind {
    // Type errors (Section 14.2)
    InvalidIntSize { size: usize },
    InvalidFloatSize { size: usize },
    TypeMismatch { expected: String, found: String },
    /// A handle (address) — an INPUT/OUTPUT/ALLOC/CALL-result — used directly
    /// where a primitive expects a value (e.g. `IADD x 1` on an un-LOADed
    /// INPUT). The value must be read out with LOAD first. Caught here with a
    /// source line instead of leaking into codegen as `got handle`.
    HandleUsedAsValue { operand: String, kind: String, op: String },

    // Label errors (Section 14.4)
    UndefinedLabel { name: String },
    DuplicateLabel { name: String, first_line: usize },

    // Memory errors (Section 14.1) - reserved for data-flow analysis
    UninitializedRead { handle: String },
    PossibleBoundsViolation { handle: String, offset: usize, size: usize },

    // Contract errors (Section 14.3)
    MissingDependency { behavior: String },
    CallArgCountMismatch { behavior: String, expected: usize, got: usize },
    CallArgTypeMismatch { behavior: String, param: String, expected: String, found: String },
    CallArgSizeMismatch { behavior: String, param: String, expected: usize, found: usize },
    OutputSizeMismatch { output: String, expected: usize, found: usize },
    HashMismatch { declared: String, computed: String },
    DependencyHashMismatch { behavior: String, expected: String, actual: String },

    // Behavior structure errors (Section 6: leaf vs composite; DESIGN-MODEL 4.3)
    /// A composite (COMPOSITION / ENTRY) performs primitive computation.
    /// Composites only wire; computation belongs in a leaf or pattern.
    CompositeComputes { op: String },
    /// A leaf (IMPLEMENTATION) contains a CALL. A behavior that calls others is
    /// a composite and must use COMPOSITION.
    LeafCalls { behavior: String },
    /// A composition produces an output that is never consumed — not routed to a
    /// CALL, branched on, written to an OUTPUT, or DISCARDed (Section 7.8).
    UnconsumedOutput { behavior: String, output: String },

    // Guarantee violations (Section 13.6)
    PureViolation { reason: String },
    NoAllocViolation { reason: String },
    OutputNotWritten { outputs: Vec<String> },

    // Concurrency errors (Section 14.5)
    SpawnRequiresPattern { target: String },
    /// Non-atomic access to SHARED memory (Section 14.5)
    NonAtomicSharedAccess { handle: String, operation: String },
    /// Channel send/receive type mismatch (Section 14.5)
    ChannelTypeMismatch { channel: String, expected: String, found: String },

    // Memory leak detection (Section 14.1)
    /// ALLOC without FREE on some path
    MemoryLeak { handle: String },
    /// LOAD without prior STORE on all paths (Section 14.1)
    UninitializedLoad { handle: String },

    // Control flow errors (Section 14.4)
    /// Path does not terminate (no END or infinite loop without exit)
    PathDoesNotTerminate,

    // Channel errors (Section 14.5)
    /// Channel send type mismatch
    ChannelSendTypeMismatch { channel: String, expected: String, found: String },
    /// Channel receive type mismatch
    ChannelReceiveTypeMismatch { channel: String, expected: String, found: String },

    // Ownership errors (SAFETY-RULES Part 2)
    /// Attempt to use a handle after it was freed
    UseAfterFree { handle: String, freed_at: usize },
    /// Attempt to free a handle that was already freed
    DoubleFree { handle: String, first_free_at: usize },
    /// Attempt to free a handle while it is borrowed by a spawned pattern
    FreeWhileBorrowed { handle: String, borrowed_by: String },
    /// Attempt to free a handle that was borrowed (INPUT parameter)
    FreeBorrowedHandle { handle: String },

    // General
    UndefinedVariable { name: String },
}

impl SemanticError {
    /// Convert to a Diagnostic for rich error output
    pub fn to_diagnostic(&self) -> Diagnostic {
        // Create span from line number (column 1, length 1 as fallback)
        let span = Span::new(self.line, 1, 1);

        let (code, message, label, help) = match &self.kind {
            // Type errors (Section 14.2)
            ErrorKind::InvalidIntSize { size } => (
                ErrorCode::E0102,
                format!("invalid int size {}", size),
                "must be 1, 2, 4, or 8",
                Some("valid integer sizes are 1, 2, 4, or 8 bytes"),
            ),
            ErrorKind::InvalidFloatSize { size } => (
                ErrorCode::E0103,
                format!("invalid float size {}", size),
                "must be 4 or 8",
                Some("valid float sizes are 4 (single) or 8 (double) bytes"),
            ),
            ErrorKind::TypeMismatch { expected, found } => (
                ErrorCode::E0101,
                format!("type mismatch: expected {}, found {}", expected, found),
                "type mismatch here",
                None,
            ),
            ErrorKind::HandleUsedAsValue { operand, kind, op } => (
                ErrorCode::E0104,
                format!("{} '{}' is a handle, not a value, but {} needs a value", kind, operand, op),
                "handle used where a value is expected",
                Some("read the value out with LOAD first, e.g. `v = LOAD <handle> <size>`, then use `v`"),
            ),

            // Memory errors (Section 14.1)
            ErrorKind::UseAfterFree { handle, freed_at } => (
                ErrorCode::E0301,
                format!("use of handle '{}' after free at line {}", handle, freed_at),
                "use of freed handle",
                Some("move this use before the FREE statement"),
            ),
            ErrorKind::DoubleFree { handle, first_free_at } => (
                ErrorCode::E0302,
                format!("double free of handle '{}' (first at line {})", handle, first_free_at),
                "second FREE here",
                Some("remove one of the FREE statements"),
            ),
            ErrorKind::MemoryLeak { handle } => (
                ErrorCode::E0303,
                format!("memory leak: '{}' not freed on all paths", handle),
                "allocated here",
                Some("add FREE before all exit paths"),
            ),
            ErrorKind::UninitializedLoad { handle } => (
                ErrorCode::E0304,
                format!("load from uninitialized handle '{}'", handle),
                "LOAD without prior STORE",
                Some("add STORE to initialize before LOAD"),
            ),
            ErrorKind::UninitializedRead { handle } => (
                ErrorCode::E0304,
                format!("read from uninitialized handle '{}'", handle),
                "may be uninitialized",
                None,
            ),
            ErrorKind::PossibleBoundsViolation { handle, offset, size } => (
                ErrorCode::E0307,
                format!("possible bounds violation on '{}' at offset {} size {}", handle, offset, size),
                "access may be out of bounds",
                None,
            ),
            ErrorKind::FreeBorrowedHandle { handle } => (
                ErrorCode::E0305,
                format!("cannot free borrowed handle '{}'", handle),
                "borrowed from caller",
                Some("INPUT handles cannot be freed"),
            ),
            ErrorKind::FreeWhileBorrowed { handle, borrowed_by } => (
                ErrorCode::E0306,
                format!("cannot free '{}' while borrowed by '{}'", handle, borrowed_by),
                "FREE while borrowed",
                Some("call WAIT before FREE"),
            ),

            // Label errors (Section 14.4)
            ErrorKind::UndefinedLabel { name } => (
                ErrorCode::E0401,
                format!("undefined label '{}'", name),
                "label not found",
                None,
            ),
            ErrorKind::DuplicateLabel { name, first_line } => (
                ErrorCode::E0402,
                format!("duplicate label '{}' (first at line {})", name, first_line),
                "duplicate definition",
                None,
            ),
            ErrorKind::PathDoesNotTerminate => (
                ErrorCode::E0403,
                "path does not terminate".to_string(),
                "no END reached",
                Some("ensure all paths reach END"),
            ),

            // Contract errors (Section 14.3)
            ErrorKind::MissingDependency { behavior } => (
                ErrorCode::E0502,
                format!("missing dependency '{}'", behavior),
                "not found",
                None,
            ),
            ErrorKind::CallArgCountMismatch { behavior, expected, got } => (
                ErrorCode::E0503,
                format!("wrong argument count for '{}': expected {}, got {}", behavior, expected, got),
                "argument count mismatch",
                None,
            ),
            ErrorKind::CallArgTypeMismatch { behavior, param, expected, found } => (
                ErrorCode::E0504,
                format!("type mismatch in call to '{}': '{}' expected {}, found {}", behavior, param, expected, found),
                "type mismatch",
                None,
            ),
            ErrorKind::CallArgSizeMismatch { behavior, param, expected, found } => (
                ErrorCode::E0505,
                format!("size mismatch in call to '{}': '{}' expected {} bytes, found {}", behavior, param, expected, found),
                "size mismatch",
                None,
            ),
            ErrorKind::OutputSizeMismatch { output, expected, found } => (
                ErrorCode::E0506,
                format!("output '{}' size mismatch: declared {}, storing {}", output, expected, found),
                "size mismatch",
                None,
            ),
            ErrorKind::HashMismatch { declared, computed } => (
                ErrorCode::E0507,
                format!("hash mismatch: declared {}, computed {}", declared, computed),
                "hash mismatch",
                Some("update HASH declaration"),
            ),
            ErrorKind::DependencyHashMismatch { behavior, expected, actual } => (
                ErrorCode::E0508,
                format!("dependency '{}' hash mismatch: expected {}, got {}", behavior, expected, actual),
                "hash mismatch",
                None,
            ),

            // Behavior structure errors (Section 6; DESIGN-MODEL 4.3)
            ErrorKind::CompositeComputes { op } => (
                ErrorCode::E0509,
                format!("composite behavior may not use the primitive '{}'", op),
                "computation in a composite",
                Some("composites only wire behaviors together; move computation into a leaf (IMPLEMENTATION) or a pattern's ON_CREATE"),
            ),
            ErrorKind::LeafCalls { behavior } => (
                ErrorCode::E0510,
                format!("leaf behavior may not CALL '{}'", behavior),
                "CALL inside a leaf",
                Some("a behavior that calls others is a composite; use COMPOSITION instead of IMPLEMENTATION"),
            ),
            ErrorKind::UnconsumedOutput { behavior, output } => (
                ErrorCode::E0511,
                format!("output '{}' from CALL '{}' is never consumed", output, behavior),
                "unconsumed output",
                Some("route it to a CALL / BRANCH / OUTPUT, or drop it explicitly with DISCARD"),
            ),

            // Guarantee violations (Section 13.6)
            ErrorKind::PureViolation { reason } => (
                ErrorCode::E0601,
                format!("pure guarantee violated: {}", reason),
                "violates pure",
                Some("pure behaviors cannot have side effects"),
            ),
            ErrorKind::NoAllocViolation { reason } => (
                ErrorCode::E0602,
                format!("no_alloc guarantee violated: {}", reason),
                "allocation not allowed",
                None,
            ),
            ErrorKind::OutputNotWritten { outputs } => (
                ErrorCode::E0604,
                format!("outputs not written on all paths: {}", outputs.join(", ")),
                "missing output write on some path",
                Some("every OUTPUT must be written on every path — including the one where a loop runs zero times or a branch is not taken; write a default before the loop/branch"),
            ),

            // Concurrency errors (Section 14.5)
            ErrorKind::NonAtomicSharedAccess { handle, operation } => (
                ErrorCode::E0701,
                format!("non-atomic {} on SHARED handle '{}'", operation, handle),
                "non-atomic access",
                Some("use ATOMIC operations for SHARED"),
            ),
            ErrorKind::ChannelTypeMismatch { channel, expected, found } |
            ErrorKind::ChannelSendTypeMismatch { channel, expected, found } |
            ErrorKind::ChannelReceiveTypeMismatch { channel, expected, found } => (
                ErrorCode::E0702,
                format!("channel '{}' type mismatch: expected {}, found {}", channel, expected, found),
                "type mismatch",
                None,
            ),
            ErrorKind::SpawnRequiresPattern { target } => (
                ErrorCode::E0703,
                format!("SPAWN requires pattern, '{}' is a behavior", target),
                "not a pattern",
                Some("use CALL for behaviors"),
            ),

            // General
            ErrorKind::UndefinedVariable { name } => (
                ErrorCode::E0901,
                format!("undefined variable '{}'", name),
                "not found",
                None,
            ),
        };

        let mut diag = Diagnostic::error(code, message)
            .with_primary_label(span, label);

        if let Some(h) = help {
            diag = diag.with_help(h);
        }

        if let Some(ctx) = &self.context {
            diag = diag.with_note(ctx.clone());
        }

        diag
    }
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Line {}: ", self.line)?;
        match &self.kind {
            ErrorKind::InvalidIntSize { size } =>
                write!(f, "Invalid int size {}. Must be 1, 2, 4, or 8", size),
            ErrorKind::InvalidFloatSize { size } =>
                write!(f, "Invalid float size {}. Must be 4 or 8", size),
            ErrorKind::TypeMismatch { expected, found } =>
                write!(f, "Type mismatch: expected {}, found {}", expected, found),
            ErrorKind::HandleUsedAsValue { operand, kind, op } =>
                write!(f, "{} '{}' is a handle, not a value, but {} needs a value. LOAD it first (e.g. `v = LOAD {} <size>`)", kind, operand, op, operand),
            ErrorKind::UndefinedLabel { name } =>
                write!(f, "Undefined label '{}'", name),
            ErrorKind::DuplicateLabel { name, first_line } =>
                write!(f, "Duplicate label '{}' (first defined at line {})", name, first_line),
            ErrorKind::UninitializedRead { handle } =>
                write!(f, "Possible read from uninitialized handle '{}'", handle),
            ErrorKind::PossibleBoundsViolation { handle, offset, size } =>
                write!(f, "Possible bounds violation: {}[{}:{}]", handle, offset, offset + size),
            ErrorKind::MissingDependency { behavior } =>
                write!(f, "Required behavior '{}' not found", behavior),
            ErrorKind::CallArgCountMismatch { behavior, expected, got } =>
                write!(f, "CALL '{}': expected {} arguments, got {}", behavior, expected, got),
            ErrorKind::CallArgTypeMismatch { behavior, param, expected, found } =>
                write!(f, "CALL '{}': argument '{}' type mismatch: expected {}, found {}", behavior, param, expected, found),
            ErrorKind::CallArgSizeMismatch { behavior, param, expected, found } =>
                write!(f, "CALL '{}': argument '{}' size mismatch: expected {} bytes, found {}", behavior, param, expected, found),
            ErrorKind::OutputSizeMismatch { output, expected, found } =>
                write!(f, "STORE to output '{}': size mismatch: declared {} bytes, storing {}", output, expected, found),
            ErrorKind::HashMismatch { declared, computed } =>
                write!(f, "Contract hash mismatch: declared {}, computed {}", declared, computed),
            ErrorKind::DependencyHashMismatch { behavior, expected, actual } =>
                write!(f, "Dependency '{}' hash mismatch: expected {}, got {}", behavior, expected, actual),
            ErrorKind::CompositeComputes { op } =>
                write!(f, "Composite behavior may not use primitive '{}'. Per the leaf/composite model, composites only wire behaviors; computation belongs in a leaf or pattern", op),
            ErrorKind::LeafCalls { behavior } =>
                write!(f, "Leaf (IMPLEMENTATION) may not CALL '{}'. A behavior that calls others is a composite; use COMPOSITION", behavior),
            ErrorKind::UnconsumedOutput { behavior, output } =>
                write!(f, "Output '{}' produced by CALL '{}' is never consumed. Route it (CALL/BRANCH/OUTPUT) or drop it with DISCARD", output, behavior),
            ErrorKind::PureViolation { reason } =>
                write!(f, "Guarantee 'pure' violated: {}", reason),
            ErrorKind::NoAllocViolation { reason } =>
                write!(f, "Guarantee 'no_alloc' violated: {}", reason),
            ErrorKind::OutputNotWritten { outputs } =>
                write!(f, "Outputs not written on all paths: {:?}", outputs),
            ErrorKind::SpawnRequiresPattern { target } =>
                write!(f, "SPAWN requires a pattern target, '{}' is a behavior. Per Section 9.1, SPAWN creates 'concurrent pattern instance'", target),
            ErrorKind::NonAtomicSharedAccess { handle, operation } =>
                write!(f, "Non-atomic {} on SHARED handle '{}'. Per Section 14.5, use ATOMIC_LOAD/ATOMIC_STORE", operation, handle),
            ErrorKind::ChannelTypeMismatch { channel, expected, found } =>
                write!(f, "Channel '{}' type mismatch: expected {}, found {}", channel, expected, found),
            ErrorKind::MemoryLeak { handle } =>
                write!(f, "Memory leak: '{}' allocated but not freed on all paths. Per Section 14.1, every ALLOC must have FREE or be scoped", handle),
            ErrorKind::UninitializedLoad { handle } =>
                write!(f, "Uninitialized LOAD from '{}'. Per Section 14.1, LOAD requires prior STORE on all paths", handle),
            ErrorKind::PathDoesNotTerminate =>
                write!(f, "Path does not terminate. Per Section 14.4, all paths must reach END or loop"),
            ErrorKind::ChannelSendTypeMismatch { channel, expected, found } =>
                write!(f, "Channel '{}' send type mismatch: expected {}, found {}", channel, expected, found),
            ErrorKind::ChannelReceiveTypeMismatch { channel, expected, found } =>
                write!(f, "Channel '{}' receive type mismatch: expected {}, found {}", channel, expected, found),
            ErrorKind::UseAfterFree { handle, freed_at } =>
                write!(f, "Use of handle '{}' after FREE at line {}", handle, freed_at),
            ErrorKind::DoubleFree { handle, first_free_at } =>
                write!(f, "Double FREE of handle '{}' (first freed at line {})", handle, first_free_at),
            ErrorKind::FreeWhileBorrowed { handle, borrowed_by } =>
                write!(f, "Cannot FREE handle '{}' while borrowed by spawned pattern '{}'", handle, borrowed_by),
            ErrorKind::FreeBorrowedHandle { handle } =>
                write!(f, "Cannot FREE handle '{}': borrowed from caller (INPUT parameter)", handle),
            ErrorKind::UndefinedVariable { name } =>
                write!(f, "Undefined variable '{}'", name),
        }?;
        if let Some(ctx) = &self.context {
            write!(f, " ({})", ctx)?;
        }
        Ok(())
    }
}

/// Semantic warning (non-fatal) - reserved for future use
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SemanticWarning {
    pub message: String,
    pub line: usize,
}
