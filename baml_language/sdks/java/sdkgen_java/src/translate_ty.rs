//! `translate_ty`: BAML `Ty` → Java type expression (phase 3).
//!
//! Every returned expression is **fully qualified** (`java.util.List`,
//! `baml_sdk.lorem.Resume`) — Java allows FQNs in every type position,
//! which deletes the import-collection machinery the TS emitter needs
//! (`TranslatedType { expr, imports }` → plain `String` here).
//!
//! Java-specific rules, per
//! `sdks/agent-docs/bridge-ref/ref-java-codegen-conventions.md`:
//!
//! - Position-aware boxing: `int` is `long` in declaration positions
//!   but `java.lang.Long` as a generic type argument or when nullable
//!   (a nullable primitive must box — boxing IS the nullability story
//!   until a `@Nullable` annotation dependency is decided).
//! - `T?` (`Ty::Union` with a `Null` arm and one other arm) collapses
//!   to the boxed inner type; it never mints a union type.
//! - Anonymous multi-arm unions render inline as the runtime's generic
//!   arity family `baml_bridge.Union{n}<...>` (declaration order, null
//!   arm stripped; arity > 10 falls back to `java.lang.Object`). Arm
//!   selection is type-directed at decode time (via the per-binding
//!   descriptor), so no nominal type is minted and [`UnionSink`] stays
//!   empty for them. **Recursive** aliases are the exception: they keep
//!   a minted nominal sealed type named after the alias (a positional
//!   generic cannot reference itself), rendered by the emitter's
//!   `render_union`.
//! - Literal unions over one base type erase to the base type
//!   (`"draft" | "sent"` → `java.lang.String`) — Java has no literal
//!   types.
//! - Type aliases erase to their resolved type (Java has no alias
//!   mechanism); **recursive** aliases mint a nominal type instead
//!   (erasure would not terminate), named after the alias itself.

use std::collections::BTreeMap;

use baml_base::qualified_name::{AI_STREAM_DONE, AI_STREAM_STREAM};
use baml_codegen_types::{Name, Ty};

use crate::routing::{PackagePath, java_identifier, route};

/// Where a type expression appears, which decides primitive boxing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TyPosition {
    /// Field declarations, parameters, return types: primitives stay
    /// unboxed (`long`, `double`, `boolean`; `void` returns).
    TopLevel,
    /// Generic type arguments and nullable positions: primitives box
    /// (`java.lang.Long`, …; `java.lang.Void`).
    Boxed,
}

/// Alias table: resolved target + whether the alias is recursive.
/// Built by the emitter from the pool's `Symbol::TypeAlias` entries.
pub(crate) type AliasTable = BTreeMap<Name, (Ty, bool)>;

/// A generated host-callable functional interface (design point E): a
/// `@FunctionalInterface` emitted for a callable whose own type carries
/// optional parameters or arity > 2, for which no `java.util.function.*`
/// shape fits. `required`/`optionals` carry the SAM's leading (positional)
/// parameters and the nested `Opts` bag fields; `ret` is the boxed return
/// type. The emitter renders it beside its owning package's other types.
#[derive(Debug, Clone)]
pub(crate) struct CallbackInterface {
    /// The interface's simple name (e.g. `IntOptCallback`).
    pub(crate) name: String,
    /// Required (positional) params: `(java identifier, boxed java type)`.
    pub(crate) required: Vec<(String, String)>,
    /// Optional params folded into the `Opts` bag: `(BAML wire name, boxed java type)`.
    pub(crate) optionals: Vec<(String, String)>,
    /// The boxed return type of the SAM.
    pub(crate) ret: String,
}

/// Collects the host-callable functional interfaces
/// ([`CallbackInterface`]) that [`translate_callable`] mints, keyed by
/// owning package. One entry per distinct callable signature within a
/// package (deduped by a structural signature key), so the emitter emits
/// each interface file exactly once. (`unions` is vestigial — anonymous
/// unions render inline and recursive aliases mint directly — but the
/// field is retained so nothing else in the sink plumbing changes.)
#[derive(Debug, Default)]
pub(crate) struct UnionSink {
    #[allow(dead_code)]
    pub(crate) unions: BTreeMap<(PackagePath, String), Vec<Ty>>,
    /// Per-package list of `(signature key, interface)` in insertion order.
    pub(crate) callbacks: BTreeMap<PackagePath, Vec<(String, CallbackInterface)>>,
}

/// Everything `translate_ty` needs beyond the type itself.
pub(crate) struct TranslateCtx<'a> {
    pub(crate) aliases: &'a AliasTable,
    /// The package currently being rendered — where a minted host-callable
    /// interface is placed (so it sits beside the function/class that uses it).
    pub(crate) pkg: PackagePath,
}

/// Translate `ty` to a Java type expression. `sink` is threaded for the
/// vestigial [`UnionSink`] contract only — anonymous unions render
/// inline and nothing is recorded.
pub(crate) fn translate_ty(
    ty: &Ty,
    pos: TyPosition,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    match ty {
        Ty::Int { .. } => primitive(pos, "long", "java.lang.Long"),
        Ty::Bigint { .. } => "java.math.BigInteger".to_string(),
        Ty::Float { .. } => primitive(pos, "double", "java.lang.Double"),
        Ty::String { .. } => "java.lang.String".to_string(),
        Ty::Bool { .. } => primitive(pos, "boolean", "java.lang.Boolean"),
        // Only `null` inhabits the BAML `null` type.
        Ty::Null { .. } => "java.lang.Void".to_string(),
        // Java has no literal types; a literal erases to its base.
        Ty::Literal(lit, ..) => match lit {
            baml_base::Literal::Int(_) => primitive(pos, "long", "java.lang.Long"),
            baml_base::Literal::Bigint(_) => "java.math.BigInteger".to_string(),
            baml_base::Literal::Float(_) => primitive(pos, "double", "java.lang.Double"),
            baml_base::Literal::String(_) => "java.lang.String".to_string(),
            baml_base::Literal::Bool(_) => primitive(pos, "boolean", "java.lang.Boolean"),
        },
        Ty::Uint8Array { .. } => "byte[]".to_string(),
        Ty::Media(kind, _) => match kind {
            baml_base::MediaKind::Image => "baml_sdk.baml.media.Image".to_string(),
            baml_base::MediaKind::Audio => "baml_sdk.baml.media.Audio".to_string(),
            baml_base::MediaKind::Video => "baml_sdk.baml.media.Video".to_string(),
            baml_base::MediaKind::Pdf => "baml_sdk.baml.media.Pdf".to_string(),
            // Any-media has no dedicated wrapper type.
            baml_base::MediaKind::Generic => "java.lang.Object".to_string(),
        },
        Ty::Class(name, args, _) => {
            let mut out = qualified_type(name);
            if !args.is_empty() {
                let rendered: Vec<String> = args
                    .iter()
                    .map(|a| {
                        annotate_element(
                            a,
                            translate_ty(a, TyPosition::Boxed, ctx, sink),
                            ctx.aliases,
                        )
                    })
                    .collect();
                out.push('<');
                out.push_str(&rendered.join(", "));
                out.push('>');
            }
            out
        }
        // An enum-variant type denotes a value of its enum; render it as
        // the enum's FQN (mirrors python treating `Enum` / `EnumVariant`
        // identically).
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => qualified_type(name),
        Ty::TypeAlias(name, _) => {
            match ctx.aliases.get(name) {
                // Recursive aliases mint a nominal type named after the
                // alias (erasure would recurse forever); the emitter
                // generates its body alongside the alias's package.
                Some((_, true)) | None => qualified_type(name),
                Some((resolved, false)) => translate_ty(resolved, pos, ctx, sink),
            }
        }
        Ty::TypeVar(name, _) => java_identifier(name.as_str()),
        Ty::List(inner, _) => format!(
            "java.util.List<{}>",
            annotate_element(
                inner,
                translate_ty(inner, TyPosition::Boxed, ctx, sink),
                ctx.aliases
            )
        ),
        // All map keys are stringified engine-side (str/int/bool/enum
        // keys all arrive as strings), so the Java map key is String.
        Ty::Map { value, .. } => format!(
            "java.util.Map<java.lang.String, {}>",
            annotate_element(
                value,
                translate_ty(value, TyPosition::Boxed, ctx, sink),
                ctx.aliases
            )
        ),
        Ty::Union(items, _) => translate_union(items, ctx, sink),
        Ty::Unknown { .. } => "java.lang.Object".to_string(),
        Ty::Function { params, ret, .. } => translate_callable(params, ret, ctx, sink),
        Ty::Void { .. } => match pos {
            TyPosition::TopLevel => "void".to_string(),
            TyPosition::Boxed => "java.lang.Void".to_string(),
        },
        Ty::RustType { .. } => "baml_bridge.BamlHandle".to_string(),
        // Types the Java SDK does not model yet: fall back to the opaque
        // `java.lang.Object` (mirrors python's `typing.Any` / TS's
        // `unknown` stance for these variants).
        Ty::Interface(..)
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::Future(..) => "java.lang.Object".to_string(),
    }
}

