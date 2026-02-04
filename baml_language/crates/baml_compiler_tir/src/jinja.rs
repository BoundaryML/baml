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

use std::collections::HashMap;

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

        // Check union types
        if let JinjaType::Union(items) = other {
            return items.iter().any(|item| self.is_subtype_of(item));
        }

        match (self, other) {
            // Undefined and None are only subtypes of themselves
            (JinjaType::Undefined | JinjaType::None, _) => false,
            (_, JinjaType::Undefined | JinjaType::None) => false,

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
            Ty::Unknown => JinjaType::Unknown,
            Ty::Null => JinjaType::None,
            Ty::Int => JinjaType::Int,
            Ty::Float => JinjaType::Float,
            Ty::String => JinjaType::String,
            Ty::Bool => JinjaType::Bool,
            Ty::List(elem) => JinjaType::List(Box::new(JinjaType::from(elem.as_ref()))),
            Ty::Map { key, value } => JinjaType::Map(
                Box::new(JinjaType::from(key.as_ref())),
                Box::new(JinjaType::from(value.as_ref())),
            ),
            Ty::Union(items) => JinjaType::Union(items.iter().map(JinjaType::from).collect()),
            Ty::Optional(inner) => {
                JinjaType::Union(vec![JinjaType::None, JinjaType::from(inner.as_ref())])
            }
            Ty::Class(name) => JinjaType::ClassRef(name.to_string()),
            Ty::Enum(name) => JinjaType::EnumRef(name.to_string()),
            Ty::Media(_) => JinjaType::Image, // Simplification
            _ => JinjaType::Unknown,
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
#[derive(Debug, Clone)]
pub struct TypeError {
    message: String,
    span: minijinja::machinery::Span,
}

impl TypeError {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn span(&self) -> minijinja::machinery::Span {
        self.span
    }

    // Error constructors

    pub fn unresolved_variable(
        name: &str,
        span: minijinja::machinery::Span,
        available: &[String],
    ) -> Self {
        let suggestions = find_close_matches(name, available, 3);
        let message = if suggestions.is_empty() {
            format!("Variable `{name}` does not exist.")
        } else if suggestions.len() == 1 {
            format!(
                "Variable `{name}` does not exist. Did you mean `{}`?",
                suggestions[0]
            )
        } else {
            format!(
                "Variable `{name}` does not exist. Did you mean one of these: `{}`?",
                suggestions.join("`, `")
            )
        };
        Self { message, span }
    }

    pub fn function_reference_without_call(name: &str, span: minijinja::machinery::Span) -> Self {
        Self {
            message: format!(
                "Function '{name}' referenced without parentheses. Did you mean '{name}()'?"
            ),
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
        let message = if suggestions.is_empty() {
            format!("Filter '{name}' does not exist")
        } else if suggestions.len() == 1 {
            format!(
                "Filter '{name}' does not exist. Did you mean '{}'?",
                suggestions[0]
            )
        } else {
            format!(
                "Filter '{name}' does not exist. Did you mean one of these: '{}'?",
                suggestions.join("', '")
            )
        };
        Self {
            message: format!(
                "{message}\n\nSee: https://docs.rs/minijinja/latest/minijinja/filters/index.html#functions"
            ),
            span,
        }
    }

    pub fn invalid_type(
        expr: &minijinja::machinery::ast::Expr,
        got: &JinjaType,
        expected: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!(
                "'{}' is {}, expected {}",
                pretty_print_expr(expr),
                if matches!(got, JinjaType::Undefined) {
                    "undefined".to_string()
                } else {
                    format!("a {}", got.name())
                },
                expected
            ),
            span,
        }
    }

    pub fn property_not_defined(
        variable: &str,
        class: &str,
        property: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!("class {class} ({variable}) does not have a property '{property}'"),
            span,
        }
    }

    pub fn enum_value_property_error(
        variable: &str,
        enum_value: &str,
        property: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!(
                "enum value {enum_value} ({variable}) does not have a property '{property}'"
            ),
            span,
        }
    }

    pub fn enum_string_comparison_deprecated(
        _expr: &minijinja::machinery::ast::Expr,
        enum_name: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!(
                "Comparing enum {enum_name} to string - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339."
            ),
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
        let classes_str = missing_on_classes.join(", ");
        Self {
            message: format!("property '{property}' does not exist on {classes_str}"),
            span,
        }
    }

    pub fn property_type_mismatch_in_union(
        _variable: &str,
        property: &str,
        _union_name: Option<&str>,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!("property '{property}' has inconsistent types across union members"),
            span,
        }
    }

    pub fn non_class_in_union(
        variable: &str,
        property: &str,
        non_class_type: &str,
        span: minijinja::machinery::Span,
    ) -> Self {
        Self {
            message: format!(
                "cannot access property '{property}' on '{variable}': union contains non-class type {non_class_type}"
            ),
            span,
        }
    }

    pub fn wrong_arg_count(
        func: &str,
        span: minijinja::machinery::Span,
        expected: usize,
        got: usize,
    ) -> Self {
        Self {
            message: format!("Function '{func}' expects {expected} arguments, but got {got}"),
            span,
        }
    }

    pub fn missing_arg(func: &str, span: minijinja::machinery::Span, name: &str) -> Self {
        Self {
            message: format!("Function '{func}' expects argument '{name}'"),
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

        let message = if suggestions.is_empty() {
            format!("Function '{func}' does not have an argument '{name}'")
        } else if suggestions.len() == 1 {
            format!(
                "Function '{func}' does not have an argument '{name}'. Did you mean '{}'?",
                suggestions[0]
            )
        } else {
            format!(
                "Function '{func}' does not have an argument '{name}'. Did you mean one of these: '{}'?",
                suggestions.join("', '")
            )
        };

        Self { message, span }
    }

    pub fn wrong_arg_type(
        func: &str,
        span: minijinja::machinery::Span,
        name: &str,
        expected: &JinjaType,
        got: &JinjaType,
    ) -> Self {
        Self {
            message: format!(
                "Function '{func}' expects argument '{name}' to be of type {}, but got {}",
                expected.name(),
                got.name()
            ),
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
    fn test_type_env_basics() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("name", JinjaType::String);
        env.add_variable("age", JinjaType::Int);

        assert_eq!(env.get_variable("name"), Some(&JinjaType::String));
        assert_eq!(env.get_variable("age"), Some(&JinjaType::Int));
        assert_eq!(env.get_variable("unknown"), None);
    }
}
