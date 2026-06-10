//! Source Manager Module
//!
//! Manages source files and provides snippet extraction for error messages.
//! Stores source text and provides efficient line-based access.
//!
//! NOTE: supports the diagnostic subsystem; not yet wired into the pipeline.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::lexer::Span;

// =============================================================================
// SourceFile
// =============================================================================

/// A source file with its content and line index
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path to the source file
    pub path: PathBuf,
    /// Full content of the file
    content: String,
    /// Byte offsets where each line starts (0-indexed)
    /// line_starts[0] = 0 (start of line 1)
    /// line_starts[1] = offset of line 2
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// Create a new SourceFile from content
    pub fn new(path: impl Into<PathBuf>, content: String) -> Self {
        let path = path.into();
        let line_starts = Self::compute_line_starts(&content);
        SourceFile {
            path,
            content,
            line_starts,
        }
    }

    /// Load a source file from disk
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        Ok(Self::new(path.to_path_buf(), content))
    }

    /// Compute line start offsets for the content
    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut starts = vec![0]; // Line 1 starts at offset 0
        for (i, c) in content.char_indices() {
            if c == '\n' {
                starts.push(i + 1); // Next line starts after the newline
            }
        }
        starts
    }

    /// Get the number of lines in the file
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Get a line by 1-based line number
    /// Returns None if line number is out of bounds
    pub fn get_line(&self, line_num: usize) -> Option<&str> {
        if line_num == 0 || line_num > self.line_starts.len() {
            return None;
        }

        let line_idx = line_num - 1; // Convert to 0-indexed
        let start = self.line_starts[line_idx];

        // Find end of line (next line start or end of content)
        let end = if line_idx + 1 < self.line_starts.len() {
            // Exclude the trailing newline
            let next_start = self.line_starts[line_idx + 1];
            if next_start > 0 && self.content.as_bytes().get(next_start - 1) == Some(&b'\n') {
                next_start - 1
            } else {
                next_start
            }
        } else {
            self.content.len()
        };

        // Handle Windows line endings (\r\n)
        let mut actual_end = end;
        if actual_end > start && self.content.as_bytes().get(actual_end - 1) == Some(&b'\r') {
            actual_end -= 1;
        }

        Some(&self.content[start..actual_end])
    }

    /// Get multiple lines as a vector
    pub fn get_lines(&self, start_line: usize, end_line: usize) -> Vec<(usize, &str)> {
        let mut lines = Vec::new();
        for line_num in start_line..=end_line {
            if let Some(line) = self.get_line(line_num) {
                lines.push((line_num, line));
            }
        }
        lines
    }

    /// Get the text at a span
    pub fn get_span_text(&self, span: Span) -> Option<&str> {
        let line = self.get_line(span.line)?;
        let col_start = span.col.saturating_sub(1); // Convert to 0-indexed
        let col_end = col_start + span.len;

        if col_start >= line.len() {
            return None;
        }

        let actual_end = col_end.min(line.len());
        Some(&line[col_start..actual_end])
    }

    /// Get column offset within a line (0-indexed byte offset)
    pub fn line_byte_offset(&self, line_num: usize) -> Option<usize> {
        if line_num == 0 || line_num > self.line_starts.len() {
            return None;
        }
        Some(self.line_starts[line_num - 1])
    }

    /// Get the full content
    pub fn content(&self) -> &str {
        &self.content
    }
}

// =============================================================================
// SourceManager
// =============================================================================

/// Manages multiple source files for error reporting
#[derive(Debug, Default)]
pub struct SourceManager {
    /// Loaded source files indexed by path
    files: HashMap<PathBuf, SourceFile>,
}

impl SourceManager {
    /// Create a new empty source manager
    pub fn new() -> Self {
        SourceManager {
            files: HashMap::new(),
        }
    }

    /// Load a source file from disk and add to manager
    pub fn load(&mut self, path: impl AsRef<Path>) -> io::Result<&SourceFile> {
        let path = path.as_ref().to_path_buf();
        if !self.files.contains_key(&path) {
            let file = SourceFile::load(&path)?;
            self.files.insert(path.clone(), file);
        }
        Ok(self.files.get(&path).unwrap())
    }

    /// Add source content directly (for in-memory compilation)
    pub fn add_source(&mut self, path: impl Into<PathBuf>, content: String) -> &SourceFile {
        let path = path.into();
        let file = SourceFile::new(path.clone(), content);
        self.files.insert(path.clone(), file);
        self.files.get(&path).unwrap()
    }

    /// Get a source file by path
    pub fn get(&self, path: impl AsRef<Path>) -> Option<&SourceFile> {
        self.files.get(path.as_ref())
    }

    /// Get a line from a specific file
    pub fn get_line(&self, path: impl AsRef<Path>, line_num: usize) -> Option<&str> {
        self.get(path)?.get_line(line_num)
    }

