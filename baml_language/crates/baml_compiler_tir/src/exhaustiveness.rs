//! Value-based exhaustiveness checking for match expressions.
//!
//! # Design Philosophy
//!
//! Pattern matching fundamentally operates on **values**, not types.
//! A pattern like `Status.Active` matches one specific value, while
//! `s: Status` matches all values of type Status.
//!
//! This module uses `ValueSet` to represent what values a pattern covers,
//! cleanly separating the concept of "value coverage" from "type membership".
//!
//! ## Key Concepts
//!
//! - **`ValueSet`**: Represents a set of runtime values a pattern can match
//! - **Finite types**: Enums and booleans have enumerable value sets
//! - **Infinite types**: int, string, classes have infinite value sets
//!   (only exhaustive via catch-all or type pattern)
//!
//! ## Example
//!
//! ```baml
//! enum Status { Active, Inactive, Pending }
//!
//! match (s) {
//!   Status.Active => ...    // ValueSet::EnumVariant("Status", "Active")
//!   Status.Inactive => ...  // ValueSet::EnumVariant("Status", "Inactive")
//!   Status.Pending => ...   // ValueSet::EnumVariant("Status", "Pending")
//! }
//! ```
//!
//! Each arm covers a single value. Together they cover all values of type Status.

use std::collections::{HashMap, HashSet};

use baml_base::{Name, Span};
use baml_compiler_hir::{ExprBody, Literal, MatchArmId, Pattern};

use crate::{LiteralValue, Ty, lower::lower_type_ref};

// ============================================================================
// ValueSet: The Core Abstraction
// ============================================================================

/// Represents a set of runtime values that a pattern can match.
///
/// This is the core abstraction for exhaustiveness checking. Unlike types
/// (which describe what values CAN exist), `ValueSet` describes what values
/// a pattern WILL match at runtime.
///
/// # Conceptual Model
///
/// ```text
/// Pattern              -> ValueSet
/// ─────────────────────────────────────
/// `_` or `other`       -> All (everything)
/// `s: Success`         -> OfType("Success")
/// `Status.Active`      -> EnumVariant("Status", "Active")
/// `42`                 -> Literal(Int(42))
/// `200 | 201`          -> Union([Literal(200), Literal(201)])
/// `x: int if x > 0`    -> Empty (guards don't guarantee coverage)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ValueSet {
    /// Matches ALL possible values.
    ///
    /// This is the catch-all case: patterns like `_`, `other`, or any
    /// untyped binding. It covers everything remaining.
    All,

    /// Matches all values of a named type.
    ///
    /// For **finite types** (enums, bool), this can be expanded into
    /// the specific values. For **infinite types** (int, string, classes),
    /// this represents an abstract "all instances of T".
    ///
    /// # Examples
    /// - `s: Success` -> `OfType("Success")` (infinite: all Success instances)
    /// - `b: bool` -> `OfType("bool")` -> expands to `[true, false]`
    /// - `s: Status` -> `OfType("Status")` -> expands to variants
    OfType(Name),

    /// Matches exactly one enum variant value.
    ///
    /// # Example
    /// `Status.Active` matches only the value `Status.Active`, not
    /// `Status.Inactive` or any other value.
    EnumVariant { enum_name: Name, variant_name: Name },

    /// Matches exactly one literal value.
    ///
    /// # Examples
    /// - `42` -> `Literal(Int(42))`
    /// - `"hello"` -> `Literal(String("hello"))`
    /// - `true` -> `Literal(Bool(true))`
    /// - `null` -> `Literal(Null)`
    Literal(Literal),

    /// Matches values in ANY of the sub-sets (union/disjunction).
    ///
    /// # Example
    /// `200 | 201 | 204` -> `Union([Literal(200), Literal(201), Literal(204)])`
    ///
    /// # Note on `OfType` in Unions
    /// This variant CAN contain multiple `OfType` values with different types.
    /// This occurs when a typed binding has a union type, e.g.:
    /// - `x: Success | Failure` creates `Union([OfType("Success"), OfType("Failure")])`
    ///
    /// This is intentional and correct. The grammar prevents mixed-type pattern
    /// unions like `x: int | y: bool` because `:` binds tighter than `|`, so
    /// `x: int | bool` parses as `x: (int | bool)`. See BEP-002 "Multiple Patterns
    /// Per Arm" for details.
    Union(Vec<ValueSet>),

    /// Matches NO values.
    ///
    /// Used for guarded patterns, which don't contribute to exhaustiveness.
    /// A pattern `x: int if x > 0` might not match `x = -1`, so it can't
    /// guarantee coverage of the int type.
    Empty,
}