fn primitive(pos: TyPosition, unboxed: &str, boxed: &str) -> String {
    match pos {
        TyPosition::TopLevel => unboxed.to_string(),
        TyPosition::Boxed => boxed.to_string(),
    }
}

/// The `JSpecify` nullness annotation, fully qualified and written inline — the
/// emitter carries no imports, so annotations are qualified at the use site like
/// every other type reference. It is a `TYPE_USE` annotation (`JSpecify` targets
/// `TYPE_USE` only), so it must sit immediately before the simple type name even
/// on a qualified type (`java.lang.@…Nullable String`); see [`annotate_nullable`].
pub(crate) const NULLABLE_ANNOTATION: &str = "@org.jspecify.annotations.Nullable";

/// Whether a BAML type is genuinely nullable — it admits the `null` value:
///
/// - a `T | null` union (any non-null arm count) — the arm-stripping point in
///   [`translate_union`] that collapses `T | null` to boxed-`T` is exactly the
///   point that knows the position is nullable; this query recovers that bit
///   alongside the rendered type, without stringly-flagging it;
/// - the bare `null` type (inhabited only by `null`);
/// - a NON-recursive alias whose resolved target is nullable (recursive aliases
///   mint a nominal reference type, treated as non-null by default).
///
/// Everything else is non-null (a plain reference, a list/map/class value, an
/// unboxed primitive). This is the emitter's single source of truth for where a
/// `org.jspecify.annotations.Nullable` belongs.
pub(crate) fn is_nullable(ty: &Ty, aliases: &AliasTable) -> bool {
    match ty {
        Ty::Union(items, _) => items.iter().any(|t| matches!(t, Ty::Null { .. })),
        Ty::Null { .. } => true,
        Ty::TypeAlias(name, _) => match aliases.get(name) {
            Some((resolved, false)) => is_nullable(resolved, aliases),
            _ => false,
        },
        _ => false,
    }
}

/// Insert the `JSpecify` `@Nullable` type-use annotation into a rendered Java type
/// at the syntactically-correct position:
///
/// - an array reference (`byte[]`) marks the array itself nullable —
///   `byte @Nullable []` (annotating before `[]`, NOT the element);
/// - a generic type (`java.util.List<…>`) annotates the OUTER raw type's simple
///   name, keeping the (possibly already element-annotated) type-args intact —
///   `java.util.@Nullable List<…>`;
/// - a qualified name (`java.lang.Long`) annotates the trailing simple name —
///   `java.lang.@Nullable Long`;
/// - a bare simple name (a type variable `T`) — `@Nullable T`.
///
/// A `TYPE_USE` annotation on a qualified name is only legal immediately before
/// the simple name, which is why this splits rather than prefixing the whole
/// expression.
pub(crate) fn annotate_nullable(rendered: &str) -> String {
    // Array reference: the annotation on the array level marks the array itself
    // nullable (element nullability, if any, was already baked into `base`).
    if let Some(base) = rendered.strip_suffix("[]") {
        return format!("{base} {NULLABLE_ANNOTATION} []");
    }
    // Split off any generic arguments; the annotation binds the OUTER raw type.
    let (raw, generic) = match rendered.find('<') {
        Some(idx) => (&rendered[..idx], &rendered[idx..]),
        None => (rendered, ""),
    };
    match raw.rfind('.') {
        Some(dot) => format!(
            "{}.{NULLABLE_ANNOTATION} {}{generic}",
            &raw[..dot],
            &raw[dot + 1..]
        ),
        None => format!("{NULLABLE_ANNOTATION} {raw}{generic}"),
    }
}

/// Annotate a rendered element/argument type with `@Nullable` iff the BAML type
/// in that position is nullable — the element-nullability half of the `JSpecify`
/// pass, applied at the `Boxed` container/generic-argument recursion sites
/// ([`translate_ty`]'s `List` / `Map` / `Class` arms) where the boxed-inner
/// collapse would otherwise drop the `null` arm.
fn annotate_element(ty: &Ty, rendered: String, aliases: &AliasTable) -> String {
    if is_nullable(ty, aliases) {
        annotate_nullable(&rendered)
    } else {
        rendered
    }
}

/// FQN of a generated (or runtime-owned) named type:
/// `baml_sdk.<package>.<Ident>`. The runtime-owned `ai.stream.Stream`
/// resolves to the runtime library's `baml_bridge.BamlStream` (no
/// generated class exists for it; the media classes keep their
/// `baml_sdk.baml.media.*` paths because the runtime jar provides
/// classes at exactly those packages).
fn qualified_type(name: &Name) -> String {
    if name.to_string() == AI_STREAM_STREAM {
        return "baml_bridge.BamlStream".to_string();
    }
    if name.to_string() == AI_STREAM_DONE {
        return "baml_sdk.ai.stream.Done".to_string();
    }
    let pkg = route(name);
    format!(
        "{}.{}",
        pkg.java_package(),
        java_identifier(name.name().as_str())
    )
}

