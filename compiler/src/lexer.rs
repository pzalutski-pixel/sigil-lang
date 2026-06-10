//! Lexer Module
//!
//! Tokenizes Sigil source according to the Language Reference Section 1.
//! Each token carries source location (Span) for precise error reporting.

use std::fmt;

// =============================================================================
// Source Location
// =============================================================================

/// Source span for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

impl Span {
    pub fn new(line: usize, col: usize, len: usize) -> Self {
        Span { line, col, len }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

// =============================================================================
// Lexer Error
// =============================================================================

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lex error at {}: {}", self.span, self.message)
    }
}

impl std::error::Error for LexError {}

// =============================================================================
// Token Types
// =============================================================================

/// Token kind - per Sigil Reference Section 1.6-1.8
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // === Structure Keywords (Section 1.6) ===
    Behavior,
    Description,
    Contract,
    Hash,
    Implementation,
    Composition,
    Native,
    End,
    Pattern,
    EndPattern,
    OnCreate,
    OnDestroy,
    Library,
    EndLibrary,
    Executable,
    EndExecutable,
    Entry,
    Uses,
    Memory,
    Scope,
    EndScope,

    // === Contract Keywords (Section 1.6) ===
    Input,
    Output,
    Requires,
    Guarantees,

    // === Guarantee Keywords (Section 1.6) ===
    Pure,
    NoAlloc,
    WritesOutput,

    // === Interpretation Keywords (Section 1.6) ===
    Int,
    Float,
    Bytes,
    String,

    // === Control Keywords (Section 1.6) ===
    Label,
    Branch,
    Jump,

    // === Composition Keywords (Section 1.6) ===
    Call,
    Set,
    Discard,

    // === Concurrency Keywords (Section 1.6) ===
    Spawn,
    Wait,
    WaitAll,
    WaitAny,
    Channel,
    ChannelSend,
    ChannelReceive,
    ChannelClose,
    Shared,

    // === Memory Primitives (Section 4.1) ===
    Alloc,
    Load,
    Store,
    Free,

    // === Atomic Primitives (Section 4.2) ===
    AtomicLoad,
    AtomicStore,
    Cas,
    AtomicAdd,
    AtomicSub,

    // === Integer Arithmetic Primitives (Section 4.3) ===
    Iadd,
    Isub,
    Imul,
    Idiv,
    Imod,
    Ineg,

    // === Integer Comparison Primitives (Section 4.4) ===
    Ieq,
    Ine,
    Ilt,
    Igt,
    Ile,
    Ige,

    // === Float Arithmetic Primitives (Section 4.5) ===
    Fadd,
    Fsub,
    Fmul,
    Fdiv,
    Fneg,

    // === Float Comparison Primitives (Section 4.6) ===
    Feq,
    Fne,
    Flt,
    Fgt,
    Fle,
    Fge,

    // === Conversion Primitives (Section 4.7) ===
    Ftoi,
    Itof,

    // === Bitwise Primitives (Section 4.8) ===
    And,
    Or,
    Xor,
    Not,
    Shl,
    Shr,
    Sar,

    // === Literals (Section 1.7) ===
    Integer(i64),
    FloatLit(f64),
    StringLit(String),

    // === Identifier (Section 1.5) ===
    Identifier(String),

    // === Operators and Punctuation (Section 1.8) ===
    Equals,      // =
    Arrow,       // ->
    Dot,         // .
    At,          // @
    Comma,       // ,
    Colon,       // :
    LeftBracket, // [
    RightBracket,// ]
    // Raw text (for unknown characters - used in descriptions)
    RawText(String),

    // === Structure ===
    Newline,
    Indent(usize),

    // === End of File ===
    Eof,
}

/// Token with source location
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }
}

// =============================================================================
// Lexer
// =============================================================================

