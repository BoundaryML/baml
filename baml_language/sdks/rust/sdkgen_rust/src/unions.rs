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

use std::collections::{BTreeMap, HashMap, HashSet};

use baml_sdkgen_types::{Name, Symbol, SymbolPool, Ty};

use crate::{
    analyze::{Analysis, ClassShape},
    idents,
};

/// The synthesized enums for one generation run, keyed by the leaf they
/// are emitted in (renamed module path) and their null-stripped arms.
#[derive(Default)]
pub(crate) struct UnionRegistry {
    /// Inner maps are keyed by [`shape_key`] — `Ty` is not `Ord`, and the
    /// `Display` form is structurally faithful (nominal arms render as
    /// FQNs), so it doubles as a stable, deterministic ordering.
    by_leaf: BTreeMap<Vec<String>, BTreeMap<String, UnionEnum>>,
    /// Named tuple structs for tuples wider than the native Rust tuple
    /// impls, keyed like `by_leaf` by [`shape_key`] of their slots.
    tuples_by_leaf: BTreeMap<Vec<String>, BTreeMap<String, TupleStruct>>,
}

/// One generated named tuple struct: the Rust surface of a BAML tuple wider
/// than `baml_bridge` implements its value trait for natively
/// ([`crate::translate_ty::MAX_BRIDGE_TUPLE_ARITY`]). A tuple struct
/// (`Tuple13_IntString…(pub i64, pub String, …)`) so its slots read like a
/// native tuple's, `.0` through `.N-1`.
pub(crate) struct TupleStruct {
    /// The struct's Rust name: [`tuple_type_name`], de-collided within its
    /// leaf.
    pub(crate) rust_name: String,
    /// The tuple's slot types, in slot order.
    pub(crate) items: Vec<Ty>,
    /// `TypeVar` names appearing in the slots, first-appearance order — the
    /// struct's own Rust generic parameters, as for a union enum.
    pub(crate) generic_params: Vec<String>,
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

    /// Look up the named struct for a wide tuple's slots in a leaf.
    pub(crate) fn lookup_tuple(&self, leaf: &[String], items: &[Ty]) -> Option<&TupleStruct> {
        self.tuples_by_leaf.get(leaf)?.get(&shape_key(items))
    }

    /// Every (leaf, named tuple struct) pair, in deterministic (leaf, shape)
    /// order.
    pub(crate) fn iter_tuples(&self) -> impl Iterator<Item = (&Vec<String>, &TupleStruct)> {
        self.tuples_by_leaf
            .iter()
            .flat_map(|(leaf, shapes)| shapes.values().map(move |t| (leaf, t)))
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
        .filter(|t| !matches!(t, Ty::Null))
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
        let leaf = analysis.names.module_path(name).to_vec();
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
                tys.extend(ClassShape::of(class).properties().iter().map(|p| &p.ty));
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
            if let Some(filtered) = filter_throws_type(ty, analysis) {
                register_unions_in(&filtered, &leaf, analysis, &mut registry);
            }
        }
    }

    // Names are derived from arm names and can collide (e.g. two classes
    // named `Bar` from different namespaces, or a user type sharing the
    // derived name). De-collide deterministically with trailing
    // underscores, in shape order.
    // Named tuple structs share the leaf's type namespace with the union
    // enums and are de-collided the same way, after them.
    let leaves: HashSet<Vec<String>> = registry
        .by_leaf
        .keys()
        .chain(registry.tuples_by_leaf.keys())
        .cloned()
        .collect();
    for leaf in leaves {
        let mut taken: HashSet<String> = analysis.type_names_in(&leaf);
        let unions = registry
            .by_leaf
            .get_mut(&leaf)
            .into_iter()
            .flat_map(|shapes| {
                shapes
                    .values_mut()
                    .map(|union_enum| &mut union_enum.rust_name)
            });
        let tuples = registry
            .tuples_by_leaf
            .get_mut(&leaf)
            .into_iter()
            .flat_map(|shapes| shapes.values_mut().map(|tuple| &mut tuple.rust_name));
        for rust_name in unions.chain(tuples) {
            while taken.contains(rust_name.as_str()) {
                rust_name.push('_');
            }
            taken.insert(rust_name.clone());
        }
    }

    registry
}

