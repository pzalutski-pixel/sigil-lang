//! Compiler Error Types
//!
//! Unified error handling for lexer, parser, semantic analysis, and codegen.

use std::fmt;
use crate::lexer::Span;

/// Compiler error with source location
#[derive(Debug)]
pub struct CompilerError {
    pub kind: ErrorKind,
    pub span: Option<Span>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Semantic,
    CodeGen,
    Load,
    Io,
    Cli,
    Link,
}

impl CompilerError {
    pub fn lex(span: Span, message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Lex,
            span: Some(span),
            message: message.into(),
        }
    }

    pub fn parse(span: Span, message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Parse,
            span: Some(span),
            message: message.into(),
        }
    }

    pub fn semantic(span: Option<Span>, message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Semantic,
            span,
            message: message.into(),
        }
    }

    pub fn load(message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Load,
            span: None,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Io,
            span: None,
            message: message.into(),
        }
    }

    pub fn codegen(message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::CodeGen,
            span: None,
            message: message.into(),
        }
    }

    pub fn cli(message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Cli,
            span: None,
            message: message.into(),
        }
    }

    pub fn link(message: impl Into<String>) -> Self {
        CompilerError {
            kind: ErrorKind::Link,
            span: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind_str = match self.kind {
            ErrorKind::Lex => "Lex error",
            ErrorKind::Parse => "Parse error",
            ErrorKind::Semantic => "Semantic error",
            ErrorKind::CodeGen => "CodeGen error",
            ErrorKind::Load => "Load error",
            ErrorKind::Io => "IO error",
            ErrorKind::Cli => "CLI error",
            ErrorKind::Link => "Link error",
        };

        if let Some(span) = &self.span {
            write!(f, "{} at {}:{}: {}", kind_str, span.line, span.col, self.message)
        } else {
            write!(f, "{}: {}", kind_str, self.message)
        }
    }
}

impl std::error::Error for CompilerError {}

impl From<std::io::Error> for CompilerError {
    fn from(e: std::io::Error) -> Self {
        CompilerError::io(e.to_string())
    }
}

impl From<crate::lexer::LexError> for CompilerError {
    fn from(e: crate::lexer::LexError) -> Self {
        CompilerError::lex(e.span, e.message)
    }
}

impl From<String> for CompilerError {
    fn from(message: String) -> Self {
        CompilerError::load(message)
    }
}

impl From<&str> for CompilerError {
    fn from(message: &str) -> Self {
        CompilerError::load(message)
    }
}
