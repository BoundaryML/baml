//! Jinja template static analysis and type checking.
//!
//! This module performs static analysis of Jinja templates used in BAML prompts,
//! including:
//! - Type checking of variables and expressions
//! - Validation of filters and their arguments
//! - Type narrowing in control flow (if/elif/else)
//! - Detection of undefined variables and invalid property accesses
//!
//! Ported from `engine/baml-lib/jinja` with adaptations for the new compiler architecture.

mod expr;
mod stmt;

use std::collections::{HashMap, HashSet};

pub use expr::infer_expression_type;
use indexmap::IndexMap;
use minijinja::{machinery::WhitespaceConfig, syntax::SyntaxConfig};
pub use stmt::validate_statement;

use crate::Ty;

// ============================================================================
// Type System for Jinja
// ============================================================================

/// Literal value for type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralValue::String(s) => write!(f, "\"{s}\""),
            LiteralValue::Int(i) => write!(f, "{i}"),
            LiteralValue::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Jinja type representation for static analysis.
///
/// This is similar to but simpler than the full TIR type system,
/// focused on what Jinja templates need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JinjaType {
    Unknown,
    Undefined,
    None,
    Int,
    Float,
    Number, // Int or Float (Jinja doesn't distinguish)
    String,
    Bool,
    Literal(LiteralValue),
    List(Box<JinjaType>),
    Map(Box<JinjaType>, Box<JinjaType>),
    Tuple(Vec<JinjaType>),
    Union(Vec<JinjaType>),
    ClassRef(String),
    EnumRef(String),
    EnumValueRef(String),
    FunctionRef(String),
    /// Type alias with its name and resolved type
    Alias {
        name: String,
        resolved: Box<JinjaType>,
    },
    RecursiveTypeAlias(String),
    Image,
    Audio,
}

impl JinjaType {
    /// Check if this type is a subtype of another.
    pub fn is_subtype_of(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }

        // Unknown is compatible with everything
        if matches!(self, JinjaType::Unknown) || matches!(other, JinjaType::Unknown) {
            return true;
        }

        // Unwrap aliases before checking
        if let JinjaType::Alias { resolved, .. } = self {
            return resolved.is_subtype_of(other);
        }
        if let JinjaType::Alias { resolved, .. } = other {
            return self.is_subtype_of(resolved);
        }

        // Check union types
        if let JinjaType::Union(items) = other {
            return items.iter().any(|item| self.is_subtype_of(item));
        }

