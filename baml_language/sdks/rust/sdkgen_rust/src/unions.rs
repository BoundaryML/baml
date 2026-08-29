//! Anonymous-union representation: one synthesized Rust enum per distinct
//! (null-stripped) union shape, per namespace leaf.
//!
//! BAML unions are structural; Rust needs nominal types, so each shape is
//! given a deterministic name derived from its arms (`IntOrString`,
//! `CardPaymentOrWirePayment`). A trailing `null` arm is optionality and
//! never reaches the enum — callers see `Option<IntOrString>`. Placement
//! is per leaf (the module whose symbols use the shape): local, stable
//! under regeneration, and `From` impls smooth the rare cross-namespace
//! seam at call sites.
//!
//! Decode is a trial of the arms in declared order, which is sound only
//! when the arms are pairwise discriminable on the wire (distinct wire
//! kinds, FQN-verified nominals, distinct literal values). Shapes that
//! are not — a `string` arm alongside a string-literal arm — and arms the
//! SDK cannot represent (non-string literals) make the whole shape
//! unsupported, fail-closed.

use std::collections::{BTreeMap, HashSet};

use baml_codegen_types::{Symbol, SymbolPool, Ty};

use crate::{analyze::Analysis, routing};

/// The synthesized enums for one generation run, keyed by the leaf they
/// are emitted in (renamed module path) and their null-stripped arms.
#[derive(Default)]
pub(crate) struct UnionRegistry {
    /// Inner maps are keyed by [`shape_key`] — `Ty` is not `Ord`, and the
    /// `Display` form is structurally faithful (nominal arms render as
    /// FQNs), so it doubles as a stable, deterministic ordering.
    by_leaf: BTreeMap<Vec<String>, BTreeMap<String, UnionEnum>>,
}

/// One synthesized union enum.
pub(crate) struct UnionEnum {
    /// The enum's Rust name (already de-collided within its leaf).
    pub(crate) rust_name: String,
    /// Null-stripped arms in declared order, with their variant names.
    pub(crate) arms: Vec<UnionArm>,
    /// `TypeVar` names appearing in the arms, first-appearance order — the
    /// enum's own Rust generic parameters (`TOrString<T>`). Empty for a
    /// fully concrete union. A `TypeVar` arm is bound by each use site,
    /// which supplies these as the reference's `<…>` arguments.
    pub(crate) generic_params: Vec<String>,
}

pub(crate) struct UnionArm {
    pub(crate) variant: String,
    pub(crate) kind: UnionArmKind,
}

pub(crate) enum UnionArmKind {
    /// A payload-carrying arm (`Int(i64)`, `Resume(crate::…::Resume)`).
    Payload(Ty),
    /// A string-literal arm: a unit variant carrying its wire value.
    StringLiteral(String),
}

impl UnionRegistry {
    /// Look up the enum for a union's arms in a leaf. `arms` must already
    /// be null-stripped.
    pub(crate) fn lookup(&self, leaf: &[String], arms: &[Ty]) -> Option<&UnionEnum> {
        self.by_leaf.get(leaf)?.get(&shape_key(arms))
    }

    /// Every (leaf, enum) pair, in deterministic (leaf, shape) order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Vec<String>, &UnionEnum)> {
        self.by_leaf
            .iter()
            .flat_map(|(leaf, shapes)| shapes.values().map(move |e| (leaf, e)))
    }
}

/// Stable identity of a null-stripped arm list. `Display` on `Ty` is
/// structurally faithful (nominal arms render as FQNs); the NUL joiner
/// cannot appear in rendered types.
fn shape_key(arms: &[Ty]) -> String {
    arms.iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("\0")
}

/// Strip the optionality arm from a union: `(arms-without-null, had_null)`.
pub(crate) fn strip_null(items: &[Ty]) -> (Vec<Ty>, bool) {
    let non_null: Vec<Ty> = items
        .iter()
        .filter(|t| !matches!(t, Ty::Null { .. }))
        .cloned()
        .collect();
    let had_null = non_null.len() != items.len();
    (non_null, had_null)
}

