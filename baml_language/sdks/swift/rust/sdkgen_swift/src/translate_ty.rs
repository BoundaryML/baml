//! BAML `Ty` → Swift type expression translation.
//!
//! Phase 2 supports primitives, lists/maps/optionals, and references
//! to *supported* user classes, enums, and (non-recursive) type
//! aliases. [`translate_ty`] returns `None` for anything not yet
//! supported, and the emitter skips whole symbols whose signature
//! contains an unsupported type — the generated package must always
//! compile, and capabilities widen phase by phase.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::qualified_name::{AI_FUNCTION_SPEC, AI_STREAM_STREAM};
use baml_sdkgen_types::{Name, ParamTy, Ty};

/// Which named types the emitter decided it can emit this run (the
/// fixpoint result from `lib.rs`), plus how to spell their Swift path.
pub(crate) struct TranslateCtx {
    /// FQNs of user classes that will be emitted.
    pub supported_classes: BTreeSet<String>,
    /// FQNs of user enums that will be emitted.
    pub supported_enums: BTreeSet<String>,
    /// FQNs of user type aliases that will be emitted.
    pub supported_aliases: BTreeSet<String>,
    /// Recursive union aliases whose union is null-bearing (stdlib
    /// `json`): the nominal enum holds the non-null arms, so every
    /// reference site spells `Name?`.
    pub nullable_aliases: BTreeSet<String>,
    /// Bodies of the supported non-recursive aliases. Swift spells such an
    /// alias as a `typealias`, and a statement spells it by its body: no
    /// alias but a recursive one exists at run time.
    pub alias_bodies: BTreeMap<String, Ty>,
}

impl TranslateCtx {
    fn named_ref(name: &Name, pool: &BTreeSet<String>) -> Option<String> {
        let fqn = name.to_string();
        if pool.contains(&fqn) {
            Some(swift_type_path(name))
        } else {
            None
        }
    }
}

/// Swift namespace segments for a symbol, mirroring Python's routing:
/// pkg `user` → the namespace path as-is; pkg `baml` (stdlib) → under
/// `baml`; any other package → under `vendor.<pkg>`.
pub(crate) fn namespace_for(name: &Name) -> Vec<String> {
    let mut ns: Vec<String> = Vec::new();
    match name.package().as_str() {
        "user" => {}
        "baml" => ns.push("baml".to_string()),
        other => {
            ns.push("vendor".to_string());
            ns.push(other.to_string());
        }
    }
    ns.extend(name.namespace().iter().map(|s| s.as_str().to_string()));
    ns
}

/// Fully-qualified Swift spelling of a generated type: the `Baml`
/// namespace-enum tree mirrors the BAML namespace path. Always
/// qualified so cross-namespace references need no imports.
pub(crate) fn swift_type_path(name: &Name) -> String {
    let mut out = String::from("Baml");
    for seg in namespace_for(name) {
        out.push('.');
        out.push_str(&crate::escape_ident(&seg));
    }
    out.push('.');
    out.push_str(&crate::escape_ident(name.name().as_str()));
    out
}