        match (self, other) {
            // Undefined and None are only subtypes of themselves
            (JinjaType::Undefined | JinjaType::None, _) => false,
            (_, JinjaType::Undefined | JinjaType::None) => false,

            // Literal subtypes
            (JinjaType::Literal(LiteralValue::Int(_)), JinjaType::Int | JinjaType::Number) => true,
            (JinjaType::Literal(LiteralValue::Bool(_)), JinjaType::Bool) => true,
            (JinjaType::Literal(LiteralValue::String(_)), JinjaType::String) => true,

            // Numeric types
            (JinjaType::Int, JinjaType::Number) => true,
            (JinjaType::Float, JinjaType::Number) => true,
            (JinjaType::Number, JinjaType::Int | JinjaType::Float) => true,

            // Container types
            (JinjaType::List(l), JinjaType::List(r)) => l.is_subtype_of(r),
            (JinjaType::Map(lk, lv), JinjaType::Map(rk, rv)) => {
                lk.is_subtype_of(rk) && lv.is_subtype_of(rv)
            }

            // Union on the left
            (JinjaType::Union(items), _) => items.iter().all(|item| item.is_subtype_of(other)),

            _ => false,
        }
    }

    /// Get a display name for this type.
    pub fn name(&self) -> String {
        match self {
            JinjaType::Unknown => "unknown".to_string(),
            JinjaType::Undefined => "undefined".to_string(),
            JinjaType::None => "none".to_string(),
            JinjaType::Int => "int".to_string(),
            JinjaType::Float => "float".to_string(),
            JinjaType::Number => "number".to_string(),
            JinjaType::String => "string".to_string(),
            JinjaType::Bool => "bool".to_string(),
            JinjaType::Literal(val) => format!("literal[{val}]"),
            JinjaType::List(elem) => format!("list[{}]", elem.name()),
            JinjaType::Map(k, v) => format!("map[{}, {}]", k.name(), v.name()),
            JinjaType::Tuple(items) => {
                let names: Vec<_> = items.iter().map(JinjaType::name).collect();
                format!("({})", names.join(", "))
            }
            JinjaType::Union(items) => {
                let names: Vec<_> = items.iter().map(JinjaType::name).collect();
                names.join(" | ")
            }
            JinjaType::ClassRef(name) => name.clone(),
            JinjaType::EnumRef(name) => name.clone(),
            JinjaType::EnumValueRef(name) => name.clone(),
            JinjaType::FunctionRef(name) => format!("function {name}"),
            JinjaType::Alias { name, resolved } => {
                format!("type alias {} (resolves to {})", name, resolved.name())
            }
            JinjaType::RecursiveTypeAlias(name) => format!("recursive type alias {name}"),
            JinjaType::Image => "image".to_string(),
            JinjaType::Audio => "audio".to_string(),
        }
    }

    /// Check if two types are equal, ignoring literal values.
    ///
    /// This is used for checking type consistency across union branches.
    pub fn equals_ignoring_literals(&self, other: &Self) -> bool {
        match (self, other) {
            (JinjaType::Literal(left), JinjaType::Literal(right)) => matches!(
                (left, right),
                (LiteralValue::Int(_), LiteralValue::Int(_))
                    | (LiteralValue::Bool(_), LiteralValue::Bool(_))
                    | (LiteralValue::String(_), LiteralValue::String(_))
            ),
            (JinjaType::List(l), JinjaType::List(r)) => l.equals_ignoring_literals(r),
            (JinjaType::Map(lk, lv), JinjaType::Map(rk, rv)) => {
                lk.equals_ignoring_literals(rk) && lv.equals_ignoring_literals(rv)
            }
            (JinjaType::Tuple(l), JinjaType::Tuple(r)) => {
                l.len() == r.len()
                    && l.iter()
                        .zip(r.iter())
                        .all(|(a, b)| a.equals_ignoring_literals(b))
            }
            (JinjaType::Union(l), JinjaType::Union(r)) => {
                l.len() == r.len()
                    && l.iter()
                        .zip(r.iter())
                        .all(|(a, b)| a.equals_ignoring_literals(b))
            }
            _ => self == other,
        }
    }
}

/// Convert TIR type to Jinja type for analysis.
impl From<&Ty> for JinjaType {
    fn from(ty: &Ty) -> Self {
        match ty {
            Ty::Unknown { .. } => JinjaType::Unknown,
            Ty::Null { .. } => JinjaType::None,
            Ty::Int { .. } => JinjaType::Int,
            Ty::Float { .. } => JinjaType::Float,
            Ty::String { .. } => JinjaType::String,
            Ty::Bool { .. } => JinjaType::Bool,
            Ty::List(elem, _) => JinjaType::List(Box::new(JinjaType::from(elem.as_ref()))),
            Ty::Map { key, value, .. } => JinjaType::Map(
                Box::new(JinjaType::from(key.as_ref())),
                Box::new(JinjaType::from(value.as_ref())),
            ),
            Ty::Union(items, _) => JinjaType::Union(items.iter().map(JinjaType::from).collect()),
            Ty::Optional(inner, _) => {
                JinjaType::Union(vec![JinjaType::None, JinjaType::from(inner.as_ref())])
            }
            Ty::Class(name, _) => JinjaType::ClassRef(name.to_string()),
            Ty::Literal(crate::LiteralValue::String(s), _) => {
                JinjaType::Literal(LiteralValue::String(s.clone()))
            }
            Ty::Literal(crate::LiteralValue::Int(i), _) => {
                JinjaType::Literal(LiteralValue::Int(*i))
            }
            Ty::Literal(crate::LiteralValue::Bool(b), _) => {
                JinjaType::Literal(LiteralValue::Bool(*b))
            }
            Ty::Literal(crate::LiteralValue::Float(_), _) => JinjaType::Float,
            Ty::Enum(name, _) => JinjaType::EnumRef(name.to_string()),
            Ty::Media(baml_base::MediaKind::Image, _) => JinjaType::Image,
            Ty::Media(baml_base::MediaKind::Audio, _) => JinjaType::Audio,
            Ty::Media(_, _) => JinjaType::Audio, // TODO: Do we need more jinja media types?
            _ => JinjaType::Unknown,
        }
    }
}

