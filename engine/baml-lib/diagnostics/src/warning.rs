use crate::{
    pretty_print::{pretty_print, DiagnosticColorer},
    Span,
};
use colored::{ColoredString, Colorize};
// use indoc::indoc;

/// A non-fatal warning emitted by the schema parser.
/// For fancy printing, please use the `pretty_print_error` function.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DatamodelWarning {
    message: String,
    span: Span,
}

impl DatamodelWarning {
    /// You should avoid using this constructor directly when possible, and define warnings as public methods of this class.
    /// The constructor is only left public for supporting connector-specific warnings (which should not live in the core).
    pub fn new(message: String, span: Span) -> DatamodelWarning {
        DatamodelWarning { message, span }
    }

    pub fn new_field_validation(
        message: &str,
        model: &str,
        field: &str,
        span: Span,
    ) -> DatamodelWarning {
        let msg = format!(
            "Warning validating field `{}` in {} `{}`: {}",
            field, "model", model, message
        );

        Self::new(msg, span)
    }

    pub fn prompt_variable_unused(message: &str, span: Span) -> DatamodelWarning {
        Self::new(message.to_string(), span)
    }

    /// The user-facing warning message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The source span the warning applies to.
    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn pretty_print(&self, f: &mut dyn std::io::Write) -> std::io::Result<()> {
        pretty_print(
            f,
            self.span(),
            self.message.as_ref(),
            &DatamodelWarningColorer {},
        )
    }
}

struct DatamodelWarningColorer {}

impl DiagnosticColorer for DatamodelWarningColorer {
    fn title(&self) -> &'static str {
        "warning"
    }

    fn primary_color(&self, token: &'_ str) -> ColoredString {
        token.bright_yellow()
    }
}