/// Swift spelling of `ty`, or `None` if the type is not yet supported.
pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    match ty {
        Ty::Int => Some("Swift.Int".to_string()),
        Ty::Bigint => Some("BamlBigInt".to_string()),
        // BAML float is f64.
        Ty::Float => Some("Swift.Double".to_string()),
        Ty::String => Some("Swift.String".to_string()),
        Ty::Bool => Some("Swift.Bool".to_string()),
        // Standalone `null` type: Swift has no untyped nil, so it gets
        // a unit-like runtime type that encodes/decodes as BAML null.
        Ty::Null => Some("BamlNull".to_string()),
        Ty::Uint8Array => Some("Foundation.Data".to_string()),
        // Provider-neutral prompts cross as a copied protobuf tree. They are
        // portable values, not generated `$rust_type` handle wrappers.
        Ty::PromptAst => Some("BamlPrompt".to_string()),
        // Media primitives are the handle-backed stdlib classes
        // (already emitted as generated structs; construction via
        // BamlMedia over the C ABI).
        // The generic media shape has no wrapper class, so no Swift type.
        Ty::Media(kind) => Some(format!("Baml.{}", kind.wrapper_class_name()?)),
        // A literal type is spelled by its base type: Swift has no literal
        // types. The slot (`slot_expr`) still carries the literal, which is
        // what a statement enclosing it spells.
        Ty::Literal(lit, ..) => Some(
            match lit {
                baml_base::Literal::String(_) => "Swift.String",
                baml_base::Literal::Int(_) => "Swift.Int",
                baml_base::Literal::Bigint(_) => "BamlBigInt",
                baml_base::Literal::Bool(_) => "Swift.Bool",
                // Float literals are unsupported for now.
                baml_base::Literal::Float(_) => return None,
            }
            .to_string(),
        ),
        Ty::List(inner) => Some(format!("[{}]", translate_ty(inner, ctx)?)),
        Ty::Map { key, value, .. } => {
            // BAML map keys are stringified engine-side; only string
            // keys are supported host-side for now (mirrors Python's
            // dict[str, Any] posture on the decode path).
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("[Swift.String: {}]", translate_ty(value, ctx)?))
        }
        Ty::Union(members) => translate_union(members, ctx),
        Ty::Class(name, args) => {
            // `ai.FunctionSpec<Final>` is a live runtime capability, never a
            // generated value-model class. Streaming belongs to the separate
            // `ai.stream.Stream<T>` projection.
            if name.to_string() == AI_FUNCTION_SPEC {
                if args.len() != 1 {
                    return None;
                }
                let final_ty = translate_ty(&args[0], ctx)?;
                return Some(format!("BamlFunctionSpec<{final_ty}>"));
            }
            // `ai.Prompt` is the nominal stdlib facade around PromptAst. The
            // generated SDK aliases it to the same portable bridge wrapper.
            if name.to_string() == "ai.Prompt" {
                if !args.is_empty() {
                    return None;
                }
                return Some("BamlPrompt".to_string());
            }
            // `ai.stream.Stream<T>` is runtime-owned: it
            // translates to the BamlBridge `BamlStream` wrapper, never
            // a generated struct (its state is an engine handle).
            if name.to_string() == AI_STREAM_STREAM {
                let [value] = &**args else {
                    return None;
                };
                let value = translate_ty(value, ctx)?;
                return Some(format!("BamlStream<{value}>"));
            }
            let path = TranslateCtx::named_ref(name, &ctx.supported_classes)?;
            if args.is_empty() {
                return Some(path);
            }
            // Parameterized generic reference: `Wrapper<int>` →
            // `Baml.ns.Wrapper<Swift.Int>`.
            let translated: Vec<String> = args
                .iter()
                .map(|a| translate_ty(a, ctx))
                .collect::<Option<_>>()?;
            Some(format!("{path}<{}>", translated.join(", ")))
        }
        // A generic parameter reference (`T`) — spelled bare; only
        // valid inside the generic declaration that binds it, which is
        // the only place the pool produces it.
        Ty::TypeVar(name) => Some(crate::escape_ident(name.as_str())),
        Ty::Enum(name) => TranslateCtx::named_ref(name, &ctx.supported_enums),
        Ty::TypeAlias(name) => {
            let path = TranslateCtx::named_ref(name, &ctx.supported_aliases)?;
            if ctx.nullable_aliases.contains(&name.to_string()) {
                Some(format!("{path}?"))
            } else {
                Some(path)
            }
        }
        // Opaque engine-owned state (`$rust_type` fields on stdlib
        // resource classes: File._handle, Response._body, media _data).
        Ty::RustType => Some("BamlHandle?".to_string()),
        // Unit is only meaningful in return position; the emitter
        // special-cases it. Everything else lands in later phases.
        _ => None,
    }
}

/// Largest `BamlUnionN` shipped in the `BamlBridge` runtime. Wider
/// unions are unsupported (symbol skipped, soft) until the family is
/// extended — an additive runtime-library change.
pub(crate) const MAX_UNION_ARITY: usize = 8;

/// Normalize a union: drop nulls (remembering they were there), dedup
/// structurally equal members, preserve declaration order — the
/// canonical arm order every bridge shares.
pub(crate) fn normalize_union(members: &[Ty]) -> (Vec<Ty>, bool) {
    let mut nullable = false;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut non_null: Vec<Ty> = Vec::new();
    for member in members {
        if matches!(member, Ty::Null) {
            nullable = true;
            continue;
        }
        if seen.insert(format!("{member:?}")) {
            non_null.push(member.clone());
        }
    }
    (non_null, nullable)
}

/// One Swift case of a union and the BAML members it stands for.
pub(crate) struct UnionArm {
    pub swift: String,
    pub members: Vec<Ty>,
}