    /// Get span text from a specific file
    pub fn get_span_text(&self, path: impl AsRef<Path>, span: Span) -> Option<&str> {
        self.get(path)?.get_span_text(span)
    }

    /// Check if a file is loaded
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.files.contains_key(path.as_ref())
    }

    /// Get all loaded file paths
    pub fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.keys()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_file_line_count() {
        let source = SourceFile::new("test.beh", "line1\nline2\nline3".to_string());
        assert_eq!(source.line_count(), 3);
    }

    #[test]
    fn test_source_file_get_line() {
        let source = SourceFile::new("test.beh", "first\nsecond\nthird".to_string());

        assert_eq!(source.get_line(1), Some("first"));
        assert_eq!(source.get_line(2), Some("second"));
        assert_eq!(source.get_line(3), Some("third"));
        assert_eq!(source.get_line(0), None);
        assert_eq!(source.get_line(4), None);
    }

    #[test]
    fn test_source_file_empty() {
        let source = SourceFile::new("empty.beh", "".to_string());
        assert_eq!(source.line_count(), 1); // Empty file has one empty line
        assert_eq!(source.get_line(1), Some(""));
    }

    #[test]
    fn test_source_file_single_line() {
        let source = SourceFile::new("single.beh", "only line".to_string());
        assert_eq!(source.line_count(), 1);
        assert_eq!(source.get_line(1), Some("only line"));
    }

    #[test]
    fn test_source_file_trailing_newline() {
        let source = SourceFile::new("trailing.beh", "line1\nline2\n".to_string());
        assert_eq!(source.line_count(), 3);
        assert_eq!(source.get_line(1), Some("line1"));
        assert_eq!(source.get_line(2), Some("line2"));
        assert_eq!(source.get_line(3), Some(""));
    }

    #[test]
    fn test_source_file_windows_line_endings() {
        let source = SourceFile::new("windows.beh", "line1\r\nline2\r\n".to_string());
        assert_eq!(source.get_line(1), Some("line1"));
        assert_eq!(source.get_line(2), Some("line2"));
    }

    #[test]
    fn test_source_file_get_lines() {
        let source = SourceFile::new("test.beh", "a\nb\nc\nd\ne".to_string());
        let lines = source.get_lines(2, 4);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], (2, "b"));
        assert_eq!(lines[1], (3, "c"));
        assert_eq!(lines[2], (4, "d"));
    }

    #[test]
    fn test_source_file_get_span_text() {
        let source = SourceFile::new("test.beh", "  hello world".to_string());
        let span = Span::new(1, 3, 5); // "hello"
        assert_eq!(source.get_span_text(span), Some("hello"));
    }

    #[test]
    fn test_source_file_get_span_text_partial() {
        let source = SourceFile::new("test.beh", "short".to_string());
        let span = Span::new(1, 1, 100); // Longer than line
        assert_eq!(source.get_span_text(span), Some("short"));
    }

    #[test]
    fn test_source_manager_add_source() {
        let mut manager = SourceManager::new();
        manager.add_source("test.beh", "content".to_string());

        assert!(manager.contains("test.beh"));
        assert_eq!(manager.get_line("test.beh", 1), Some("content"));
    }

    #[test]
    fn test_source_manager_get_span_text() {
        let mut manager = SourceManager::new();
        manager.add_source("test.beh", "  FREE buffer".to_string());

        let span = Span::new(1, 3, 4); // "FREE"
        assert_eq!(manager.get_span_text("test.beh", span), Some("FREE"));
    }

    #[test]
    fn test_source_manager_unknown_file() {
        let manager = SourceManager::new();
        assert_eq!(manager.get_line("unknown.beh", 1), None);
        assert!(!manager.contains("unknown.beh"));
    }

    #[test]
    fn test_realistic_sigil_source() {
        let source = SourceFile::new("example.beh", r#"BEHAVIOR read_file
CONTRACT
  INPUT path bytes 256
  OUTPUT content bytes 4096
  REQUIRES syscall:read
  GUARANTEES writes_output
HASH abc123
IMPLEMENTATION
  fd = SYSCALL open path 0
  n = SYSCALL read fd content 4096
  SYSCALL close fd
END
"#.to_string());

        assert_eq!(source.line_count(), 13); // 12 lines + trailing empty
        assert_eq!(source.get_line(1), Some("BEHAVIOR read_file"));
        assert_eq!(source.get_line(4), Some("  OUTPUT content bytes 4096"));
        assert_eq!(source.get_line(9), Some("  fd = SYSCALL open path 0"));
    }

    #[test]
    fn test_line_byte_offset() {
        let source = SourceFile::new("test.beh", "abc\ndef\nghi".to_string());
        assert_eq!(source.line_byte_offset(1), Some(0));
        assert_eq!(source.line_byte_offset(2), Some(4)); // After "abc\n"
        assert_eq!(source.line_byte_offset(3), Some(8)); // After "abc\ndef\n"
    }
}