/// Collect every representable union shape in the pool into its leaf.
///
/// Shapes with unsupported or non-discriminable arms are simply not
/// registered — the symbols using them fail translation with a reason at
/// their own emission site, so no separate warning is recorded here.
pub(crate) fn collect(pool: &SymbolPool, analysis: &Analysis) -> UnionRegistry {
    let mut registry = UnionRegistry::default();

    let mut symbols: Vec<_> = pool.iter().collect();
    symbols.sort_by_key(|(name, _)| *name);
    for (name, symbol) in symbols {
        let leaf = analysis.renamed(&routing::route(name).segments).to_vec();
        let mut tys: Vec<&Ty> = Vec::new();
        let mut throws_tys: Vec<&Ty> = Vec::new();
        match symbol {
            Symbol::Function(function) => {
                tys.extend(function.arguments.iter().map(|a| &a.ty));
                tys.push(&function.return_type);
                if let Some(throws) = &function.throws {
                    throws_tys.push(throws);
                }
            }
            Symbol::Class(class) => {
                if !analysis.is_emitted(name) {
                    continue;
                }
                tys.extend(class.properties.iter().map(|p| &p.ty));
                // Method signatures surface unions too: the bindings in
                // the class's `impl` block translate against this same
                // leaf's registry.
                for method in class.static_methods.iter().chain(&class.instance_methods) {
                    tys.extend(method.arguments.iter().map(|a| &a.ty));
                    tys.push(&method.return_type);
                    if let Some(throws) = &method.throws {
                        throws_tys.push(throws);
                    }
                }
            }
            Symbol::Enum(_) => {}
            Symbol::TypeAlias(alias) => {
                if !analysis.is_emitted(name) {
                    continue;
                }
                tys.push(&alias.resolves_to);
            }
        }
        for ty in tys {
            register_unions_in(ty, &leaf, analysis, &mut registry);
        }
        for ty in throws_tys {
            register_throws_unions_in(ty, &leaf, analysis, &mut registry);
        }
    }

    // Names are derived from arm names and can collide (e.g. two classes
    // named `Bar` from different namespaces, or a user type sharing the
    // derived name). De-collide deterministically with trailing
    // underscores, in shape order.
    for (leaf, shapes) in &mut registry.by_leaf {
        let mut taken: HashSet<String> = analysis.type_names_in(leaf);
        for union_enum in shapes.values_mut() {
            while taken.contains(&union_enum.rust_name) {
                union_enum.rust_name.push('_');
            }
            taken.insert(union_enum.rust_name.clone());
        }
    }

    registry
}

/// Register unions as they appear on Rust's typed error surface. Interface
/// arms are intentionally omitted: open interfaces cannot become a closed
/// Rust enum, and values selecting those arms are preserved by
/// `baml_bridge::Error::Runtime` instead.
fn register_throws_unions_in(
    ty: &Ty,
    leaf: &[String],
    analysis: &Analysis,
    registry: &mut UnionRegistry,
) {
    match ty {
        Ty::Union(items, _) => {
            let representable: Vec<_> = items
                .iter()
                .filter(|item| arm_is_representable(item, analysis))
                .cloned()
                .collect();
            let (arms, _) = strip_null(&representable);
            if arms.len() >= 2
                && let Some(union_enum) = synthesize(&arms, analysis)
            {
                registry
                    .by_leaf
                    .entry(leaf.to_vec())
                    .or_default()
                    .entry(shape_key(&arms))
                    .or_insert(union_enum);
            }
            for item in &representable {
                register_throws_unions_in(item, leaf, analysis, registry);
            }
        }
        Ty::List(inner, _) => register_throws_unions_in(inner, leaf, analysis, registry),
        Ty::Map { value, .. } => register_throws_unions_in(value, leaf, analysis, registry),
        Ty::Class(_, args, _) => {
            for arg in args {
                register_throws_unions_in(arg, leaf, analysis, registry);
            }
        }
        _ => {}
    }
}