impl std::fmt::Display for ValueSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueSet::All => write!(f, "_"),
            ValueSet::OfType(name) => write!(f, "{name}"),
            ValueSet::EnumVariant {
                enum_name,
                variant_name,
            } => write!(f, "{enum_name}.{variant_name}"),
            ValueSet::Literal(lit) => match lit {
                Literal::Int(v) => write!(f, "{v}"),
                Literal::Float(v) => write!(f, "{v}"),
                Literal::String(v) => write!(f, "\"{v}\""),
                Literal::Bool(v) => write!(f, "{v}"),
                Literal::Null => write!(f, "null"),
            },
            ValueSet::Union(sets) => {
                let parts: Vec<String> =
                    sets.iter().map(std::string::ToString::to_string).collect();
                write!(f, "{}", parts.join(" | "))
            }
            ValueSet::Empty => write!(f, "∅"),
        }
    }
}

impl From<&LiteralValue> for ValueSet {
    fn from(value: &LiteralValue) -> Self {
        match value {
            LiteralValue::Int(v) => ValueSet::Literal(Literal::Int(*v)),
            LiteralValue::Float(v) => ValueSet::Literal(Literal::Float(v.clone())),
            LiteralValue::String(v) => ValueSet::Literal(Literal::String(v.clone())),
            LiteralValue::Bool(v) => ValueSet::Literal(Literal::Bool(*v)),
        }
    }
}

// ============================================================================
// ExhaustivenessChecker: The Algorithm
// ============================================================================

/// Checker for match expression exhaustiveness.
///
/// This struct holds the context needed to expand types into their
/// constituent values and check coverage.
pub(crate) struct ExhaustivenessChecker<'a> {
    /// Enum definitions: `enum_name` -> [`variant_names`]
    enum_variants: &'a HashMap<Name, Vec<Name>>,

    /// Type alias definitions: `alias_name` -> `underlying_type`
    type_aliases: &'a HashMap<Name, Ty>,

    /// Class names for type resolution
    class_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,

    /// Enum names for type resolution
    enum_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,

    /// Type alias names for validation
    type_alias_names: &'a HashSet<Name>,
}

/// Result of exhaustiveness checking.
#[derive(Debug)]
pub(crate) struct ExhaustivenessResult {
    /// Whether all cases are covered
    pub(crate) is_exhaustive: bool,

    /// Value sets that are not covered (empty if exhaustive)
    pub(crate) uncovered: Vec<ValueSet>,

    /// Indices (0-based) into the `arms` slice of unreachable arms (arms that can never match).
    pub(crate) unreachable_arms: Vec<usize>,
}

impl<'a> ExhaustivenessChecker<'a> {
    /// Create a new exhaustiveness checker.
    pub(crate) fn new(
        enum_variants: &'a HashMap<Name, Vec<Name>>,
        type_aliases: &'a HashMap<Name, Ty>,
        class_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
        enum_names: &'a HashMap<Name, baml_compiler_hir::QualifiedName>,
        type_alias_names: &'a HashSet<Name>,
    ) -> Self {
        Self {
            enum_variants,
            type_aliases,
            class_names,
            enum_names,
            type_alias_names,
        }
    }