/// Union translation. `T | null` collapses to boxed-`T`; a literal
/// union over one base erases to the base; anything else mints a
/// nominal sealed type in the current package.
fn translate_union(items: &[Ty], ctx: &TranslateCtx<'_>, sink: &mut UnionSink) -> String {
    let non_null: Vec<&Ty> = items
        .iter()
        .filter(|t| !matches!(t, Ty::Null { .. }))
        .collect();

    match non_null.len() {
        0 => "java.lang.Void".to_string(),
        // `T?` — nullability is expressed by boxing, never by a union
        // type.
        1 => translate_ty(non_null[0], TyPosition::Boxed, ctx, sink),
        _ => {
            // Literal unions over a single base type erase to the base
            // (`"a" | "b"` → String). Mixed-base unions fall through to
            // the nominal union.
            if let Some(base) = common_literal_base(&non_null) {
                return base;
            }
            // A union with a TypeVar arm (`T | string | null`,
            // `T | U | int`) degenerates to `java.lang.Object`: the arm is
            // whatever the caller's bound `T` inhabits, so no positional
            // arity family can hold it and callers pass the raw value (the
            // engine does the real check). This covers both the parameter
            // position (an explicit/inferred generic call passes a bare
            // value) and the field position (its accessor returns the bare
            // decoded value). Decode stays type-directed via the per-binding
            // union descriptor, unaffected by this Java-type choice.
            if non_null.iter().any(|arm| {
                let mut tvs = Vec::new();
                collect_type_vars(arm, &mut tvs);
                !tvs.is_empty()
            }) {
                return "java.lang.Object".to_string();
            }
            // TEAM DECISION (2026-07-16): anonymous unions render as the
            // runtime's generic arity family in DECLARATION order; arm
            // selection is type-directed at decode time, so no nominal
            // type (and no wire-order trust) is involved. Arity > 10
            // falls back to Object until the threshold/alias policy
            // lands (decoder then uses the wire-driven path).
            let n = non_null.len();
            if n > 10 {
                return "java.lang.Object".to_string();
            }
            let rendered: Vec<String> = non_null
                .iter()
                .map(|t| translate_ty(t, TyPosition::Boxed, ctx, sink))
                .collect();
            format!("baml_bridge.Union{n}<{}>", rendered.join(", "))
        }
    }
}

/// If every arm is a literal of the same base type, the union erases
/// to that base (boxed — a multi-literal union in Java carries no
/// compile-time constraint anyway, and these appear in nullable and
/// generic positions).
fn common_literal_base(arms: &[&Ty]) -> Option<String> {
    let mut base: Option<&'static str> = None;
    for arm in arms {
        let Ty::Literal(lit, ..) = arm else {
            return None;
        };
        let this = match lit {
            baml_base::Literal::Int(_) => "java.lang.Long",
            baml_base::Literal::Bigint(_) => "java.math.BigInteger",
            baml_base::Literal::Float(_) => "java.lang.Double",
            baml_base::Literal::String(_) => "java.lang.String",
            baml_base::Literal::Bool(_) => "java.lang.Boolean",
        };
        match base {
            None => base = Some(this),
            Some(prev) if prev == this => {}
            Some(_) => return None,
        }
    }
    base.map(str::to_string)
}

/// Collect the type variables a type expression references, in first-
/// appearance order (deduped). A minted union over such arms must be
/// generic over exactly this list.
pub(crate) fn collect_type_vars(ty: &Ty, out: &mut Vec<String>) {
    match ty {
        Ty::TypeVar(name, _) => {
            let id = java_identifier(name.as_str());
            if !out.contains(&id) {
                out.push(id);
            }
        }
        Ty::Class(_, args, _) => {
            for a in args {
                collect_type_vars(a, out);
            }
        }
        Ty::List(inner, _) => collect_type_vars(inner, out),
        Ty::Map { key, value, .. } => {
            collect_type_vars(key, out);
            collect_type_vars(value, out);
        }
        Ty::Union(items, _) => {
            for i in items {
                collect_type_vars(i, out);
            }
        }
        Ty::Function { params, ret, .. } => {
            for p in params {
                collect_type_vars(&p.ty, out);
            }
            collect_type_vars(ret, out);
        }
        _ => {}
    }
}

/// The Java `baml_bridge.BamlType` wildcard — a decode-only "decode this value
/// wire-driven / match any arm" hint. A wholly decode-inert descriptor renders
/// as this; [`descriptor_expr_opt`] collapses it to a `null` argument.
const BAMLTYPE_UNKNOWN: &str = "baml_bridge.BamlType.UNKNOWN";

/// Render a type-directed decode descriptor as a Java `baml_bridge.BamlType`
/// builder expression — the typed replacement for the old stringly grammar. This
/// is the never-`null` form used for the children of list/map/union descriptors;
/// the top-level entry point is [`descriptor_expr_opt`], which collapses a wholly
/// wire-driven descriptor to the literal `null` (the runtime's wire-driven mode).
///
/// A class/enum/recursive-alias renders as a BARE `classByFqn(<fqn>)` (decode
/// resolves the FQN via the registry — the class-decode path is by FQN, never by
/// generic-args identity); `T?` collapses to its inner (nullability never affects
/// decode); an anonymous union renders as `BamlType.union(<arms…>)` in
/// declaration order (a union with a `TypeVar` arm, or a same-base literal union,
/// degenerates to the base / `UNKNOWN`, matching [`translate_union`]'s Java type);
/// a decode-inert leaf (bigint / uint8array / null / void / media / callable /
/// handle / unknown / unmodeled) renders as `UNKNOWN`.
pub(crate) fn descriptor_expr(ty: &Ty, aliases: &AliasTable) -> String {
    match ty {
        Ty::Int { .. } => "baml_bridge.BamlType.INT".to_string(),
        Ty::Float { .. } => "baml_bridge.BamlType.FLOAT".to_string(),
        Ty::String { .. } => "baml_bridge.BamlType.STRING".to_string(),
        Ty::Bool { .. } => "baml_bridge.BamlType.BOOL".to_string(),
        Ty::List(inner, _) => {
            format!(
                "baml_bridge.BamlType.list({})",
                descriptor_expr(inner, aliases)
            )
        }
        // The declared key token is preserved (the decoder ignores it — map keys
        // are stringified engine-side — but a map union arm matches on it); in
        // practice the codegen key is always `String`.
        Ty::Map { key, value, .. } => format!(
            "baml_bridge.BamlType.map({}, {})",
            descriptor_expr(key, aliases),
            descriptor_expr(value, aliases)
        ),
        // A class descriptor is the BARE FQN — decode resolves the class by FQN
        // (`TypeRegistry.isClass`); the generic-args identity is for union-arm /
        // registry keying only ([`registry_arm_expr`]), not the class-decode path.
        Ty::Class(name, _, _) => class_by_fqn_expr(&crate::baml_fqn(name)),
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            class_by_fqn_expr(&crate::baml_fqn(name))
        }
        Ty::TypeAlias(name, _) => match aliases.get(name) {
            Some((resolved, false)) => descriptor_expr(resolved, aliases),
            _ => class_by_fqn_expr(&crate::baml_fqn(name)),
        },
        Ty::TypeVar(name, _) => {
            format!("baml_bridge.BamlType.typeVar({:?})", name.as_str())
        }
        Ty::Literal(lit, ..) => literal_expr(lit),
        Ty::Union(items, _) => descriptor_union_expr(items, aliases),
        _ => BAMLTYPE_UNKNOWN.to_string(),
    }
}

/// The top-level decode descriptor: [`descriptor_expr`], or `None` when the whole
/// type is wire-driven (renders as `UNKNOWN`) — the emitter then passes the
/// literal `null` (the runtime's wire-driven decode mode, also used for streaming).
pub(crate) fn descriptor_expr_opt(ty: &Ty, aliases: &AliasTable) -> Option<String> {
    let e = descriptor_expr(ty, aliases);
    if e == BAMLTYPE_UNKNOWN { None } else { Some(e) }
}