/// Group a union's non-null members into Swift cases, in declaration order.
/// Members share a case only when they are leaves of one Swift type
/// (`"draft" | "sent"` is one `String` case): a leaf states nothing, so the
/// merged case loses nothing a statement needs. Members of one Swift type
/// that are not leaves (`"a"[] | "b"[]`) keep a case each, because a list
/// states its own exact type and only the case says which one.
pub(crate) fn union_arms(non_null: &[Ty], ctx: &TranslateCtx) -> Option<Vec<UnionArm>> {
    let mut arms: Vec<UnionArm> = Vec::new();
    for member in non_null {
        let swift = translate_ty(member, ctx)?;
        let leaf = is_leaf(member, ctx);
        match arms
            .iter_mut()
            .find(|arm| arm.swift == swift && leaf && arm.members.iter().all(|m| is_leaf(m, ctx)))
        {
            Some(arm) => arm.members.push(member.clone()),
            None => arms.push(UnionArm {
                swift,
                members: vec![member.clone()],
            }),
        }
    }
    Some(arms)
}

/// Does a value of `ty` encode as a node that implies its own type, so its
/// slot contributes nothing but that type?
fn is_leaf(ty: &Ty, ctx: &TranslateCtx) -> bool {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::Literal(..)
        | Ty::Enum(_)
        | Ty::PromptAst => true,
        Ty::Class(name, _) => name.to_string() == "ai.Prompt",
        Ty::TypeAlias(name) => ctx
            .alias_bodies
            .get(&name.to_string())
            .is_some_and(|body| is_leaf(body, ctx)),
        _ => false,
    }
}

/// Swift spelling of a union: `BamlUnionN<...>` from the runtime
/// family — never a generated named type. Null members strip into a
/// trailing `?`; a single surviving arm collapses to that member.
fn translate_union(members: &[Ty], ctx: &TranslateCtx) -> Option<String> {
    let (non_null, nullable) = normalize_union(members);
    let suffix = if nullable { "?" } else { "" };
    let arms = translate_union_arms(&non_null, ctx)?;
    match arms.len() {
        0 => Some("BamlNull".to_string()),
        1 => Some(format!("{}{suffix}", arms[0])),
        n if n <= MAX_UNION_ARITY => Some(format!("BamlUnion{n}<{}>{suffix}", arms.join(", "))),
        _ => None,
    }
}

/// The Swift spelling of each case of [`union_arms`].
pub(crate) fn translate_union_arms(non_null: &[Ty], ctx: &TranslateCtx) -> Option<Vec<String>> {
    Some(
        union_arms(non_null, ctx)?
            .into_iter()
            .map(|arm| arm.swift)
            .collect(),
    )
}

/// For an optional-argument slot: the `T` in `BamlOptional<T>`. A
/// nullable declared type contributes its non-null part (the `.null`
/// case covers the rest); a non-nullable defaulted type is used as-is.
pub(crate) fn translate_optional_arg_inner(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    if let Ty::Union(members) = ty {
        let (non_null, nullable) = normalize_union(members);
        if nullable {
            let arms = translate_union_arms(&non_null, ctx)?;
            return match arms.len() {
                0 => Some("BamlNull".to_string()),
                1 => Some(arms[0].clone()),
                n if n <= MAX_UNION_ARITY => Some(format!("BamlUnion{n}<{}>", arms.join(", "))),
                _ => None,
            };
        }
    }
    translate_ty(ty, ctx)
}

/// How a type variable's slot is spelled where the slot is emitted: a
/// function's `T` is the Swift type's own slot, a class's `T` the type
/// argument its instance was stated with.
pub(crate) type TypeVarSlot<'a> = &'a dyn Fn(&ParamTy) -> String;

/// A function or method: each `T` states its Swift type's own type.
pub(crate) fn generic_param_slot(param: &ParamTy) -> String {
    own_slot(&crate::escape_ident(param.as_str()))
}

/// The own slot of the Swift type `swift`. Spelled with the type in a
/// generic argument, a type position: a type named `Type` cannot be followed
/// by a member in an expression.
pub(crate) fn own_slot(swift: &str) -> String {
    format!("BamlSlot<{swift}>.own")
}

