//! Diagnostic Renderer Module
//!
//! Renders diagnostics in Rust-style format with source snippets,
//! underlines, and contextual information.
//!
//! NOTE: paired with the diagnostic module; not yet wired into the pipeline.
#![allow(dead_code)]

use std::fmt::Write;
use crate::diagnostic::{Diagnostic, Label, Severity};
use crate::source::SourceManager;

// =============================================================================
// DiagnosticRenderer
// =============================================================================

/// Renders diagnostics to formatted strings
pub struct DiagnosticRenderer<'a> {
    source: &'a SourceManager,
}

impl<'a> DiagnosticRenderer<'a> {
    /// Create a new renderer with a source manager
    pub fn new(source: &'a SourceManager) -> Self {
        DiagnosticRenderer { source }
    }

    /// Render a diagnostic to a string
    pub fn render(&self, diag: &Diagnostic) -> String {
        let mut output = String::new();

        // Header: error[E0301]: message
        self.render_header(&mut output, diag);

        // File location: --> file:line:col
        self.render_location(&mut output, diag);

        // Source snippets with labels
        self.render_snippets(&mut output, diag);

        // Help text
        if let Some(ref help) = diag.help {
            writeln!(output, "   |").unwrap();
            writeln!(output, "   = help: {}", help).unwrap();
        }

        // Notes
        for note in &diag.notes {
            writeln!(output, "   = note: {}", note).unwrap();
        }

        output
    }

    /// Render the error header line
    fn render_header(&self, output: &mut String, diag: &Diagnostic) {
        let severity = match diag.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        writeln!(output, "{}[{}]: {}", severity, diag.code, diag.message).unwrap();
    }

    /// Render the file location line
    fn render_location(&self, output: &mut String, diag: &Diagnostic) {
        if let Some(ref primary) = diag.labels.iter().find(|l| l.primary) {
            let file_str = diag.file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<source>".to_string());

            writeln!(output, "  --> {}:{}:{}",
                file_str,
                primary.span.line,
                primary.span.col
            ).unwrap();
        }
    }

    /// Render source snippets with labels
    fn render_snippets(&self, output: &mut String, diag: &Diagnostic) {
        if diag.labels.is_empty() {
            return;
        }

        // Group labels by line
        let mut labels_by_line: std::collections::BTreeMap<usize, Vec<&Label>> =
            std::collections::BTreeMap::new();

        for label in &diag.labels {
            labels_by_line
                .entry(label.span.line)
                .or_default()
                .push(label);
        }

        // Calculate gutter width (max line number width)
        let max_line = labels_by_line.keys().max().copied().unwrap_or(1);
        let gutter_width = max_line.to_string().len();

        // Get file path for source lookup
        let file_path = diag.file.as_ref();

        // Track previous line to add "..." for gaps
        let mut prev_line: Option<usize> = None;

        writeln!(output, "{:width$} |", "", width = gutter_width).unwrap();

        for (&line_num, labels) in &labels_by_line {
            // Add "..." for gaps between lines
            if let Some(prev) = prev_line {
                if line_num > prev + 1 {
                    writeln!(output, "{:>width$}", "...", width = gutter_width).unwrap();
                }
            }

            // Get the source line
            let source_line = file_path
                .and_then(|p| self.source.get_line(p, line_num))
                .unwrap_or("");

            // Print the source line
            writeln!(output, "{:>width$} | {}", line_num, source_line, width = gutter_width).unwrap();

            // Print underlines and labels for this line
            self.render_labels_for_line(output, gutter_width, source_line, labels);

            prev_line = Some(line_num);
        }
    }

    /// Render label underlines for a single line
    fn render_labels_for_line(
        &self,
        output: &mut String,
        gutter_width: usize,
        source_line: &str,
        labels: &[&Label]
    ) {
        // Sort labels by column for rendering
        let mut sorted_labels: Vec<&&Label> = labels.iter().collect();
        sorted_labels.sort_by_key(|l| l.span.col);

        // Build the underline string
        let mut underline = String::new();
        let mut label_messages: Vec<(usize, &str, bool)> = Vec::new(); // (col, message, is_primary)

        for label in sorted_labels {
            let col_start = label.span.col.saturating_sub(1); // 0-indexed

            // Pad to column start
            while underline.len() < col_start {
                underline.push(' ');
            }

            // Add underline characters
            let underline_char = if label.primary { '^' } else { '-' };
            for _ in 0..label.span.len.max(1) {
                if underline.len() < source_line.len() + label.span.len {
                    underline.push(underline_char);
                }
            }

            label_messages.push((col_start, &label.message, label.primary));
        }

        // Print underline
        if !underline.is_empty() {
            writeln!(output, "{:width$} | {}", "", underline, width = gutter_width).unwrap();
        }

        // Print label messages (one per line for clarity)
        for (col, message, is_primary) in label_messages {
            if !message.is_empty() {
                let prefix: String = std::iter::repeat(' ').take(col).collect();
                let marker = if is_primary { "|" } else { "|" };
                writeln!(output, "{:width$} | {}{} {}", "", prefix, marker, message, width = gutter_width).unwrap();
            }
        }
    }

    /// Render multiple diagnostics
    pub fn render_all(&self, diagnostics: &[Diagnostic]) -> String {
        let mut output = String::new();
        for (i, diag) in diagnostics.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&self.render(diag));
        }
        output
    }
}

// =============================================================================
// Standalone render function
// =============================================================================

