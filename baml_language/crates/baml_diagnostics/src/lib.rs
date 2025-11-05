//! Beautiful diagnostic rendering using Ariadne.
//!
//! This crate converts compiler diagnostics into beautiful error messages.
//! It doesn't define error types - those live in each compiler phase.

use std::collections::HashMap;

use ariadne::{Color, Label, Report, ReportKind, Source};
use baml_base::{Diagnostic, FileId, Severity, Span};

/// Convert a compiler diagnostic to Ariadne span format.
fn span_to_ariadne(span: Span) -> (usize, std::ops::Range<usize>) {
    (
        span.file_id.as_u32() as usize,
        span.range.start().into()..span.range.end().into(),
    )
}

/// Render any diagnostic to a beautiful string using Ariadne.
pub fn render_diagnostic(diag: &dyn Diagnostic, sources: &HashMap<FileId, String>) -> String {
    let Some(span) = diag.span() else {
        // If no span, just return the message
        return diag.message();
    };

    let kind = match diag.severity() {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
    };

    let (file_id, range) = span_to_ariadne(span);

    let report = Report::build(kind, (file_id, range.clone()))
        .with_message(diag.message())
        .with_label(
            Label::new((file_id, range))
                .with_message(diag.message())
                .with_color(match diag.severity() {
                    Severity::Error => Color::Red,
                    Severity::Warning => Color::Yellow,
                    Severity::Info => Color::Blue,
                }),
        );

    let report = report.finish();

    // Render to string
    let mut output = Vec::new();
    if let Some(source) = sources.get(&span.file_id) {
        let cache = (file_id, Source::from(source));
        report.write(cache, &mut output).unwrap();
    } else {
        report
            .write_for_stdout((file_id, Source::from("")), &mut output)
            .unwrap();
    }

    String::from_utf8(output).unwrap_or_else(|_| diag.message())
}

/// Render multiple diagnostics to a string.
pub fn render_diagnostics(
    diagnostics: &[&dyn Diagnostic],
    sources: &HashMap<FileId, String>,
) -> String {
    diagnostics
        .iter()
        .map(|diag| render_diagnostic(*diag, sources))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use baml_base::{FileId, Span};
    use text_size::{TextRange, TextSize};

    use super::*;

    // A simple test diagnostic
    #[derive(Debug)]
    struct TestError {
        message: String,
        span: Span,
    }

    impl Diagnostic for TestError {
        fn message(&self) -> String {
            self.message.clone()
        }
        fn span(&self) -> Option<Span> {
            Some(self.span)
        }
        fn severity(&self) -> Severity {
            Severity::Error
        }
    }

    #[test]
    fn test_diagnostic_rendering() {
        let file_id = FileId::new(0);
        let span = Span::new(
            file_id,
            TextRange::new(TextSize::from(0), TextSize::from(5)),
        );

        let error = TestError {
            message: "test error".to_string(),
            span,
        };

        let mut sources = HashMap::new();
        sources.insert(file_id, "hello world".to_string());

        let rendered = render_diagnostic(&error, &sources);
        assert!(rendered.contains("test error"));
    }
}