/// Project a throws contract onto Rust's typed error surface. Open interfaces
/// are omitted recursively; values selecting those arms remain available as
/// `baml_bridge::Error::Runtime` instead of entering a misleading typed arm.
pub(crate) fn filter_throws_type(ty: &Ty, analysis: &Analysis) -> Option<Ty> {
    match ty {
        Ty::Interface(..) => None,
        Ty::Union(items) => {
            let representable = items
                .iter()
                .filter_map(|item| filter_throws_type(item, analysis))
                .filter(|item| arm_is_representable(item, analysis))
                .collect::<Vec<_>>();
            if representable.is_empty() {
                None
            } else {
                Some(Ty::Union(representable.into()))
            }
        }
        Ty::List(inner) => Some(Ty::List(Box::new(filter_throws_type(inner, analysis)?))),
        Ty::Map { key, value } => Some(Ty::Map {
            key: key.clone(),
            value: Box::new(filter_throws_type(value, analysis)?),
        }),
        Ty::Class(name, args) => {
            if !analysis.is_emitted(name) {
                return None;
            }
            let args = args
                .iter()
                .map(|arg| filter_throws_type(arg, analysis))
                .collect::<Option<Box<[_]>>>()?;
            Some(Ty::Class(name.clone(), args))
        }
        // A tuple is typed only if every element is; an element that must
        // fall back to `Error::Runtime` takes the whole tuple with it.
        Ty::Tuple(items) => Some(Ty::Tuple(
            items
                .iter()
                .map(|item| filter_throws_type(item, analysis))
                .collect::<Option<Box<[_]>>>()?,
        )),
        Ty::TypeAlias(name)
            if !crate::translate_ty::is_canonical_json(name) && !analysis.is_emitted(name) =>
        {
            None
        }
        _ => Some(ty.clone()),
    }
}

