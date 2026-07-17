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

/// **Vestigial — recursive-alias path only.** Anonymous unions render
/// inline as `baml_bridge.Union{n}<...>` (decode is type-directed), so
/// nothing records into this sink anymore and it stays empty in
/// practice; recursive aliases mint their nominal type directly in the
/// emitter. It is retained solely because `translate_ty`'s signatures
/// still thread a `&mut UnionSink`. Keyed by (package, union
/// identifier); the value would hold the arm list (null arm stripped).
#[derive(Debug, Default)]
pub(crate) struct UnionSink {
    // Never read: anonymous unions render inline and recursive aliases
    // mint directly, so nothing records here. Retained so the sink type
    // still threads through `translate_ty`'s signatures.
    #[allow(dead_code)]
    pub(crate) unions: BTreeMap<(PackagePath, String), Vec<Ty>>,
}

/// Everything `translate_ty` needs beyond the type itself.
pub(crate) struct TranslateCtx<'a> {
    pub(crate) aliases: &'a AliasTable,
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
                    .map(|a| translate_ty(a, TyPosition::Boxed, ctx, sink))
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
            translate_ty(inner, TyPosition::Boxed, ctx, sink)
        ),
        // All map keys are stringified engine-side (str/int/bool/enum
        // keys all arrive as strings), so the Java map key is String.
        Ty::Map { value, .. } => format!(
            "java.util.Map<java.lang.String, {}>",
            translate_ty(value, TyPosition::Boxed, ctx, sink)
        ),
        Ty::Union(items, _) => translate_union(items, ctx, sink),
        Ty::BuiltinUnknown { .. } => "java.lang.Object".to_string(),
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

/// FQN of a generated (or runtime-owned) named type:
/// `baml_sdk.<package>.<Ident>`. The runtime-owned `baml.llm.Stream`
/// resolves to the runtime library's `baml_bridge.BamlStream` (no
/// generated class exists for it; the media classes keep their
/// `baml_sdk.baml.media.*` paths because the runtime jar provides
/// classes at exactly those packages).
fn qualified_type(name: &Name) -> String {
    if name.package().as_str() == "baml"
        && name.namespace().len() == 1
        && name.namespace()[0].as_str() == "llm"
        && name.name().as_str() == "Stream"
    {
        return "baml_bridge.BamlStream".to_string();
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

/// Type-directed decode descriptor for a declared type (the string a
/// generated binding passes to `BamlFfi.callSync/callAsync`, and the
/// per-field entries `registerClass` carries). Same token vocabulary
/// as [`signature_token`] except anonymous unions render ORDERED as
/// `union[a;b;...]` (declaration order — the decoder matches arms in
/// this order) and `T | null` collapses to the inner descriptor
/// (nullability never affects decode).
pub(crate) fn descriptor_token(ty: &Ty, aliases: &AliasTable) -> String {
    match ty {
        Ty::Union(items, _) => {
            let non_null: Vec<&Ty> = items
                .iter()
                .filter(|t| !matches!(t, Ty::Null { .. }))
                .collect();
            match non_null.len() {
                0 => "null".to_string(),
                1 => descriptor_token(non_null[0], aliases),
                _ => {
                    if common_literal_base(&non_null).is_some() {
                        // Erased literal union: decodes as its base type.
                        if let Ty::Literal(lit, ..) = non_null[0] {
                            return match lit {
                                baml_base::Literal::Int(_) => "int".to_string(),
                                baml_base::Literal::Bigint(_) => "bigint".to_string(),
                                baml_base::Literal::Float(_) => "float".to_string(),
                                baml_base::Literal::String(_) => "string".to_string(),
                                baml_base::Literal::Bool(_) => "bool".to_string(),
                            };
                        }
                    }
                    // A union with a TypeVar arm renders as `java.lang.Object`
                    // (see `translate_union`); there is no arity family to build,
                    // so decode must yield the bare wire value. `unknown` is the
                    // wire-driven passthrough descriptor (no union wrapping) —
                    // keep it in lockstep with the Java type choice.
                    if non_null.iter().any(|arm| {
                        let mut tvs = Vec::new();
                        collect_type_vars(arm, &mut tvs);
                        !tvs.is_empty()
                    }) {
                        return "unknown".to_string();
                    }
                    let arms: Vec<String> = non_null
                        .iter()
                        .map(|a| descriptor_token(a, aliases))
                        .collect();
                    format!("union[{}]", arms.join(";"))
                }
            }
        }
        Ty::TypeAlias(name, _) => match aliases.get(name) {
            Some((resolved, false)) => descriptor_token(resolved, aliases),
            _ => crate::signature_token(ty, aliases),
        },
        Ty::List(inner, _) => format!("list<{}>", descriptor_token(inner, aliases)),
        Ty::Map { key, value, .. } => format!(
            "map<{},{}>",
            descriptor_token(key, aliases),
            descriptor_token(value, aliases)
        ),
        _ => crate::signature_token(ty, aliases),
    }
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
        Ty::BuiltinUnknown { .. } => "Unknown".to_string(),
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
/// Callables with optional parameters or arity > 2 need a generated
/// functional interface (later capability); until then they fall back
/// to `java.lang.Object` so surrounding code still compiles.
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
    if has_optional || params.len() > 2 {
        return "java.lang.Object".to_string();
    }
    let ret_is_unit = matches!(ret, Ty::Void { .. });
    let p: Vec<String> = params
        .iter()
        .map(|p| translate_ty(&p.ty, TyPosition::Boxed, ctx, sink))
        .collect();
    let r = translate_ty(ret, TyPosition::Boxed, ctx, sink);
    match (p.len(), ret_is_unit) {
        (0, true) => "java.lang.Runnable".to_string(),
        (0, false) => format!("java.util.function.Supplier<{r}>"),
        (1, true) => format!("java.util.function.Consumer<{}>", p[0]),
        (1, false) => format!("java.util.function.Function<{}, {r}>", p[0]),
        (2, true) => format!("java.util.function.BiConsumer<{}, {}>", p[0], p[1]),
        (2, false) => format!("java.util.function.BiFunction<{}, {}, {r}>", p[0], p[1]),
        _ => unreachable!("arity > 2 handled above"),
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
        TranslateCtx { aliases }
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
        Ty::BuiltinUnknown { attr: a() }
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
        Ty::TypeVar(n, a())
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
        assert_eq!(descriptor_token(&u, &aliases), "union[int;string]");
    }

    #[test]
    fn descriptor_nullable_collapses_to_inner() {
        let aliases = AliasTable::new();
        let opt_int = union(vec![int(), null()]);
        assert_eq!(descriptor_token(&opt_int, &aliases), "int");
    }

    #[test]
    fn descriptor_union_with_typevar_is_unknown() {
        // A union with a TypeVar arm renders as Object and must decode to the
        // bare wire value (no arity family) — its descriptor degenerates to the
        // `unknown` passthrough, in lockstep with `translate_union`.
        let aliases = AliasTable::new();
        let u = union(vec![typevar(BaseName::new("T")), string(), null()]);
        assert_eq!(descriptor_token(&u, &aliases), "unknown");
    }

    #[test]
    fn descriptor_erased_literal_union_is_base() {
        let aliases = AliasTable::new();
        let u = union(vec![
            literal(baml_base::Literal::String("draft".into())),
            literal(baml_base::Literal::String("sent".into())),
        ]);
        assert_eq!(descriptor_token(&u, &aliases), "string");
    }
}
