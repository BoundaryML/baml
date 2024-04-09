use crate::{
    error::sort_by_match,
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

    pub fn new_type_not_found_error(
        type_name: &str,
        names: Vec<String>,
        span: Span,
    ) -> DatamodelWarning {
        let close_names = sort_by_match(type_name, &names, Some(10));

        let msg = if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            format!("Type `{}` does not exist.", type_name)
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Type `{}` does not exist. Did you mean `{}`?",
                type_name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("`, `");
            format!(
                "Type `{}` does not exist. Did you mean one of these: `{}`?",
                type_name, suggestions
            )
        };

        Self::new(msg, span)
    }

    pub fn type_not_used_in_prompt_error(
        is_enum: bool,
        type_exists: bool,
        function_name: &str,
        type_name: &str,
        names: Vec<String>,
        span: Span,
    ) -> DatamodelWarning {
        // Filter names that are within the threshold
        let close_names = {
            // Calculate OSA distances and sort names by distance
            let mut distances = names
                .iter()
                .map(|n| {
                    (
                        strsim::osa_distance(&n.to_lowercase(), &type_name.to_lowercase()),
                        n.to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            if !is_enum {
                distances.push((
                    strsim::osa_distance("output", &type_name.to_lowercase()),
                    "output".to_string(),
                ));
            }
            distances.sort_by_key(|k| k.0);

            // Set a threshold for "closeness"
            let threshold = 10; // for example, you can adjust this based on your needs

            distances
                .iter()
                .filter(|&&(dist, _)| dist <= threshold)
                .map(|(_, name)| name.to_owned())
                .collect::<Vec<_>>()
        };

        let prefix = if type_exists {
            format!(
                "{} `{}` is not used in in the output of function `{}`.",
                if is_enum { "Enum" } else { "Type" },
                type_name,
                function_name
            )
        } else {
            format!(
                "{} `{}` does not exist.",
                if is_enum { "Enum" } else { "Type" },
                type_name,
            )
        };

        let suggestions = if names.is_empty() {
            if is_enum {
                " No Enums are used in the output of this function.".to_string()
            } else {
                " Did you mean `output`?".to_string()
            }
        } else if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            "".to_string()
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(" Did you mean `{}`?", close_names[0])
        } else {
            // If there are multiple close names, suggest them all
            format!(
                " Did you mean one of these: `{}`?",
                close_names
                    .iter()
                    .take(3)
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("`, `")
            )
        };

        Self::new(format!("{}{}", prefix, suggestions), span)
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
