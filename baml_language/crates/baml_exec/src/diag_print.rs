// Styled `Error:` / `Warning:` line printers shared by every CLI
// surface (`baml-cli` and the packed-binary host). Lives here — rather
// than in `baml_cli::reporter` — so the pack host, which can't depend
// on `baml_cli`, still gets the bold-red / bold-yellow prefixes that
// match ariadne's diagnostic header. `console::Style` strips ANSI
// automatically when stderr isn't a TTY, so piping through `grep`
// produces clean plain text.

use std::io::{self, Write};

use console::Style;

/// Bold-red `error` keyword, matching ariadne's lowercase
/// custom-kind header.
fn error_prefix() -> impl std::fmt::Display {
    Style::new().red().bold().apply_to("error")
}

/// Bold-yellow `warning` keyword, matching ariadne's lowercase
/// custom-kind header.
fn warning_prefix() -> impl std::fmt::Display {
    Style::new().yellow().bold().apply_to("warning")
}

/// Print a single-line error with the ariadne-matching bold-red
/// `Error:` header. Routes to stderr.
/// Diagnostic delivery is best effort: a write failure must not replace the
/// original failure or bypass the caller's process recording and shutdown.
pub fn print_error(msg: impl std::fmt::Display) {
    let _ = writeln!(io::stderr().lock(), "{}: {msg}", error_prefix());
}

/// Print a one-line warning with the ariadne-matching bold-yellow
/// `Warning:` header. Routes to stderr.
pub fn print_warning(msg: impl std::fmt::Display) {
    let _ = writeln!(io::stderr().lock(), "{}: {msg}", warning_prefix());
}

/// Print an [`anyhow::Error`] with the ariadne-matching header, walking
/// the cause chain so each tier is visible — matches anyhow's default
/// Debug format but with the bold-red header so it lines up visually
/// with the ariadne source-snippet blocks the compiler emits.
pub fn print_anyhow_error(err: &anyhow::Error) {
    let _ = write_anyhow_error(&mut io::stderr().lock(), err);
}

fn write_anyhow_error(writer: &mut impl Write, err: &anyhow::Error) -> io::Result<()> {
    writeln!(writer, "{}: {err}", error_prefix())?;
    let causes: Vec<_> = err.chain().skip(1).collect();
    if !causes.is_empty() {
        writeln!(writer)?;
        writeln!(writer, "caused by:")?;
        for (i, cause) in causes.iter().enumerate() {
            writeln!(writer, "    {i}: {cause}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_preserves_cause_chain() {
        let err = anyhow::anyhow!("inner").context("outer");
        let mut output = Vec::new();
        write_anyhow_error(&mut output, &err).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .ends_with("outer\n\ncaused by:\n    0: inner\n")
        );
    }

    #[test]
    fn diagnostic_propagates_write_errors_without_panicking() {
        struct FailedWriter(io::ErrorKind);
        impl Write for FailedWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(self.0.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::StorageFull] {
            assert_eq!(
                write_anyhow_error(&mut FailedWriter(kind), &anyhow::anyhow!("original"))
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
    }
}