/// The union arm of [`descriptor_expr`]: `T | null` collapses to boxed-inner; a
/// same-base literal union erases to the base primitive; a union carrying a
/// `TypeVar` arm degenerates to `UNKNOWN` (wire-driven); otherwise a
/// `BamlType.union(<arms…>)` in declaration order.
fn descriptor_union_expr(items: &[Ty], aliases: &AliasTable) -> String {
    let non_null: Vec<&Ty> = items
        .iter()
        .filter(|t| !matches!(t, Ty::Null { .. }))
        .collect();
    match non_null.len() {
        0 => BAMLTYPE_UNKNOWN.to_string(),
        1 => descriptor_expr(non_null[0], aliases),
        _ => {
            if common_literal_base(&non_null).is_some() {
                if let Ty::Literal(lit, ..) = non_null[0] {
                    return match lit {
                        baml_base::Literal::Int(_) => "baml_bridge.BamlType.INT".to_string(),
                        baml_base::Literal::Float(_) => "baml_bridge.BamlType.FLOAT".to_string(),
                        baml_base::Literal::String(_) => "baml_bridge.BamlType.STRING".to_string(),
                        baml_base::Literal::Bool(_) => "baml_bridge.BamlType.BOOL".to_string(),
                        // bigint has no BamlType primitive → wire-driven.
                        baml_base::Literal::Bigint(_) => BAMLTYPE_UNKNOWN.to_string(),
                    };
                }
            }
            if non_null.iter().any(|arm| {
                let mut tvs = Vec::new();
                collect_type_vars(arm, &mut tvs);
                !tvs.is_empty()
            }) {
                return BAMLTYPE_UNKNOWN.to_string();
            }
            let arms: Vec<String> = non_null
                .iter()
                .map(|a| descriptor_expr(a, aliases))
                .collect();
            format!("baml_bridge.BamlType.union({})", arms.join(", "))
        }
    }
}

/// A union-arm identity as a `baml_bridge.BamlType` (the arm token
/// `registerUnion` / `registerUnionAlias` carry). Full fidelity, mirroring
/// [`signature_token`]: named types render as `classByFqn` (kind-agnostic —
/// decode matches by FQN), a generic class carries its args so two
/// instantiations get distinct registry keys, and it never collapses (a union
/// arm is an individual type). The runtime derives the equal `BamlType` from the
/// wire `self_type` (`ProtoReader.wireArmType`), so a wire union's arm set equals
/// the registered one.
pub(crate) fn registry_arm_expr(ty: &Ty, aliases: &AliasTable) -> String {
    match ty {
        Ty::Int { .. } => "baml_bridge.BamlType.INT".to_string(),
        Ty::Float { .. } => "baml_bridge.BamlType.FLOAT".to_string(),
        Ty::String { .. } => "baml_bridge.BamlType.STRING".to_string(),
        Ty::Bool { .. } => "baml_bridge.BamlType.BOOL".to_string(),
        Ty::List(inner, _) => {
            format!(
                "baml_bridge.BamlType.list({})",
                registry_arm_expr(inner, aliases)
            )
        }
        Ty::Map { key, value, .. } => format!(
            "baml_bridge.BamlType.map({}, {})",
            registry_arm_expr(key, aliases),
            registry_arm_expr(value, aliases)
        ),
        Ty::Class(name, args, _) => {
            let fqn = crate::baml_fqn(name);
            if args.is_empty() {
                class_by_fqn_expr(&fqn)
            } else {
                let inner: Vec<String> =
                    args.iter().map(|a| registry_arm_expr(a, aliases)).collect();
                format!(
                    "baml_bridge.BamlType.classByFqn({fqn:?}, {})",
                    inner.join(", ")
                )
            }
        }
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            class_by_fqn_expr(&crate::baml_fqn(name))
        }
        Ty::TypeAlias(name, _) => match aliases.get(name) {
            Some((resolved, false)) => registry_arm_expr(resolved, aliases),
            _ => class_by_fqn_expr(&crate::baml_fqn(name)),
        },
        Ty::TypeVar(name, _) => {
            format!("baml_bridge.BamlType.typeVar({:?})", name.as_str())
        }
        Ty::Literal(lit, ..) => literal_expr(lit),
        _ => BAMLTYPE_UNKNOWN.to_string(),
    }
}

fn class_by_fqn_expr(fqn: &str) -> String {
    format!("baml_bridge.BamlType.classByFqn({fqn:?})")
}

fn literal_expr(lit: &baml_base::Literal) -> String {
    match lit {
        baml_base::Literal::Int(v) => format!("baml_bridge.BamlType.literalInt({v}L)"),
        baml_base::Literal::Bool(v) => format!("baml_bridge.BamlType.literalBool({v})"),
        baml_base::Literal::String(s) => {
            format!("baml_bridge.BamlType.literalString({s:?})")
        }
        baml_base::Literal::Float(s) => {
            format!("baml_bridge.BamlType.literalFloat({s:?})")
        }
        baml_base::Literal::Bigint(v) => format!(
            "baml_bridge.BamlType.literalBigint(new java.math.BigInteger({:?}))",
            v.to_string()
        ),
    }
}

/// Per-arm record tokens for a whole union arm set, with COLLISIONS resolved:
/// two arms whose simple [`union_arm_token`] coincides (`a.Node` and `b.Node`
/// both → `Node`, which would mint two `NodeValue` records in one sealed
/// interface — a javac duplicate) are disambiguated by a trailing counter
/// (`Node`, `Node2`, …). A set with no collision is returned byte-identically,
/// so existing single-name arms keep their record names. Both the emitter
/// ([`crate::emit::render_union`]) and the registry
/// ([`crate::union_registration`]) derive record names through this, so they
/// stay in lockstep.
pub(crate) fn union_arm_tokens(arms: &[Ty]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(arms.len());
    for arm in arms {
        let base = union_arm_token(arm);
        if !out.contains(&base) {
            out.push(base);
            continue;
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}{n}");
            if !out.contains(&candidate) {
                out.push(candidate);
                break;
            }
            n += 1;
        }
    }
    out
}

/// Deterministic per-arm token, used to name a recursive alias's
/// nominal union arm records (`{token}Value`) and their registry
/// entries. Literal arms use a `K`-prefixed token (legacy Go
/// precedent): `"draft"` → `Kdraft`, `1` → `IntK1`, `true` →
/// `BoolKTrue`.
pub(crate) fn union_arm_token(ty: &Ty) -> String {
    match ty {
        Ty::Int { .. } => "Int".to_string(),
        Ty::Bigint { .. } => "Bigint".to_string(),
        Ty::Float { .. } => "Float".to_string(),
        Ty::String { .. } => "String".to_string(),
        Ty::Bool { .. } => "Boolean".to_string(),
        Ty::Null { .. } => "Null".to_string(),
        Ty::Uint8Array { .. } => "Uint8Array".to_string(),
        Ty::Media(kind, _) => format!("{kind:?}"),
        Ty::Class(name, _, _)
        | Ty::Enum(name, _)
        | Ty::EnumVariant(name, _, _)
        | Ty::TypeAlias(name, _) => java_identifier(name.name().as_str()),
        Ty::TypeVar(name, _) => java_identifier(name.as_str()),
        Ty::List(inner, _) => format!("{}List", union_arm_token(inner)),
        Ty::Map { value, .. } => format!("{}Map", union_arm_token(value)),
        Ty::Literal(lit, ..) => match lit {
            baml_base::Literal::String(s) => format!("K{}", java_identifier(s)),
            baml_base::Literal::Int(v) => {
                format!("IntK{}", v.to_string().replace('-', "Neg"))
            }
            baml_base::Literal::Bigint(v) => {
                format!("BigintK{}", v.to_string().replace('-', "Neg"))
            }
            baml_base::Literal::Float(s) => format!("FloatK{}", java_identifier(s)),
            baml_base::Literal::Bool(v) => {
                if *v {
                    "BoolKTrue".to_string()
                } else {
                    "BoolKFalse".to_string()
                }
            }
        },
        Ty::Unknown { .. } => "Unknown".to_string(),
        Ty::Function { .. } => "Callable".to_string(),
        Ty::Void { .. } => "Void".to_string(),
        Ty::Never { .. } => "Never".to_string(),
        Ty::RustType { .. } => "Handle".to_string(),
        Ty::Interface(..) => "Interface".to_string(),
        Ty::Type { .. } => "Type".to_string(),
        Ty::Resource { .. } => "Resource".to_string(),
        Ty::PromptAst { .. } => "PromptAst".to_string(),
        Ty::Future(..) => "Future".to_string(),
        // validate() bans nested unions; unreachable in valid pools.
        Ty::Union(..) => "Union".to_string(),
    }
}

