//! Parser Module
//!
//! Parses Sigil source according to the Language Reference grammar.
//! Uses Token struct with TokenKind from lexer, returns CompilerError on failure.

use crate::ast::*;
use crate::error::CompilerError;
use crate::lexer::{Token, TokenKind, Span};

/// Parser state
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token], source: &'a str) -> Self {
        Parser { tokens, pos: 0, source }
    }

    // =========================================================================
    // Token Access
    // =========================================================================

    /// Peek at current token
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            static EOF: Token = Token {
                kind: TokenKind::Eof,
                span: Span { line: 0, col: 0, len: 0 }
            };
            &EOF
        })
    }

    /// Peek at current token kind
    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    /// Get current span
    fn current_span(&self) -> Span {
        self.peek().span
    }

    /// Advance and return current token
    fn advance(&mut self) -> &Token {
        let token = self.peek();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        self.tokens.get(self.pos - 1).unwrap_or(self.peek())
    }

    /// Skip newlines and indentation
    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent(_)) {
            self.pos += 1;
        }
    }

    /// Create a parse error at current position
    fn error(&self, message: impl Into<String>) -> CompilerError {
        CompilerError::parse(self.current_span(), message)
    }

    // =========================================================================
    // Expect Helpers
    // =========================================================================

    /// Expect a specific token kind
    fn expect(&mut self, expected: TokenKind) -> Result<Span, CompilerError> {
        let span = self.current_span();
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(span)
        } else {
            Err(self.error(format!("Expected {:?}, got {:?}", expected, self.peek_kind())))
        }
    }

    /// Expect an identifier and return its name
    fn expect_identifier(&mut self) -> Result<String, CompilerError> {
        match self.peek_kind().clone() {
            TokenKind::Identifier(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.error(format!("Expected identifier, got {:?}", self.peek_kind()))),
        }
    }

    /// Expect a hex string (for hashes)
    /// Handles cases where hash starts with digits (tokenized as Integer)
    /// Returns lowercase hex string, padded to 8 chars only if it looks like a truncated 8-char hash
    fn expect_hex_string(&mut self) -> Result<String, CompilerError> {
        let mut result = String::new();

        match self.peek_kind().clone() {
            TokenKind::Identifier(s) => {
                self.advance();
                result.push_str(&s);
            }
            TokenKind::Integer(n) => {
                self.advance();
                // Keep as decimal string (lexer parsed "04553303" as decimal 4553303)
                result.push_str(&n.to_string());
                // Check if there's a trailing identifier part that looks like hex (e.g., "3" + "dd7d332")
                // Only append if the identifier is all hex digits (not a behavior name like "slot-write")
                if let TokenKind::Identifier(s) = self.peek_kind().clone() {
                    if s.chars().all(|c| c.is_ascii_hexdigit()) {
                        self.advance();
                        result.push_str(&s);
                    }
                }
            }
            _ => {
                return Err(self.error(format!("Expected hex string, got {:?}", self.peek_kind())));
            }
        }

        // Contract hashes are always exactly 8 hex chars (hex::encode of 4 bytes,
        // zero-padded). When the leading nibble(s) are zero, the lexer tokenizes
        // them as an Integer and drops the leading zeros — e.g. "004ec810" becomes
        // Integer(4) + "ec810" -> "4ec810" — so the declared hash no longer matches
        // the computed (padded) one. Left-pad any short all-hex result back to 8.
        // This is the only place hashes are parsed (HASH lines and REQUIRES@hash
        // pins), and both are always 8 chars, so the pad is always correct.
        if !result.is_empty() && result.len() < 8 && result.chars().all(|c| c.is_ascii_hexdigit()) {
            result = format!("{:0>8}", result);
        }

        Ok(result.to_lowercase())
    }

    /// Parse a type interpretation
    fn parse_type(&mut self) -> Result<Type, CompilerError> {
        match self.peek_kind() {
            TokenKind::Bytes => {
                self.advance();
                Ok(Type::Bytes)
            }
            TokenKind::Int => {
                self.advance();
                Ok(Type::Int)
            }
            TokenKind::Float => {
                self.advance();
                Ok(Type::Float)
            }
            TokenKind::String => {
                self.advance();
                Ok(Type::String)
            }
            _ => Err(self.error(format!("Expected type (bytes/int/float/string), got {:?}", self.peek_kind()))),
        }
    }

    /// Parse an integer literal
    fn parse_integer(&mut self) -> Result<i64, CompilerError> {
        match self.peek_kind().clone() {
            TokenKind::Integer(n) => {
                self.advance();
                Ok(n)
            }
            _ => Err(self.error(format!("Expected integer, got {:?}", self.peek_kind()))),
        }
    }

    // =========================================================================
    // Top-level Parsing
    // =========================================================================

    /// Parse a BEHAVIOR definition
    pub fn parse_behavior(&mut self) -> Result<Behavior, CompilerError> {
        self.skip_newlines();

        self.expect(TokenKind::Behavior)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut behavior = Behavior::new(name);

        loop {
            self.skip_newlines();

            match self.peek_kind() {
                TokenKind::Description => {
                    self.advance();
                    behavior.description = Some(self.parse_description()?);
                }
                TokenKind::Contract => {
                    self.advance();
                    self.skip_newlines();
                    behavior.contract = self.parse_contract()?;
                }
                TokenKind::Hash => {
                    self.advance();
                    behavior.hash = self.expect_hex_string()?;
                }
                TokenKind::Implementation => {
                    self.advance();
                    let platform = if matches!(self.peek_kind(), TokenKind::LeftBracket) {
                        self.advance();
                        let platform_name = self.expect_identifier()?;
                        self.expect(TokenKind::RightBracket)?;
                        Some(platform_name)
                    } else {
                        None
                    };
                    self.skip_newlines();
                    let impl_body = self.parse_implementation()?;
                    behavior.implementations.push(PlatformImpl {
                        platform: platform.clone(),
                        nodes: impl_body.nodes,
                    });
                    // Per Section 12: Each platform implementation ends with END
                    // Consume the END token to allow more implementations to follow
                    if platform.is_some() && matches!(self.peek_kind(), TokenKind::End) {
                        self.advance();
                    }
                }
                TokenKind::Composition => {
                    self.advance();
                    self.skip_newlines();
                    behavior.composition = Some(self.parse_composition()?);
                }
                TokenKind::Native => {
                    self.advance();
                    behavior.is_native = true;
                    break;  // NATIVE has no body, behavior is complete
                }
                TokenKind::End => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => {
                    return Err(self.error("Unexpected end of file, expected END".to_string()));
                }
                _ => {
                    return Err(self.error(format!("Unexpected token in behavior: {:?}", self.peek_kind())));
                }
            }
        }

        Ok(behavior)
    }

    /// Parse DESCRIPTION section - extracts raw text from source
    fn parse_description(&mut self) -> Result<String, CompilerError> {
        // Skip initial newlines to find first content token
        self.skip_newlines();

        // Record start position in source
        let start_span = self.current_span();
        let mut end_span = start_span;

        // Scan tokens until we hit a section keyword at line start
        let mut at_line_start = true;

        loop {
            if at_line_start {
                if matches!(
                    self.peek_kind(),
                    TokenKind::Contract
                        | TokenKind::Hash
                        | TokenKind::Implementation
                        | TokenKind::Composition
                        | TokenKind::Native
                        | TokenKind::End
                        | TokenKind::Eof
                        | TokenKind::Entry
                        | TokenKind::Uses
                        | TokenKind::EndExecutable
                ) {
                    break;
                }
            }

            match self.peek_kind() {
                TokenKind::Eof => break,
                TokenKind::Newline => {
                    end_span = self.current_span();
                    self.advance();
                    at_line_start = true;
                }
                TokenKind::Indent(_) => {
                    self.advance();
                    // Don't change at_line_start - indent follows newline
                }
                _ => {
                    end_span = self.current_span();
                    self.advance();
                    at_line_start = false;
                }
            }
        }

        // Extract raw text from source using line numbers
        let lines: Vec<&str> = self.source.lines().collect();
        let mut desc = String::new();

        for line_num in start_span.line..=end_span.line {
            if line_num > 0 && line_num <= lines.len() {
                let line = lines[line_num - 1]; // line numbers are 1-based
                // Skip leading indentation (2 spaces typical for description)
                let trimmed = if line.starts_with("  ") { &line[2..] } else { line };
                if !desc.is_empty() {
                    desc.push('\n');
                }
                desc.push_str(trimmed);
            }
        }

        Ok(desc.trim().to_string())
    }

    /// Parse CONTRACT section
    fn parse_contract(&mut self) -> Result<Contract, CompilerError> {
        let mut contract = Contract {
            inputs: Vec::new(),
            outputs: Vec::new(),
            requires: Requirements::default(),
            guarantees: Vec::new(),
        };

        loop {
            self.skip_newlines();

            match self.peek_kind() {
                TokenKind::Input => {
                    self.advance();
                    let param = self.parse_parameter()?;
                    contract.inputs.push(param);
                }
                TokenKind::Output => {
                    self.advance();
                    let param = self.parse_parameter()?;
                    contract.outputs.push(param);
                }
                TokenKind::Requires => {
                    self.advance();
                    self.skip_newlines();
                    contract.requires = self.parse_requires()?;
                }
                TokenKind::Guarantees => {
                    self.advance();
                    contract.guarantees = self.parse_guarantees()?;
                }
                TokenKind::Memory => {
                    self.advance();
                    while let TokenKind::Identifier(name) = self.peek_kind().clone() {
                        contract.requires.memory.push(name);
                        self.advance();
                    }
                }
                _ => break,
            }
        }

        Ok(contract)
    }

    /// Parse parameter: name interpretation size
    fn parse_parameter(&mut self) -> Result<Parameter, CompilerError> {
        let name = self.expect_identifier()?;
        let typ = self.parse_type()?;
        // A `string` is self-describing (carries its own length), so its size is
        // optional; other interpretations require an explicit size.
        let size = if matches!(typ, Type::String) && !matches!(self.peek_kind(), TokenKind::Integer(_)) {
            0
        } else {
            self.parse_integer()? as usize
        };

        Ok(Parameter { name, typ, size })
    }

    /// Parse REQUIRES section
    fn parse_requires(&mut self) -> Result<Requirements, CompilerError> {
        let mut reqs = Requirements::default();

        self.skip_newlines();

        loop {
            self.skip_newlines();

            match self.peek_kind() {
                TokenKind::Identifier(_) => {
                    reqs.behaviors = self.parse_behavior_refs()?;
                }
                _ => break,
            }
        }

        Ok(reqs)
    }

    /// Parse behavior references: behavior@hash behavior@hash ...
    fn parse_behavior_refs(&mut self) -> Result<Vec<BehaviorRef>, CompilerError> {
        let mut refs = Vec::new();

        while let TokenKind::Identifier(_) = self.peek_kind() {
            let name = self.expect_identifier()?;

            let hash = if matches!(self.peek_kind(), TokenKind::At) {
                self.advance();
                self.expect_hex_string()?
            } else {
                String::new()
            };

            refs.push(BehaviorRef { name, hash });
        }

        Ok(refs)
    }

    /// Parse GUARANTEES
    fn parse_guarantees(&mut self) -> Result<Vec<Guarantee>, CompilerError> {
        let mut guarantees = Vec::new();

        loop {
            match self.peek_kind() {
                TokenKind::Pure => {
                    self.advance();
                    guarantees.push(Guarantee::Pure);
                }
                TokenKind::NoAlloc => {
                    self.advance();
                    guarantees.push(Guarantee::NoAlloc);
                }
                TokenKind::WritesOutput => {
                    self.advance();
                    guarantees.push(Guarantee::WritesOutput);
                }
                _ => break,
            }
        }

        Ok(guarantees)
    }

    // =========================================================================
    // Implementation Parsing
    // =========================================================================

    /// Parse IMPLEMENTATION section
    fn parse_implementation(&mut self) -> Result<Implementation, CompilerError> {
        let mut nodes = Vec::new();

        loop {
            self.skip_newlines();

            match self.peek_kind() {
                TokenKind::End | TokenKind::Eof => break,
                _ => {
                    let node = self.parse_node()?;
                    nodes.push(node);
                }
            }
        }

        Ok(Implementation { nodes })
    }

    /// Parse COMPOSITION section
    fn parse_composition(&mut self) -> Result<Composition, CompilerError> {
        let impl_ = self.parse_implementation()?;
        Ok(Composition { nodes: impl_.nodes })
    }

    /// Parse a single node (statement)
    fn parse_node(&mut self) -> Result<Node, CompilerError> {
        // Skip indent first, then capture span of actual statement
        if matches!(self.peek_kind(), TokenKind::Indent(_)) {
            self.advance();
        }

        let span = self.current_span();

        match self.peek_kind().clone() {
            TokenKind::Label => {
                self.advance();
                let name = self.expect_identifier()?;
                Ok(Node { span, kind: NodeKind::Label(name) })
            }
            TokenKind::Scope => {
                self.advance();
                self.skip_newlines();
                let mut nodes = Vec::new();
                loop {
                    self.skip_newlines();
                    match self.peek_kind() {
                        TokenKind::EndScope => {
                            self.advance();
                            break;
                        }
                        TokenKind::End | TokenKind::Eof => break,
                        _ => {
                            nodes.push(self.parse_node()?);
                        }
                    }
                }
                Ok(Node { span, kind: NodeKind::Scope { nodes } })
            }
            TokenKind::Branch => {
                self.advance();
                let condition = self.parse_expr()?;
                let true_label = self.expect_identifier()?;
                let false_label = self.expect_identifier()?;
                Ok(Node {
                    span,
                    kind: NodeKind::Branch {
                        condition: Box::new(condition),
                        true_label,
                        false_label,
                    },
                })
            }
            TokenKind::Jump => {
                self.advance();
                let target = self.expect_identifier()?;
                Ok(Node { span, kind: NodeKind::Jump(target) })
            }
            TokenKind::Set => {
                self.advance();
                let target = self.expect_identifier()?;
                let value = self.parse_expr()?;
                Ok(Node {
                    span,
                    kind: NodeKind::Set { target, value: Box::new(value) },
                })
            }
            TokenKind::Discard => {
                // DISCARD ( field_access | identifier )  (Section 7.8)
                self.advance();
                let name = self.expect_identifier()?;
                let value = if matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance();
                    let field = self.expect_identifier()?;
                    Expr::Field { base: Box::new(Expr::Var(name)), field }
                } else {
                    Expr::Var(name)
                };
                Ok(Node { span, kind: NodeKind::Discard(Box::new(value)) })
            }
            TokenKind::Store => {
                self.advance();
                let target = self.expect_identifier()?;
                let value = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                let offset = if matches!(self.peek_kind(), TokenKind::Integer(_) | TokenKind::Identifier(_)) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Ok(Node {
                    span,
                    kind: NodeKind::Store { target, value: Box::new(value), size, offset },
                })
            }
            TokenKind::Free => {
                self.advance();
                let name = self.expect_identifier()?;
                Ok(Node { span, kind: NodeKind::Free(name) })
            }
            TokenKind::ChannelSend => {
                self.advance();
                let channel = self.expect_identifier()?;
                let value = self.parse_expr()?;
                Ok(Node {
                    span,
                    kind: NodeKind::ChannelSend { channel, value: Box::new(value) },
                })
            }
            TokenKind::ChannelClose => {
                self.advance();
                let channel = self.expect_identifier()?;
                Ok(Node { span, kind: NodeKind::ChannelClose(channel) })
            }
            // Per spec: wait_all = [ identifier { "," identifier } "=" ] "WAIT_ALL" identifier { identifier } ;
            TokenKind::WaitAll => {
                self.advance();
                let mut handles = vec![self.expect_identifier()?];
                while let TokenKind::Identifier(_) = self.peek_kind() {
                    handles.push(self.expect_identifier()?);
                }
                Ok(Node {
                    span,
                    kind: NodeKind::WaitAll { targets: Vec::new(), handles },
                })
            }
            TokenKind::Call => {
                self.advance();
                let behavior = self.expect_identifier()?;
                let mut args = Vec::new();
                while !matches!(
                    self.peek_kind(),
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Arrow
                ) {
                    args.push(self.parse_expr()?);
                }
                // Handle optional -> output_list per spec Section 7.1
                let outputs = if matches!(self.peek_kind(), TokenKind::Arrow) {
                    self.advance();
                    let mut outs = vec![self.expect_identifier()?];
                    while matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                        outs.push(self.expect_identifier()?);
                    }
                    outs
                } else {
                    Vec::new()
                };
                Ok(Node {
                    span,
                    kind: NodeKind::Call { target: None, behavior, args, outputs },
                })
            }
            TokenKind::Identifier(name) => {
                self.advance();

                // Check what follows the identifier
                match self.peek_kind() {
                    TokenKind::Equals => {
                        // identifier = expr (assignment) or identifier = WAIT_ALL/WAIT_ANY
                        self.advance();

                        // Check for WAIT_ALL with single target
                        if matches!(self.peek_kind(), TokenKind::WaitAll) {
                            self.advance();
                            let mut handles = vec![self.expect_identifier()?];
                            while let TokenKind::Identifier(_) = self.peek_kind() {
                                handles.push(self.expect_identifier()?);
                            }
                            return Ok(Node {
                                span,
                                kind: NodeKind::WaitAll { targets: vec![name], handles },
                            });
                        }

                        let expr = self.parse_expr()?;
                        Ok(Node {
                            span,
                            kind: NodeKind::Assignment { target: name, expr: Box::new(expr) },
                        })
                    }
                    TokenKind::Comma => {
                        // Could be: id1, id2 = WAIT_ANY ... or id1, id2, id3 = WAIT_ALL ...
                        let mut targets = vec![name];
                        while matches!(self.peek_kind(), TokenKind::Comma) {
                            self.advance();
                            targets.push(self.expect_identifier()?);
                        }
                        self.expect(TokenKind::Equals)?;

                        match self.peek_kind() {
                            TokenKind::WaitAny => {
                                // Per spec: wait_any = identifier "," identifier "=" "WAIT_ANY" { identifier } ;
                                self.advance();
                                if targets.len() != 2 {
                                    return Err(self.error("WAIT_ANY requires exactly 2 targets (result, which)"));
                                }
                                let mut handles = Vec::new();
                                while let TokenKind::Identifier(_) = self.peek_kind() {
                                    handles.push(self.expect_identifier()?);
                                }
                                Ok(Node {
                                    span,
                                    kind: NodeKind::WaitAny {
                                        result: targets[0].clone(),
                                        which: targets[1].clone(),
                                        handles,
                                    },
                                })
                            }
                            TokenKind::WaitAll => {
                                // Per spec: wait_all = [ identifier { "," identifier } "=" ] "WAIT_ALL" identifier { identifier } ;
                                self.advance();
                                let mut handles = vec![self.expect_identifier()?];
                                while let TokenKind::Identifier(_) = self.peek_kind() {
                                    handles.push(self.expect_identifier()?);
                                }
                                Ok(Node {
                                    span,
                                    kind: NodeKind::WaitAll { targets, handles },
                                })
                            }
                            _ => {
                                Err(self.error(format!("Expected WAIT_ANY or WAIT_ALL after comma-separated targets")))
                            }
                        }
                    }
                    _ => Err(self.error(format!("Unexpected token after identifier '{}': {:?}", name, self.peek_kind()))),
                }
            }
            _ => Err(self.error(format!("Unexpected token at start of statement: {:?}", self.peek_kind()))),
        }
    }

    // =========================================================================
    // Expression Parsing
    // =========================================================================

    /// Parse an expression
    fn parse_expr(&mut self) -> Result<Expr, CompilerError> {
        match self.peek_kind().clone() {
            TokenKind::Integer(n) => {
                self.advance();
                Ok(Expr::IntLit(n))
            }
            TokenKind::FloatLit(f) => {
                self.advance();
                Ok(Expr::FloatLit(f))
            }
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(Expr::StringLit(s))
            }
            TokenKind::Load => {
                self.advance();
                let source = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                let offset = if matches!(self.peek_kind(), TokenKind::Integer(_) | TokenKind::Identifier(_)) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Ok(Expr::Load { source: Box::new(source), size, offset })
            }
            TokenKind::Alloc => {
                self.advance();
                let size = self.parse_expr()?;
                let typ = self.parse_type()?;
                let shared = if matches!(self.peek_kind(), TokenKind::Shared) {
                    self.advance();
                    true
                } else {
                    false
                };
                Ok(Expr::Alloc { size: Box::new(size), typ, shared })
            }
            // Integer arithmetic
            TokenKind::Iadd => self.parse_binary_op(|a, b, s| Expr::Iadd(a, b, s)),
            TokenKind::Isub => self.parse_binary_op(|a, b, s| Expr::Isub(a, b, s)),
            TokenKind::Imul => self.parse_binary_op(|a, b, s| Expr::Imul(a, b, s)),
            TokenKind::Idiv => self.parse_binary_op(|a, b, s| Expr::Idiv(a, b, s)),
            TokenKind::Imod => self.parse_binary_op(|a, b, s| Expr::Imod(a, b, s)),
            TokenKind::Ineg => self.parse_unary_op(|a, s| Expr::Ineg(a, s)),
            // Integer comparison
            TokenKind::Ieq => self.parse_binary_op(|a, b, s| Expr::Ieq(a, b, s)),
            TokenKind::Ine => self.parse_binary_op(|a, b, s| Expr::Ine(a, b, s)),
            TokenKind::Ilt => self.parse_binary_op(|a, b, s| Expr::Ilt(a, b, s)),
            TokenKind::Igt => self.parse_binary_op(|a, b, s| Expr::Igt(a, b, s)),
            TokenKind::Ile => self.parse_binary_op(|a, b, s| Expr::Ile(a, b, s)),
            TokenKind::Ige => self.parse_binary_op(|a, b, s| Expr::Ige(a, b, s)),
            // Bitwise
            TokenKind::And => self.parse_binary_op(|a, b, s| Expr::And(a, b, s)),
            TokenKind::Or => self.parse_binary_op(|a, b, s| Expr::Or(a, b, s)),
            TokenKind::Xor => self.parse_binary_op(|a, b, s| Expr::Xor(a, b, s)),
            TokenKind::Not => self.parse_unary_op(|a, s| Expr::Not(a, s)),
            TokenKind::Shl => self.parse_binary_op(|a, b, s| Expr::Shl(a, b, s)),
            TokenKind::Shr => self.parse_binary_op(|a, b, s| Expr::Shr(a, b, s)),
            TokenKind::Sar => self.parse_binary_op(|a, b, s| Expr::Sar(a, b, s)),
            // Float arithmetic
            TokenKind::Fadd => self.parse_binary_op(|a, b, s| Expr::Fadd(a, b, s)),
            TokenKind::Fsub => self.parse_binary_op(|a, b, s| Expr::Fsub(a, b, s)),
            TokenKind::Fmul => self.parse_binary_op(|a, b, s| Expr::Fmul(a, b, s)),
            TokenKind::Fdiv => self.parse_binary_op(|a, b, s| Expr::Fdiv(a, b, s)),
            TokenKind::Fneg => self.parse_unary_op(|a, s| Expr::Fneg(a, s)),
            // Float comparison
            TokenKind::Feq => self.parse_binary_op(|a, b, s| Expr::Feq(a, b, s)),
            TokenKind::Fne => self.parse_binary_op(|a, b, s| Expr::Fne(a, b, s)),
            TokenKind::Flt => self.parse_binary_op(|a, b, s| Expr::Flt(a, b, s)),
            TokenKind::Fgt => self.parse_binary_op(|a, b, s| Expr::Fgt(a, b, s)),
            TokenKind::Fle => self.parse_binary_op(|a, b, s| Expr::Fle(a, b, s)),
            TokenKind::Fge => self.parse_binary_op(|a, b, s| Expr::Fge(a, b, s)),
            // Atomic primitives
            TokenKind::AtomicLoad => {
                self.advance();
                let source = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                Ok(Expr::AtomicLoad(Box::new(source), size))
            }
            TokenKind::AtomicStore => {
                self.advance();
                let target = self.parse_expr()?;
                let value = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                Ok(Expr::AtomicStore(Box::new(target), Box::new(value), size))
            }
            TokenKind::Cas => {
                self.advance();
                let addr = self.parse_expr()?;
                let expected = self.parse_expr()?;
                let new = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                Ok(Expr::Cas {
                    addr: Box::new(addr),
                    expected: Box::new(expected),
                    new: Box::new(new),
                    size,
                })
            }
            TokenKind::AtomicAdd => {
                self.advance();
                let target = self.parse_expr()?;
                let value = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                Ok(Expr::AtomicAdd(Box::new(target), Box::new(value), size))
            }
            TokenKind::AtomicSub => {
                self.advance();
                let target = self.parse_expr()?;
                let value = self.parse_expr()?;
                let size = self.parse_integer()? as usize;
                Ok(Expr::AtomicSub(Box::new(target), Box::new(value), size))
            }
            // Conversion primitives
            TokenKind::Ftoi => {
                self.advance();
                let value = self.parse_expr()?;
                let float_size = self.parse_integer()? as usize;
                let int_size = self.parse_integer()? as usize;
                Ok(Expr::Ftoi { value: Box::new(value), float_size, int_size })
            }
            TokenKind::Itof => {
                self.advance();
                let value = self.parse_expr()?;
                let int_size = self.parse_integer()? as usize;
                let float_size = self.parse_integer()? as usize;
                Ok(Expr::Itof { value: Box::new(value), int_size, float_size })
            }
            // Concurrency expressions
            TokenKind::Call => {
                self.advance();
                let behavior = self.expect_identifier()?;
                let mut args = Vec::new();
                while !matches!(
                    self.peek_kind(),
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Arrow
                ) {
                    args.push(self.parse_expr()?);
                }
                Ok(Expr::Call { behavior, args })
            }
            TokenKind::ChannelReceive => {
                self.advance();
                let channel = self.expect_identifier()?;
                Ok(Expr::ChannelReceive(channel))
            }
            TokenKind::Channel => {
                self.advance();
                let typ = self.parse_type()?;
                let capacity = self.parse_expr()?;
                Ok(Expr::Channel { typ, capacity: Box::new(capacity) })
            }
            // Per spec: spawn = identifier "=" "SPAWN" identifier { argument } ;
            TokenKind::Spawn => {
                self.advance();
                let pattern = self.expect_identifier()?;
                let mut args = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
                    args.push(self.parse_expr()?);
                }
                Ok(Expr::Spawn { pattern, args })
            }
            // Per spec: wait = identifier "=" "WAIT" identifier ;
            TokenKind::Wait => {
                self.advance();
                let handle = self.expect_identifier()?;
                Ok(Expr::Wait(handle))
            }
            TokenKind::Identifier(name) => {
                self.advance();
                // Check for field access
                if matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance();
                    let field = self.expect_identifier()?;
                    Ok(Expr::Field {
                        base: Box::new(Expr::Var(name)),
                        field,
                    })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            _ => Err(self.error(format!("Unexpected token in expression: {:?}", self.peek_kind()))),
        }
    }

    /// Parse binary operation: OP a b size
    fn parse_binary_op<F>(&mut self, constructor: F) -> Result<Expr, CompilerError>
    where
        F: FnOnce(Box<Expr>, Box<Expr>, usize) -> Expr,
    {
        self.advance();
        let a = self.parse_expr()?;
        let b = self.parse_expr()?;
        let size = self.parse_integer()? as usize;
        Ok(constructor(Box::new(a), Box::new(b), size))
    }

    /// Parse unary operation: OP a size
    fn parse_unary_op<F>(&mut self, constructor: F) -> Result<Expr, CompilerError>
    where
        F: FnOnce(Box<Expr>, usize) -> Expr,
    {
        self.advance();
        let a = self.parse_expr()?;
        let size = self.parse_integer()? as usize;
        Ok(constructor(Box::new(a), size))
    }

    // =========================================================================
    // Pattern Parsing (Section 8)
    // =========================================================================

    /// Parse a PATTERN definition
    pub fn parse_pattern(&mut self) -> Result<Pattern, CompilerError> {
        self.expect(TokenKind::Pattern)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut memory = Vec::new();
        let mut on_create = Vec::new();
        let mut behaviors = Vec::new();
        let mut on_destroy = Vec::new();

        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Memory => {
                    self.advance();
                    self.skip_newlines();
                    while let TokenKind::Identifier(_) = self.peek_kind() {
                        let mem_name = self.expect_identifier()?;
                        let typ = self.parse_type()?;
                        let size = self.parse_integer()? as usize;
                        memory.push(MemoryDecl { name: mem_name, typ, size });
                        self.skip_newlines();
                    }
                }
                TokenKind::OnCreate => {
                    self.advance();
                    self.skip_newlines();
                    while !matches!(self.peek_kind(), TokenKind::OnDestroy | TokenKind::Behavior | TokenKind::EndPattern | TokenKind::Eof) {
                        on_create.push(self.parse_node()?);
                        self.skip_newlines();
                    }
                }
                TokenKind::Behavior => {
                    let behavior = self.parse_behavior()?;
                    behaviors.push(behavior);
                }
                TokenKind::OnDestroy => {
                    self.advance();
                    self.skip_newlines();
                    while !matches!(self.peek_kind(), TokenKind::EndPattern | TokenKind::Eof) {
                        on_destroy.push(self.parse_node()?);
                        self.skip_newlines();
                    }
                }
                TokenKind::EndPattern => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => break,
                _ => {
                    return Err(self.error(format!("Unexpected token in pattern: {:?}", self.peek_kind())));
                }
            }
        }

        Ok(Pattern { name, memory, on_create, behaviors, on_destroy })
    }

    // =========================================================================
    // Library Parsing (Section 11.1)
    // =========================================================================

    /// Parse a LIBRARY definition
    #[allow(dead_code)]
    pub fn parse_library(&mut self) -> Result<Library, CompilerError> {
        self.expect(TokenKind::Library)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut behaviors = Vec::new();

        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Behavior => {
                    let behavior = self.parse_behavior()?;
                    behaviors.push(behavior);
                }
                TokenKind::EndLibrary => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => break,
                _ => {
                    return Err(self.error(format!("Unexpected token in library: {:?}", self.peek_kind())));
                }
            }
        }

        Ok(Library { name, behaviors })
    }

    // =========================================================================
    // Executable Parsing (Section 11.2)
    // =========================================================================

    /// Parse an EXECUTABLE definition
    pub fn parse_executable(&mut self) -> Result<Executable, CompilerError> {
        self.expect(TokenKind::Executable)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut description = None;
        let mut uses = Vec::new();
        let mut entry = Vec::new();

        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Description => {
                    self.advance();
                    description = Some(self.parse_description()?);
                }
                TokenKind::Uses => {
                    self.advance();
                    let use_path = self.expect_identifier()?;
                    // Per grammar: uses = "USES" identifier ".pattern" ;
                    self.expect(TokenKind::Dot)?;
                    let ext = self.expect_identifier()?;
                    if ext != "pattern" {
                        return Err(self.error(format!("Expected '.pattern' after USES identifier, got '.{}'", ext)));
                    }
                    uses.push(use_path);
                }
                TokenKind::Entry => {
                    self.advance();
                    self.skip_newlines();
                    while !matches!(self.peek_kind(), TokenKind::EndExecutable | TokenKind::Eof) {
                        entry.push(self.parse_node()?);
                        self.skip_newlines();
                    }
                }
                TokenKind::EndExecutable => {
                    self.advance();
                    break;
                }
                TokenKind::Eof => break,
                _ => {
                    return Err(self.error(format!("Unexpected token in executable: {:?}", self.peek_kind())));
                }
            }
        }

        Ok(Executable { name, description, uses, entry })
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Parse tokens into a Behavior AST
pub fn parse(tokens: &[Token], source: &str) -> Result<Behavior, CompilerError> {
    let mut parser = Parser::new(tokens, source);
    parser.parse_behavior()
}