    /// Check exhaustiveness of a match expression.
    ///
    /// # Arguments
    /// - `scrutinee_ty`: The type of the value being matched
    /// - `arm_ids`: The match arm IDs to check
    /// - `body`: The expression body (for pattern and arm lookup)
    ///
    /// # Returns
    /// An `ExhaustivenessResult` with coverage info and any issues found.
    pub(crate) fn check(
        &self,
        scrutinee_ty: &Ty,
        arm_ids: &[MatchArmId],
        body: &ExprBody,
    ) -> ExhaustivenessResult {
        // Expand the scrutinee type into the value sets that need to be covered
        let required = self.expand_type_to_values(scrutinee_ty);

        // Track what's been covered and which arms are unreachable
        let mut covered: Vec<ValueSet> = Vec::new();
        let mut has_catch_all = false;
        let mut unreachable_arms: Vec<usize> = Vec::new();

        for (arm_idx, arm_id) in arm_ids.iter().enumerate() {
            let arm = &body.match_arms[*arm_id];
            let pattern = &body.patterns[arm.pattern];
            let has_guard = arm.guard.is_some();
            let value_set = self.pattern_to_value_set(pattern, has_guard, body);

            // Check if this arm is unreachable
            if has_catch_all {
                // After a catch-all, everything is unreachable
                unreachable_arms.push(arm_idx);
                continue;
            }

            // Check if this arm's values are already fully covered
            if !has_guard && Self::is_fully_covered(&value_set, &covered, &required) {
                unreachable_arms.push(arm_idx);
                // Don't skip - we still add to coverage for accurate error messages
            }

            // Update coverage
            if !has_guard {
                match &value_set {
                    ValueSet::All => {
                        has_catch_all = true;
                    }
                    ValueSet::Empty => {
                        // Guarded patterns don't contribute
                    }
                    _ => {
                        self.add_coverage(&mut covered, &value_set);
                    }
                }
            }
        }

        // Find uncovered cases
        let uncovered = if has_catch_all {
            Vec::new()
        } else {
            Self::find_uncovered(&required, &covered)
        };

        ExhaustivenessResult {
            is_exhaustive: uncovered.is_empty(),
            uncovered,
            unreachable_arms,
        }
    }

    // ========================================================================
    // Type -> ValueSet Expansion
    // ========================================================================

    /// Expand a type into the value sets that need to be covered.
    fn expand_type_to_values(&self, ty: &Ty) -> Vec<ValueSet> {
        self.expand_ty(ty, &mut HashSet::new())
    }

    /// Recursive core of type expansion with a cycle guard.
    ///
    /// `visiting` tracks which aliases are on the current expansion stack.
    /// A back-reference means the alias is recursive: we stop there and treat
    /// the alias as an opaque infinite type (`OfType`), which is the correct
    /// semantic base case — recursive aliases are structurally unbounded and
    /// cannot be enumerated. Invalid (non-structural) cycles are already
    /// reported as errors by `validate_type_alias_cycles` before this runs.
    ///
    /// # TODO(type-alias-architecture)
    /// Alias resolution should be a dedicated compiler phase (Salsa query)
    /// that runs once after name resolution, making pre-resolved aliases
    /// available to all consumers (codegen, prompt rendering, exhaustiveness)
    /// without per-call re-traversal.
    fn expand_ty(&self, ty: &Ty, visiting: &mut HashSet<Name>) -> Vec<ValueSet> {
        match ty {
            Ty::Union(members) => members
                .iter()
                .flat_map(|m| self.expand_ty(m, visiting))
                .collect(),

            Ty::Optional(inner) => {
                let mut values = self.expand_ty(inner, visiting);
                // Deduplicate null (handles T?? = T? flattening)
                let null = ValueSet::Literal(Literal::Null);
                if !values.contains(&null) {
                    values.push(null);
                }
                values
            }

            Ty::TypeAlias(fqn) => {
                // Use display_name() consistently with Ty::Enum and Ty::Class.
                // For local type aliases (the only kind that exist), display_name() == fqn.name,
                // so the type_aliases map lookup is always correct.
                let name = fqn.display_name();
                if visiting.contains(&name) {
                    // Cycle detected: treat as opaque to prevent infinite recursion.
                    return vec![ValueSet::OfType(name)];
                }
                visiting.insert(name.clone());
                let result = if let Some(alias_ty) = self.type_aliases.get(&name) {
                    self.expand_ty(alias_ty, visiting)
                } else {
                    vec![ValueSet::OfType(name.clone())]
                };
                visiting.remove(&name);
                result
            }

            // Bool is finite: {true, false}
            Ty::Bool => vec![
                ValueSet::Literal(Literal::Bool(true)),
                ValueSet::Literal(Literal::Bool(false)),
            ],

            // Singleton types
            Ty::Null => vec![ValueSet::Literal(Literal::Null)],
            Ty::Literal(value) => vec![ValueSet::from(value)],

            // Infinite primitive types
            Ty::Int => vec![ValueSet::OfType(Name::new("int"))],
            Ty::Float => vec![ValueSet::OfType(Name::new("float"))],
            Ty::String => vec![ValueSet::OfType(Name::new("string"))],
            Ty::Resource => vec![ValueSet::OfType(Name::new("resource"))],
            Ty::Type => vec![ValueSet::OfType(Name::new("type"))],
            Ty::Media(kind) => vec![ValueSet::OfType(Name::new(kind.to_string()))],

            // Classes are opaque infinite types for exhaustiveness purposes.
            Ty::Class(fqn) => vec![ValueSet::OfType(fqn.display_name())],

            // Enums are finite: expand to their variants.
            // Uses display_name (FQN for builtins, short name for locals).
            Ty::Enum(fqn) => {
                let display = fqn.display_name();
                match self.enum_variants.get(&display) {
                    Some(variants) => variants
                        .iter()
                        .map(|variant_name| ValueSet::EnumVariant {
                            enum_name: display.clone(),
                            variant_name: variant_name.clone(),
                        })
                        .collect(),
                    None => vec![ValueSet::OfType(display)],
                }
            }

            // Composite types: opaque (element type embedded for disambiguation)
            Ty::List(inner) => vec![ValueSet::OfType(Name::new(format!("{inner}[]")))],
            Ty::Map { .. } => vec![ValueSet::OfType(Name::new("<map>"))],

            // Special / error types produce no required value sets
            Ty::Unknown | Ty::Error | Ty::Void | Ty::BuiltinUnknown => vec![],
            Ty::Function { .. } => vec![ValueSet::OfType(Name::new("<function>"))],
            Ty::WatchAccessor(_) => vec![ValueSet::OfType(Name::new("<$watch>"))],
        }
    }