impl JinjaType {
    /// Convert a TIR type to a Jinja type, resolving type aliases.
    ///
    /// Unlike the `From<&Ty>` impl, this has access to the type alias map
    /// so it can resolve `Ty::TypeAlias` to `JinjaType::Alias { name, resolved }`.
    pub fn from_ty(ty: &Ty, aliases: &HashMap<baml_base::Name, Ty>) -> Self {
        let mut expanding = HashSet::new();
        Self::from_ty_impl(ty, aliases, &mut expanding)
    }

    fn from_ty_impl(
        ty: &Ty,
        aliases: &HashMap<baml_base::Name, Ty>,
        expanding: &mut HashSet<baml_base::Name>,
    ) -> Self {
        match ty {
            Ty::TypeAlias(fqn, _) => {
                if expanding.contains(&fqn.name) {
                    return JinjaType::RecursiveTypeAlias(fqn.name.to_string());
                }
                if let Some(alias_ty) = aliases.get(&fqn.name) {
                    expanding.insert(fqn.name.clone());
                    let resolved = Self::from_ty_impl(alias_ty, aliases, expanding);
                    expanding.remove(&fqn.name);
                    JinjaType::Alias {
                        name: fqn.name.to_string(),
                        resolved: Box::new(resolved),
                    }
                } else {
                    JinjaType::RecursiveTypeAlias(fqn.name.to_string())
                }
            }
            Ty::List(elem, _) => {
                JinjaType::List(Box::new(Self::from_ty_impl(elem, aliases, expanding)))
            }
            Ty::Map { key, value, .. } => JinjaType::Map(
                Box::new(Self::from_ty_impl(key, aliases, expanding)),
                Box::new(Self::from_ty_impl(value, aliases, expanding)),
            ),
            Ty::Union(items, _) => JinjaType::Union(
                items
                    .iter()
                    .map(|t| Self::from_ty_impl(t, aliases, expanding))
                    .collect(),
            ),
            Ty::Optional(inner, _) => JinjaType::Union(vec![
                JinjaType::None,
                Self::from_ty_impl(inner, aliases, expanding),
            ]),
            // All other types don't need alias resolution
            other => JinjaType::from(other),
        }
    }
}

// ============================================================================
// Type Environment
// ============================================================================

/// Type environment for Jinja template analysis.
///
/// Tracks:
/// - Available variables and their types
/// - Class definitions (for property access)
/// - Enum definitions
/// - Function signatures (template strings)
/// - Scope stack for control flow
pub struct JinjaTypeEnv {
    /// Variables in scope (e.g., function parameters)
    variables: HashMap<String, JinjaType>,

    /// Class definitions (name -> field types)
    classes: HashMap<String, IndexMap<String, JinjaType>>,

    /// Enum definitions (name -> values)
    enums: HashMap<String, Vec<String>>,

    /// Function signatures (name -> (`return_type`, parameters))
    functions: HashMap<String, (JinjaType, Vec<(String, JinjaType)>)>,

    /// Scope stack for tracking variables in nested contexts
    scopes: Vec<HashMap<String, JinjaType>>,
}

