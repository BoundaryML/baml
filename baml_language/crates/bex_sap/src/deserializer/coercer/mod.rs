mod array_helper;
mod coerce_array;
mod coerce_class;
mod coerce_enum;
mod coerce_literal;
mod coerce_map;
mod coerce_primitive;
mod coerce_stream_state;
mod coerce_union;
mod field_type;
mod match_string;

use std::{borrow::Cow, collections::HashSet};

use crate::{
    baml_value::ValueWithMeta,
    deserializer::types::DeserializerMeta,
    sap_model::{
        FromLiteral, TyWithMeta, TypeAnnotations, TypeIdent, TypeName, TypeRefDb, TypeValue,
    },
};
// use baml_types::{BamlValue, Constraint, JinjaExpression};
// use internal_baml_core::ir::jinja_helpers::evaluate_predicate;
// use internal_baml_jinja::types::OutputFormatContent;

use super::types::BamlValueWithFlags;
use crate::jsonish;

pub struct ParsingContext<'s, 'v, 't, N: TypeIdent> {
    pub scope: Vec<String>,
    visited_during_coerce: HashSet<(String, &'v jsonish::Value<'s>)>,
    visited_during_try_cast: HashSet<(String, &'v jsonish::Value<'s>)>,
    pub db: &'t TypeRefDb<'t, N>,
    /// Hint for union coercion: the variant index that succeeded on the previous
    /// array element. Used to optimize arrays of unions by trying the likely
    /// variant first.
    pub union_variant_hint: Option<usize>,
}

impl<'s, 'v, 't, N: TypeIdent> ParsingContext<'s, 'v, 't, N> {
    pub fn display_scope(&self) -> String {
        if self.scope.is_empty() {
            return "<root>".to_string();
        }
        self.scope.join(".")
    }

    #[allow(dead_code)]
    pub(crate) fn new(db: &'t TypeRefDb<'t, N>) -> Self {
        ParsingContext {
            scope: Vec::new(),
            visited_during_coerce: HashSet::new(),
            visited_during_try_cast: HashSet::new(),
            db,
            union_variant_hint: None,
        }
    }

    pub(crate) fn enter_scope(&self, scope: &str) -> Self {
        let mut new_scope = self.scope.clone();
        new_scope.push(scope.to_string());
        ParsingContext {
            scope: new_scope,
            visited_during_coerce: self.visited_during_coerce.clone(),
            visited_during_try_cast: self.visited_during_try_cast.clone(),
            db: self.db,
            // Don't propagate hint to nested scopes by default
            union_variant_hint: None,
        }
    }

    /// Enter a scope with a union variant hint for optimizing arrays of unions.
    /// The hint suggests which variant to try first based on previous successful coercions.
    pub(crate) fn enter_scope_with_hint(&self, scope: &str, hint: Option<usize>) -> Self {
        let mut new_scope = self.scope.clone();
        new_scope.push(scope.to_string());
        ParsingContext {
            scope: new_scope,
            visited_during_coerce: self.visited_during_coerce.clone(),
            visited_during_try_cast: self.visited_during_try_cast.clone(),
            db: self.db,
            union_variant_hint: hint,
        }
    }

    // TODO: This function and `enter_scope` are clonning both the scope vector
    // and visited hash set each time. Maybe it can be optimized with interior
    // mutability or something.
    pub(crate) fn visit_class_value_pair(
        &self,
        cls_value_pair: (String, &'v jsonish::Value<'s>),
        is_coerce: bool,
    ) -> Self {
        let mut new_visited_coerce = self.visited_during_coerce.clone();
        let mut new_visited_try_cast = self.visited_during_try_cast.clone();
        if is_coerce {
            new_visited_coerce.insert(cls_value_pair);
        } else {
            new_visited_try_cast.insert(cls_value_pair);
        }
        ParsingContext {
            scope: self.scope.clone(),
            visited_during_coerce: new_visited_coerce,
            visited_during_try_cast: new_visited_try_cast,
            db: self.db,
            union_variant_hint: None,
        }
    }

