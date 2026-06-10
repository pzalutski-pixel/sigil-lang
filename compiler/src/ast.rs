//! Abstract Syntax Tree
//!
//! AST types for Sigil language per Language Reference.
//!
//! Some nodes are defined per the language spec but not yet emitted by the
//! parser or consumed by codegen (e.g. concurrency: Spawn/Wait, libraries).
#![allow(dead_code)]

use crate::lexer::Span;

#[derive(Debug, Clone)]
pub struct Behavior {
    pub name: String,
    pub description: Option<String>,
    pub contract: Contract,
    pub hash: String,
    pub implementations: Vec<PlatformImpl>,  // Can have multiple platform-specific
    pub composition: Option<Composition>,
    pub is_native: bool,  // NATIVE: implementation provided by runtime
}

#[derive(Debug, Clone)]
pub struct Contract {
    pub inputs: Vec<Parameter>,
    pub outputs: Vec<Parameter>,
    pub requires: Requirements,
    pub guarantees: Vec<Guarantee>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub typ: Type,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Bytes,
    Int,
    Float,
    String,
}

#[derive(Debug, Clone, Default)]
pub struct Requirements {
    pub behaviors: Vec<BehaviorRef>,
    pub memory: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BehaviorRef {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Guarantee {
    Pure,
    NoAlloc,
    WritesOutput,
}

#[derive(Debug, Clone)]
pub struct PlatformImpl {
    pub platform: Option<String>,  // None = all platforms, Some("linux"|"windows"|"macos")
    pub nodes: Vec<Node>,
}

// Pattern: unit with persistent memory (Section 8)
// Per spec: SPAWN args map to MEMORY slots by position
#[derive(Debug, Clone)]
pub struct Pattern {
    pub name: String,
    pub memory: Vec<MemoryDecl>,       // MEMORY section (includes SPAWN args + local state)
    pub on_create: Vec<Node>,          // ON_CREATE statements
    pub behaviors: Vec<Behavior>,      // Nested behaviors
    pub on_destroy: Vec<Node>,         // ON_DESTROY statements
}

#[derive(Debug, Clone)]
pub struct MemoryDecl {
    pub name: String,
    pub typ: Type,
    pub size: usize,
}

// Library: collection of behaviors (Section 11.1)
#[derive(Debug, Clone)]
pub struct Library {
    pub name: String,
    pub behaviors: Vec<Behavior>,
}

// Executable: entry point (Section 11.2)
#[derive(Debug, Clone)]
pub struct Executable {
    pub name: String,
    pub description: Option<String>,
    pub uses: Vec<String>,             // USES declarations
    pub entry: Vec<Node>,              // ENTRY statements
}

#[derive(Debug, Clone)]
pub struct Implementation {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct Composition {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub span: Span,
    pub kind: NodeKind,
}

impl Node {
    /// Get line number for backward compatibility
    pub fn line(&self) -> usize {
        self.span.line
    }
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    // Assignment: result = operation
    Assignment {
        target: String,
        expr: Box<Expr>,
    },

    // Label definition
    Label(String),

    // Scoped memory (automatic cleanup)
    Scope {
        nodes: Vec<Node>,
    },

    // Control flow
    Branch {
        condition: Box<Expr>,
        true_label: String,
        false_label: String,
    },
    Jump(String),

    // Assignment (composition)
    Set {
        target: String,
        value: Box<Expr>,
    },

    // Explicitly drop a produced output so it counts as consumed (Section 7.8)
    Discard(Box<Expr>),

    // Memory
    Store {
        target: String,
        value: Box<Expr>,
        size: usize,
        offset: Option<Box<Expr>>,
    },
    Free(String),

    // Concurrency (Section 9)
    // Per spec: spawn = identifier "=" "SPAWN" identifier { argument } ;
    Spawn {
        target: String,
        pattern: String,
        args: Vec<Expr>,
    },
    // Per spec: wait = identifier "=" "WAIT" identifier ;
    Wait {
        target: String,
        handle: String,
    },
    // Per spec: wait_all = [ identifier { "," identifier } "=" ] "WAIT_ALL" identifier { identifier } ;
    WaitAll {
        targets: Vec<String>,
        handles: Vec<String>,
    },
    // Per spec: wait_any = identifier "," identifier "=" "WAIT_ANY" { identifier } ;
    WaitAny {
        result: String,
        which: String,
        handles: Vec<String>,
    },
    // Per spec: channel_send = "CHANNEL_SEND" identifier value ;
    ChannelSend {
        channel: String,
        value: Box<Expr>,
    },
    // Per spec: channel_close = "CHANNEL_CLOSE" identifier ;
    ChannelClose(String),

    // Behavior call (in composition)
    // Per spec Section 7.1:
    //   result = CALL behavior args...      -> target = Some("result"), outputs = []
    //   CALL behavior args -> out1, out2    -> target = None, outputs = ["out1", "out2"]
    Call {
        target: Option<String>,
        behavior: String,
        args: Vec<Expr>,
        outputs: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),

    // Variable reference
    Var(String),

    // Field access
    Field {
        base: Box<Expr>,
        field: String,
    },

    // Primitives
    Load {
        source: Box<Expr>,
        size: usize,
        offset: Option<Box<Expr>>,
    },
    Alloc {
        size: Box<Expr>,
        typ: Type,
        shared: bool,  // SHARED modifier for concurrent access
    },

    // Arithmetic
    Iadd(Box<Expr>, Box<Expr>, usize),
    Isub(Box<Expr>, Box<Expr>, usize),
    Imul(Box<Expr>, Box<Expr>, usize),
    Idiv(Box<Expr>, Box<Expr>, usize),
    Imod(Box<Expr>, Box<Expr>, usize),
    Ineg(Box<Expr>, usize),

    Fadd(Box<Expr>, Box<Expr>, usize),
    Fsub(Box<Expr>, Box<Expr>, usize),
    Fmul(Box<Expr>, Box<Expr>, usize),
    Fdiv(Box<Expr>, Box<Expr>, usize),
    Fneg(Box<Expr>, usize),

    // Comparison
    Ieq(Box<Expr>, Box<Expr>, usize),
    Ine(Box<Expr>, Box<Expr>, usize),
    Ilt(Box<Expr>, Box<Expr>, usize),
    Igt(Box<Expr>, Box<Expr>, usize),
    Ile(Box<Expr>, Box<Expr>, usize),
    Ige(Box<Expr>, Box<Expr>, usize),

    Feq(Box<Expr>, Box<Expr>, usize),
    Fne(Box<Expr>, Box<Expr>, usize),
    Flt(Box<Expr>, Box<Expr>, usize),
    Fgt(Box<Expr>, Box<Expr>, usize),
    Fle(Box<Expr>, Box<Expr>, usize),
    Fge(Box<Expr>, Box<Expr>, usize),

    // Bitwise
    And(Box<Expr>, Box<Expr>, usize),
    Or(Box<Expr>, Box<Expr>, usize),
    Xor(Box<Expr>, Box<Expr>, usize),
    Not(Box<Expr>, usize),
    Shl(Box<Expr>, Box<Expr>, usize),
    Shr(Box<Expr>, Box<Expr>, usize),
    Sar(Box<Expr>, Box<Expr>, usize),

    // Atomic
    AtomicLoad(Box<Expr>, usize),
    AtomicStore(Box<Expr>, Box<Expr>, usize),
    Cas {
        addr: Box<Expr>,
        expected: Box<Expr>,
        new: Box<Expr>,
        size: usize,
    },
    AtomicAdd(Box<Expr>, Box<Expr>, usize),
    AtomicSub(Box<Expr>, Box<Expr>, usize),

    // Conversion
    Ftoi {
        value: Box<Expr>,
        float_size: usize,
        int_size: usize,
    },
    Itof {
        value: Box<Expr>,
        int_size: usize,
        float_size: usize,
    },

    // Behavior call (in composition)
    Call {
        behavior: String,
        args: Vec<Expr>,
    },

    // Concurrency expressions
    // Per spec: channel = identifier "=" "CHANNEL" interpretation capacity ;
    Channel {
        typ: Type,
        capacity: Box<Expr>,
    },
    // Per spec: channel_receive = identifier "=" "CHANNEL_RECEIVE" identifier ;
    ChannelReceive(String),
    // Per spec: spawn = identifier "=" "SPAWN" identifier { argument } ;
    Spawn {
        pattern: String,
        args: Vec<Expr>,
    },
    // Per spec: wait = identifier "=" "WAIT" identifier ;
    Wait(String),
}

impl Behavior {
    pub fn new(name: String) -> Self {
        Behavior {
            name,
            description: None,
            contract: Contract {
                inputs: Vec::new(),
                outputs: Vec::new(),
                requires: Requirements::default(),
                guarantees: Vec::new(),
            },
            hash: String::new(),
            implementations: Vec::new(),
            composition: None,
            is_native: false,
        }
    }
}