    // ========================================================================
    // Pattern -> ValueSet Conversion
    // ========================================================================

    /// Convert a pattern to the value set it matches.
    fn pattern_to_value_set(
        &self,
        pattern: &Pattern,
        has_guard: bool,
        body: &ExprBody,
    ) -> ValueSet {
        // Guards prevent patterns from contributing to exhaustiveness
        if has_guard {
            return ValueSet::Empty;
        }

        match pattern {
            // Catch-all: matches everything
            Pattern::Binding(_) => ValueSet::All,

            // Typed binding: matches all values of that type
            Pattern::TypedBinding { ty, .. } => {
                let (lowered_ty, _) = lower_type_ref(
                    ty,
                    self.type_alias_names,
                    self.class_names,
                    self.enum_names,
                    Span::default(),
                );
                self.ty_to_value_set(&lowered_ty)
            }

            // Literal: matches exactly that value
            Pattern::Literal(lit) => ValueSet::Literal(lit.clone()),

            // Enum variant: matches exactly that variant
            Pattern::EnumVariant { enum_name, variant } => ValueSet::EnumVariant {
                enum_name: enum_name.clone(),
                variant_name: variant.clone(),
            },

            // Union: matches any of the sub-patterns
            Pattern::Union(sub_pats) => {
                let sub_sets: Vec<ValueSet> = sub_pats
                    .iter()
                    .map(|pat_id| {
                        let sub_pattern = &body.patterns[*pat_id];
                        self.pattern_to_value_set(sub_pattern, false, body)
                    })
                    .collect();

                values_into_value_set(sub_sets)
            }
        }
    }

    /// Convert a type to a single `ValueSet` covering all its values.
    ///
    /// Delegates to `expand_ty` so alias resolution, enum expansion, and the
    /// cycle guard are shared with the scrutinee path. A single-element result
    /// is returned as-is; multiple elements become a `Union`; zero (error/void)
    /// becomes `Empty`.
    fn ty_to_value_set(&self, ty: &Ty) -> ValueSet {
        values_into_value_set(self.expand_ty(ty, &mut HashSet::new()))
    }

    // ========================================================================
    // Coverage Checking
    // ========================================================================

    /// Check if a value set is fully covered by existing coverage.
    /// `value_set`: a particular branch's `ValueSet`.
    /// covered: `ValueSet`s covered by earlier branches.
    /// required: `ValueSet`s that need coverage, derived from the
    ///           top-level match scrutinee.
    fn is_fully_covered(value_set: &ValueSet, covered: &[ValueSet], required: &[ValueSet]) -> bool {
        is_value_set_covered(value_set, covered, required)
    }

    /// Add a value set to the coverage list.
    fn add_coverage(&self, covered: &mut Vec<ValueSet>, value_set: &ValueSet) {
        add_to_coverage(covered, value_set, self.enum_variants);
    }

    /// Find value sets that are both required and not covered.
    fn find_uncovered(required: &[ValueSet], covered: &[ValueSet]) -> Vec<ValueSet> {
        required
            .iter()
            .filter(|req| !Self::is_fully_covered(req, covered, required))
            .cloned()
            .collect()
    }
}