    pub(crate) fn error_too_many_matches<T: std::fmt::Display>(
        &self,
        target: &impl TryTypeName<'t, N>,
        options: impl IntoIterator<Item = T>,
    ) -> ParsingError {
        let got = options.into_iter().fold(String::new(), |acc, f| {
            if acc.is_empty() {
                return f.to_string();
            }
            format!("{acc}, {f}")
        });
        match target.error_type_resolution(self) {
            Ok(ty) => ParsingError {
                reason: format!("Too many matches for {ty}. Got: {got}"),
                scope: self.scope.clone(),
                causes: Vec::new(),
            },
            Err(ident) => ParsingError {
                reason: format!("Failed to resolve type {ident}"),
                scope: self.scope.clone(),
                causes: vec![ParsingError {
                    reason: format!("Too many matches for <UNRESOLVED>. Got: {got}"),
                    scope: self.scope.clone(),
                    causes: Vec::new(),
                }],
            },
        }
    }

    pub(crate) fn error_merge_multiple<'a>(
        &self,
        summary: &str,
        error: impl IntoIterator<Item = &'a ParsingError>,
    ) -> ParsingError {
        ParsingError {
            reason: summary.to_string(),
            scope: self.scope.clone(),
            causes: error.into_iter().cloned().collect(),
        }
    }

    pub(crate) fn error_unexpected_empty_array(
        &self,
        target: &impl TryTypeName<'t, N>,
    ) -> ParsingError {
        match target.error_type_resolution(self) {
            Ok(ty) => ParsingError {
                reason: format!("Expected {ty}, got empty array"),
                scope: self.scope.clone(),
                causes: Vec::new(),
            },
            Err(ident) => ParsingError {
                reason: format!("Failed to resolve type {ident}"),
                scope: self.scope.clone(),
                causes: vec![ParsingError {
                    reason: "Expected <UNRESOLVED>, got empty array".to_string(),
                    scope: self.scope.clone(),
                    causes: Vec::new(),
                }],
            },
        }
    }

    pub(crate) fn error_unexpected_null(&self, target: &impl TryTypeName<'t, N>) -> ParsingError {
        match target.error_type_resolution(self) {
            Ok(ty) => ParsingError {
                reason: format!("Expected {ty}, got null"),
                scope: self.scope.clone(),
                causes: Vec::new(),
            },
            Err(ident) => ParsingError {
                reason: format!("Failed to resolve type {ident}"),
                scope: self.scope.clone(),
                causes: vec![ParsingError {
                    reason: "Expected <UNRESOLVED>, got null".to_string(),
                    scope: self.scope.clone(),
                    causes: Vec::new(),
                }],
            },
        }
    }

    pub(crate) fn error_image_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Image type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_audio_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Audio type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_pdf_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Pdf type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_video_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Video type is not supported here".to_string(),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_map_must_have_supported_key(
        &self,
        key_type: &impl TryTypeName<'t, N>,
    ) -> ParsingError {
        match key_type.error_type_resolution(self) {
            Ok(key_type) => ParsingError {
            reason: format!(
                "Maps may only have strings, enums or literal strings for keys, but got {key_type}",
            ),
            scope: self.scope.clone(),
            causes: Vec::new(),
        },
            Err(ident) => ParsingError {
                reason: format!("Failed to resolve type {ident}"),
                scope: self.scope.clone(),
                causes: vec![
                    ParsingError {
                        reason: "Maps may only have strings, enums or literal strings for keys, but got <UNRESOLVED>".to_string(),
                        scope: self.scope.clone(),
                        causes: Vec::new(),
                    }
                ],
            },
        }
    }

    pub(crate) fn error_missing_required_field(
        &self,
        unparsed: Vec<(impl AsRef<str>, ParsingError)>,
        missing: Vec<impl AsRef<str>>,
        _item: Option<&crate::jsonish::Value>,
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
                    reason: format!("Missing required field: {}", k.as_ref()),
                    causes: Vec::new(),
                })
                .chain(unparsed.into_iter().map(|(k, e)| ParsingError {
                    scope: self.scope.clone(),
                    reason: format!("Failed to parse field {}: {}", k.as_ref(), e),
                    causes: vec![e],
                }))
                .collect(),
        }
    }

    pub(crate) fn error_unexpected_type<T: std::fmt::Display + std::fmt::Debug>(
        &self,
        target: &impl TryTypeName<'t, N>,
        got: &T,
    ) -> ParsingError {
        match target.error_type_resolution(self) {
            Ok(ty) => ParsingError {
                reason: format!("Expected {ty}, got {got:?}."),
                scope: self.scope.clone(),
                causes: Vec::new(),
            },
            Err(ident) => ParsingError {
                reason: format!("Failed to resolve type {ident}"),
                scope: self.scope.clone(),
                causes: vec![ParsingError {
                    reason: format!("Expected <UNRESOLVED>, got {got:?}."),
                    scope: self.scope.clone(),
                    causes: Vec::new(),
                }],
            },
        }
    }

    pub(crate) fn error_internal<T: std::fmt::Display>(&self, error: T) -> ParsingError {
        ParsingError {
            reason: format!("Internal error: {error}"),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_circular_reference(
        &self,
        cls: &str,
        value: &jsonish::Value,
    ) -> ParsingError {
        ParsingError {
            reason: format!("Circular reference detected for class-value pair {cls} <-> {value}"),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_integer_out_of_bounds(&self, int: &serde_json::Number) -> ParsingError {
        ParsingError {
            reason: format!("Integer was out of bounds: {int}"),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_type_resolution(&self, ident: &N) -> ParsingError {
        ParsingError {
            reason: format!("Failed to resolve type {ident}"),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }

    pub(crate) fn error_assertion_failure(&self) -> ParsingError {
        ParsingError {
            reason: "Assertion failed".to_string(),
            scope: self.scope.clone(),
            causes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsingError {
    pub scope: Vec<String>,
    pub reason: String,
    pub causes: Vec<ParsingError>,
}

impl ParsingError {
    #[allow(clippy::must_use_candidate)]
    #[must_use]
    pub fn with_cause(mut self, cause: ParsingError) -> Self {
        self.causes.push(cause);
        self
    }
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
            write!(f, "\n  - {}", format!("{cause}").replace('\n', "\n  "))?;
        }
        Ok(())
    }
}

impl std::error::Error for ParsingError {}

pub trait TypeCoercer<'s, 'v, 't, N: TypeIdent>:
    TypeValue<'s, 'v, 't> + FromLiteral<'s, 'v, 't, N>
where
    's: 'v,
{
    /// Tries to coerce a value to an annotated type. May perform transformations.
    ///
    /// Returns `Ok(None)` if the value was incomplete and has `in_progress = never`
    #[allow(clippy::type_complexity)]
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<
        Option<
            ValueWithMeta<<Self as TypeValue<'s, 'v, 't>>::Value, DeserializerMeta<'s, 'v, 't, N>>,
        >,
        ParsingError,
    >;

    /// Tries to cast a value to an annotated type. Does not perform any transformations.
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<
        ValueWithMeta<<Self as TypeValue<'s, 'v, 't>>::Value, DeserializerMeta<'s, 'v, 't, N>>,
    >;
}

pub trait DefaultValue<'s, 'v, 't, N: TypeIdent> {
    fn default_value(
        &self,
        error: Option<&ParsingError>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>>;
}

/// A trait that gets the type name (permitting resolution errors) from a type.
pub(crate) trait TryTypeName<'t, N: TypeIdent> {
    fn error_type_resolution(
        &self,
        ctx: &ParsingContext<'_, '_, 't, N>,
    ) -> Result<Cow<'static, str>, &'t N>;
}
impl<'t, T: TypeName, N: TypeIdent> TryTypeName<'t, N> for T {
    fn error_type_resolution(
        &self,
        _ctx: &ParsingContext<'_, '_, 't, N>,
    ) -> Result<Cow<'static, str>, &'t N> {
        Ok(self.type_name())
    }
}