impl Default for JinjaTypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl JinjaTypeEnv {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            classes: HashMap::new(),
            enums: HashMap::new(),
            functions: HashMap::new(),
            scopes: Vec::new(),
        }
    }

    /// Add a variable to the current scope.
    pub fn add_variable(&mut self, name: impl Into<String>, ty: JinjaType) {
        let name = name.into();
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        } else {
            self.variables.insert(name, ty);
        }
    }

    /// Push a new scope onto the stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope from the stack.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Add a class definition.
    pub fn add_class(&mut self, name: impl Into<String>, fields: IndexMap<String, JinjaType>) {
        self.classes.insert(name.into(), fields);
    }

    /// Add an enum definition.
    pub fn add_enum(&mut self, name: impl Into<String>, values: Vec<String>) {
        self.enums.insert(name.into(), values);
    }

    /// Look up a variable's type.
    pub fn get_variable(&self, name: &str) -> Option<&JinjaType> {
        self.variables.get(name)
    }

    /// Resolve a variable by name (used in expression inference).
    /// Searches through scopes from innermost to outermost.
    pub fn resolve_variable(&self, name: &str) -> Option<JinjaType> {
        // Search scopes from innermost to outermost
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        // Fall back to base variables
        self.variables.get(name).cloned()
    }

    /// Get all variable names (for error suggestions).
    pub fn variable_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.variables.keys().cloned().collect();
        for scope in &self.scopes {
            names.extend(scope.keys().cloned());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Add a function signature.
    pub fn add_function(
        &mut self,
        name: impl Into<String>,
        return_type: JinjaType,
        params: Vec<(String, JinjaType)>,
    ) {
        self.functions.insert(name.into(), (return_type, params));
    }

    /// Check if a name is a function.
    pub fn is_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get function signature (return type and parameters).
    pub fn get_function(&self, name: &str) -> Option<&(JinjaType, Vec<(String, JinjaType)>)> {
        self.functions.get(name)
    }

    /// Look up a class's fields.
    pub fn get_class(&self, name: &str) -> Option<&IndexMap<String, JinjaType>> {
        self.classes.get(name)
    }

    /// Get a property type from a class.
    pub fn get_class_property(&self, class_name: &str, property: &str) -> Option<JinjaType> {
        self.classes
            .get(class_name)
            .and_then(|fields| fields.get(property).cloned())
    }

    /// Look up an enum's values.
    pub fn get_enum(&self, name: &str) -> Option<&Vec<String>> {
        self.enums.get(name)
    }

    /// Get an enum value by name.
    pub fn get_enum_value(&self, enum_name: &str, value: &str) -> Option<String> {
        self.enums
            .get(enum_name)
            .and_then(|values| values.iter().find(|v| *v == value).cloned())
    }
}

// ============================================================================
// Type Errors
// ============================================================================

/// Type error found during Jinja template analysis.
///
/// This enum captures the structured error data without rendering messages.
/// Message rendering is the responsibility of `baml_compiler_diagnostics`.
#[derive(Debug, Clone)]
pub enum TypeError {
    /// Variable referenced does not exist.
    UnresolvedVariable {
        name: String,
        suggestions: Vec<String>,
        span: minijinja::machinery::Span,
    },
    /// Function referenced without calling it (missing parentheses).
    FunctionReferenceWithoutCall {
        function_name: String,
        span: minijinja::machinery::Span,
    },
    /// Unknown Jinja filter.
    InvalidFilter {
        filter_name: String,
        suggestions: Vec<String>,
        span: minijinja::machinery::Span,
    },
    /// Type mismatch in expression.
    InvalidType {
        expression: String,
        expected: String,
        found: String,
        span: minijinja::machinery::Span,
    },
    /// Property access on class that doesn't have the property.
    PropertyNotDefined {
        variable: String,
        class_name: String,
        property: String,
        span: minijinja::machinery::Span,
    },
    /// Property access on enum value (not allowed).
    EnumValuePropertyAccess {
        variable: String,
        enum_value: String,
        property: String,
        span: minijinja::machinery::Span,
    },
    /// Comparing enum to string (deprecated).
    EnumStringComparison {
        enum_name: String,
        span: minijinja::machinery::Span,
    },
    /// Property access on union where some members don't have the property.
    PropertyNotFoundInUnion {
        property: String,
        missing_on: Vec<String>,
        span: minijinja::machinery::Span,
    },
    /// Property has inconsistent types across union members.
    PropertyTypeMismatchInUnion {
        property: String,
        span: minijinja::machinery::Span,
    },
    /// Union contains non-class type when accessing property.
    NonClassInUnion {
        variable: String,
        property: String,
        non_class_type: String,
        span: minijinja::machinery::Span,
    },
    /// Wrong number of arguments in function call.
    WrongArgCount {
        function_name: String,
        expected: usize,
        found: usize,
        span: minijinja::machinery::Span,
    },
    /// Missing required argument in function call.
    MissingArg {
        function_name: String,
        arg_name: String,
        span: minijinja::machinery::Span,
    },
    /// Unknown argument name in function call.
    UnknownArg {
        function_name: String,
        arg_name: String,
        suggestions: Vec<String>,
        span: minijinja::machinery::Span,
    },
    /// Wrong argument type in function call.
    WrongArgType {
        function_name: String,
        arg_name: String,
        expected: String,
        found: String,
        span: minijinja::machinery::Span,
    },
    /// Unsupported Jinja feature.
    UnsupportedFeature {
        feature: String,
        span: minijinja::machinery::Span,
    },
    /// Invalid Jinja syntax (e.g., invalid for loop target).
    InvalidSyntax {
        message: String,
        span: minijinja::machinery::Span,
    },
    /// Unknown Jinja test (e.g., `x is foo`).
    InvalidTest {
        test_name: String,
        suggestions: Vec<String>,
        span: minijinja::machinery::Span,
    },
}