/// Host-callable types map onto `java.util.function` shapes by arity.
/// A callable whose own type carries optional parameters (or arity > 2)
/// has no `java.util.function` equivalent — Java SAMs are fixed-arity and
/// have no structural `$opts?: { … }` — so a `@FunctionalInterface` is
/// minted (design point E), one per distinct signature within the package,
/// and recorded in the [`UnionSink`] for the emitter to render.
fn translate_callable(
    params: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    use baml_codegen_types::CodegenFunctionParamMode;
    let has_optional = params
        .iter()
        .any(|p| matches!(p.mode, CodegenFunctionParamMode::Optional));
    if !has_optional && params.len() <= 2 {
        // Plain arity-≤-2 all-required callable → a java.util.function shape.
        let ret_is_unit = matches!(ret, Ty::Void { .. });
        let p: Vec<String> = params
            .iter()
            .map(|p| translate_ty(&p.ty, TyPosition::Boxed, ctx, sink))
            .collect();
        let r = translate_ty(ret, TyPosition::Boxed, ctx, sink);
        return match (p.len(), ret_is_unit) {
            (0, true) => "java.lang.Runnable".to_string(),
            (0, false) => format!("java.util.function.Supplier<{r}>"),
            (1, true) => format!("java.util.function.Consumer<{}>", p[0]),
            (1, false) => format!("java.util.function.Function<{}, {r}>", p[0]),
            (2, true) => format!("java.util.function.BiConsumer<{}, {}>", p[0], p[1]),
            (2, false) => format!("java.util.function.BiFunction<{}, {}, {r}>", p[0], p[1]),
            _ => unreachable!("arity > 2 handled below"),
        };
    }

    // Needs a generated functional interface. Dedup per package by a structural
    // signature key so identical callable types share one interface.
    let sig = callback_signature_key(params, ret, ctx.aliases);
    if let Some(name) = sink.callbacks.get(&ctx.pkg).and_then(|v| {
        v.iter()
            .find(|(k, _)| *k == sig)
            .map(|(_, i)| i.name.clone())
    }) {
        return format!("{}.{}", ctx.pkg.java_package(), name);
    }

    // Translate the SAM's required params, optional-bag fields, and return type.
    // (Recursion may register OTHER interfaces in the sink — harmless; a callable
    // cannot contain itself, so it never re-registers this same signature.)
    let mut required: Vec<(String, String)> = Vec::new();
    let mut optionals: Vec<(String, String)> = Vec::new();
    for (i, p) in params.iter().enumerate() {
        let ty = translate_ty(&p.ty, TyPosition::Boxed, ctx, sink);
        match p.mode {
            CodegenFunctionParamMode::Optional => {
                let wire = p
                    .name
                    .as_ref()
                    .map_or_else(|| format!("opt{i}"), |n| n.as_str().to_string());
                optionals.push((wire, ty));
            }
            CodegenFunctionParamMode::Required => {
                let ident = p
                    .name
                    .as_ref()
                    .map_or_else(|| format!("arg{i}"), |n| java_identifier(n.as_str()));
                required.push((ident, ty));
            }
        }
    }
    let ret_ty = translate_ty(ret, TyPosition::Boxed, ctx, sink);
    let base = callback_base_name(params, has_optional);

    let entries = sink.callbacks.entry(ctx.pkg.clone()).or_default();
    // Defensive re-check after the recursive translate_ty calls above.
    if let Some((_, iface)) = entries.iter().find(|(k, _)| *k == sig) {
        return format!("{}.{}", ctx.pkg.java_package(), iface.name);
    }
    let name = dedup_callback_name(&base, entries);
    entries.push((
        sig,
        CallbackInterface {
            name: name.clone(),
            required,
            optionals,
            ret: ret_ty,
        },
    ));
    format!("{}.{}", ctx.pkg.java_package(), name)
}

/// A structural signature key for a callable, used to dedup the minted
/// interface per package: identical callable types (same param names, modes,
/// types, and return) share one interface. Uses the same token vocabulary as
/// the union/decode signatures.
fn callback_signature_key(
    params: &[baml_codegen_types::CallableParam],
    ret: &Ty,
    aliases: &AliasTable,
) -> String {
    use baml_codegen_types::CodegenFunctionParamMode;
    let mut s = String::from("cb(");
    for p in params {
        let opt = matches!(p.mode, CodegenFunctionParamMode::Optional);
        let name = p.name.as_ref().map_or("", |n| n.as_str());
        s.push_str(name);
        s.push_str(if opt { "?:" } else { ":" });
        s.push_str(&crate::signature_token(&p.ty, aliases));
        s.push(',');
    }
    s.push_str(")->");
    s.push_str(&crate::signature_token(ret, aliases));
    s
}

/// The base name of a minted callback interface: the required params' arm
/// tokens concatenated, an `Opt` marker when the callable has optionals, then
/// `Callback` — e.g. `(x: int, y?: int, z?: int) -> int` → `IntOptCallback`.
/// Deduped against colliding distinct signatures by [`dedup_callback_name`].
fn callback_base_name(params: &[baml_codegen_types::CallableParam], has_optional: bool) -> String {
    use baml_codegen_types::CodegenFunctionParamMode;
    let mut s = String::new();
    for p in params
        .iter()
        .filter(|p| matches!(p.mode, CodegenFunctionParamMode::Required))
    {
        s.push_str(&union_arm_token(&p.ty));
    }
    if has_optional {
        s.push_str("Opt");
    }
    s.push_str("Callback");
    s
}