// ============================================================================
// Shared Coverage Functions
// ============================================================================

/// Collapse a `Vec<ValueSet>` into a single `ValueSet`.
///
/// - Empty  → `Empty` (error/void/unreachable types produce no values)
/// - Single → the element itself (no unnecessary wrapping)
/// - Many   → `Union` (flat, since `expand_ty` never nests unions)
///
/// `Empty` entries are stripped before the union is built: a guarded or
/// void-typed sub-pattern contributes nothing, so including it in a
/// `Union` would be semantically misleading and would leave a dead entry
/// in the coverage list via `add_to_coverage`'s catch-all arm.
fn values_into_value_set(values: Vec<ValueSet>) -> ValueSet {
    let mut iter = values.into_iter().filter(|v| v != &ValueSet::Empty);
    match (iter.next(), iter.next()) {
        (None, _) => ValueSet::Empty,
        (Some(only), None) => only,
        (Some(first), Some(second)) => {
            let mut union = vec![first, second];
            union.extend(iter);
            ValueSet::Union(union)
        }
    }
}

/// Check if a value set is fully covered by existing coverage.
///
/// `value_set`: the requirements of a particular match arm.
/// covered: the requirements satisfied by another context (usually preceding match arms).
/// required: the requirements imposed by the match scrutinee.
fn is_value_set_covered(value_set: &ValueSet, covered: &[ValueSet], required: &[ValueSet]) -> bool {
    // If the existing coverage covers all the requirements of the scrutinee, then
    // any value_set being checked is already covered.
    let all_requirements_are_covered = !required.is_empty()
        && required
            .iter()
            .all(|requirement| is_value_set_covered(requirement, covered, &[]));
    if all_requirements_are_covered {
        return true;
    }
    match value_set {
        ValueSet::All => {
            // Catch-all is never "already covered" - it's the ultimate cover
            false
        }
        ValueSet::Empty => {
            // Empty is always "covered" (it matches nothing)
            true
        }
        ValueSet::OfType(name) => {
            // Check if this type is covered by an existing OfType or All
            covered.iter().any(|c| match c {
                ValueSet::All => true,
                ValueSet::OfType(covered_name) => covered_name == name,
                _ => false,
            })
        }
        ValueSet::EnumVariant {
            enum_name,
            variant_name,
        } => {
            // Check if this specific variant is covered
            covered.iter().any(|c| match c {
                ValueSet::All => true,
                ValueSet::OfType(covered_name) => covered_name == enum_name,
                ValueSet::EnumVariant {
                    enum_name: ce,
                    variant_name: cv,
                } => ce == enum_name && cv == variant_name,
                ValueSet::Union(subs) => subs.iter().any(|s| {
                    is_value_set_covered(
                        &ValueSet::EnumVariant {
                            enum_name: enum_name.clone(),
                            variant_name: variant_name.clone(),
                        },
                        std::slice::from_ref(s),
                        required,
                    )
                }),
                _ => false,
            })
        }
        ValueSet::Literal(lit) => {
            // Check if this specific literal is covered
            covered.iter().any(|c| match c {
                ValueSet::All => true,
                ValueSet::OfType(name) => literal_has_type(lit, name),
                ValueSet::Literal(covered_lit) => covered_lit == lit,
                ValueSet::Union(subs) => subs.iter().any(|s| {
                    is_value_set_covered(
                        &ValueSet::Literal(lit.clone()),
                        std::slice::from_ref(s),
                        required,
                    )
                }),
                _ => false,
            })
        }
        ValueSet::Union(subs) => {
            // Union is covered if ALL sub-sets are covered
            subs.iter()
                .all(|s| is_value_set_covered(s, covered, required))
        }
    }
}

/// Check if a literal has a given type name.
fn literal_has_type(lit: &Literal, type_name: &Name) -> bool {
    let type_str = type_name.as_str();
    match lit {
        Literal::Int(_) => type_str == "int",
        Literal::Float(_) => type_str == "float",
        Literal::String(_) => type_str == "string",
        Literal::Bool(_) => type_str == "bool",
        Literal::Null => type_str == "null",
    }
}

