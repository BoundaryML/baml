//! BAML `Ty` → Swift type expression translation.
//!
//! Phase 2 supports primitives, lists/maps/optionals, and references
//! to *supported* user classes, enums, and (non-recursive) type
//! aliases. [`translate_ty`] returns `None` for anything not yet
//! supported, and the emitter skips whole symbols whose signature
//! contains an unsupported type — the generated package must always
//! compile, and capabilities widen phase by phase.

use std::collections::BTreeSet;

use baml_base::qualified_name::AI_STREAM_STREAM;
use baml_codegen_types::{Name, Ty};

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
/// `baml`; any other package → under `vendor.<pkg>`. `$stream`
/// companion CLASSES route under a `stream_types` prefix (Python's
/// `baml_sdk.stream_types.<ns>`) — the suffix strips from the type
/// name, so `Resume$stream` is `Baml.stream_types.lorem.Resume`.
pub(crate) fn namespace_for(name: &Name) -> Vec<String> {
    let mut ns: Vec<String> = Vec::new();
    if name.is_stream() {
        ns.push("stream_types".to_string());
    }
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
    out.push_str(&crate::escape_ident(name.bare_name()));
    out
}

/// Swift spelling of `ty`, or `None` if the type is not yet supported.
pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    match ty {
        Ty::Int { .. } => Some("Swift.Int".to_string()),
        // BAML float is f64.
        Ty::Float { .. } => Some("Swift.Double".to_string()),
        Ty::String { .. } => Some("Swift.String".to_string()),
        Ty::Bool { .. } => Some("Swift.Bool".to_string()),
        // Standalone `null` type: Swift has no untyped nil, so it gets
        // a unit-like runtime type that encodes/decodes as BAML null.
        Ty::Null { .. } => Some("BamlNull".to_string()),
        Ty::Uint8Array { .. } => Some("Foundation.Data".to_string()),
        // Media primitives are the handle-backed stdlib classes
        // (already emitted as generated structs; construction via
        // BamlMedia over the C ABI).
        Ty::Media(kind, _) => {
            let name = match format!("{kind:?}").as_str() {
                "Image" => "Image",
                "Audio" => "Audio",
                "Video" => "Video",
                "Pdf" => "Pdf",
                _ => return None, // generic media shape → unsupported
            };
            Some(format!("Baml.baml.media.{name}"))
        }
        // Standalone literal types collapse to their base type; Swift
        // has no literal types and the engine re-validates values.
        // (Literal-only UNIONS collapse the same way — no raw enums.)
        Ty::Literal(lit, ..) => Some(
            match lit {
                baml_base::Literal::String(_) => "Swift.String",
                baml_base::Literal::Int(_) => "Swift.Int",
                baml_base::Literal::Bool(_) => "Swift.Bool",
                // Bigint / float literals: unsupported for now.
                _ => return None,
            }
            .to_string(),
        ),
        Ty::List(inner, _) => Some(format!("[{}]", translate_ty(inner, ctx)?)),
        Ty::Map { key, value, .. } => {
            // BAML map keys are stringified engine-side; only string
            // keys are supported host-side for now (mirrors Python's
            // dict[str, Any] posture on the decode path).
            if !matches!(**key, Ty::String { .. }) {
                return None;
            }
            Some(format!("[Swift.String: {}]", translate_ty(value, ctx)?))
        }
        Ty::Union(members, _) => translate_union(members, ctx),
        Ty::Class(name, args, _) => {
            // `ai.stream.Stream<Partial, Final>` is runtime-owned: it
            // translates to the BamlBridge `BamlStream` wrapper, never
            // a generated struct (its state is an engine handle).
            if name.to_string() == AI_STREAM_STREAM {
                if args.len() != 2 {
                    return None;
                }
                let partial = translate_ty(&args[0], ctx)?;
                let final_ty = translate_ty(&args[1], ctx)?;
                return Some(format!("BamlStream<{partial}, {final_ty}>"));
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
        Ty::TypeVar(name, _) => Some(crate::escape_ident(name.as_str())),
        Ty::Enum(name, _) => TranslateCtx::named_ref(name, &ctx.supported_enums),
        Ty::TypeAlias(name, _) => {
            let path = TranslateCtx::named_ref(name, &ctx.supported_aliases)?;
            if ctx.nullable_aliases.contains(&name.to_string()) {
                Some(format!("{path}?"))
            } else {
                Some(path)
            }
        }
        // Opaque engine-owned state (`$rust_type` fields on stdlib
        // resource classes: File._handle, Response._body, media _data).
        Ty::RustType { .. } => Some("BamlHandle?".to_string()),
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
        if matches!(member, Ty::Null { .. }) {
            nullable = true;
            continue;
        }
        if seen.insert(format!("{member:?}")) {
            non_null.push(member.clone());
        }
    }
    (non_null, nullable)
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

/// Translate each non-null arm, then dedup identical *translated*
/// types. Structural dedup already ran; translated duplicates only
/// arise from literal arms sharing a base today (`"draft" | "sent"` →
/// String, String) — no generics yet — so collapsing loses nothing
/// host-side (the engine validates values). Revisit when generic
/// projections can alias distinct arms.
pub(crate) fn translate_union_arms(non_null: &[Ty], ctx: &TranslateCtx) -> Option<Vec<String>> {
    let mut arms: Vec<String> = Vec::new();
    for member in non_null {
        let ty = translate_ty(member, ctx)?;
        if !arms.contains(&ty) {
            arms.push(ty);
        }
    }
    Some(arms)
}

/// For an optional-argument slot: the `T` in `BamlOptional<T>`. A
/// nullable declared type contributes its non-null part (the `.null`
/// case covers the rest); a non-nullable defaulted type is used as-is.
pub(crate) fn translate_optional_arg_inner(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    if let Ty::Union(members, _) = ty {
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