impl TypeError {
    /// Get the span where this error occurred.
    pub fn span(&self) -> minijinja::machinery::Span {
        match self {
            TypeError::UnresolvedVariable { span, .. }
            | TypeError::FunctionReferenceWithoutCall { span, .. }
            | TypeError::InvalidFilter { span, .. }
            | TypeError::InvalidType { span, .. }
            | TypeError::PropertyNotDefined { span, .. }
            | TypeError::EnumValuePropertyAccess { span, .. }
            | TypeError::EnumStringComparison { span, .. }
            | TypeError::PropertyNotFoundInUnion { span, .. }
            | TypeError::PropertyTypeMismatchInUnion { span, .. }
            | TypeError::NonClassInUnion { span, .. }
            | TypeError::WrongArgCount { span, .. }
            | TypeError::MissingArg { span, .. }
            | TypeError::UnknownArg { span, .. }
            | TypeError::WrongArgType { span, .. }
            | TypeError::UnsupportedFeature { span, .. }
            | TypeError::InvalidSyntax { span, .. }
            | TypeError::InvalidTest { span, .. } => *span,
        }
    }

    // Error constructors

    pub fn unresolved_variable(
        name: &str,
        span: minijinja::machinery::Span,
        available: &[String],
    ) -> Self {
        let suggestions = find_close_matches(name, available, 3);
        TypeError::UnresolvedVariable {
            name: name.to_string(),
            suggestions,
            span,
        }
    }

    pub fn function_reference_without_call(name: &str, span: minijinja::machinery::Span) -> Self {
        TypeError::FunctionReferenceWithoutCall {
            function_name: name.to_string(),
            span,
        }
    }

    pub fn invalid_filter(
        name: &str,
        span: minijinja::machinery::Span,
        valid_filters: &[&str],
    ) -> Self {
        let suggestions = find_close_matches(
            name,
            &valid_filters
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            5,
        );
        TypeError::InvalidFilter {
            filter_name: name.to_string(),
            suggestions,
            span,
        }
    }

    pub fn invalid_type(
        expr: &minijinja::machinery::ast::Expr,
        got: &JinjaType,
        expected: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        let found = if matches!(got, JinjaType::Undefined) {
            "undefined".to_string()
        } else {
            got.name()
        };
        TypeError::InvalidType {
            expression: pretty_print_expr(expr),
            expected: expected.to_string(),
            found,
            span,
        }
    }

    pub fn property_not_defined(
        variable: &str,
        class: &str,
        property: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::PropertyNotDefined {
            variable: variable.to_string(),
            class_name: class.to_string(),
            property: property.to_string(),
            span,
        }
    }

    pub fn enum_value_property_error(
        variable: &str,
        enum_value: &str,
        property: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::EnumValuePropertyAccess {
            variable: variable.to_string(),
            enum_value: enum_value.to_string(),
            property: property.to_string(),
            span,
        }
    }

