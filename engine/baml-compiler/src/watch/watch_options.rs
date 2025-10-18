use internal_baml_ast::{
    self,
    ast::{Expression, Identifier, WatchArgument, WatchDecorator},
};
use internal_baml_diagnostics::Span;

/// The user-specified options for an emit variable.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchSpec {
    pub when: WatchWhen,
    pub skip_def: bool,
    pub name: String,
    pub span: Span,
}

/// The user-specified option for when to auto-notify watchers for a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum WatchWhen {
    Manual, // Manual notification only (via .watchers.$notify())
    True,
    FunctionName(Identifier),
}

impl WatchSpec {
    /// Lower the EmitDecorator AST node into an EmitSpec.
    /// Ther are some invariants on `EmitSpec`. They are not handler here,
    /// they are handled upstream in the grammar (which rules out many invalid
    /// key/value combinations), and in the typechecker, which ensures that
    /// when-functions have the correct type.
    pub fn from_ast_with_name(ast_watch: &WatchDecorator, ast_channel_name: String) -> Self {
        let mut watch = WatchSpec {
            when: WatchWhen::True,
            skip_def: false,
            name: ast_channel_name.clone(),
            span: ast_watch.span.clone(),
        };
        for WatchArgument { name, value, .. } in &ast_watch.arguments {
            let mut has_error = false;
            let key_str = name.to_string();

            // For convenience, convert the value to a string.
            // We use this string when it's "true" or "false".
            // But we ignore it if it's something else, such as
            // an identifier, when we parse the `when` key's value.
            if let Some(val_str) = value.as_string_value().map(|(s, _)| s) {
                // Enumerate all the valid key-value pairs.
                match (key_str.as_ref(), val_str) {
                    ("when", "manual") => {
                        watch.when = WatchWhen::Manual;
                    }
                    ("when", "false") => {
                        // Support legacy "false" syntax, map to Manual
                        watch.when = WatchWhen::Manual;
                    }
                    ("when", "true") => {
                        watch.when = WatchWhen::True;
                    }
                    ("when", _other) => {
                        match value {
                            Expression::Identifier(ident) => {
                                watch.when = WatchWhen::FunctionName(ident.clone());
                            }
                            _ => {
                                // Impossible case, ruled out by the parser.
                            }
                        }
                    }
                    ("skip_def", "true") => {
                        watch.skip_def = true;
                    }
                    ("skip_def", "false") => {
                        watch.skip_def = false;
                    }
                    ("name", channel_name) => {
                        watch.name = channel_name.to_string();
                    }
                    _ => {
                        has_error = true;
                    }
                }
            }

            if has_error {
                log::error!(
                    "Impossible case: the grammar should never produce emit argument {name:?}={value:?}"
                );
            }
        }
        watch
    }
}