/// The Swift expression of the [`BamlSlot`] for a value of declared type
/// `ty`: a `BamlSlot<V>` where `V` is exactly `translate_ty(ty)`, carrying the
/// exact BAML type a statement spells. `None` exactly where `translate_ty`
/// is `None`.
pub(crate) fn slot_expr(ty: &Ty, ctx: &TranslateCtx, vars: TypeVarSlot<'_>) -> Option<String> {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::PromptAst
        | Ty::Media(_)
        | Ty::Enum(_) => Some(own_slot(&translate_ty(ty, ctx)?)),
        Ty::Literal(lit, ..) => Some(match lit {
            // Rust's `Debug` escapes are valid Swift string escapes.
            baml_base::Literal::String(value) => {
                format!("BamlSlot<Swift.String>.literal({value:?})")
            }
            baml_base::Literal::Int(value) => format!("BamlSlot<Swift.Int>.literal({value})"),
            baml_base::Literal::Bool(value) => format!("BamlSlot<Swift.Bool>.literal({value})"),
            baml_base::Literal::Bigint(value) => {
                format!("BamlSlot<BamlBigInt>.literal(bigint: \"{value}\")")
            }
            baml_base::Literal::Float(_) => return None,
        }),
        Ty::List(inner) => Some(format!("BamlSlots.list({})", slot_expr(inner, ctx, vars)?)),
        Ty::Map { key, value, .. } => {
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("BamlSlots.map({})", slot_expr(value, ctx, vars)?))
        }
        Ty::Union(members) => {
            let (non_null, nullable) = normalize_union(members);
            let inner = union_slot(&non_null, ctx, vars)?;
            Some(if nullable && !non_null.is_empty() {
                format!("BamlSlots.optional({inner})")
            } else {
                inner
            })
        }
        Ty::Class(name, args) => {
            let fqn = name.to_string();
            let swift = translate_ty(ty, ctx)?;
            // Runtime-owned handle wrappers: a handle states nothing, so the
            // slot carries only the type an enclosing statement spells.
            if fqn == AI_FUNCTION_SPEC || fqn == AI_STREAM_STREAM {
                let arguments = args
                    .iter()
                    .map(|arg| Some(format!("({}).type", slot_expr(arg, ctx, vars)?)))
                    .collect::<Option<Vec<_>>>()?;
                return Some(format!(
                    "BamlSlot<{swift}>._leaf(.classType(\"{fqn}\", typeArguments: [{}]))",
                    arguments.join(", ")
                ));
            }
            if args.is_empty() {
                return Some(own_slot(&swift));
            }
            let arguments = args
                .iter()
                .map(|arg| slot_expr(arg, ctx, vars))
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "BamlSlot<{swift}>._classType(\"{fqn}\", {})",
                arguments.join(", ")
            ))
        }
        Ty::TypeVar(param) => Some(vars(param)),
        Ty::TypeAlias(name) => {
            let fqn = name.to_string();
            if let Some(body) = ctx.alias_bodies.get(&fqn) {
                return slot_expr(body, ctx, vars);
            }
            let path = TranslateCtx::named_ref(name, &ctx.supported_aliases)?;
            Some(if ctx.nullable_aliases.contains(&fqn) {
                format!("BamlSlots.nullable({})", own_slot(&path))
            } else {
                own_slot(&path)
            })
        }
        Ty::RustType => {
            Some("BamlSlots.optional(BamlSlot<BamlHandle>._leaf(.rustType))".to_string())
        }
        _ => None,
    }
}

/// The slot of a union with nulls already stripped, matching
/// [`translate_union`]'s spelling of the non-null part.
fn union_slot(non_null: &[Ty], ctx: &TranslateCtx, vars: TypeVarSlot<'_>) -> Option<String> {
    let arms = union_arms(non_null, ctx)?
        .iter()
        .map(|arm| arm_slot(arm, ctx, vars))
        .collect::<Option<Vec<_>>>()?;
    match arms.len() {
        0 => Some(own_slot("BamlNull")),
        1 => arms.into_iter().next(),
        n if n <= MAX_UNION_ARITY => Some(format!("BamlSlots.union({})", arms.join(", "))),
        _ => None,
    }
}

/// The slot of one union case: its member's, or the union of its merged
/// leaf members'.
pub(crate) fn arm_slot(
    arm: &UnionArm,
    ctx: &TranslateCtx,
    vars: TypeVarSlot<'_>,
) -> Option<String> {
    match arm.members.as_slice() {
        [member] => slot_expr(member, ctx, vars),
        members => {
            let slots = members
                .iter()
                .map(|member| slot_expr(member, ctx, vars))
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "BamlSlot<{}>.oneOf({})",
                arm.swift,
                slots.join(", ")
            ))
        }
    }
}

/// The slot of an optional argument's value, matching
/// [`translate_optional_arg_inner`]: the declared type without its null.
pub(crate) fn optional_arg_inner_slot(
    ty: &Ty,
    ctx: &TranslateCtx,
    vars: TypeVarSlot<'_>,
) -> Option<String> {
    if let Ty::Union(members) = ty {
        let (non_null, nullable) = normalize_union(members);
        if nullable {
            return union_slot(&non_null, ctx, vars);
        }
    }
    slot_expr(ty, ctx, vars)
}