/// Add a value set to the coverage list.
///
/// This is a free function that can be used by both `ExhaustivenessChecker`
/// and test mocks without duplicating logic.
fn add_to_coverage(
    covered: &mut Vec<ValueSet>,
    value_set: &ValueSet,
    enum_variants: &HashMap<Name, Vec<Name>>,
) {
    match value_set {
        ValueSet::Union(subs) => {
            // Flatten unions
            for sub in subs {
                add_to_coverage(covered, sub, enum_variants);
            }
        }
        ValueSet::OfType(name) => {
            // For OfType, expand if it's a finite type (enum)
            if let Some(variants) = enum_variants.get(name) {
                for variant_name in variants {
                    let variant = ValueSet::EnumVariant {
                        enum_name: name.clone(),
                        variant_name: variant_name.clone(),
                    };
                    if !covered.contains(&variant) {
                        covered.push(variant);
                    }
                }
            } else if !covered.contains(value_set) {
                covered.push(value_set.clone());
            }
        }
        _ => {
            if !covered.contains(value_set) {
                covered.push(value_set.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_name(s: &str) -> Name {
        Name::new(s)
    }

    #[test]
    fn test_value_set_display() {
        assert_eq!(ValueSet::All.to_string(), "_");
        assert_eq!(ValueSet::OfType(make_name("int")).to_string(), "int");
        assert_eq!(
            ValueSet::EnumVariant {
                enum_name: make_name("Status"),
                variant_name: make_name("Active"),
            }
            .to_string(),
            "Status.Active"
        );
        assert_eq!(ValueSet::Literal(Literal::Int(42)).to_string(), "42");
        assert_eq!(
            ValueSet::Literal(Literal::String("hello".to_string())).to_string(),
            "\"hello\""
        );
        assert_eq!(ValueSet::Literal(Literal::Bool(true)).to_string(), "true");
        assert_eq!(ValueSet::Literal(Literal::Null).to_string(), "null");
        assert_eq!(ValueSet::Empty.to_string(), "∅");
    }

    #[test]
    fn test_value_set_union_display() {
        let union = ValueSet::Union(vec![
            ValueSet::Literal(Literal::Int(200)),
            ValueSet::Literal(Literal::Int(201)),
        ]);
        assert_eq!(union.to_string(), "200 | 201");
    }

    // ========================================================================
    // Coverage Tests - Testing is_value_set_covered and add_to_coverage
    // ========================================================================

    /// Helper to create an `enum_variants` map for tests.
    fn enum_variants_with(name: &str, variants: &[&str]) -> HashMap<Name, Vec<Name>> {
        let mut map = HashMap::new();
        map.insert(
            make_name(name),
            variants.iter().map(|v| make_name(v)).collect(),
        );
        map
    }

    #[test]
    fn test_coverage_of_type_matches_same_type() {
        let covered = vec![ValueSet::OfType(make_name("Success"))];

        assert!(is_value_set_covered(
            &ValueSet::OfType(make_name("Success")),
            &covered,
            &[]
        ));
        assert!(!is_value_set_covered(
            &ValueSet::OfType(make_name("Failure")),
            &covered,
            &[]
        ));
    }

    #[test]
    fn test_coverage_type_alias_union_both_covered() {
        // Simulates: type Result = Success | Failure
        // Match arms: s: Success, f: Failure
        // Required: [OfType("Success"), OfType("Failure")]
        // Covered after processing: [OfType("Success"), OfType("Failure")]
        let required = [
            ValueSet::OfType(make_name("Success")),
            ValueSet::OfType(make_name("Failure")),
        ];

        let covered = vec![
            ValueSet::OfType(make_name("Success")),
            ValueSet::OfType(make_name("Failure")),
        ];

        // Both should be covered
        assert!(is_value_set_covered(&required[0], &covered, &[]));
        assert!(is_value_set_covered(&required[1], &covered, &[]));

        // Find uncovered - should be empty
        let uncovered: Vec<_> = required
            .iter()
            .filter(|req| !is_value_set_covered(req, &covered, &[]))
            .cloned()
            .collect();

        assert!(
            uncovered.is_empty(),
            "Expected no uncovered cases, got: {uncovered:?}"
        );
    }

    #[test]
    fn test_add_coverage_of_type() {
        let enum_variants = HashMap::new();
        let mut covered = Vec::new();

        add_to_coverage(
            &mut covered,
            &ValueSet::OfType(make_name("Success")),
            &enum_variants,
        );
        assert_eq!(covered.len(), 1);
        assert_eq!(covered[0], ValueSet::OfType(make_name("Success")));

        add_to_coverage(
            &mut covered,
            &ValueSet::OfType(make_name("Failure")),
            &enum_variants,
        );
        assert_eq!(covered.len(), 2);

        // Adding same type again should not duplicate
        add_to_coverage(
            &mut covered,
            &ValueSet::OfType(make_name("Success")),
            &enum_variants,
        );
        assert_eq!(covered.len(), 2);
    }

    #[test]
    fn test_enum_exhaustiveness() {
        let enum_variants = enum_variants_with("Status", &["Active", "Inactive", "Pending"]);

        // If we match _: Status, it should expand to all variants
        let mut covered = Vec::new();
        add_to_coverage(
            &mut covered,
            &ValueSet::OfType(make_name("Status")),
            &enum_variants,
        );

        // Should have 3 enum variants
        assert_eq!(covered.len(), 3);
        assert!(covered.contains(&ValueSet::EnumVariant {
            enum_name: make_name("Status"),
            variant_name: make_name("Active"),
        }));
    }

    #[test]
    fn test_literal_covered_by_base_type() {
        let covered = vec![ValueSet::OfType(make_name("int"))];

        // A literal 42 should be covered by "int" type pattern
        assert!(is_value_set_covered(
            &ValueSet::Literal(Literal::Int(42)),
            &covered,
            &[]
        ));
        // But not a string literal
        assert!(!is_value_set_covered(
            &ValueSet::Literal(Literal::String("hello".to_string())),
            &covered,
            &[]
        ));
    }

    #[test]
    fn test_catch_all_covers_everything() {
        let covered = vec![ValueSet::All];

        assert!(is_value_set_covered(
            &ValueSet::OfType(make_name("Success")),
            &covered,
            &[]
        ));
        assert!(is_value_set_covered(
            &ValueSet::Literal(Literal::Int(42)),
            &covered,
            &[]
        ));
        assert!(is_value_set_covered(
            &ValueSet::EnumVariant {
                enum_name: make_name("Status"),
                variant_name: make_name("Active"),
            },
            &covered,
            &[]
        ));
    }

    // ========================================================================
    // Type Alias Expansion Tests
    // ========================================================================

    /// Owns all maps so that `ExhaustivenessChecker`'s borrows are valid.
    struct TestCtx {
        enum_variants: HashMap<Name, Vec<Name>>,
        type_aliases: HashMap<Name, Ty>,
        class_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
        enum_names: HashMap<Name, baml_compiler_hir::QualifiedName>,
        type_alias_names: HashSet<Name>,
    }

    impl TestCtx {
        fn new(
            enum_variants: HashMap<Name, Vec<Name>>,
            type_aliases: HashMap<Name, Ty>,
        ) -> Self {
            Self {
                enum_variants,
                type_aliases,
                class_names: HashMap::new(),
                enum_names: HashMap::new(),
                type_alias_names: HashSet::new(),
            }
        }

        fn checker(&self) -> ExhaustivenessChecker<'_> {
            ExhaustivenessChecker::new(
                &self.enum_variants,
                &self.type_aliases,
                &self.class_names,
                &self.enum_names,
                &self.type_alias_names,
            )
        }
    }

    fn ty_alias(name: &str) -> Ty {
        Ty::TypeAlias(baml_compiler_hir::QualifiedName::local(make_name(name)))
    }

    fn ty_enum(name: &str) -> Ty {
        Ty::Enum(baml_compiler_hir::QualifiedName::local(make_name(name)))
    }

    #[test]
    fn test_alias_to_primitive_expands() {
        let mut aliases = HashMap::new();
        aliases.insert(make_name("MyInt"), Ty::Int);
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("MyInt"));
        assert_eq!(values, vec![ValueSet::OfType(make_name("int"))]);
    }

    #[test]
    fn test_alias_to_bool_expands_finite() {
        let mut aliases = HashMap::new();
        aliases.insert(make_name("Flag"), Ty::Bool);
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("Flag"));
        assert_eq!(
            values,
            vec![
                ValueSet::Literal(Literal::Bool(true)),
                ValueSet::Literal(Literal::Bool(false)),
            ]
        );
    }

    #[test]
    fn test_alias_to_enum_expands_variants() {
        let enum_variants = enum_variants_with("Status", &["Active", "Inactive"]);
        let mut aliases = HashMap::new();
        aliases.insert(make_name("MyStatus"), ty_enum("Status"));
        let ctx = TestCtx::new(enum_variants, aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("MyStatus"));
        assert_eq!(
            values,
            vec![
                ValueSet::EnumVariant {
                    enum_name: make_name("Status"),
                    variant_name: make_name("Active"),
                },
                ValueSet::EnumVariant {
                    enum_name: make_name("Status"),
                    variant_name: make_name("Inactive"),
                },
            ]
        );
    }

    #[test]
    fn test_transitive_alias_expands() {
        // type A = B; type B = int
        let mut aliases = HashMap::new();
        aliases.insert(make_name("A"), ty_alias("B"));
        aliases.insert(make_name("B"), Ty::Int);
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("A"));
        assert_eq!(values, vec![ValueSet::OfType(make_name("int"))]);
    }

    #[test]
    fn test_alias_union_expands_members() {
        // type Result = int | string
        let mut aliases = HashMap::new();
        aliases.insert(make_name("Result"), Ty::Union(vec![Ty::Int, Ty::String]));
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("Result"));
        assert_eq!(
            values,
            vec![
                ValueSet::OfType(make_name("int")),
                ValueSet::OfType(make_name("string")),
            ]
        );
    }

    #[test]
    fn test_cyclic_alias_does_not_overflow() {
        // type A = A | int — invalid self-referential cycle, handled gracefully.
        //
        // Trace: expand(A, visiting={})
        //   → insert A, visiting={A}
        //   → expand Union([A, int], visiting={A})
        //     → expand A: A ∈ visiting → [OfType("A")]   (cycle guard)
        //     → expand int            → [OfType("int")]
        //   → remove A, visiting={}
        //   → [OfType("A"), OfType("int")]
        let mut aliases = HashMap::new();
        aliases.insert(make_name("A"), Ty::Union(vec![ty_alias("A"), Ty::Int]));
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("A"));
        assert_eq!(
            values,
            vec![ValueSet::OfType(make_name("A")), ValueSet::OfType(make_name("int"))],
            "cyclic alias should expand to [OfType(A), OfType(int)]"
        );
    }

    #[test]
    fn test_mutually_recursive_aliases_do_not_overflow() {
        // type A = B; type B = A — invalid mutual cycle, handled gracefully.
        //
        // Trace: expand(A, visiting={})
        //   → insert A, visiting={A}
        //   → expand B, visiting={A}
        //     → insert B, visiting={A,B}
        //     → expand A: A ∈ visiting → [OfType("A")]   (cycle guard)
        //     → remove B, visiting={A}
        //     → [OfType("A")]
        //   → remove A, visiting={}
        //   → [OfType("A")]
        let mut aliases = HashMap::new();
        aliases.insert(make_name("A"), ty_alias("B"));
        aliases.insert(make_name("B"), ty_alias("A"));
        let ctx = TestCtx::new(HashMap::new(), aliases);

        let values = ctx.checker().expand_type_to_values(&ty_alias("A"));
        assert_eq!(
            values,
            vec![ValueSet::OfType(make_name("A"))],
            "mutually recursive aliases should expand to [OfType(A)]"
        );
    }

    /// Regression: `x: MyStatus` must cover a scrutinee of type `MyStatus`
    /// when `MyStatus` is an alias for an enum. Previously, `ty_to_value_set`
    /// returned an opaque `OfType("MyStatus")` which never matched the
    /// expanded enum variants, producing a false non-exhaustive error.
    #[test]
    fn test_alias_pattern_covers_aliased_enum_scrutinee() {
        let enum_variants = enum_variants_with("Status", &["Active", "Inactive"]);
        let mut aliases = HashMap::new();
        aliases.insert(make_name("MyStatus"), ty_enum("Status"));
        let ctx = TestCtx::new(enum_variants.clone(), aliases);
        let checker = ctx.checker();

        // Scrutinee: MyStatus alias → expands to [Active, Inactive]
        let required = checker.expand_type_to_values(&ty_alias("MyStatus"));
        assert_eq!(required.len(), 2, "alias scrutinee should expand to 2 variants");

        // Pattern `x: MyStatus` → must also expand through the alias
        let pattern_coverage = checker.ty_to_value_set(&ty_alias("MyStatus"));
        let mut covered = Vec::new();
        add_to_coverage(&mut covered, &pattern_coverage, &enum_variants);

        let uncovered: Vec<_> = required
            .iter()
            .filter(|req| !is_value_set_covered(req, &covered, &required))
            .collect();

        assert!(
            uncovered.is_empty(),
            "x: MyStatus should fully cover MyStatus scrutinee; uncovered: {uncovered:?}"
        );
    }
}
