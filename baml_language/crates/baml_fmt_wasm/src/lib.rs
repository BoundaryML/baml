//! The BAML formatter, on its own, for a browser.
//!
//! The type-system quiz shows a learner generated programs and asks what the
//! compiler will make of them. Those programs should be laid out the way this
//! project lays out every other program, so that the learner spends their
//! attention on what the code means rather than on how it happens to be
//! spaced. The only way to be sure of that without tracking the formatter's
//! rules by hand — and having to revisit them every time the formatter
//! changes — is to run the formatter.
//!
//! One function is the whole crate, on purpose. The browser language server
//! links `baml_fmt` too and answers `textDocument/formatting`, so the quiz
//! could format through it; but it brings the analysis engine along, which
//! offers hover types, definitions, and live diagnostics. Diagnostics are the
//! answer to every question the quiz asks, so the page the quiz runs in is the
//! last place that engine belongs. Both paths link the same formatter, so
//! neither can drift from `baml fmt`.

use wasm_bindgen::{JsError, prelude::wasm_bindgen};

/// `source`, laid out as `baml fmt` lays it out.
///
/// Source that does not parse comes back unchanged. That is what the language
/// server does with it as well: there is no tree to lay out, and the caller is
/// displaying the text either way.
///
/// # Errors
///
/// Returns an error when source that *did* parse could not be formatted, which
/// is a defect in the formatter rather than in the source.
#[wasm_bindgen]
pub fn format(source: &str) -> Result<String, JsError> {
    match baml_fmt::format(source, &baml_fmt::FormatOptions::default()) {
        Ok(formatted) => Ok(formatted),
        Err(baml_fmt::FormatterError::ParseErrors(_)) => Ok(source.to_owned()),
        Err(error @ baml_fmt::FormatterError::StrongAstError(_)) => {
            Err(JsError::new(&format!("cannot format: {error}")))
        }
    }
}
