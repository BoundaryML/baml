// Styled `Error:` / `Warning:` line printers shared by every CLI
// surface (`baml-cli` and the packed-binary host). Lives here — rather
// than in `baml_cli::reporter` — so the pack host, which can't depend
// on `baml_cli`, still gets the bold-red / bold-yellow prefixes that
// match ariadne's diagnostic header. `console::Style` strips ANSI
// automatically when stderr isn't a TTY, so piping through `grep`
// produces clean plain text.

#![allow(clippy::print_stderr)]

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
pub fn print_error(msg: impl std::fmt::Display) {
    eprintln!("{}: {msg}", error_prefix());
}

/// Print a one-line warning with the ariadne-matching bold-yellow
/// `Warning:` header. Routes to stderr.
pub fn print_warning(msg: impl std::fmt::Display) {
    eprintln!("{}: {msg}", warning_prefix());
}

/// Print an [`anyhow::Error`] with the ariadne-matching header, walking
/// the cause chain so each tier is visible — matches anyhow's default
/// Debug format but with the bold-red header so it lines up visually
/// with the ariadne source-snippet blocks the compiler emits.
pub fn print_anyhow_error(err: &anyhow::Error) {
    eprintln!("{}: {err}", error_prefix());
    let causes: Vec<_> = err.chain().skip(1).collect();
    if !causes.is_empty() {
        eprintln!();
        eprintln!("caused by:");
        for (i, cause) in causes.iter().enumerate() {
            eprintln!("    {i}: {cause}");
        }
    }
}