/// Disambiguate a callback interface base name against the names already
/// assigned in the package: `IntOptCallback`, then `IntOptCallback2`, … for
/// distinct signatures that would otherwise collide.
fn dedup_callback_name(base: &str, entries: &[(String, CallbackInterface)]) -> String {
    if entries.iter().all(|(_, i)| i.name != base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}{n}");
        if entries.iter().all(|(_, i)| i.name != candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{CallableParam, CodegenFunctionParamMode};

    use super::*;

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    fn ctx_in(aliases: &AliasTable) -> TranslateCtx<'_> {
        TranslateCtx {
            aliases,
            pkg: PackagePath {
                segments: vec!["callables".to_string()],
            },
        }
    }

    fn tr(ty: &Ty, pos: TyPosition) -> String {
        let aliases = AliasTable::new();
        let ctx = ctx_in(&aliases);
        let mut sink = UnionSink::default();
        translate_ty(ty, pos, &ctx, &mut sink)
    }

    // Leaf-type constructors: the codegen `Ty` is now a re-export of
    // `baml_type::CodegenTy`, whose variants carry a `TyAttr` (and
    // literals a `Freshness`). These keep the assertions readable.
    fn a() -> baml_base::TyAttr {
        baml_base::TyAttr::EMPTY
    }
    fn int() -> Ty {
        Ty::Int { attr: a() }
    }
    fn bigint() -> Ty {
        Ty::Bigint { attr: a() }
    }
    fn float() -> Ty {
        Ty::Float { attr: a() }
    }
    fn string() -> Ty {
        Ty::String { attr: a() }
    }
    fn bool_() -> Ty {
        Ty::Bool { attr: a() }
    }
    fn null() -> Ty {
        Ty::Null { attr: a() }
    }
    fn uint8array() -> Ty {
        Ty::Uint8Array { attr: a() }
    }
    fn void() -> Ty {
        Ty::Void { attr: a() }
    }
    fn unknown() -> Ty {
        Ty::Unknown { attr: a() }
    }
    fn rust_type() -> Ty {
        Ty::RustType { attr: a() }
    }
    fn media(kind: baml_base::MediaKind) -> Ty {
        Ty::Media(kind, a())
    }
    fn literal(lit: baml_base::Literal) -> Ty {
        Ty::Literal(lit, baml_codegen_types::Freshness::Regular, a())
    }
    fn list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner), a())
    }
    fn map(key: Ty, value: Ty) -> Ty {
        Ty::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: a(),
        }
    }
    fn union(items: Vec<Ty>) -> Ty {
        Ty::Union(items, a())
    }
    fn class_ty(n: Name, args: Vec<Ty>) -> Ty {
        Ty::Class(n, args, a())
    }
    fn enum_ty(n: Name) -> Ty {
        Ty::Enum(n, a())
    }
    fn alias_ty(n: Name) -> Ty {
        Ty::TypeAlias(n, a())
    }
    fn typevar(n: BaseName) -> Ty {
        Ty::TypeVar(baml_codegen_types::ParamTy::new(0, n), a())
    }
    fn callable(params: Vec<CallableParam>, ret: Ty) -> Ty {
        Ty::Function {
            params,
            ret: Box::new(ret),
            throws: Box::new(Ty::Never { attr: a() }),
            attr: a(),
        }
    }

    #[test]
    fn primitives_positional_boxing() {
        assert_eq!(tr(&int(), TyPosition::TopLevel), "long");
        assert_eq!(tr(&int(), TyPosition::Boxed), "java.lang.Long");
        assert_eq!(tr(&float(), TyPosition::TopLevel), "double");
        assert_eq!(tr(&bool_(), TyPosition::Boxed), "java.lang.Boolean");
        assert_eq!(tr(&string(), TyPosition::TopLevel), "java.lang.String");
        assert_eq!(tr(&bigint(), TyPosition::TopLevel), "java.math.BigInteger");
        assert_eq!(tr(&uint8array(), TyPosition::TopLevel), "byte[]");
        assert_eq!(tr(&null(), TyPosition::TopLevel), "java.lang.Void");
    }

    #[test]
    fn unit_is_void_only_at_top_level() {
        assert_eq!(tr(&void(), TyPosition::TopLevel), "void");
        assert_eq!(tr(&void(), TyPosition::Boxed), "java.lang.Void");
    }

    #[test]
    fn containers_box_their_elements() {
        assert_eq!(
            tr(&list(int()), TyPosition::TopLevel),
            "java.util.List<java.lang.Long>"
        );
        assert_eq!(
            tr(&map(string(), bool_()), TyPosition::TopLevel),
            "java.util.Map<java.lang.String, java.lang.Boolean>"
        );
    }

    #[test]
    fn class_and_enum_are_fqn() {
        let c = class_ty(name("user", &["lorem"], "Resume"), vec![]);
        assert_eq!(tr(&c, TyPosition::TopLevel), "baml_sdk.lorem.Resume");
        let e = enum_ty(name("user", &["ipsum"], "Sentiment"));
        assert_eq!(tr(&e, TyPosition::TopLevel), "baml_sdk.ipsum.Sentiment");
        let s = class_ty(name("user", &["lorem"], "Resume$stream"), vec![]);
        assert_eq!(tr(&s, TyPosition::TopLevel), "baml_sdk.lorem.Resume$stream");

        let stream = class_ty(name("ai", &["stream"], "Stream"), vec![string(), string()]);
        assert_eq!(
            tr(&stream, TyPosition::TopLevel),
            "baml_bridge.BamlStream<java.lang.String, java.lang.String>"
        );
        let done = class_ty(name("ai", &["stream"], "Done"), vec![]);
        assert_eq!(tr(&done, TyPosition::TopLevel), "baml_sdk.ai.stream.Done");
    }

    #[test]
    fn generic_class_args_box() {
        let c = class_ty(name("user", &["generics"], "Wrapper"), vec![int()]);
        assert_eq!(
            tr(&c, TyPosition::TopLevel),
            "baml_sdk.generics.Wrapper<java.lang.Long>"
        );
    }

    #[test]
    fn typevar_renders_bare() {
        assert_eq!(tr(&typevar(BaseName::new("T")), TyPosition::TopLevel), "T");
    }

    #[test]
    fn media_types() {
        assert_eq!(
            tr(&media(baml_base::MediaKind::Image), TyPosition::TopLevel),
            "baml_sdk.baml.media.Image"
        );
        assert_eq!(
            tr(&media(baml_base::MediaKind::Generic), TyPosition::TopLevel),
            "java.lang.Object"
        );
    }

    #[test]
    fn optional_collapses_to_boxed_inner() {
        let opt_int = union(vec![int(), null()]);
        assert_eq!(tr(&opt_int, TyPosition::TopLevel), "java.lang.Long");
        let opt_class = union(vec![
            class_ty(name("user", &["lorem"], "Resume"), vec![]),
            null(),
        ]);
        assert_eq!(
            tr(&opt_class, TyPosition::TopLevel),
            "baml_sdk.lorem.Resume"
        );
    }

    #[test]
    fn multi_arm_union_renders_generic_arity_family() {
        // Anonymous unions render inline as `baml_bridge.Union{n}<...>`
        // in declaration order (arms boxed); nothing records into the
        // now-vestigial sink.
        let aliases = AliasTable::new();
        let ctx = ctx_in(&aliases);
        let mut sink = UnionSink::default();
        let u = union(vec![int(), string()]);
        let expr = translate_ty(&u, TyPosition::TopLevel, &ctx, &mut sink);
        assert_eq!(expr, "baml_bridge.Union2<java.lang.Long, java.lang.String>");
        assert!(sink.unions.is_empty());
    }

    #[test]
    fn nullable_multi_arm_union_strips_null_arm() {
        let u = union(vec![int(), string(), null()]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_bridge.Union2<java.lang.Long, java.lang.String>"
        );
    }

    #[test]
    fn union_arms_render_in_declaration_order() {
        let t = class_ty(name("user", &["unions"], "T"), vec![]);
        let u = union(vec![t, string()]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_bridge.Union2<baml_sdk.unions.T, java.lang.String>"
        );
    }

    #[test]
    fn union_over_ten_arms_falls_back_to_object() {
        // Arity > 10 exceeds the Union2..Union10 family; until the
        // threshold/alias policy lands the emitter falls back to Object
        // (the decoder then uses the wire-driven path).
        let arms: Vec<Ty> = (0..11)
            .map(|i| class_ty(name("user", &["unions"], &format!("C{i}")), vec![]))
            .collect();
        let u = union(arms);
        assert_eq!(tr(&u, TyPosition::TopLevel), "java.lang.Object");
    }

    #[test]
    fn union_with_typevar_arm_degenerates_to_object() {
        // A union with a TypeVar arm can't be held by a positional arity family
        // (the arm is whatever the caller's `T` inhabits) → `java.lang.Object`.
        let u = union(vec![typevar(BaseName::new("T")), string(), null()]);
        assert_eq!(tr(&u, TyPosition::TopLevel), "java.lang.Object");
        let u2 = union(vec![
            typevar(BaseName::new("T")),
            typevar(BaseName::new("U")),
            int(),
        ]);
        assert_eq!(tr(&u2, TyPosition::TopLevel), "java.lang.Object");
        // A concrete multi-arm union is unaffected.
        assert_eq!(
            tr(&union(vec![int(), string()]), TyPosition::TopLevel),
            "baml_bridge.Union2<java.lang.Long, java.lang.String>"
        );
        // A nested TypeVar (e.g. `GenericBox<T> | string`) also degenerates.
        let nested = union(vec![
            class_ty(
                name("user", &["g"], "GenericBox"),
                vec![typevar(BaseName::new("T"))],
            ),
            string(),
        ]);
        assert_eq!(tr(&nested, TyPosition::TopLevel), "java.lang.Object");
    }

    #[test]
    fn literal_union_same_base_erases() {
        let u = union(vec![
            literal(baml_base::Literal::String("draft".into())),
            literal(baml_base::Literal::String("sent".into())),
        ]);
        assert_eq!(tr(&u, TyPosition::TopLevel), "java.lang.String");
    }

    #[test]
    fn literal_union_mixed_base_renders_generic_arity_family() {
        // Mixed-base literal unions don't erase; each literal arm boxes
        // to its base type inside the arity family.
        let u = union(vec![
            literal(baml_base::Literal::Int(1)),
            literal(baml_base::Literal::String("draft".into())),
        ]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_bridge.Union2<java.lang.Long, java.lang.String>"
        );
    }

    #[test]
    fn alias_erases_nonrecursive_and_names_recursive() {
        let alias_name = name("user", &["aliases"], "StringList");
        let mut aliases = AliasTable::new();
        aliases.insert(alias_name.clone(), (list(string()), false));
        let rec_name = name("user", &["aliases"], "RecList");
        aliases.insert(
            rec_name.clone(),
            (union(vec![int(), list(alias_ty(rec_name.clone()))]), true),
        );
        let ctx = ctx_in(&aliases);
        let mut sink = UnionSink::default();
        assert_eq!(
            translate_ty(&alias_ty(alias_name), TyPosition::TopLevel, &ctx, &mut sink),
            "java.util.List<java.lang.String>"
        );
        assert_eq!(
            translate_ty(&alias_ty(rec_name), TyPosition::TopLevel, &ctx, &mut sink),
            "baml_sdk.aliases.RecList"
        );
    }

    #[test]
    fn callable_maps_to_java_util_function() {
        let f = callable(
            vec![CallableParam {
                name: Some(BaseName::new("x")),
                ty: int(),
                mode: CodegenFunctionParamMode::Required,
            }],
            string(),
        );
        assert_eq!(
            tr(&f, TyPosition::TopLevel),
            "java.util.function.Function<java.lang.Long, java.lang.String>"
        );
        let c = callable(
            vec![CallableParam {
                name: Some(BaseName::new("x")),
                ty: int(),
                mode: CodegenFunctionParamMode::Required,
            }],
            void(),
        );
        assert_eq!(
            tr(&c, TyPosition::TopLevel),
            "java.util.function.Consumer<java.lang.Long>"
        );
    }

    #[test]
    fn callable_with_optionals_mints_functional_interface() {
        // `(x: int, y?: int, z?: int) -> int` has no java.util.function shape, so
        // a `@FunctionalInterface` (`IntOptCallback`) is minted and recorded.
        let aliases = AliasTable::new();
        let ctx = ctx_in(&aliases);
        let mut sink = UnionSink::default();
        let f = callable(
            vec![
                CallableParam {
                    name: Some(BaseName::new("x")),
                    ty: int(),
                    mode: CodegenFunctionParamMode::Required,
                },
                CallableParam {
                    name: Some(BaseName::new("y")),
                    ty: int(),
                    mode: CodegenFunctionParamMode::Optional,
                },
                CallableParam {
                    name: Some(BaseName::new("z")),
                    ty: int(),
                    mode: CodegenFunctionParamMode::Optional,
                },
            ],
            int(),
        );
        let expr = translate_ty(&f, TyPosition::TopLevel, &ctx, &mut sink);
        assert_eq!(expr, "baml_sdk.callables.IntOptCallback");

        let pkg = PackagePath {
            segments: vec!["callables".to_string()],
        };
        let entries = sink.callbacks.get(&pkg).expect("interface registered");
        assert_eq!(entries.len(), 1);
        let iface = &entries[0].1;
        assert_eq!(iface.name, "IntOptCallback");
        assert_eq!(
            iface.required,
            vec![("x".to_string(), "java.lang.Long".to_string())]
        );
        assert_eq!(
            iface.optionals,
            vec![
                ("y".to_string(), "java.lang.Long".to_string()),
                ("z".to_string(), "java.lang.Long".to_string()),
            ]
        );
        assert_eq!(iface.ret, "java.lang.Long");

        // The same signature reuses the interface (deduped, no second entry).
        let again = translate_ty(&f, TyPosition::TopLevel, &ctx, &mut sink);
        assert_eq!(again, "baml_sdk.callables.IntOptCallback");
        assert_eq!(sink.callbacks[&pkg].len(), 1);
    }

    #[test]
    fn misc_types() {
        assert_eq!(tr(&unknown(), TyPosition::TopLevel), "java.lang.Object");
        assert_eq!(
            tr(&rust_type(), TyPosition::TopLevel),
            "baml_bridge.BamlHandle"
        );
        assert_eq!(
            tr(
                &literal(baml_base::Literal::String("draft".into())),
                TyPosition::TopLevel
            ),
            "java.lang.String"
        );
    }

    #[test]
    fn descriptor_union_return_is_ordered() {
        let aliases = AliasTable::new();
        let u = union(vec![int(), string()]);
        assert_eq!(
            descriptor_expr(&u, &aliases),
            "baml_bridge.BamlType.union(baml_bridge.BamlType.INT, baml_bridge.BamlType.STRING)"
        );
    }

    #[test]
    fn descriptor_nullable_collapses_to_inner() {
        let aliases = AliasTable::new();
        let opt_int = union(vec![int(), null()]);
        assert_eq!(
            descriptor_expr(&opt_int, &aliases),
            "baml_bridge.BamlType.INT"
        );
    }

    #[test]
    fn descriptor_union_with_typevar_is_unknown() {
        // A union with a TypeVar arm renders as Object and must decode to the
        // bare wire value (no arity family) — its descriptor degenerates to the
        // wire-driven passthrough (`descriptor_expr_opt` → None / `null` arg).
        let aliases = AliasTable::new();
        let u = union(vec![typevar(BaseName::new("T")), string(), null()]);
        assert_eq!(
            descriptor_expr(&u, &aliases),
            "baml_bridge.BamlType.UNKNOWN"
        );
        assert_eq!(descriptor_expr_opt(&u, &aliases), None);
    }

    #[test]
    fn descriptor_erased_literal_union_is_base() {
        let aliases = AliasTable::new();
        let u = union(vec![
            literal(baml_base::Literal::String("draft".into())),
            literal(baml_base::Literal::String("sent".into())),
        ]);
        assert_eq!(descriptor_expr(&u, &aliases), "baml_bridge.BamlType.STRING");
    }

    #[test]
    fn signature_token_carries_class_generic_args() {
        // A generic class's registry arm token includes its concrete args, so two
        // distinct instantiations no longer collide (silently first-winning in a
        // union / the registry). `signature_token` (kept for the callback dedup
        // key) and `registry_arm_expr` (the typed arm token) both carry them.
        let aliases = AliasTable::new();
        let w_int = class_ty(name("user", &["g"], "Wrapper"), vec![int()]);
        let w_str = class_ty(name("user", &["g"], "Wrapper"), vec![string()]);
        assert_eq!(
            crate::signature_token(&w_int, &aliases),
            "user.g.Wrapper<int>"
        );
        assert_eq!(
            registry_arm_expr(&w_int, &aliases),
            "baml_bridge.BamlType.classByFqn(\"user.g.Wrapper\", baml_bridge.BamlType.INT)"
        );
        assert_ne!(
            registry_arm_expr(&w_int, &aliases),
            registry_arm_expr(&w_str, &aliases)
        );
        // The class DESCRIPTOR stays the bare FQN (decode resolves by FQN via
        // TypeRegistry.isClass), so the class-decode path is unaffected.
        assert_eq!(
            descriptor_expr(&w_int, &aliases),
            "baml_bridge.BamlType.classByFqn(\"user.g.Wrapper\")"
        );
    }

    #[test]
    fn descriptor_literal_rides_raw_value_no_escaping() {
        // With a typed descriptor there is no grammar to collide with, so a
        // literal value carrying structural characters rides RAW (no escaping) —
        // the value is a `literalString` argument, matched by value equality.
        let aliases = AliasTable::new();
        let u = union(vec![
            literal(baml_base::Literal::String("a;b".into())),
            literal(baml_base::Literal::Int(1)),
        ]);
        assert_eq!(
            descriptor_expr(&u, &aliases),
            "baml_bridge.BamlType.union(baml_bridge.BamlType.literalString(\"a;b\"), \
             baml_bridge.BamlType.literalInt(1L))"
        );
        // The retained `signature_token` (callback dedup key only) also drops the
        // old percent-escaping.
        assert_eq!(
            crate::signature_token(&literal(baml_base::Literal::String("a;b".into())), &aliases),
            "lit:string:a;b"
        );
    }

    #[test]
    fn union_arm_tokens_disambiguates_colliding_simple_names() {
        // `a.Node` and `b.Node` both simple-name to `Node`; the arm set gets
        // distinct record tokens (`Node`, `Node2`) rather than two colliding
        // `NodeValue` records. A non-colliding set is byte-identical.
        let a_node = class_ty(name("user", &["a"], "Node"), vec![]);
        let b_node = class_ty(name("user", &["b"], "Node"), vec![]);
        assert_eq!(
            union_arm_tokens(&[a_node, b_node]),
            vec!["Node".to_string(), "Node2".to_string()]
        );
        assert_eq!(
            union_arm_tokens(&[int(), string()]),
            vec!["Int".to_string(), "String".to_string()]
        );
    }

    #[test]
    fn is_nullable_detects_null_admitting_types() {
        let aliases = AliasTable::new();
        // `T | null` (single non-null arm) is nullable.
        assert!(is_nullable(&union(vec![int(), null()]), &aliases));
        // A multi-arm union with a null arm is nullable (the null arm is
        // stripped from the rendered arity family, but the position is nullable).
        assert!(is_nullable(&union(vec![int(), string(), null()]), &aliases));
        // The bare `null` type is nullable.
        assert!(is_nullable(&null(), &aliases));
        // Plain non-null types are not.
        assert!(!is_nullable(&int(), &aliases));
        assert!(!is_nullable(&string(), &aliases));
        assert!(!is_nullable(&list(int()), &aliases));
        assert!(!is_nullable(&union(vec![int(), string()]), &aliases));
    }

    #[test]
    fn is_nullable_follows_non_recursive_aliases() {
        let alias_name = name("user", &["aliases"], "MaybeInt");
        let mut aliases = AliasTable::new();
        aliases.insert(alias_name.clone(), (union(vec![int(), null()]), false));
        assert!(is_nullable(&alias_ty(alias_name), &aliases));
        // A recursive alias mints a nominal (non-null) reference type.
        let rec_name = name("user", &["aliases"], "RecList");
        aliases.insert(
            rec_name.clone(),
            (union(vec![int(), list(alias_ty(rec_name.clone()))]), true),
        );
        assert!(!is_nullable(&alias_ty(rec_name), &aliases));
    }

    #[test]
    fn annotate_nullable_places_type_use_annotation() {
        // Qualified class name: annotation binds the trailing simple name.
        assert_eq!(
            annotate_nullable("java.lang.String"),
            "java.lang.@org.jspecify.annotations.Nullable String"
        );
        // Generic type: annotate the OUTER raw type, keep the type-args intact.
        assert_eq!(
            annotate_nullable("java.util.List<java.lang.Long>"),
            "java.util.@org.jspecify.annotations.Nullable List<java.lang.Long>"
        );
        // Already-element-annotated generic: outer annotation composes cleanly.
        assert_eq!(
            annotate_nullable("java.util.List<java.lang.@org.jspecify.annotations.Nullable Long>"),
            "java.util.@org.jspecify.annotations.Nullable List<java.lang.@org.jspecify.annotations.Nullable Long>"
        );
        // Array reference: annotate the array level, not the element.
        assert_eq!(
            annotate_nullable("byte[]"),
            "byte @org.jspecify.annotations.Nullable []"
        );
        // Bare simple name (a type variable).
        assert_eq!(
            annotate_nullable("T"),
            "@org.jspecify.annotations.Nullable T"
        );
    }

    #[test]
    fn nullable_list_element_is_annotated() {
        // `(int | null)[]` renders the element as `@Nullable`-annotated boxed
        // inner; the list itself is not nullable, so it is not annotated.
        assert_eq!(
            tr(&list(union(vec![int(), null()])), TyPosition::TopLevel),
            "java.util.List<java.lang.@org.jspecify.annotations.Nullable Long>"
        );
        // A non-null element list is unchanged.
        assert_eq!(
            tr(&list(int()), TyPosition::TopLevel),
            "java.util.List<java.lang.Long>"
        );
    }

    #[test]
    fn nullable_map_value_is_annotated() {
        assert_eq!(
            tr(
                &map(string(), union(vec![string(), null()])),
                TyPosition::TopLevel
            ),
            "java.util.Map<java.lang.String, java.lang.@org.jspecify.annotations.Nullable String>"
        );
    }

    #[test]
    fn nullable_class_type_arg_is_annotated() {
        let c = class_ty(
            name("user", &["generics"], "Wrapper"),
            vec![union(vec![int(), null()])],
        );
        assert_eq!(
            tr(&c, TyPosition::TopLevel),
            "baml_sdk.generics.Wrapper<java.lang.@org.jspecify.annotations.Nullable Long>"
        );
    }
}