    pub fn enum_string_comparison_deprecated(
        _expr: &minijinja::machinery::ast::Expr,
        enum_name: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::EnumStringComparison {
            enum_name: enum_name.to_string(),
            span,
        }
    }

    pub fn property_not_found_in_union(
        _variable: &str,
        property: &str,
        missing_on_classes: &[&str],
        _union_name: Option<&str>,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::PropertyNotFoundInUnion {
            property: property.to_string(),
            missing_on: missing_on_classes
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            span,
        }
    }

    pub fn property_type_mismatch_in_union(
        _variable: &str,
        property: &str,
        _union_name: Option<&str>,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::PropertyTypeMismatchInUnion {
            property: property.to_string(),
            span,
        }
    }

    pub fn non_class_in_union(
        variable: &str,
        property: &str,
        non_class_type: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        TypeError::NonClassInUnion {
            variable: variable.to_string(),
            property: property.to_string(),
            non_class_type: non_class_type.to_string(),
            span,
        }
    }

    pub fn wrong_arg_count(
        func: &str,
        span: minijinja::machinery::Span,
        expected: usize,
        got: usize,
    ) -> Self {
        TypeError::WrongArgCount {
            function_name: func.to_string(),
            expected,
            found: got,
            span,
        }
    }

    pub fn missing_arg(func: &str, span: minijinja::machinery::Span, name: &str) -> Self {
        TypeError::MissingArg {
            function_name: func.to_string(),
            arg_name: name.to_string(),
            span,
        }
    }

    pub fn unknown_arg(
        func: &str,
        span: minijinja::machinery::Span,
        name: &str,
        valid_args: std::collections::HashSet<&String>,
    ) -> Self {
        let names: Vec<_> = valid_args.into_iter().collect();
        let suggestions = find_close_matches(
            name,
            &names.iter().map(|s| (*s).clone()).collect::<Vec<_>>(),
            3,
        );
        TypeError::UnknownArg {
            function_name: func.to_string(),
            arg_name: name.to_string(),
            suggestions,
            span,
        }
    }

    pub fn wrong_arg_type(
        func: &str,
        span: minijinja::machinery::Span,
        name: &str,
        expected: &JinjaType,
        got: &JinjaType,
    ) -> Self {
        TypeError::WrongArgType {
            function_name: func.to_string(),
            arg_name: name.to_string(),
            expected: expected.name(),
            found: got.name(),
            span,
        }
    }

    pub fn unsupported_feature(feature: &str, span: minijinja::machinery::Span) -> Self {
        TypeError::UnsupportedFeature {
            feature: feature.to_string(),
            span,
        }
    }

    pub fn invalid_syntax(message: &str, span: minijinja::machinery::Span) -> Self {
        TypeError::InvalidSyntax {
            message: message.to_string(),
            span,
        }
    }

    pub fn invalid_test(
        name: &str,
        span: minijinja::machinery::Span,
        valid_tests: &[&str],
    ) -> Self {
        let valid_as_strings: Vec<String> = valid_tests.iter().map(|s| (*s).to_string()).collect();
        let suggestions = find_close_matches(name, &valid_as_strings, 3);
        TypeError::InvalidTest {
            test_name: name.to_string(),
            suggestions,
            span,
        }
    }
}

/// Find close string matches using edit distance.
fn find_close_matches(target: &str, options: &[String], max_results: usize) -> Vec<String> {
    const THRESHOLD: usize = 20;

    let mut distances: Vec<_> = options
        .iter()
        .map(|opt| {
            let dist = strsim::osa_distance(&opt.to_lowercase(), &target.to_lowercase());
            (dist, opt.clone())
        })
        .collect();

    distances.sort_by_key(|(dist, _)| *dist);

    distances
        .into_iter()
        .filter(|(dist, _)| *dist <= THRESHOLD)
        .take(max_results)
        .map(|(_, opt)| opt)
        .collect()
}