/// Walk a type and register every representable multi-arm union in it.
fn register_unions_in(ty: &Ty, leaf: &[String], analysis: &Analysis, registry: &mut UnionRegistry) {
    match ty {
        Ty::Union(items) => {
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
        Ty::List(inner) => register_unions_in(inner, leaf, analysis, registry),
        Ty::Map { key: _, value, .. } => register_unions_in(value, leaf, analysis, registry),
        // A tuple with no native or named form is a list of its slots' join:
        // only the join's union (if any) is used, so only it is registered.
        Ty::Tuple(items) if tuple_falls_back(items) => {
            if let Some(join) = tuple_join_stage(items) {
                register_unions_in(&join, leaf, analysis, registry);
            }
        }
        // A tuple wider than the native Rust tuple impls is a named struct,
        // registered in the leaf of the symbol using it like a union enum.
        // Whether its slots are representable is decided where it is
        // referenced (`translate_ty` translates every slot first); a struct
        // only a skipped symbol uses is dropped at emission.
        Ty::Tuple(items) if needs_named_tuple(items) => {
            if let Some(rust_name) = tuple_type_name(items) {
                registry
                    .tuples_by_leaf
                    .entry(leaf.to_vec())
                    .or_default()
                    .entry(shape_key(items))
                    .or_insert_with(|| TupleStruct {
                        rust_name,
                        items: items.to_vec(),
                        generic_params: union_generic_params(items),
                    });
            }
            for item in items {
                register_unions_in(item, leaf, analysis, registry);
            }
        }
        // A union nested inside a generic instantiation (`GenericBox<int |
        // string>`) or a tuple element is registered in the same leaf as its
        // enclosing symbol.
        Ty::Class(_, args) | Ty::Tuple(args) => {
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
    for arm in arms {
        match arm {
            // `json` already spans null, bool, numbers, strings, lists and
            // maps, so beside any other arm the wire value alone cannot pick
            // the arm, and an engine-selected arm index may name either side
            // of the overlap — fail closed.
            Ty::TypeAlias(name) if crate::translate_ty::is_canonical_json(name) => {
                return Some(format!(
                    "unsupported union arm: {arm} alongside other arms (it overlaps them on \
                     the wire)"
                ));
            }
            Ty::Literal(baml_base::Literal::String(_), ..) => has_string_literal_arm = true,
            // Non-string literal arms have no variant-name story yet.
            Ty::Literal(lit, ..) => {
                return Some(format!(
                    "unsupported union arm: non-string literal ({lit:?})"
                ));
            }
            Ty::String => has_bare_string_arm = true,
            _ => {}
        }
    }
    if let Err(reason) = variant_names(arms) {
        return Some(reason);
    }
    // A tuple held as a list ([`tuple_falls_back`]) emits `Vec<element>`,
    // which can equal a list arm's payload (`Vec<i64>` for a wide tuple of
    // int literals beside `int[]`) or another fallback tuple's; the enum
    // would then carry two `From<Vec<_>>` impls — fail closed.
    let mut list_payloads = HashSet::new();
    for arm in arms {
        let element = match arm {
            Ty::List(inner) => Some(inner.to_string()),
            Ty::Tuple(items) if tuple_falls_back(items) => Some(
                tuple_join_stage(items).map_or_else(|| "unknown".to_string(), |j| j.to_string()),
            ),
            _ => None,
        };
        if let Some(element) = element
            && !list_payloads.insert(element)
        {
            return Some(format!(
                "union arm {arm} has the same Rust list type as another arm"
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
    let variants = variant_names(arms).ok()?;
    let mut built = Vec::new();
    for (arm, variant) in arms.iter().zip(variants) {
        let kind = match arm {
            Ty::Literal(baml_base::Literal::String(value), ..) => {
                UnionArmKind::StringLiteral(value.clone())
            }
            supported if arm_is_representable(supported, analysis) => {
                UnionArmKind::Payload(arm.clone())
            }
            _ => return None,
        };
        built.push(UnionArm { variant, kind });
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
        Ty::TypeVar(var) => {
            let name = var.as_str().to_string();
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
        Ty::List(inner) => collect_arm_type_vars(inner, out, seen),
        Ty::Map { key, value, .. } => {
            collect_arm_type_vars(key, out, seen);
            collect_arm_type_vars(value, out, seen);
        }
        // A tuple held as a list uses only its join's type variables.
        Ty::Tuple(items) if tuple_falls_back(items) => {
            if let Some(join) = tuple_join_stage(items) {
                collect_arm_type_vars(&join, out, seen);
            }
        }
        Ty::Union(items) | Ty::Tuple(items) => items
            .iter()
            .for_each(|item| collect_arm_type_vars(item, out, seen)),
        Ty::Class(_, args) => args
            .iter()
            .for_each(|arg| collect_arm_type_vars(arg, out, seen)),
        _ => {}
    }
}

/// Whether a payload arm's type is representable given the emitted set
/// (the structural checks live in [`shape_error`]).
pub(crate) fn arm_is_representable(ty: &Ty, analysis: &Analysis) -> bool {
    arm_is_representable_given(ty, &|name| analysis.is_emitted(name))
}

/// [`arm_is_representable`], with `is_emitted` deciding which nominal
/// types exist. [`tuple_join_stage`] passes "all of them": whether a
/// referenced type is emitted is decided where the join is translated, as
/// for any other reference.
fn arm_is_representable_given(ty: &Ty, is_emitted: &dyn Fn(&Name) -> bool) -> bool {
    match ty {
        Ty::Int | Ty::Bigint | Ty::Float | Ty::String | Ty::Bool | Ty::Uint8Array => true,
        // A `TypeVar` arm makes the enum generic over that param: the
        // variant holds a bare `T`. Whether `T` is actually in scope at the
        // use site is enforced when the reference is translated.
        Ty::TypeVar(..) => true,
        Ty::Class(name, args) => args.is_empty() && is_emitted(name),
        Ty::TypeAlias(name) if crate::translate_ty::is_canonical_json(name) => true,
        Ty::Enum(name) | Ty::EnumVariant(name, _) | Ty::TypeAlias(name) => is_emitted(name),
        Ty::List(inner) => arm_is_representable_given(inner, is_emitted),
        // A `null` element is the unit `()`; only a top-level `null` arm is
        // optionality (stripped before arms are classified).
        // A wide tuple is its leaf's named tuple struct, registered beside
        // the union that holds it. A tuple with neither form is a list of its
        // slots' join, else of dynamic values, which holds anything.
        Ty::Tuple(items) if tuple_falls_back(items) => tuple_join_stage(items).is_none_or(|join| {
            join_arms(&join).iter().all(|arm| {
                matches!(arm, Ty::Literal(baml_base::Literal::String(_), ..))
                    || arm_is_representable_given(arm, is_emitted)
            })
        }),
        Ty::Tuple(items) => items
            .iter()
            .all(|item| matches!(item, Ty::Null) || arm_is_representable_given(item, is_emitted)),
        Ty::Map { key, value, .. } => {
            matches!(key.as_ref(), Ty::String) && arm_is_representable_given(value, is_emitted)
        }
        // A nested union arm inside a list/map arm collapses to its only
        // non-null member here; real multi-arm nesting is rejected
        // upstream by `Ty::validate`.
        Ty::Union(items) => {
            let (arms, _) = strip_null(items);
            arms.len() == 1 && arm_is_representable_given(&arms[0], is_emitted)
        }
        Ty::Null
        | Ty::Literal(..)
        | Ty::Media(..)
        | Ty::Unknown
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Never
        | Ty::RustType => false,
    }
}

/// Whether a tuple of these slots is a generated named struct rather than a
/// native Rust tuple: the bridge implements its value trait for native
/// tuples of arity 1 through [`crate::translate_ty::MAX_BRIDGE_TUPLE_ARITY`].
pub(crate) fn needs_named_tuple(items: &[Ty]) -> bool {
    items.len() > crate::translate_ty::MAX_BRIDGE_TUPLE_ARITY
}

/// Whether a tuple of these slots has neither a native Rust tuple nor a
/// named tuple struct: a slot has no Rust type ([`has_no_rust_type`]), or
/// the tuple is wide and a slot has no name token for its struct. Such a
/// tuple is held as a list of the join of its slot types
/// ([`tuple_join_stage`]), else of `baml_bridge::dynamic::Value`s: either
/// list keeps every element in order and is sent stating the declared tuple
/// type, so the engine still checks each slot. Structural only (no emitted
/// set), so the class fixpoint, the registry and the translator agree; a
/// slot referencing a skipped type still skips its user, as anywhere else.
pub(crate) fn tuple_falls_back(items: &[Ty]) -> bool {
    items.iter().any(has_no_rust_type)
        || (needs_named_tuple(items) && tuple_type_name(items).is_none())
}

/// Whether `ty` has no Rust type in this SDK for a structural reason (its
/// kind, a non-string map key, a union shape no enum represents), so a
/// tuple holding it falls back. A nested tuple always has a form.
fn has_no_rust_type(ty: &Ty) -> bool {
    match ty {
        Ty::Media(..)
        | Ty::Unknown
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Never => true,
        Ty::List(inner) => has_no_rust_type(inner),
        Ty::Map { key, value } => !matches!(key.as_ref(), Ty::String) || has_no_rust_type(value),
        Ty::Union(items) => {
            let (arms, _) = strip_null(items);
            (arms.len() >= 2 && shape_error(&arms).is_some()) || arms.iter().any(has_no_rust_type)
        }
        Ty::Class(_, args) => args.iter().any(has_no_rust_type),
        Ty::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| has_no_rust_type(&param.ty))
                || has_no_rust_type(ret)
                || (!matches!(throws.as_ref(), Ty::Never) && has_no_rust_type(throws))
        }
        _ => false,
    }
}

/// The join of a tuple's slot types: the distinct types its elements can
/// have, in first-slot order, a union slot contributing its members. A
/// non-string literal widens to its base type and an enum variant to its
/// enum, as each translates in Rust, so a union of them is still an enum
/// the SDK can name. One distinct type is that type, several are their
/// union (a `null` member making it optional).
pub(crate) fn tuple_join(items: &[Ty]) -> Ty {
    fn push(ty: &Ty, arms: &mut Vec<Ty>, seen: &mut HashSet<String>) {
        let widened = match ty {
            Ty::Union(members) => {
                for member in members {
                    push(member, arms, seen);
                }
                return;
            }
            Ty::Literal(baml_base::Literal::Int(_), ..) => Ty::Int,
            Ty::Literal(baml_base::Literal::Bigint(_), ..) => Ty::Bigint,
            Ty::Literal(baml_base::Literal::Float(_), ..) => Ty::Float,
            Ty::Literal(baml_base::Literal::Bool(_), ..) => Ty::Bool,
            Ty::EnumVariant(name, _) => Ty::Enum(name.clone()),
            other => other.clone(),
        };
        if seen.insert(widened.to_string()) {
            arms.push(widened);
        }
    }
    let mut arms = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        push(item, &mut arms, &mut seen);
    }
    match <[Ty; 1]>::try_from(arms) {
        Ok([only]) => only,
        Err(arms) => Ty::Union(arms.into_boxed_slice()),
    }
}

/// The non-null members of a join.
fn join_arms(join: &Ty) -> Vec<Ty> {
    match join {
        Ty::Union(items) => strip_null(items).0,
        Ty::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// The element type of a falling-back tuple's list: its [`tuple_join`] when
/// every member can be a union arm (one member is held bare, several in
/// their union enum), else `None` — a list of dynamic values.
pub(crate) fn tuple_join_stage(items: &[Ty]) -> Option<Ty> {
    let join = tuple_join(items);
    let arms = join_arms(&join);
    let representable = arms.iter().all(|arm| {
        // A string-literal arm is a unit variant of the union enum, or
        // the `String` it widens to when alone.
        matches!(arm, Ty::Literal(baml_base::Literal::String(_), ..))
            || (!has_no_rust_type(arm) && arm_is_representable_given(arm, &|_| true))
    });
    (representable && (arms.len() < 2 || shape_error(&arms).is_none())).then_some(join)
}

/// The generated name of a named tuple struct: `TupleN_` followed by each
/// slot's token ([`tuple_slot_token`]) in slot order, repeats kept —
/// `(int, string, int, …)` is `Tuple13_IntStringInt…`. The one helper
/// behind both the struct's declaration (through the registry) and a union
/// arm holding it. `None` when a slot has no identifier-safe name; the
/// tuple is then unsupported.
pub(crate) fn tuple_type_name(items: &[Ty]) -> Option<String> {
    let mut name = format!("Tuple{}_", items.len());
    for item in items {
        name.push_str(&tuple_slot_token(item)?);
    }
    Some(name)
}

/// A tuple slot's name token: its union-arm name ([`variant_name`]), with
/// the shapes a slot can hold but an arm cannot spelled the way the union
/// enum would be named — a multi-arm union is its arms joined by `Or`
/// (`IntOrString`), a nullable slot is prefixed `Optional`, and a `null`
/// slot is `Null`.
fn tuple_slot_token(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Null => Some("Null".to_string()),
        Ty::Union(items) => {
            let (arms, had_null) = strip_null(items);
            let inner = match arms.as_slice() {
                [] => "Null".to_string(),
                [only] => tuple_slot_token(only)?,
                arms => variant_names(arms).ok()?.join("Or"),
            };
            Some(if had_null {
                format!("Optional{inner}")
            } else {
                inner
            })
        }
        Ty::List(inner) => Some(format!("{}List", tuple_slot_token(inner)?)),
        Ty::Map { value, .. } => Some(format!("{}Map", tuple_slot_token(value)?)),
        Ty::Tuple(items) if tuple_falls_back(items) => Some(fallback_tuple_name(items)),
        Ty::Tuple(items) if needs_named_tuple(items) => tuple_type_name(items),
        Ty::Tuple(items) => {
            let mut name = String::new();
            for item in items {
                name.push_str(&tuple_slot_token(item)?);
            }
            name.push_str("Tuple");
            Some(name)
        }
        other => variant_name(other, 0),
    }
}

/// The name token of a tuple held as a list ([`tuple_falls_back`]): its
/// arity, `Tuple13`, since its slots have no tokens to spell.
fn fallback_tuple_name(items: &[Ty]) -> String {
    format!("Tuple{}", items.len())
}

/// The variant names for a null-stripped arm list, in arm order.
///
/// Each arm starts with its short name (`ParseError`). Arms whose names
/// clash are qualified with one more enclosing namespace segment at a time
/// (`ErrorsParseError`, `JsonParseError`) until every name is unique; arms
/// that still clash once fully qualified would not compile, so the shape
/// fails closed.
fn variant_names(arms: &[Ty]) -> Result<Vec<String>, String> {
    let mut depths = vec![0; arms.len()];
    loop {
        let names = arms
            .iter()
            .zip(&depths)
            .map(|(arm, &depth)| {
                variant_name(arm, depth).ok_or_else(|| format!("unsupported union arm: {arm}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut indices_by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, name) in names.iter().enumerate() {
            indices_by_name.entry(name).or_default().push(index);
        }
        let clashing: Vec<usize> = indices_by_name
            .values()
            .filter(|indices| indices.len() > 1)
            .flatten()
            .copied()
            .collect();
        if clashing.is_empty() {
            return Ok(names);
        }
        let mut qualified_further = false;
        for index in clashing {
            if depths[index] < namespace_depth(&arms[index]) {
                depths[index] += 1;
                qualified_further = true;
            }
        }
        if !qualified_further {
            let mut seen = HashSet::new();
            let duplicate = names
                .iter()
                .find(|name| !seen.insert(name.as_str()))
                .expect("a clash has a repeated name");
            return Err(format!(
                "union arms produce a duplicate variant name `{duplicate}`, even \
                 when qualified by namespace"
            ));
        }
    }
}

/// How many namespace segments [`variant_name`] can prefix onto an arm.
/// Qualification is bounded by this rather than by the rendered name
/// changing: a segment like `_` renders empty, yet the segments beyond it
/// can still separate two arms.
fn namespace_depth(arm: &Ty) -> usize {
    match arm {
        Ty::Class(name, _) | Ty::Enum(name) | Ty::EnumVariant(name, _) | Ty::TypeAlias(name) => {
            baml_sdkgen_types::namespace_segments(name).len()
        }
        Ty::List(inner) | Ty::Map { value: inner, .. } => namespace_depth(inner),
        Ty::Union(items) => match strip_null(items).0.as_slice() {
            [only] => namespace_depth(only),
            _ => 0,
        },
        _ => 0,
    }
}

/// The variant name for an arm, or `None` when no identifier-safe name
/// can be derived. Nominal types are prefixed with their innermost
/// `qualify` namespace segments (`qualify = 1` turns
/// `baml.json.ParseError` into `JsonParseError`).
fn variant_name(arm: &Ty, qualify: usize) -> Option<String> {
    match arm {
        Ty::Int => Some("Int".to_string()),
        Ty::Bigint => Some("Bigint".to_string()),
        Ty::Float => Some("Float".to_string()),
        Ty::String => Some("String".to_string()),
        Ty::Bool => Some("Bool".to_string()),
        Ty::Uint8Array => Some("Uint8Array".to_string()),
        Ty::Class(name, _) | Ty::Enum(name) | Ty::EnumVariant(name, _) | Ty::TypeAlias(name) => {
            let segments = baml_sdkgen_types::namespace_segments(name);
            let prefix: String = segments[segments.len().saturating_sub(qualify)..]
                .iter()
                .map(|segment| idents::pascal_case(segment))
                .collect();
            Some(format!("{prefix}{}", name.name()))
        }
        // A `TypeVar` arm's variant is named after the type parameter
        // (`T | string` → `TOrString { T(T), String(String) }`).
        Ty::TypeVar(var) => Some(var.as_str().to_string()),
        Ty::List(inner) => Some(format!("{}List", variant_name(inner, qualify)?)),
        // A tuple held as a list has no type of its own to name an arm
        // after; it is named by its arity (`Tuple13`).
        Ty::Tuple(items) if tuple_falls_back(items) => Some(fallback_tuple_name(items)),
        // A wide tuple is named after its generated struct
        // (`Tuple13_IntInt…`), so a union arm holding it reads as that type.
        Ty::Tuple(items) if needs_named_tuple(items) => tuple_type_name(items),
        // `(int, string)` → `IntStringTuple`. Distinct tuple shapes yield
        // distinct names except for collisions `shape_error` already rejects
        // as duplicate variant names.
        Ty::Tuple(items) => {
            let mut name = String::new();
            for item in items {
                match item {
                    Ty::Null => name.push_str("Null"),
                    item => name.push_str(&variant_name(item, qualify)?),
                }
            }
            name.push_str("Tuple");
            Some(name)
        }
        Ty::Map { key: _, value, .. } => Some(format!("{}Map", variant_name(value, qualify)?)),
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
        Ty::Union(items) => {
            // Only reachable for the degenerate single-arm nested case.
            let (arms, _) = strip_null(items);
            match arms.as_slice() {
                [only] => variant_name(only, qualify),
                _ => None,
            }
        }
        Ty::Null
        | Ty::Literal(..)
        | Ty::Media(..)
        | Ty::Unknown
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Never
        | Ty::RustType => None,
    }
}