/// Lexer state
pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: usize,
    col: usize,
    at_line_start: bool,
    /// Inside a DESCRIPTION block: free documentation text, lexed permissively
    /// (a `"` is plain text, not a string-literal start). Set on the DESCRIPTION
    /// keyword, cleared on the next section keyword at line start — mirroring the
    /// parser's `parse_description` termination.
    in_description: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        // Strip a leading UTF-8 byte-order mark if present. Editors and tools
        // (notably PowerShell's Set-Content) often prepend a BOM; without this it
        // lexes as a stray token and parsing fails with a confusing error — a real
        // hazard for a language whose files are written by tooling.
        let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);
        Lexer {
            chars: source.char_indices().peekable(),
            line: 1,
            col: 1,
            at_line_start: true,
            in_description: false,
        }
    }

    /// Peek at next character without consuming
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    /// Advance and return next character
    fn advance(&mut self) -> Option<char> {
        let result = self.chars.next().map(|(_, c)| c);
        if let Some(c) = result {
            if c == '\n' {
                self.line += 1;
                self.col = 1;
                self.at_line_start = true;
            } else {
                self.col += 1;
            }
        }
        result
    }

    /// Skip horizontal whitespace (space/tab)
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Read identifier: letter { letter | digit | "-" }
    /// Per spec Section 1.5
    fn read_identifier(&mut self, first: char) -> String {
        let mut ident = String::new();
        ident.push(first);

        while let Some(c) = self.peek() {
            // Per spec: identifier = letter { letter | digit | "-" }
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    /// Read number literal (integer or float)
    /// Per spec Section 1.7
    fn read_number(&mut self, first: char, start_col: usize) -> Result<Token, LexError> {
        let start_line = self.line;
        let mut num = String::new();
        num.push(first);

        let mut is_float = false;
        let mut is_hex = false;

        // Check for hex prefix
        if first == '0' && self.peek() == Some('x') {
            is_hex = true;
            num.push('x');
            self.advance();

            // Read hex digits
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    num.push(c);
                    self.advance();
                } else {
                    break;
                }
            }

            if num.len() == 2 {
                return Err(LexError {
                    span: Span::new(start_line, start_col, num.len()),
                    message: "Expected hex digits after 0x".to_string(),
                });
            }
        } else {
            // Read decimal digits
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    num.push(c);
                    self.advance();
                } else if c == '.' && !is_float {
                    // Check next char is digit (not method call like x.field)
                    is_float = true;
                    num.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let span = Span::new(start_line, start_col, num.len());

        if is_hex {
            // Accept the full 64-bit range: parse as u64 and reinterpret as the
            // i64 bit pattern (so 0xcbf29ce484222325 etc. are valid, not just
            // values <= i64::MAX). Fall back to i64 parse for a leading '-'.
            u64::from_str_radix(&num[2..], 16)
                .map(|n| Token::new(TokenKind::Integer(n as i64), span))
                .or_else(|_| i64::from_str_radix(&num[2..], 16)
                    .map(|n| Token::new(TokenKind::Integer(n), span)))
                .map_err(|_| LexError {
                    span,
                    message: format!("Invalid hex literal: {}", num),
                })
        } else if is_float {
            num.parse::<f64>()
                .map(|f| Token::new(TokenKind::FloatLit(f), span))
                .map_err(|_| LexError {
                    span,
                    message: format!("Invalid float literal: {}", num),
                })
        } else {
            num.parse::<i64>()
                .map(|n| Token::new(TokenKind::Integer(n), span))
                .map_err(|_| LexError {
                    span,
                    message: format!("Invalid integer literal: {}", num),
                })
        }
    }

    /// Read string literal
    /// Per spec Section 1.7
    fn read_string(&mut self, start_col: usize) -> Result<Token, LexError> {
        let start_line = self.line;
        let mut s = String::new();

        loop {
            match self.advance() {
                Some('"') => break,
                Some('\\') => {
                    // Escape sequences per spec
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('r') => s.push('\r'),
                        Some('t') => s.push('\t'),
                        Some('\\') => s.push('\\'),
                        Some('"') => s.push('"'),
                        Some(c) => {
                            return Err(LexError {
                                span: Span::new(self.line, self.col - 1, 2),
                                message: format!("Invalid escape sequence: \\{}", c),
                            });
                        }
                        None => {
                            return Err(LexError {
                                span: Span::new(start_line, start_col, 1),
                                message: "Unterminated string literal".to_string(),
                            });
                        }
                    }
                }
                Some('\n') => {
                    return Err(LexError {
                        span: Span::new(start_line, start_col, 1),
                        message: "Unterminated string literal (newline in string)".to_string(),
                    });
                }
                Some(c) => s.push(c),
                None => {
                    return Err(LexError {
                        span: Span::new(start_line, start_col, 1),
                        message: "Unterminated string literal".to_string(),
                    });
                }
            }
        }

        let span = Span::new(start_line, start_col, s.len() + 2);
        Ok(Token::new(TokenKind::StringLit(s), span))
    }

    /// Match identifier to keyword or return as identifier
    /// Keywords per spec Section 1.6 and Appendix C
    fn keyword_or_ident(&self, ident: &str, span: Span) -> Token {
        let kind = match ident {
            // Structure
            "BEHAVIOR" => TokenKind::Behavior,
            "DESCRIPTION" => TokenKind::Description,
            "CONTRACT" => TokenKind::Contract,
            "HASH" => TokenKind::Hash,
            "IMPLEMENTATION" => TokenKind::Implementation,
            "COMPOSITION" => TokenKind::Composition,
            "NATIVE" => TokenKind::Native,
            "END" => TokenKind::End,
            "PATTERN" => TokenKind::Pattern,
            "END_PATTERN" => TokenKind::EndPattern,
            "ON_CREATE" => TokenKind::OnCreate,
            "ON_DESTROY" => TokenKind::OnDestroy,
            "LIBRARY" => TokenKind::Library,
            "END_LIBRARY" => TokenKind::EndLibrary,
            "EXECUTABLE" => TokenKind::Executable,
            "END_EXECUTABLE" => TokenKind::EndExecutable,
            "ENTRY" => TokenKind::Entry,
            "USES" => TokenKind::Uses,
            "MEMORY" => TokenKind::Memory,
            "SCOPE" => TokenKind::Scope,
            "END_SCOPE" => TokenKind::EndScope,

            // Contract
            "INPUT" => TokenKind::Input,
            "OUTPUT" => TokenKind::Output,
            "REQUIRES" => TokenKind::Requires,
            "GUARANTEES" => TokenKind::Guarantees,

            // Guarantees
            "pure" => TokenKind::Pure,
            "no_alloc" => TokenKind::NoAlloc,
            "writes_output" => TokenKind::WritesOutput,

            // Interpretations
            "int" => TokenKind::Int,
            "float" => TokenKind::Float,
            "bytes" => TokenKind::Bytes,
            "string" => TokenKind::String,

            // Control
            "LABEL" => TokenKind::Label,
            "BRANCH" => TokenKind::Branch,
            "JUMP" => TokenKind::Jump,

            // Composition
            "CALL" => TokenKind::Call,
            "SET" => TokenKind::Set,
            "DISCARD" => TokenKind::Discard,

            // Concurrency
            "SPAWN" => TokenKind::Spawn,
            "WAIT" => TokenKind::Wait,
            "WAIT_ALL" => TokenKind::WaitAll,
            "WAIT_ANY" => TokenKind::WaitAny,
            "CHANNEL" => TokenKind::Channel,
            "CHANNEL_SEND" => TokenKind::ChannelSend,
            "CHANNEL_RECEIVE" => TokenKind::ChannelReceive,
            "CHANNEL_CLOSE" => TokenKind::ChannelClose,
            "SHARED" => TokenKind::Shared,

            // Memory primitives
            "ALLOC" => TokenKind::Alloc,
            "LOAD" => TokenKind::Load,
            "STORE" => TokenKind::Store,
            "FREE" => TokenKind::Free,

            // Atomic primitives
            "ATOMIC_LOAD" => TokenKind::AtomicLoad,
            "ATOMIC_STORE" => TokenKind::AtomicStore,
            "CAS" => TokenKind::Cas,
            "ATOMIC_ADD" => TokenKind::AtomicAdd,
            "ATOMIC_SUB" => TokenKind::AtomicSub,

            // Integer arithmetic
            "IADD" => TokenKind::Iadd,
            "ISUB" => TokenKind::Isub,
            "IMUL" => TokenKind::Imul,
            "IDIV" => TokenKind::Idiv,
            "IMOD" => TokenKind::Imod,
            "INEG" => TokenKind::Ineg,

            // Integer comparison
            "IEQ" => TokenKind::Ieq,
            "INE" => TokenKind::Ine,
            "ILT" => TokenKind::Ilt,
            "IGT" => TokenKind::Igt,
            "ILE" => TokenKind::Ile,
            "IGE" => TokenKind::Ige,

            // Float arithmetic
            "FADD" => TokenKind::Fadd,
            "FSUB" => TokenKind::Fsub,
            "FMUL" => TokenKind::Fmul,
            "FDIV" => TokenKind::Fdiv,
            "FNEG" => TokenKind::Fneg,

            // Float comparison
            "FEQ" => TokenKind::Feq,
            "FNE" => TokenKind::Fne,
            "FLT" => TokenKind::Flt,
            "FGT" => TokenKind::Fgt,
            "FLE" => TokenKind::Fle,
            "FGE" => TokenKind::Fge,

            // Conversion
            "FTOI" => TokenKind::Ftoi,
            "ITOF" => TokenKind::Itof,

            // Bitwise
            "AND" => TokenKind::And,
            "OR" => TokenKind::Or,
            "XOR" => TokenKind::Xor,
            "NOT" => TokenKind::Not,
            "SHL" => TokenKind::Shl,
            "SHR" => TokenKind::Shr,
            "SAR" => TokenKind::Sar,

            // Identifier
            _ => TokenKind::Identifier(ident.to_string()),
        };

        Token::new(kind, span)
    }

    /// Get next token
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        // Whether this token begins a line (before indent handling clears the
        // flag). A section keyword here ends a DESCRIPTION block — section
        // keywords sit at column 1, so the same word indented inside a
        // description body does not (matching the parser's line-start rule).
        let was_line_start = self.at_line_start;
        // Handle indentation at line start
        if self.at_line_start {
            let start_col = self.col;
            let mut indent = 0;

            while let Some(c) = self.peek() {
                if c == ' ' {
                    indent += 1;
                    self.advance();
                } else if c == '\t' {
                    indent += 2;
                    self.advance();
                } else {
                    break;
                }
            }

            self.at_line_start = false;

            if indent > 0 {
                if let Some(c) = self.peek() {
                    if c != '\n' && c != '#' {
                        return Ok(Token::new(
                            TokenKind::Indent(indent),
                            Span::new(self.line, start_col, indent),
                        ));
                    }
                }
            }
        }

        self.skip_whitespace();

        let start_line = self.line;
        let start_col = self.col;

        let c = match self.advance() {
            Some(c) => c,
            None => return Ok(Token::new(TokenKind::Eof, Span::new(start_line, start_col, 0))),
        };

        match c {
            '\n' => Ok(Token::new(TokenKind::Newline, Span::new(start_line, start_col, 1))),

            '#' => {
                // Comment - skip to end of line
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.advance();
                }
                // Return next token (skip comments)
                self.next_token()
            }

            '"' => {
                if self.in_description {
                    // Inside a DESCRIPTION, a quote is ordinary text — not a
                    // string-literal start. (parse_description re-reads the body
                    // from source by line, so this token's value is irrelevant;
                    // it only must not raise an unterminated-string lex error.)
                    Ok(Token::new(TokenKind::RawText("\"".to_string()), Span::new(start_line, start_col, 1)))
                } else {
                    self.read_string(start_col)
                }
            }

            '=' => Ok(Token::new(TokenKind::Equals, Span::new(start_line, start_col, 1))),
            ':' => Ok(Token::new(TokenKind::Colon, Span::new(start_line, start_col, 1))),
            '@' => Ok(Token::new(TokenKind::At, Span::new(start_line, start_col, 1))),
            '[' => Ok(Token::new(TokenKind::LeftBracket, Span::new(start_line, start_col, 1))),
            ']' => Ok(Token::new(TokenKind::RightBracket, Span::new(start_line, start_col, 1))),
            ',' => Ok(Token::new(TokenKind::Comma, Span::new(start_line, start_col, 1))),
            '.' => Ok(Token::new(TokenKind::Dot, Span::new(start_line, start_col, 1))),

            '-' => {
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::new(TokenKind::Arrow, Span::new(start_line, start_col, 2)))
                } else if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    self.read_number('-', start_col)
                } else {
                    // Standalone minus (for descriptions)
                    Ok(Token::new(TokenKind::RawText("-".to_string()), Span::new(start_line, start_col, 1)))
                }
            }

            c if c.is_ascii_digit() => self.read_number(c, start_col),

            c if c.is_ascii_alphabetic() || c == '_' => {
                let ident = self.read_identifier(c);
                let span = Span::new(start_line, start_col, ident.len());
                let tok = self.keyword_or_ident(&ident, span);
                // DESCRIPTION block bookkeeping: enter on the keyword, leave on
                // the next section keyword at line start (col 1). A section word
                // appearing indented inside the body does not end it.
                match tok.kind {
                    TokenKind::Description => self.in_description = true,
                    TokenKind::Contract | TokenKind::Hash | TokenKind::Implementation
                    | TokenKind::Composition | TokenKind::Native | TokenKind::End
                    | TokenKind::Entry | TokenKind::Uses | TokenKind::EndExecutable
                        if was_line_start =>
                    {
                        self.in_description = false;
                    }
                    _ => {}
                }
                Ok(tok)
            }

            // For descriptions and comments: accept any character as raw text
            c => Ok(Token::new(TokenKind::RawText(c.to_string()), Span::new(start_line, start_col, 1))),
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Tokenize source code
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token()?;
        let is_eof = matches!(token.kind, TokenKind::Eof);
        tokens.push(token);
        if is_eof {
            break;
        }
    }

    Ok(tokens)
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: lex source and return token kinds (ignoring spans)
    fn lex_kinds(source: &str) -> Vec<TokenKind> {
        lex(source).unwrap().into_iter().map(|t| t.kind).collect()
    }

    /// Helper: lex source and return token kinds, filtering out newlines/indent/eof
    fn lex_meaningful(source: &str) -> Vec<TokenKind> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Indent(_) | TokenKind::Eof))
            .collect()
    }

    // =========================================================================
    // Structure Keywords
    // =========================================================================

    #[test]
    fn test_structure_keywords() {
        let kinds = lex_meaningful("BEHAVIOR DESCRIPTION CONTRACT HASH IMPLEMENTATION COMPOSITION END");
        assert_eq!(kinds, vec![
            TokenKind::Behavior,
            TokenKind::Description,
            TokenKind::Contract,
            TokenKind::Hash,
            TokenKind::Implementation,
            TokenKind::Composition,
            TokenKind::End,
        ]);
    }

    #[test]
    fn test_contract_keywords() {
        let kinds = lex_meaningful("INPUT OUTPUT REQUIRES GUARANTEES");
        assert_eq!(kinds, vec![
            TokenKind::Input,
            TokenKind::Output,
            TokenKind::Requires,
            TokenKind::Guarantees,
        ]);
    }

    #[test]
    fn test_guarantee_keywords() {
        let kinds = lex_meaningful("pure no_alloc writes_output");
        assert_eq!(kinds, vec![
            TokenKind::Pure,
            TokenKind::NoAlloc,
            TokenKind::WritesOutput,
        ]);
    }

    #[test]
    fn test_type_keywords() {
        let kinds = lex_meaningful("int float bytes");
        assert_eq!(kinds, vec![
            TokenKind::Int,
            TokenKind::Float,
            TokenKind::Bytes,
        ]);
    }

    // =========================================================================
    // Memory Primitives
    // =========================================================================

    #[test]
    fn test_memory_primitives() {
        let kinds = lex_meaningful("ALLOC LOAD STORE FREE");
        assert_eq!(kinds, vec![
            TokenKind::Alloc,
            TokenKind::Load,
            TokenKind::Store,
            TokenKind::Free,
        ]);
    }

    #[test]
    fn test_atomic_primitives() {
        let kinds = lex_meaningful("ATOMIC_LOAD ATOMIC_STORE CAS ATOMIC_ADD ATOMIC_SUB");
        assert_eq!(kinds, vec![
            TokenKind::AtomicLoad,
            TokenKind::AtomicStore,
            TokenKind::Cas,
            TokenKind::AtomicAdd,
            TokenKind::AtomicSub,
        ]);
    }

    // =========================================================================
    // Arithmetic Operations
    // =========================================================================

    #[test]
    fn test_integer_arithmetic() {
        let kinds = lex_meaningful("IADD ISUB IMUL IDIV IMOD INEG");
        assert_eq!(kinds, vec![
            TokenKind::Iadd,
            TokenKind::Isub,
            TokenKind::Imul,
            TokenKind::Idiv,
            TokenKind::Imod,
            TokenKind::Ineg,
        ]);
    }

    #[test]
    fn test_integer_comparisons() {
        let kinds = lex_meaningful("IEQ INE ILT IGT ILE IGE");
        assert_eq!(kinds, vec![
            TokenKind::Ieq,
            TokenKind::Ine,
            TokenKind::Ilt,
            TokenKind::Igt,
            TokenKind::Ile,
            TokenKind::Ige,
        ]);
    }

    #[test]
    fn test_float_arithmetic() {
        let kinds = lex_meaningful("FADD FSUB FMUL FDIV FNEG");
        assert_eq!(kinds, vec![
            TokenKind::Fadd,
            TokenKind::Fsub,
            TokenKind::Fmul,
            TokenKind::Fdiv,
            TokenKind::Fneg,
        ]);
    }

    #[test]
    fn test_float_comparisons() {
        let kinds = lex_meaningful("FEQ FNE FLT FGT FLE FGE");
        assert_eq!(kinds, vec![
            TokenKind::Feq,
            TokenKind::Fne,
            TokenKind::Flt,
            TokenKind::Fgt,
            TokenKind::Fle,
            TokenKind::Fge,
        ]);
    }

    #[test]
    fn test_bitwise_operations() {
        let kinds = lex_meaningful("AND OR XOR NOT SHL SHR SAR");
        assert_eq!(kinds, vec![
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Xor,
            TokenKind::Not,
            TokenKind::Shl,
            TokenKind::Shr,
            TokenKind::Sar,
        ]);
    }

    #[test]
    fn test_conversions() {
        let kinds = lex_meaningful("FTOI ITOF");
        assert_eq!(kinds, vec![TokenKind::Ftoi, TokenKind::Itof]);
    }

    // =========================================================================
    // Control Flow
    // =========================================================================

    #[test]
    fn test_control_keywords() {
        let kinds = lex_meaningful("LABEL BRANCH JUMP");
        assert_eq!(kinds, vec![
            TokenKind::Label,
            TokenKind::Branch,
            TokenKind::Jump,
        ]);
    }

    #[test]
    fn test_composition_keywords() {
        let kinds = lex_meaningful("CALL SET");
        assert_eq!(kinds, vec![TokenKind::Call, TokenKind::Set]);
    }

    // =========================================================================
    // Concurrency
    // =========================================================================

    #[test]
    fn test_concurrency_keywords() {
        let kinds = lex_meaningful("SPAWN WAIT WAIT_ALL WAIT_ANY CHANNEL SHARED");
        assert_eq!(kinds, vec![
            TokenKind::Spawn,
            TokenKind::Wait,
            TokenKind::WaitAll,
            TokenKind::WaitAny,
            TokenKind::Channel,
            TokenKind::Shared,
        ]);
    }

    #[test]
    fn test_channel_operations() {
        let kinds = lex_meaningful("CHANNEL_SEND CHANNEL_RECEIVE CHANNEL_CLOSE");
        assert_eq!(kinds, vec![
            TokenKind::ChannelSend,
            TokenKind::ChannelReceive,
            TokenKind::ChannelClose,
        ]);
    }

    // =========================================================================
    // Literals
    // =========================================================================

    #[test]
    fn test_integer_decimal() {
        let kinds = lex_meaningful("0 42 123456");
        assert_eq!(kinds, vec![
            TokenKind::Integer(0),
            TokenKind::Integer(42),
            TokenKind::Integer(123456),
        ]);
    }

    #[test]
    fn test_integer_negative() {
        let kinds = lex_meaningful("-1 -42 -999");
        assert_eq!(kinds, vec![
            TokenKind::Integer(-1),
            TokenKind::Integer(-42),
            TokenKind::Integer(-999),
        ]);
    }

    #[test]
    fn test_integer_hex() {
        let kinds = lex_meaningful("0x00 0xFF 0xDEAD 0xBEEF");
        assert_eq!(kinds, vec![
            TokenKind::Integer(0x00),
            TokenKind::Integer(0xFF),
            TokenKind::Integer(0xDEAD),
            TokenKind::Integer(0xBEEF),
        ]);
    }

    #[test]
    fn test_float_literal() {
        let kinds = lex_meaningful("3.14 0.5 1.0");
        assert_eq!(kinds, vec![
            TokenKind::FloatLit(3.14),
            TokenKind::FloatLit(0.5),
            TokenKind::FloatLit(1.0),
        ]);
    }

    #[test]
    fn test_string_literal() {
        let kinds = lex_meaningful(r#""hello" "world""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLit("hello".to_string()),
            TokenKind::StringLit("world".to_string()),
        ]);
    }

    #[test]
    fn test_string_escape_sequences() {
        let kinds = lex_meaningful(r#""hello\nworld" "tab\there" "quote\"here""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLit("hello\nworld".to_string()),
            TokenKind::StringLit("tab\there".to_string()),
            TokenKind::StringLit("quote\"here".to_string()),
        ]);
    }

    // =========================================================================
    // Identifiers
    // =========================================================================

    #[test]
    fn test_identifier_simple() {
        let kinds = lex_meaningful("foo bar baz");
        assert_eq!(kinds, vec![
            TokenKind::Identifier("foo".to_string()),
            TokenKind::Identifier("bar".to_string()),
            TokenKind::Identifier("baz".to_string()),
        ]);
    }

    #[test]
    fn test_identifier_with_hyphen() {
        let kinds = lex_meaningful("my-var store-init kv-demo-keys");
        assert_eq!(kinds, vec![
            TokenKind::Identifier("my-var".to_string()),
            TokenKind::Identifier("store-init".to_string()),
            TokenKind::Identifier("kv-demo-keys".to_string()),
        ]);
    }

    #[test]
    fn test_identifier_with_underscore() {
        let kinds = lex_meaningful("_foo my_var __private");
        assert_eq!(kinds, vec![
            TokenKind::Identifier("_foo".to_string()),
            TokenKind::Identifier("my_var".to_string()),
            TokenKind::Identifier("__private".to_string()),
        ]);
    }

    #[test]
    fn test_identifier_with_numbers() {
        let kinds = lex_meaningful("var1 count2 x86");
        assert_eq!(kinds, vec![
            TokenKind::Identifier("var1".to_string()),
            TokenKind::Identifier("count2".to_string()),
            TokenKind::Identifier("x86".to_string()),
        ]);
    }

    // =========================================================================
    // Operators and Punctuation
    // =========================================================================

    #[test]
    fn test_operators() {
        let kinds = lex_meaningful("= -> . @ , : [ ]");
        assert_eq!(kinds, vec![
            TokenKind::Equals,
            TokenKind::Arrow,
            TokenKind::Dot,
            TokenKind::At,
            TokenKind::Comma,
            TokenKind::Colon,
            TokenKind::LeftBracket,
            TokenKind::RightBracket,
        ]);
    }

    // =========================================================================
    // Comments
    // =========================================================================

    #[test]
    fn test_comment_skipped() {
        let kinds = lex_meaningful("LABEL # this is a comment\nfoo");
        assert_eq!(kinds, vec![
            TokenKind::Label,
            TokenKind::Identifier("foo".to_string()),
        ]);
    }

    #[test]
    fn test_full_line_comment() {
        let kinds = lex_meaningful("# full line comment\nLABEL");
        assert_eq!(kinds, vec![TokenKind::Label]);
    }

    // =========================================================================
    // Newlines and Indentation
    // =========================================================================

    #[test]
    fn test_newline_token() {
        let kinds = lex_kinds("LABEL\nJUMP");
        assert!(kinds.contains(&TokenKind::Newline));
    }

    #[test]
    fn test_indent_token() {
        let kinds = lex_kinds("LABEL\n  indented");
        assert!(kinds.iter().any(|k| matches!(k, TokenKind::Indent(_))));
    }

    // =========================================================================
    // Error Cases
    // =========================================================================

    #[test]
    fn test_invalid_hex_error() {
        let result = lex("0x");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("hex"));
    }

    #[test]
    fn test_unterminated_string_error() {
        let result = lex("\"unterminated");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unterminated"));
    }

    #[test]
    fn test_description_tolerates_stray_quotes() {
        // A DESCRIPTION is free documentation text. A lone or line-spanning `"`
        // in it must NOT raise an unterminated-string error (it would block
        // writing docs like `11" rows`). The block still ends at CONTRACT.
        let src = "BEHAVIOR d\nDESCRIPTION\n  width is 11\" per row; see \"the open quote\nthat continues onto this line.\nCONTRACT\n  OUTPUT s int 8\n  GUARANTEES writes_output\nIMPLEMENTATION\n  STORE s 0 8 0\nEND\n";
        let toks = lex(src).expect("DESCRIPTION with stray quotes must lex");
        // The section keyword after the description is still recognized.
        assert!(toks.iter().any(|t| matches!(t.kind, TokenKind::Contract)),
            "CONTRACT after the description must still tokenize");
    }

    #[test]
    fn test_string_after_description_still_validated() {
        // The permissive mode is scoped to the DESCRIPTION block only: a genuine
        // unterminated string in the body still errors.
        let src = "BEHAVIOR d\nDESCRIPTION\n  ok text\nIMPLEMENTATION\n  x = \"oops\nEND\n";
        assert!(lex(src).is_err(), "an unterminated string after DESCRIPTION must still error");
    }

    #[test]
    fn test_invalid_escape_error() {
        let result = lex(r#""\z""#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("escape"));
    }

    #[test]
    fn test_newline_in_string_error() {
        let result = lex("\"hello\nworld\"");
        assert!(result.is_err());
    }

    // =========================================================================
    // Span Tracking
    // =========================================================================

    #[test]
    fn test_span_line_tracking() {
        let tokens = lex("LABEL\nJUMP").unwrap();
        let label = &tokens[0];
        let jump = tokens.iter().find(|t| matches!(t.kind, TokenKind::Jump)).unwrap();
        assert_eq!(label.span.line, 1);
        assert_eq!(jump.span.line, 2);
    }

    #[test]
    fn test_span_column_tracking() {
        let tokens = lex("LABEL foo").unwrap();
        let label = &tokens[0];
        let foo = tokens.iter().find(|t| matches!(t.kind, TokenKind::Identifier(_))).unwrap();
        assert_eq!(label.span.col, 1);
        assert_eq!(foo.span.col, 7);
    }

    // =========================================================================
    // Complex Examples
    // =========================================================================

    #[test]
    fn test_behavior_header() {
        let kinds = lex_meaningful("BEHAVIOR my-behavior");
        assert_eq!(kinds, vec![
            TokenKind::Behavior,
            TokenKind::Identifier("my-behavior".to_string()),
        ]);
    }

    #[test]
    fn test_parameter_declaration() {
        let kinds = lex_meaningful("INPUT data bytes 256");
        assert_eq!(kinds, vec![
            TokenKind::Input,
            TokenKind::Identifier("data".to_string()),
            TokenKind::Bytes,
            TokenKind::Integer(256),
        ]);
    }

    #[test]
    fn test_requires_with_hash() {
        let kinds = lex_meaningful("REQUIRES foo@abc123");
        assert_eq!(kinds, vec![
            TokenKind::Requires,
            TokenKind::Identifier("foo".to_string()),
            TokenKind::At,
            TokenKind::Identifier("abc123".to_string()),
        ]);
    }

    #[test]
    fn test_store_statement() {
        let kinds = lex_meaningful("STORE handle value 8");
        assert_eq!(kinds, vec![
            TokenKind::Store,
            TokenKind::Identifier("handle".to_string()),
            TokenKind::Identifier("value".to_string()),
            TokenKind::Integer(8),
        ]);
    }

    #[test]
    fn test_call_with_arrow() {
        let kinds = lex_meaningful("CALL my-func arg1 arg2 -> result");
        assert_eq!(kinds, vec![
            TokenKind::Call,
            TokenKind::Identifier("my-func".to_string()),
            TokenKind::Identifier("arg1".to_string()),
            TokenKind::Identifier("arg2".to_string()),
            TokenKind::Arrow,
            TokenKind::Identifier("result".to_string()),
        ]);
    }

    #[test]
    fn test_leading_bom_is_ignored() {
        // A UTF-8 BOM ('\u{FEFF}'), commonly prepended by editors/tools, must not
        // break lexing — it should produce exactly the tokens of the BOM-free input.
        let with_bom = lex_meaningful("\u{FEFF}BEHAVIOR foo");
        let without = lex_meaningful("BEHAVIOR foo");
        assert_eq!(with_bom, without);
    }
}