/// Walk a type and register every representable multi-arm union in it.
fn register_unions_in(ty: &Ty, leaf: &[String], analysis: &Analysis, registry: &mut UnionRegistry) {
    match ty {
        Ty::Union(items, _) => {
            let (arms, _) = strip_null(items);
            if arms.len() >= 2
                && let Some(union_enum) = synthesize(&arms, analysis)
            {
                registry
                    .by_leaf
                    .entry(leaf.to_vec())
                    .or_default()
                    .entry(shape_key(&arms))
                    .or_insert(union_enum);
            }
            for item in items {
                register_unions_in(item, leaf, analysis, registry);
            }
        }
        Ty::List(inner, _) => register_unions_in(inner, leaf, analysis, registry),
        Ty::Map { key: _, value, .. } => register_unions_in(value, leaf, analysis, registry),
        // A union nested inside a generic instantiation (`GenericBox<int |
        // string>`) is registered in the same leaf as its enclosing symbol.
        Ty::Class(_, args, _) => {
            for arg in args {
                register_unions_in(arg, leaf, analysis, registry);
            }
        }
        _ => {}
    }
}

/// The pure (emitted-set-independent) reasons a null-stripped arm list
/// cannot become a Rust enum: undecodable arm kinds, wire-ambiguous arm
/// combinations, or underivable/duplicate variant names. Shared by the
/// class-fixpoint checks in `analyze` and [`collect`] so the two can
/// never disagree.
pub(crate) fn shape_error(arms: &[Ty]) -> Option<String> {
    let mut has_bare_string_arm = false;
    let mut has_string_literal_arm = false;
    let mut seen = HashSet::new();
    for arm in arms {
        match arm {
            Ty::Literal(baml_base::Literal::String(_), ..) => has_string_literal_arm = true,
            // Non-string literal arms have no variant-name story yet.
            Ty::Literal(lit, ..) => {
                return Some(format!(
                    "unsupported union arm: non-string literal ({lit:?})"
                ));
            }
            Ty::String { .. } => has_bare_string_arm = true,
            _ => {}
        }
        let Some(variant) = variant_name(arm) else {
            return Some(format!("unsupported union arm: {arm}"));
        };
        // Duplicate variant names (two `Bar` classes from different
        // namespaces) would not compile — fail closed.
        if !seen.insert(variant.clone()) {
            return Some(format!(
                "union arms produce a duplicate variant name `{variant}`"
            ));
        }
    }
    // A bare `string` arm makes string-literal arms indistinguishable on
    // the wire (trial order would decide, i.e. guess) — fail closed.
    if has_bare_string_arm && has_string_literal_arm {
        return Some(
            "union mixes a `string` arm with string-literal arms, which are \
             indistinguishable on the wire"
                .to_string(),
        );
    }
    None
}

/// Build the enum for a null-stripped arm list, or `None` when the shape
/// is unsupported ([`shape_error`]) or an arm references a type that is
/// not emitted (the using symbol then fails translation with its own
/// reason).
fn synthesize(arms: &[Ty], analysis: &Analysis) -> Option<UnionEnum> {
    if shape_error(arms).is_some() {
        return None;
    }
    let mut built = Vec::new();
    for arm in arms {
        let kind = match arm {
            Ty::Literal(baml_base::Literal::String(value), ..) => {
                UnionArmKind::StringLiteral(value.clone())
            }
            supported if arm_is_representable(supported, analysis) => {
                UnionArmKind::Payload(arm.clone())
            }
            _ => return None,
        };
        built.push(UnionArm {
            variant: variant_name(arm)?,
            kind,
        });
    }
    let rust_name = built
        .iter()
        .map(|arm| arm.variant.as_str())
        .collect::<Vec<_>>()
        .join("Or");
    Some(UnionEnum {
        rust_name,
        arms: built,
        generic_params: union_generic_params(arms),
    })
}

/// The `TypeVar` names appearing anywhere in the arms, in first-appearance
/// order — the synthesized enum's own generic parameters.
fn union_generic_params(arms: &[Ty]) -> Vec<String> {
    let mut params = Vec::new();
    let mut seen = HashSet::new();
    for arm in arms {
        collect_arm_type_vars(arm, &mut params, &mut seen);
    }
    params
}