/// Render a diagnostic without a source manager (no source snippets)
pub fn render_simple(diag: &Diagnostic) -> String {
    let mut output = String::new();

    // Header
    let severity = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };
    writeln!(output, "{}[{}]: {}", severity, diag.code, diag.message).unwrap();

    // Location
    if let Some(ref primary) = diag.labels.iter().find(|l| l.primary) {
        let file_str = diag.file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<source>".to_string());

        writeln!(output, "  --> {}:{}:{}", file_str, primary.span.line, primary.span.col).unwrap();
    }

    // Labels as simple list
    for label in &diag.labels {
        let marker = if label.primary { "^" } else { "-" };
        writeln!(output, "   {} line {}:{}: {}", marker, label.span.line, label.span.col, label.message).unwrap();
    }

    // Help
    if let Some(ref help) = diag.help {
        writeln!(output, "   = help: {}", help).unwrap();
    }

    // Notes
    for note in &diag.notes {
        writeln!(output, "   = note: {}", note).unwrap();
    }

    output
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, ErrorCode};
    use crate::lexer::Span;
    use std::path::PathBuf;

    #[test]
    fn test_render_simple_error() {
        let span = Span::new(10, 5, 4);
        let diag = Diagnostic::error(ErrorCode::E0301, "use after free")
            .with_file("test.beh")
            .with_primary_label(span, "used here");

        let output = render_simple(&diag);

        assert!(output.contains("error[E0301]: use after free"));
        assert!(output.contains("--> test.beh:10:5"));
        assert!(output.contains("used here"));
    }

    #[test]
    fn test_render_with_help() {
        let span = Span::new(5, 3, 4);
        let diag = Diagnostic::error(ErrorCode::E0102, "invalid int size")
            .with_file("test.beh")
            .with_primary_label(span, "here")
            .with_help("valid sizes are 1, 2, 4, or 8");

        let output = render_simple(&diag);

        assert!(output.contains("= help: valid sizes are 1, 2, 4, or 8"));
    }

    #[test]
    fn test_render_with_source() {
        let mut source_mgr = SourceManager::new();
        source_mgr.add_source(
            PathBuf::from("test.beh"),
            "  FREE buffer\n  LOAD buffer 8\n".to_string()
        );

        let free_span = Span::new(1, 3, 4);  // FREE
        let use_span = Span::new(2, 3, 4);   // LOAD

        let diag = Diagnostic::use_after_free(use_span, free_span, "buffer")
            .with_file("test.beh");

        let renderer = DiagnosticRenderer::new(&source_mgr);
        let output = renderer.render(&diag);

        assert!(output.contains("error[E0301]"));
        assert!(output.contains("FREE buffer"));
        assert!(output.contains("LOAD buffer"));
    }

    #[test]
    fn test_render_multi_label() {
        let span1 = Span::new(10, 3, 4);
        let span2 = Span::new(15, 3, 4);

        let diag = Diagnostic::double_free(span2, span1, "buf")
            .with_file("test.beh");

        let output = render_simple(&diag);

        assert!(output.contains("error[E0302]"));
        assert!(output.contains("second FREE"));
        assert!(output.contains("first FREE"));
    }

    #[test]
    fn test_render_with_notes() {
        let span = Span::new(5, 1, 6);
        let diag = Diagnostic::error(ErrorCode::E0601, "pure violation")
            .with_file("test.beh")
            .with_primary_label(span, "here")
            .with_note("pure behaviors cannot have side effects")
            .with_note("consider removing the pure guarantee");

        let output = render_simple(&diag);

        assert!(output.contains("= note: pure behaviors cannot have side effects"));
        assert!(output.contains("= note: consider removing the pure guarantee"));
    }

    #[test]
    fn test_render_all() {
        let diag1 = Diagnostic::error(ErrorCode::E0901, "error 1")
            .with_primary_label(Span::new(1, 1, 1), "here");
        let diag2 = Diagnostic::error(ErrorCode::E0901, "error 2")
            .with_primary_label(Span::new(5, 1, 1), "there");

        let source_mgr = SourceManager::new();
        let renderer = DiagnosticRenderer::new(&source_mgr);
        let output = renderer.render_all(&[diag1, diag2]);

        assert!(output.contains("error 1"));
        assert!(output.contains("error 2"));
    }

    #[test]
    fn test_gutter_width_calculation() {
        let mut source_mgr = SourceManager::new();
        let mut content = String::new();
        for i in 1..=150 {
            content.push_str(&format!("line {}\n", i));
        }
        source_mgr.add_source(PathBuf::from("big.beh"), content);

        let span = Span::new(100, 1, 4);
        let diag = Diagnostic::error(ErrorCode::E0901, "test")
            .with_file("big.beh")
            .with_primary_label(span, "here");

        let renderer = DiagnosticRenderer::new(&source_mgr);
        let output = renderer.render(&diag);

        // Line 100 should be right-aligned with 3 characters
        assert!(output.contains("100 |"));
    }

    #[test]
    fn test_render_underline_primary_vs_secondary() {
        let mut source_mgr = SourceManager::new();
        source_mgr.add_source(
            PathBuf::from("test.beh"),
            "  first second third\n".to_string()
        );

        let primary_span = Span::new(1, 9, 6);   // "second"
        let secondary_span = Span::new(1, 3, 5); // "first"

        let diag = Diagnostic::error(ErrorCode::E0101, "mismatch")
            .with_file("test.beh")
            .with_primary_label(primary_span, "primary")
            .with_secondary_label(secondary_span, "secondary");

        let renderer = DiagnosticRenderer::new(&source_mgr);
        let output = renderer.render(&diag);

        // Primary uses ^, secondary uses -
        assert!(output.contains("^"));
        assert!(output.contains("-"));
    }
}