/// Parse tokens into a Pattern AST
pub fn parse_pattern(tokens: &[Token], source: &str) -> Result<Pattern, CompilerError> {
    let mut parser = Parser::new(tokens, source);
    parser.parse_pattern()
}

/// Parse tokens into a Library AST
#[allow(dead_code)]
pub fn parse_library(tokens: &[Token], source: &str) -> Result<Library, CompilerError> {
    let mut parser = Parser::new(tokens, source);
    parser.parse_library()
}

/// Parse tokens into an Executable AST
pub fn parse_executable(tokens: &[Token], source: &str) -> Result<Executable, CompilerError> {
    let mut parser = Parser::new(tokens, source);
    parser.parse_executable()
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    /// Helper: parse source into behavior
    fn parse_behavior(source: &str) -> Result<Behavior, CompilerError> {
        let tokens = lex(source).map_err(|e| CompilerError::lex(e.span, e.message))?;
        parse(&tokens, source)
    }

    /// Helper: parse and unwrap, panicking on error
    fn must_parse(source: &str) -> Behavior {
        parse_behavior(source).expect("Parse failed")
    }

    // =========================================================================
    // Minimal Behavior Parsing
    // =========================================================================

    #[test]
    fn test_minimal_behavior() {
        let source = r#"
BEHAVIOR minimal

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH abcd1234

IMPLEMENTATION
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.name, "minimal");
        assert_eq!(behavior.contract.outputs.len(), 1);
        assert_eq!(behavior.hash, "abcd1234");
    }

    #[test]
    fn test_leading_zero_hash_preserved() {
        // Regression: a contract hash whose leading nibble(s) are zero is
        // tokenized as Integer (which drops leading zeros), e.g. "004ec810" ->
        // Integer(4) + "ec810". The parser must restore the full 8-char value,
        // otherwise the declared hash never matches the computed (padded) one
        // and the behavior fails its hash self-check (E0507). Surfaced by two
        // independent experiment trials, both on hash 004ec810.
        for hash in [
            "004ec810", // two leading zeros + hex letters (the reproduced case)
            "00abc123", // two leading zeros
            "0abc1234", // one leading zero
            "00123456", // leading zeros, all-numeric (whole token is an Integer)
            "abcd1234", // no leading zero — unchanged
        ] {
            let source = format!(
                "BEHAVIOR z\nCONTRACT\n  OUTPUT status int 8\n  GUARANTEES writes_output\nHASH {}\nIMPLEMENTATION\n  SET status 0\nEND\n",
                hash
            );
            let behavior = must_parse(&source);
            assert_eq!(behavior.hash, hash, "hash {} not preserved through parsing", hash);
        }
    }

    #[test]
    fn test_behavior_with_description() {
        let source = r#"
BEHAVIOR described

DESCRIPTION
  This is a test behavior.
  It has multiple lines.

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH desc1234

IMPLEMENTATION
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.name, "described");
        assert!(behavior.description.is_some());
        assert!(!behavior.description.as_ref().unwrap().is_empty());
    }

    // =========================================================================
    // Contract Parsing
    // =========================================================================

    #[test]
    fn test_behavior_with_inputs() {
        let source = r#"
BEHAVIOR with-inputs

CONTRACT
  INPUT a int 8
  INPUT b int 4
  OUTPUT result int 8
  GUARANTEES writes_output

HASH inp12345

IMPLEMENTATION
  v = LOAD a 8
  SET result v
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.contract.inputs.len(), 2);
        assert_eq!(behavior.contract.inputs[0].name, "a");
        assert_eq!(behavior.contract.inputs[0].typ, Type::Int);
        assert_eq!(behavior.contract.inputs[0].size, 8);
        assert_eq!(behavior.contract.inputs[1].name, "b");
        assert_eq!(behavior.contract.inputs[1].size, 4);
    }

    #[test]
    fn test_behavior_with_bytes_parameter() {
        let source = r#"
BEHAVIOR with-bytes

CONTRACT
  INPUT data bytes 256
  OUTPUT len int 8
  GUARANTEES writes_output

HASH bytes123

IMPLEMENTATION
  SET len 0
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.contract.inputs[0].typ, Type::Bytes);
        assert_eq!(behavior.contract.inputs[0].size, 256);
    }

    #[test]
    fn test_behavior_with_requires() {
        let source = r#"
BEHAVIOR with-requires

CONTRACT
  OUTPUT status int 8
  REQUIRES foo@abc12345 bar@def45678 baz@004ec810
  GUARANTEES writes_output

HASH req12345

COMPOSITION
  CALL foo
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.contract.requires.behaviors.len(), 3);
        assert_eq!(behavior.contract.requires.behaviors[0].name, "foo");
        assert_eq!(behavior.contract.requires.behaviors[0].hash, "abc12345");
        assert_eq!(behavior.contract.requires.behaviors[1].name, "bar");
        // a leading-zero dependency pin must round-trip, same as HASH lines
        assert_eq!(behavior.contract.requires.behaviors[2].name, "baz");
        assert_eq!(behavior.contract.requires.behaviors[2].hash, "004ec810");
    }

    #[test]
    fn test_all_guarantees() {
        let source = r#"
BEHAVIOR all-guarantees

CONTRACT
  OUTPUT status int 8
  GUARANTEES pure no_alloc writes_output

HASH guar1234

IMPLEMENTATION
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert!(behavior.contract.guarantees.contains(&Guarantee::Pure));
        assert!(behavior.contract.guarantees.contains(&Guarantee::NoAlloc));
        assert!(behavior.contract.guarantees.contains(&Guarantee::WritesOutput));
    }

    // =========================================================================
    // Implementation vs Composition
    // =========================================================================

    #[test]
    fn test_implementation_section() {
        let source = r#"
BEHAVIOR impl-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH impl1234

IMPLEMENTATION
  temp = ALLOC 8 int
  STORE temp 42 8
  v = LOAD temp 8
  FREE temp
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert!(behavior.implementations.len() > 0);
        assert!(behavior.composition.is_none());
    }

    #[test]
    fn test_composition_section() {
        let source = r#"
BEHAVIOR comp-test

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH comp1234

COMPOSITION
  result = CALL helper 42
  SET status 0
END
"#;
        let behavior = must_parse(source);
        assert!(behavior.composition.is_some());
    }

    #[test]
    fn test_native_behavior() {
        let source = r#"
BEHAVIOR socket

DESCRIPTION
  Creates a network socket.

CONTRACT
  INPUT domain int 8
  INPUT sock_type int 8
  OUTPUT fd int 8
  GUARANTEES writes_output

HASH f6b8d0e2

NATIVE
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.name, "socket");
        assert!(behavior.is_native);
        assert!(behavior.implementations.is_empty());
        assert!(behavior.composition.is_none());
        assert_eq!(behavior.contract.inputs.len(), 2);
        assert_eq!(behavior.contract.outputs.len(), 1);
    }

    // =========================================================================
    // Node Parsing
    // =========================================================================

    #[test]
    fn test_label_node() {
        let source = r#"
BEHAVIOR label-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH labl1234

IMPLEMENTATION
  LABEL start
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| matches!(&n.kind, NodeKind::Label(name) if name == "start")));
    }

    #[test]
    fn test_branch_and_jump() {
        let source = r#"
BEHAVIOR branch-test

CONTRACT
  INPUT cond int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH bran1234

IMPLEMENTATION
  c = LOAD cond 8
  BRANCH c true_branch false_branch

  LABEL true_branch
    SET status 1
    JUMP done

  LABEL false_branch
    SET status 0
    JUMP done

  LABEL done
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| matches!(&n.kind, NodeKind::Branch { .. })));
        assert!(nodes.iter().any(|n| matches!(&n.kind, NodeKind::Jump(_))));
    }

    #[test]
    fn test_store_with_offset() {
        let source = r#"
BEHAVIOR store-offset

CONTRACT
  OUTPUT data bytes 16
  GUARANTEES writes_output

HASH stor1234

IMPLEMENTATION
  STORE data 42 8 0
  STORE data 99 8 8
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        let store_count = nodes.iter().filter(|n| matches!(&n.kind, NodeKind::Store { .. })).count();
        assert_eq!(store_count, 2);
    }

    #[test]
    fn test_free_node() {
        let source = r#"
BEHAVIOR free-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH free1234

IMPLEMENTATION
  temp = ALLOC 8 int
  FREE temp
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| matches!(&n.kind, NodeKind::Free(name) if name == "temp")));
    }

    // =========================================================================
    // Expression Parsing
    // =========================================================================

    #[test]
    fn test_integer_literal_expr() {
        let source = r#"
BEHAVIOR int-lit

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH intl1234

IMPLEMENTATION
  SET status 42
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Set { value, .. } = &n.kind {
                matches!(value.as_ref(), Expr::IntLit(42))
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_load_expr() {
        let source = r#"
BEHAVIOR load-expr

CONTRACT
  INPUT x int 8
  OUTPUT status int 8
  GUARANTEES writes_output

HASH load1234

IMPLEMENTATION
  v = LOAD x 8
  SET status v
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::Load { .. })
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_alloc_expr() {
        let source = r#"
BEHAVIOR alloc-expr

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH allo1234

IMPLEMENTATION
  temp = ALLOC 16 bytes
  FREE temp
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                if let Expr::Alloc { size, typ, .. } = expr.as_ref() {
                    *typ == Type::Bytes && matches!(size.as_ref(), Expr::IntLit(16))
                } else {
                    false
                }
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_arithmetic_expr() {
        let source = r#"
BEHAVIOR arith-expr

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH arit1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  sum = IADD va vb 8
  STORE result sum 8
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::Iadd(_, _, 8))
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_comparison_expr() {
        let source = r#"
BEHAVIOR cmp-expr

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH comp1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  cmp = ILT va vb 8
  STORE result cmp 8
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::Ilt(_, _, 8))
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_field_access_expr() {
        let source = r#"
BEHAVIOR field-expr

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH fiel1234

COMPOSITION
  result = CALL helper 42
  v = LOAD result.value 8
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.composition.unwrap().nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                if let Expr::Load { source: src, .. } = expr.as_ref() {
                    matches!(src.as_ref(), Expr::Field { .. })
                } else {
                    false
                }
            } else {
                false
            }
        }));
    }

    // =========================================================================
    // Call Parsing
    // =========================================================================

    #[test]
    fn test_call_in_composition() {
        let source = r#"
BEHAVIOR call-comp

CONTRACT
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH call1234

COMPOSITION
  result = CALL helper 1 2 3
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.composition.unwrap().nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                if let Expr::Call { behavior, args } = expr.as_ref() {
                    behavior == "helper" && args.len() == 3
                } else {
                    false
                }
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_call_standalone() {
        let source = r#"
BEHAVIOR call-standalone

CONTRACT
  OUTPUT status int 8
  REQUIRES printer@abc123
  GUARANTEES writes_output

HASH cals1234

COMPOSITION
  CALL printer
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.composition.unwrap().nodes;
        assert!(nodes.iter().any(|n| matches!(&n.kind, NodeKind::Call { behavior, .. } if behavior == "printer")));
    }

    // =========================================================================
    // Error Cases
    // =========================================================================

    #[test]
    fn test_missing_behavior_name_error() {
        let source = r#"
BEHAVIOR

CONTRACT
  OUTPUT status int 8

HASH err12345

IMPLEMENTATION
  SET status 0
END
"#;
        let result = parse_behavior(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_end_error() {
        let source = r#"
BEHAVIOR missing-end

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH miss1234

IMPLEMENTATION
  SET status 0
"#;
        let result = parse_behavior(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_type_error() {
        let source = r#"
BEHAVIOR invalid-type

CONTRACT
  INPUT x invalid 8
  OUTPUT status int 8

HASH inva1234

IMPLEMENTATION
  SET status 0
END
"#;
        let result = parse_behavior(source);
        assert!(result.is_err());
    }

    // =========================================================================
    // Complex Examples
    // =========================================================================

    #[test]
    fn test_complex_behavior() {
        let source = r#"
BEHAVIOR complex

DESCRIPTION
  A complex behavior with multiple features.

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  OUTPUT status int 8
  REQUIRES helper@abc123
  GUARANTEES writes_output

HASH comp1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  sum = IADD va vb 8
  STORE result sum 8

  BRANCH sum positive negative

  LABEL positive
    SET status 1
    JUMP done

  LABEL negative
    SET status 0
    JUMP done

  LABEL done
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.name, "complex");
        assert_eq!(behavior.contract.inputs.len(), 2);
        assert_eq!(behavior.contract.outputs.len(), 2);
        assert_eq!(behavior.contract.requires.behaviors.len(), 1);
    }

    #[test]
    fn test_string_literal_in_behavior() {
        let source = r#"
BEHAVIOR string-test

CONTRACT
  OUTPUT status int 8
  GUARANTEES writes_output

HASH str12345

IMPLEMENTATION
  msg = "Hello, World!"
  SET status 0
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::StringLit(s) if s == "Hello, World!")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_float_operations() {
        let source = r#"
BEHAVIOR float-ops

CONTRACT
  INPUT a float 8
  INPUT b float 8
  OUTPUT result float 8
  GUARANTEES writes_output

HASH floa1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  sum = FADD va vb 8
  STORE result sum 8
END
"#;
        let behavior = must_parse(source);
        assert_eq!(behavior.contract.inputs[0].typ, Type::Float);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::Fadd(_, _, 8))
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_bitwise_operations() {
        let source = r#"
BEHAVIOR bitwise

CONTRACT
  INPUT a int 8
  INPUT b int 8
  OUTPUT result int 8
  GUARANTEES writes_output

HASH bitw1234

IMPLEMENTATION
  va = LOAD a 8
  vb = LOAD b 8
  r = AND va vb 8
  STORE result r 8
END
"#;
        let behavior = must_parse(source);
        let nodes = &behavior.implementations[0].nodes;
        assert!(nodes.iter().any(|n| {
            if let NodeKind::Assignment { expr, .. } = &n.kind {
                matches!(expr.as_ref(), Expr::And(_, _, 8))
            } else {
                false
            }
        }));
    }
}