fn collect_arm_type_vars(ty: &Ty, out: &mut Vec<String>, seen: &mut HashSet<String>) {
    match ty {
        Ty::TypeVar(var, _) => {
            let name = var.as_str().to_string();
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
        Ty::List(inner, _) => collect_arm_type_vars(inner, out, seen),
        Ty::Map { key, value, .. } => {
            collect_arm_type_vars(key, out, seen);
            collect_arm_type_vars(value, out, seen);
        }
        Ty::Union(items, _) => items
            .iter()
            .for_each(|item| collect_arm_type_vars(item, out, seen)),
        Ty::Class(_, args, _) => args
            .iter()
            .for_each(|arg| collect_arm_type_vars(arg, out, seen)),
        _ => {}
    }
}

/// Whether a payload arm's type is representable given the emitted set
/// (the structural checks live in [`shape_error`]).
pub(crate) fn arm_is_representable(ty: &Ty, analysis: &Analysis) -> bool {
    match ty {
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Uint8Array { .. } => true,
        // A `TypeVar` arm makes the enum generic over that param: the
        // variant holds a bare `T`. Whether `T` is actually in scope at the
        // use site is enforced when the reference is translated.
        Ty::TypeVar(..) => true,
        Ty::Class(name, args, _) => args.is_empty() && analysis.is_emitted(name),
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) | Ty::TypeAlias(name, _) => {
            analysis.is_emitted(name)
        }
        Ty::List(inner, _) => arm_is_representable(inner, analysis),
        Ty::Map { key, value, .. } => {
            matches!(key.as_ref(), Ty::String { .. }) && arm_is_representable(value, analysis)
        }
        // A nested union arm inside a list/map arm collapses to its only
        // non-null member here; real multi-arm nesting is rejected
        // upstream by `Ty::validate`.
        Ty::Union(items, _) => {
            let (arms, _) = strip_null(items);
            arms.len() == 1 && arm_is_representable(&arms[0], analysis)
        }
        Ty::Null { .. }
        | Ty::Void { .. }
        | Ty::Literal(..)
        | Ty::Media(..)
        | Ty::Unknown { .. }
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::RustType { .. } => false,
    }
}

/// The variant name for an arm, or `None` when no identifier-safe name
/// can be derived.
fn variant_name(arm: &Ty) -> Option<String> {
    match arm {
        Ty::Int { .. } => Some("Int".to_string()),
        Ty::Bigint { .. } => Some("Bigint".to_string()),
        Ty::Float { .. } => Some("Float".to_string()),
        Ty::String { .. } => Some("String".to_string()),
        Ty::Bool { .. } => Some("Bool".to_string()),
        Ty::Uint8Array { .. } => Some("Uint8Array".to_string()),
        Ty::Class(name, _, _)
        | Ty::Enum(name, _)
        | Ty::EnumVariant(name, _, _)
        | Ty::TypeAlias(name, _) => {
            let mut variant = name.bare_name().to_string();
            if name.name().as_str().ends_with("$stream") {
                variant.push_str("Stream");
            }
            Some(variant)
        }
        // A `TypeVar` arm's variant is named after the type parameter
        // (`T | string` → `TOrString { T(T), String(String) }`).
        Ty::TypeVar(var, _) => Some(var.as_str().to_string()),
        Ty::List(inner, _) => Some(format!("{}List", variant_name(inner)?)),
        Ty::Map { key: _, value, .. } => Some(format!("{}Map", variant_name(value)?)),
        Ty::Literal(baml_base::Literal::String(value), ..) => {
            let mut chars = value.chars();
            let first = chars.next()?;
            if !first.is_ascii_alphabetic() {
                return None;
            }
            let rest: String = chars.collect();
            if !rest.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            Some(format!("{}{rest}", first.to_ascii_uppercase()))
        }
        Ty::Union(items, _) => {
            // Only reachable for the degenerate single-arm nested case.
            let (arms, _) = strip_null(items);
            match arms.as_slice() {
                [only] => variant_name(only),
                _ => None,
            }
        }
        Ty::Null { .. }
        | Ty::Void { .. }
        | Ty::Literal(..)
        | Ty::Media(..)
        | Ty::Unknown { .. }
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::RustType { .. } => None,
    }
}
