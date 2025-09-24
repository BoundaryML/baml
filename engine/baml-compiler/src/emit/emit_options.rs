use baml_types::TypeIR;
use internal_baml_ast::{
    self,
    ast::{EmitArgument, EmitDecorator, Expression, Identifier},
};

/// The user-specified options for an emit variable.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitSpec {
    pub when: EmitWhen,
    pub skip_def: bool,
    pub name: String,
}

/// The user-specified option for when to auto-emit a variable.
#[derive(Clone, Debug, PartialEq)]
pub enum EmitWhen {
    False, // TODO: Revisit the name for this variant. I prefer "Manual"
    True,
    FunctionName(Identifier),
}

impl EmitSpec {
    /// Lower the EmitDecorator AST node into an EmitSpec.
    pub fn from_ast_with_name(ast_emit: &EmitDecorator, ast_channel_name: String) -> Self {
        let mut emit = EmitSpec {
            when: EmitWhen::True,
            skip_def: false,
            name: ast_channel_name.clone(),
        };
        for EmitArgument { name, value, .. } in &ast_emit.arguments {
            let mut has_error = false;
            let key_str = name.to_string();
            if let Some(val_str) = value.as_string_value().map(|(s, _)| s.as_ref()) {
                // Enumerate all the valid key-value pairs.
                match (key_str.as_ref(), val_str) {
                    ("when", "false") => {
                        emit.when = EmitWhen::False;
                    }
                    ("when", _other) => {
                        match value {
                            Expression::Identifier(ident) => {
                                emit.when = EmitWhen::FunctionName(ident.clone());
                            }
                            _ => {
                                // Impossible case, ruled out by the parser.
                            }
                        }
                    }
                    ("skip_def", "true") => {
                        emit.skip_def = true;
                    }
                    ("skip_def", "false") => {
                        emit.skip_def = false;
                    }
                    ("name", channel_name) => {
                        emit.name = channel_name.to_string();
                    }
                    _ => {
                        has_error = true;
                    }
                }
            }

            if has_error {
                panic!(
                    "Impossible case: the grammar should never produce emit argument {:?}={:?}",
                    name, value
                )
            }
        }
        emit
    }
}