/// Pretty-print an expression for error messages (simplified).
fn pretty_print_expr(expr: &minijinja::machinery::ast::Expr) -> String {
    use minijinja::machinery::ast::Expr;
    match expr {
        Expr::Var(v) => v.id.to_string(),
        Expr::Const(c) => c.value.to_string(),
        Expr::GetAttr(attr) => format!("{}.{}", pretty_print_expr(&attr.expr), attr.name),
        _ => "...".to_string(),
    }
}

// ============================================================================
// Validation
// ============================================================================

/// Validate a Jinja template with type checking.
///
/// Returns a list of type errors found in the template.
pub fn validate_template(
    template_text: &str,
    env: &mut JinjaTypeEnv,
) -> Result<Vec<TypeError>, minijinja::Error> {
    // Parse the template using minijinja
    let ast = minijinja::machinery::parse(
        template_text,
        "prompt",
        SyntaxConfig,
        WhitespaceConfig::default(),
    )?;

    // Walk the statement tree and collect type errors
    let errors = validate_statement(&ast, env);

    Ok(errors)
}

/// Validate a single Jinja expression.
///
/// Returns the inferred type and any type errors found.
pub fn validate_expression(
    expr_text: &str,
    env: &JinjaTypeEnv,
) -> Result<(JinjaType, Vec<TypeError>), minijinja::Error> {
    // Parse the expression using minijinja
    let ast = minijinja::machinery::parse_expr(expr_text)?;

    // Infer the type and collect errors
    match infer_expression_type(&ast, env) {
        Ok(ty) => Ok((ty, Vec::new())),
        Err(errors) => Ok((JinjaType::Unknown, errors)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jinja_type_subtyping() {
        assert!(JinjaType::Int.is_subtype_of(&JinjaType::Number));
        assert!(JinjaType::Float.is_subtype_of(&JinjaType::Number));
        assert!(JinjaType::Unknown.is_subtype_of(&JinjaType::Int));
        assert!(JinjaType::Int.is_subtype_of(&JinjaType::Unknown));
    }

    #[test]
    fn test_jinja_union_subtyping() {
        let union = JinjaType::Union(vec![JinjaType::Int, JinjaType::String]);
        assert!(JinjaType::Int.is_subtype_of(&union));
        assert!(JinjaType::String.is_subtype_of(&union));
        assert!(!JinjaType::Bool.is_subtype_of(&union));
    }

    #[test]
    fn test_alias_subtyping() {
        // Alias resolving to a union: type Pet = Cat | Dog
        let pet_alias = JinjaType::Alias {
            name: "Pet".to_string(),
            resolved: Box::new(JinjaType::Union(vec![
                JinjaType::ClassRef("Cat".to_string()),
                JinjaType::ClassRef("Dog".to_string()),
            ])),
        };

        // ClassRef("Cat") should be a subtype of Alias("Pet" -> Cat | Dog)
        assert!(JinjaType::ClassRef("Cat".to_string()).is_subtype_of(&pet_alias));
        assert!(JinjaType::ClassRef("Dog".to_string()).is_subtype_of(&pet_alias));
        assert!(!JinjaType::String.is_subtype_of(&pet_alias));

        // Alias on the left side: Pet should be subtype of Union(Cat, Dog)
        let union = JinjaType::Union(vec![
            JinjaType::ClassRef("Cat".to_string()),
            JinjaType::ClassRef("Dog".to_string()),
        ]);
        assert!(pet_alias.is_subtype_of(&union));

        // Alias resolving to a simple type
        let name_alias = JinjaType::Alias {
            name: "Name".to_string(),
            resolved: Box::new(JinjaType::String),
        };
        assert!(JinjaType::String.is_subtype_of(&name_alias));
        assert!(!JinjaType::Int.is_subtype_of(&name_alias));
        assert!(name_alias.is_subtype_of(&JinjaType::String));
    }

    #[test]
    fn test_type_env_basics() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("name", JinjaType::String);
        env.add_variable("age", JinjaType::Int);

        assert_eq!(env.get_variable("name"), Some(&JinjaType::String));
        assert_eq!(env.get_variable("age"), Some(&JinjaType::Int));
        assert_eq!(env.get_variable("unknown"), None);
    }
}
