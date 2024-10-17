mod array_helper;
mod coerce_array;
mod coerce_literal;
mod coerce_map;
mod coerce_optional;
mod coerce_primitive;
mod coerce_union;
mod field_type;
mod ir_ref;
mod match_string;

use anyhow::Result;
use internal_baml_jinja::types::OutputFormatContent;

use internal_baml_core::ir::FieldType;

use super::types::BamlValueWithFlags;

pub struct ParsingContext<'a> {
    scope: Vec<String>,
    of: &'a OutputFormatContent,
    allow_partials: bool,
}

impl ParsingContext<'_> {
    pub fn display_scope(&self) -> String {
        if self.scope.is_empty() {
            return "<root>".to_string();
        }
        self.scope.join(".")
    }

    pub(crate) fn new<'a>(of: &'a OutputFormatContent, allow_partials: bool) -> ParsingContext<'a> {
        ParsingContext {
            scope: Vec::new(),
            of,
            allow_partials,
        }
    }

    pub(crate) fn enter_scope(&self, scope: &str) -> ParsingContext {
        let mut new_scope = self.scope.clone();
        new_scope.push(scope.to_string());
        ParsingContext {
            scope: new_scope,
            of: self.of,
            allow_partials: self.allow_partials,
        }
    }

    pub(crate) fn error_too_many_matches<T: std::fmt::Display>(
        &self,
        target: &FieldType,
        options: impl IntoIterator<Item = T>,
    ) -> ParsingError {
        ParsingError {
            reason: format!(
                "Too many matches for {}. Got: {}",
                target,
                options.into_iter().fold("".to_string(), |acc, f| {
                    if acc.is_empty() {
                        return f.to_string();
                    }
                    return format!("{}, {}", acc, f);
                })
            ),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_merge_multiple<'a>(
        &self,
        summary: &str,
        error: impl IntoIterator<Item = &'a ParsingError>,
    ) -> ParsingError {
        ParsingError {
            reason: format!("{}", summary),
            scope: self.scope.clone(),
            causes: error.into_iter().map(|e| e.clone()).collect(),
        }
    }

    pub(crate) fn error_unexpected_empty_array(&self, target: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Expected {}, got empty array", target.to_string()),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_unexpected_null(&self, target: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Expected {}, got null", target),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_image_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Image type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_audio_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Audio type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_map_must_have_string_key(&self, key_type: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Maps may only have strings for keys, but got {}", key_type),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_missing_required_field(
        &self,
        unparsed: Vec<(String, &ParsingError)>,
        missing: Vec<String>,
        item: Option<&crate::jsonish::Value>,
    ) -> ParsingError {
        ParsingError {
            reason: format!(
                "Failed while parsing required fields: missing={}, unparsed={}",
                missing.len(),
                unparsed.len()
            ),
            scope: self.scope.clone(),
            causes: missing
                .into_iter()
                .map(|k| ParsingError {
                    scope: self.scope.clone(),
                    reason: format!("Missing required field: {}", k),
                    causes: vec![],
                })
                .chain(unparsed.into_iter().map(|(k, e)| ParsingError {
                    scope: self.scope.clone(),
                    reason: format!("Failed to parse field {}: {}", k, e),
                    causes: vec![e.clone()],
                }))
                .collect(),
        }
    }

    pub(crate) fn error_unexpected_type<T: std::fmt::Display + std::fmt::Debug>(
        &self,
        target: &FieldType,
        got: &T,
    ) -> ParsingError {
        ParsingError {
            reason: format!(
                "Expected {}, got {:?}.",
                match target {
                    FieldType::Enum(_) => format!("{} enum value", target),
                    FieldType::Class(_) => format!("{}", target),
                    _ => format!("{target}"),
                },
                got
            ),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }

    pub(crate) fn error_internal<T: std::fmt::Display>(&self, error: T) -> ParsingError {
        ParsingError {
            reason: format!("Internal error: {}", error),
            scope: self.scope.clone(),
            causes: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsingError {
    pub scope: Vec<String>,
    pub reason: String,
    pub causes: Vec<ParsingError>,
}

impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}",
            if self.scope.is_empty() {
                "<root>".to_string()
            } else {
                self.scope.join(".")
            },
            self.reason
        )?;
        for cause in &self.causes {
            write!(f, "\n  - {}", format!("{}", cause).replace("\n", "\n  "))?;
        }
        Ok(())
    }
}

impl std::error::Error for ParsingError {}

pub trait TypeCoercer {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError>;
}

pub trait DefaultValue {
    fn default_value(&self, error: Option<&ParsingError>) -> Option<BamlValueWithFlags>;
}
