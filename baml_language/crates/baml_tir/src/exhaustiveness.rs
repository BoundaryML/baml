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

use std::collections::HashMap;

use baml_base::Name;
use baml_hir::{ExprBody, Literal, MatchArm, Pattern};

use crate::{Db, LiteralValue, Ty, lower_type_ref};

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSet {
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

// ============================================================================
// ExhaustivenessChecker: The Algorithm
// ============================================================================

/// Checker for match expression exhaustiveness.
///
/// This struct holds the context needed to expand types into their
/// constituent values and check coverage.
pub struct ExhaustivenessChecker<'a, 'db> {
    /// Database for type resolution
    db: &'db dyn Db,

    /// Enum definitions: `enum_name` -> [`variant_names`]
    enum_variants: &'a HashMap<Name, Vec<Name>>,

    /// Type alias definitions: `alias_name` -> `underlying_type`
    type_aliases: &'a HashMap<Name, Ty<'db>>,
}

/// Result of exhaustiveness checking.
#[derive(Debug)]
pub struct ExhaustivenessResult {
    /// Whether all cases are covered
    pub is_exhaustive: bool,

    /// Value sets that are not covered (empty if exhaustive)
    pub uncovered: Vec<ValueSet>,

    /// Indices (0-based) into the `arms` slice of unreachable arms (arms that can never match).
    pub unreachable_arms: Vec<usize>,
}

impl<'a, 'db> ExhaustivenessChecker<'a, 'db> {
    /// Create a new exhaustiveness checker.
    pub fn new(
        db: &'db dyn Db,
        enum_variants: &'a HashMap<Name, Vec<Name>>,
        type_aliases: &'a HashMap<Name, Ty<'db>>,
    ) -> Self {
        Self {
            db,
            enum_variants,
            type_aliases,
        }
    }

    /// Check exhaustiveness of a match expression.
    ///
    /// # Arguments
    /// - `scrutinee_ty`: The type of the value being matched
    /// - `arms`: The match arms to check
    /// - `body`: The expression body (for pattern lookup)
    ///
    /// # Returns
    /// An `ExhaustivenessResult` with coverage info and any issues found.
    pub fn check(
        &self,
        scrutinee_ty: &Ty<'db>,
        arms: &[MatchArm],
        body: &ExprBody,
    ) -> ExhaustivenessResult {
        // Expand the scrutinee type into the value sets that need to be covered
        let required = self.expand_type_to_values(scrutinee_ty);

        // Track what's been covered and which arms are unreachable
        let mut covered: Vec<ValueSet> = Vec::new();
        let mut has_catch_all = false;
        let mut unreachable_arms: Vec<usize> = Vec::new();

        for (arm_idx, arm) in arms.iter().enumerate() {
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
            if !has_guard && Self::is_fully_covered(&value_set, &covered) {
                unreachable_arms.push(arm_idx);
                // Don't skip - we still add to coverage for accurate error messages
            }

            // Update coverage
            if !has_guard {
                match &value_set {
                    ValueSet::All => {
                        has_catch_all = true;
                        covered.clone_from(&required); // Everything is now covered
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
    ///
    /// For finite types (enums, bool), this produces individual value sets.
    /// For infinite types, this produces a single `OfType` value set.
    fn expand_type_to_values(&self, ty: &Ty<'db>) -> Vec<ValueSet> {
        match ty {
            // Union types: expand each member
            Ty::Union(members) => members
                .iter()
                .flat_map(|m| self.expand_type_to_values(m))
                .collect(),

            // Optional is T | null
            Ty::Optional(inner) => {
                let mut values = self.expand_type_to_values(inner);
                // Only add null if not already present (handles T?? = T? flattening)
                let null_value = ValueSet::Literal(Literal::Null);
                if !values.contains(&null_value) {
                    values.push(null_value);
                }
                values
            }

            // Named type: could be enum, class, or type alias
            Ty::Named(name) => {
                // Check if it's a type alias
                //
                // TODO(type-alias-architecture): Type alias resolution should be its own
                // dedicated phase that runs once after name resolution. Resolved aliases
                // are used in multiple places:
                //   - Bytecode generation
                //   - Target language codegen (TS, Python, Go, Ruby)
                //   - Prompt rendering
                //   - Exhaustiveness checking (here)
                //
                // Currently we build the type_aliases map per-compilation, but as more
                // consumers are added, this should become a cached Salsa query to avoid
                // redundant resolution.
                //
                // TODO(recursive-type-aliases): Recursive type aliases like `type A = A | B`
                // or structural recursion like `type LinkedList = { val: int, next: LinkedList? }`
                // are NOT handled here. Currently this would cause infinite recursion.
                //
                // The legacy compiler solved this problem:
                //   - PR #1163: Implement Type Aliases (basic support)
                //   - PR #1207: Allow structural recursion in type aliases
                //   - PR #1416: Recurse into recursive type alias unions
                //
                // The solution involves:
                //   1. Building a dependency graph of alias references
                //   2. Using Tarjan's SCC algorithm for cycle detection
                //   3. Distinguishing structural vs non-structural recursion
                //   4. Reporting diagnostics for invalid cycles, inserting Ty::Error
                //
                // Reference implementation: engine/baml-lib/parser-database/src/tarjan.rs
                // and engine/baml-lib/parser-database/src/types/mod.rs (resolve_type_aliases)
                //
                // Porting this to the new compiler requires its own task for feature parity.
                if let Some(alias_ty) = self.type_aliases.get(name) {
                    return self.expand_type_to_values(alias_ty);
                }

                // Check if it's an enum (finite type)
                if let Some(variants) = self.enum_variants.get(name) {
                    return variants
                        .iter()
                        .map(|variant_name| ValueSet::EnumVariant {
                            enum_name: name.clone(),
                            variant_name: variant_name.clone(),
                        })
                        .collect();
                }

                // Unknown named type or class (infinite)
                vec![ValueSet::OfType(name.clone())]
            }

            // Bool is finite: {true, false}
            Ty::Bool => vec![
                ValueSet::Literal(Literal::Bool(true)),
                ValueSet::Literal(Literal::Bool(false)),
            ],

            // Singleton types (types containing exactly one value)
            Ty::Null => vec![ValueSet::Literal(Literal::Null)],
            Ty::Literal(value) => match value {
                LiteralValue::Int(v) => vec![ValueSet::Literal(Literal::Int(*v))],
                LiteralValue::Float(v) => {
                    vec![ValueSet::Literal(Literal::Float(v.clone()))]
                }
                LiteralValue::String(v) => {
                    vec![ValueSet::Literal(Literal::String(v.clone()))]
                }
                LiteralValue::Bool(v) => vec![ValueSet::Literal(Literal::Bool(*v))],
            },

            // Infinite types: int, float, string, classes, etc.
            Ty::Int => vec![ValueSet::OfType(Name::new("int"))],
            Ty::Float => vec![ValueSet::OfType(Name::new("float"))],
            Ty::String => vec![ValueSet::OfType(Name::new("string"))],
            Ty::Image => vec![ValueSet::OfType(Name::new("image"))],
            Ty::Audio => vec![ValueSet::OfType(Name::new("audio"))],
            Ty::Video => vec![ValueSet::OfType(Name::new("video"))],
            Ty::Pdf => vec![ValueSet::OfType(Name::new("pdf"))],

            // NOTE: Ty::Class and Ty::Enum branches are currently unreachable.
            // All user-defined types flow through Ty::Named (see lower.rs), not these
            // resolved ID variants. The ID variants exist for potential future use but
            // aren't constructed during type inference. Generic names are safe here,
            // but we add debug_assert to catch if this assumption ever changes.
            Ty::Class(_) => {
                debug_assert!(
                    false,
                    "Ty::Class reached in exhaustiveness checking - expected Ty::Named. \
                    If this is intentional, extract the class name from ClassId."
                );
                vec![ValueSet::OfType(Name::new("<class>"))]
            }
            Ty::Enum(_) => {
                debug_assert!(
                    false,
                    "Ty::Enum reached in exhaustiveness checking - expected Ty::Named. \
                    If this is intentional, extract the enum name from EnumId."
                );
                vec![ValueSet::OfType(Name::new("<enum>"))]
            }

            // List types: include element type for proper distinction between e.g. int[] vs string[]
            Ty::List(inner) => vec![ValueSet::OfType(Name::new(format!("{inner}[]")))],

            // Map types are not yet fully implemented in HIR (see tests/maps.rs).
            // When they are, this should include key/value types: map<{key}, {value}>
            Ty::Map { .. } => vec![ValueSet::OfType(Name::new("<map>"))],

            // Special types
            Ty::Unknown | Ty::Error | Ty::Void => Vec::new(),
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
                let lowered_ty = lower_type_ref(self.db, ty);
                Self::ty_to_value_set(&lowered_ty)
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

                if sub_sets.len() == 1 {
                    sub_sets.into_iter().next().unwrap()
                } else {
                    ValueSet::Union(sub_sets)
                }
            }
        }
    }

    /// Convert a type to a value set (for typed bindings).
    fn ty_to_value_set(ty: &Ty<'db>) -> ValueSet {
        match ty {
            Ty::Union(members) => {
                let sub_sets: Vec<ValueSet> = members.iter().map(Self::ty_to_value_set).collect();
                if sub_sets.len() == 1 {
                    sub_sets.into_iter().next().unwrap()
                } else {
                    ValueSet::Union(sub_sets)
                }
            }
            Ty::Optional(inner) => {
                let inner_set = Self::ty_to_value_set(inner);
                ValueSet::Union(vec![inner_set, ValueSet::Literal(Literal::Null)])
            }
            Ty::Named(name) => {
                // For type aliases, keep the alias name (don't expand)
                // The coverage check will handle expansion
                ValueSet::OfType(name.clone())
            }
            Ty::Literal(value) => match value {
                LiteralValue::Int(v) => ValueSet::Literal(Literal::Int(*v)),
                LiteralValue::Float(v) => ValueSet::Literal(Literal::Float(v.clone())),
                LiteralValue::String(v) => ValueSet::Literal(Literal::String(v.clone())),
                LiteralValue::Bool(v) => ValueSet::Literal(Literal::Bool(*v)),
            },
            Ty::Bool => ValueSet::OfType(Name::new("bool")),
            Ty::Int => ValueSet::OfType(Name::new("int")),
            Ty::Float => ValueSet::OfType(Name::new("float")),
            Ty::String => ValueSet::OfType(Name::new("string")),
            Ty::Null => ValueSet::Literal(Literal::Null),
            _ => ValueSet::OfType(Name::new(ty.to_string())),
        }
    }

    // ========================================================================
    // Coverage Checking
    // ========================================================================

    /// Check if a value set is fully covered by existing coverage.
    fn is_fully_covered(value_set: &ValueSet, covered: &[ValueSet]) -> bool {
        is_value_set_covered(value_set, covered)
    }

    /// Add a value set to the coverage list.
    fn add_coverage(&self, covered: &mut Vec<ValueSet>, value_set: &ValueSet) {
        add_to_coverage(covered, value_set, self.enum_variants);
    }

    /// Find value sets that are not covered.
    fn find_uncovered(required: &[ValueSet], covered: &[ValueSet]) -> Vec<ValueSet> {
        required
            .iter()
            .filter(|req| !Self::is_fully_covered(req, covered))
            .cloned()
            .collect()
    }
}

// ============================================================================
// Shared Coverage Functions
// ============================================================================

/// Check if a value set is fully covered by existing coverage.
///
/// This is a free function that can be used by both `ExhaustivenessChecker`
/// and test mocks without duplicating logic.
fn is_value_set_covered(value_set: &ValueSet, covered: &[ValueSet]) -> bool {
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
                    is_value_set_covered(&ValueSet::Literal(lit.clone()), std::slice::from_ref(s))
                }),
                _ => false,
            })
        }
        ValueSet::Union(subs) => {
            // Union is covered if ALL sub-sets are covered
            subs.iter().all(|s| is_value_set_covered(s, covered))
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
        ));
        assert!(!is_value_set_covered(
            &ValueSet::OfType(make_name("Failure")),
            &covered,
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
        assert!(is_value_set_covered(&required[0], &covered));
        assert!(is_value_set_covered(&required[1], &covered));

        // Find uncovered - should be empty
        let uncovered: Vec<_> = required
            .iter()
            .filter(|req| !is_value_set_covered(req, &covered))
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
        ));
        // But not a string literal
        assert!(!is_value_set_covered(
            &ValueSet::Literal(Literal::String("hello".to_string())),
            &covered,
        ));
    }

    #[test]
    fn test_catch_all_covers_everything() {
        let covered = vec![ValueSet::All];

        assert!(is_value_set_covered(
            &ValueSet::OfType(make_name("Success")),
            &covered,
        ));
        assert!(is_value_set_covered(
            &ValueSet::Literal(Literal::Int(42)),
            &covered,
        ));
        assert!(is_value_set_covered(
            &ValueSet::EnumVariant {
                enum_name: make_name("Status"),
                variant_name: make_name("Active"),
            },
            &covered,
        ));
    }
}
