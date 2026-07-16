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
//! - Multi-arm unions mint a **nominal sealed type** named
//!   `Union<Arm>Or<Arm>...` (declaration order, no arity prefix; see
//!   [`union_ident`]). Encountered unions are reported through
//!   [`UnionSink`] so the emitter can generate their files beside the
//!   symbol that referenced them.
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

/// Collects the union types a translation encounters, so the emitter
/// can mint one nominal sealed type per distinct union per package.
/// Keyed by (package, union identifier); the value keeps the arm list
/// (null arm already stripped).
#[derive(Debug, Default)]
pub(crate) struct UnionSink {
    pub(crate) unions: BTreeMap<(PackagePath, String), Vec<Ty>>,
}

/// Everything `translate_ty` needs beyond the type itself.
pub(crate) struct TranslateCtx<'a> {
    /// Package of the symbol being translated — minted unions land here.
    pub(crate) current_package: PackagePath,
    pub(crate) aliases: &'a AliasTable,
}

/// Translate `ty` to a Java type expression, recording any minted
/// unions into `sink`.
pub(crate) fn translate_ty(
    ty: &Ty,
    pos: TyPosition,
    ctx: &TranslateCtx<'_>,
    sink: &mut UnionSink,
) -> String {
    match ty {
        Ty::Int => primitive(pos, "long", "java.lang.Long"),
        Ty::Bigint => "java.math.BigInteger".to_string(),
        Ty::Float => primitive(pos, "double", "java.lang.Double"),
        Ty::String => "java.lang.String".to_string(),
        Ty::Bool => primitive(pos, "boolean", "java.lang.Boolean"),
        // Only `null` inhabits the BAML `null` type.
        Ty::Null => "java.lang.Void".to_string(),
        // Java has no literal types; a literal erases to its base.
        Ty::Literal(lit) => match lit {
            baml_base::Literal::Int(_) => primitive(pos, "long", "java.lang.Long"),
            baml_base::Literal::Bigint(_) => "java.math.BigInteger".to_string(),
            baml_base::Literal::Float(_) => primitive(pos, "double", "java.lang.Double"),
            baml_base::Literal::String(_) => "java.lang.String".to_string(),
            baml_base::Literal::Bool(_) => primitive(pos, "boolean", "java.lang.Boolean"),
        },
        Ty::Uint8Array => "byte[]".to_string(),
        Ty::Media(kind) => match kind {
            baml_base::MediaKind::Image => "baml_sdk.baml.media.Image".to_string(),
            baml_base::MediaKind::Audio => "baml_sdk.baml.media.Audio".to_string(),
            baml_base::MediaKind::Video => "baml_sdk.baml.media.Video".to_string(),
            baml_base::MediaKind::Pdf => "baml_sdk.baml.media.Pdf".to_string(),
            // Any-media has no dedicated wrapper type.
            baml_base::MediaKind::Generic => "java.lang.Object".to_string(),
        },
        Ty::Class(name, args) => {
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
        Ty::Enum(name) => qualified_type(name),
        Ty::TypeAlias(name) => {
            match ctx.aliases.get(name) {
                // Recursive aliases mint a nominal type named after the
                // alias (erasure would recurse forever); the emitter
                // generates its body alongside the alias's package.
                Some((_, true)) | None => qualified_type(name),
                Some((resolved, false)) => translate_ty(resolved, pos, ctx, sink),
            }
        }
        Ty::TypeVar(name) => java_identifier(name.as_str()),
        Ty::List(inner) => format!(
            "java.util.List<{}>",
            translate_ty(inner, TyPosition::Boxed, ctx, sink)
        ),
        // All map keys are stringified engine-side (str/int/bool/enum
        // keys all arrive as strings), so the Java map key is String.
        Ty::Map { value, .. } => format!(
            "java.util.Map<java.lang.String, {}>",
            translate_ty(value, TyPosition::Boxed, ctx, sink)
        ),
        Ty::Union(items) => translate_union(items, ctx, sink),
        Ty::BuiltinUnknown => "java.lang.Object".to_string(),
        Ty::Callable { params, ret } => translate_callable(params, ret, ctx, sink),
        Ty::Unit => match pos {
            TyPosition::TopLevel => "void".to_string(),
            TyPosition::Boxed => "java.lang.Void".to_string(),
        },
        Ty::BamlOptions => "baml_bridge.BamlOptions".to_string(),
        Ty::RustType => "baml_bridge.BamlHandle".to_string(),
    }
}

fn primitive(pos: TyPosition, unboxed: &str, boxed: &str) -> String {
    match pos {
        TyPosition::TopLevel => unboxed.to_string(),
        TyPosition::Boxed => boxed.to_string(),
    }
}

/// FQN of a generated (or runtime-owned) named type:
/// `baml_sdk.<package>.<Ident>`.
fn qualified_type(name: &Name) -> String {
    let pkg = route(name);
    format!(
        "{}.{}",
        pkg.java_package(),
        java_identifier(name.name.as_str())
    )
}

/// Union translation. `T | null` collapses to boxed-`T`; a literal
/// union over one base erases to the base; anything else mints a
/// nominal sealed type in the current package.
fn translate_union(items: &[Ty], ctx: &TranslateCtx<'_>, sink: &mut UnionSink) -> String {
    let non_null: Vec<&Ty> = items.iter().filter(|t| !matches!(t, Ty::Null)).collect();

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
            let ident = union_ident(&non_null);
            let arms: Vec<Ty> = non_null.iter().map(|t| (*t).clone()).collect();
            sink.unions
                .insert((ctx.current_package.clone(), ident.clone()), arms);
            format!("{}.{}", ctx.current_package.java_package(), ident)
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
        let Ty::Literal(lit) = arm else { return None };
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

/// Deterministic union type name: `Union<Arm>Or<Arm>...` in
/// declaration order (the engine already normalizes/dedups union
/// arms). Literal arms use a `K`-prefixed token (legacy Go precedent):
/// `"draft"` → `Kdraft`, `1` → `IntK1`, `true` → `BoolKTrue`.
pub(crate) fn union_ident(arms: &[&Ty]) -> String {
    let mut out = String::from("Union");
    for (i, arm) in arms.iter().enumerate() {
        if i > 0 {
            out.push_str("Or");
        }
        out.push_str(&union_arm_token(arm));
    }
    out
}

pub(crate) fn union_arm_token(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".to_string(),
        Ty::Bigint => "Bigint".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::String => "String".to_string(),
        Ty::Bool => "Boolean".to_string(),
        Ty::Null => "Null".to_string(),
        Ty::Uint8Array => "Uint8Array".to_string(),
        Ty::Media(kind) => format!("{kind:?}"),
        Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => {
            java_identifier(name.name.as_str())
        }
        Ty::TypeVar(name) => java_identifier(name.as_str()),
        Ty::List(inner) => format!("{}List", union_arm_token(inner)),
        Ty::Map { value, .. } => format!("{}Map", union_arm_token(value)),
        Ty::Literal(lit) => match lit {
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
        Ty::BuiltinUnknown => "Unknown".to_string(),
        Ty::Callable { .. } => "Callable".to_string(),
        Ty::Unit => "Void".to_string(),
        Ty::BamlOptions => "BamlOptions".to_string(),
        Ty::RustType => "Handle".to_string(),
        // validate() bans nested unions; unreachable in valid pools.
        Ty::Union(_) => "Union".to_string(),
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
    let ret_is_unit = matches!(ret, Ty::Unit);
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

    fn ctx_in<'a>(segments: &[&str], aliases: &'a AliasTable) -> TranslateCtx<'a> {
        TranslateCtx {
            current_package: PackagePath {
                segments: segments.iter().map(ToString::to_string).collect(),
            },
            aliases,
        }
    }

    fn tr(ty: &Ty, pos: TyPosition) -> String {
        let aliases = AliasTable::new();
        let ctx = ctx_in(&["unions"], &aliases);
        let mut sink = UnionSink::default();
        translate_ty(ty, pos, &ctx, &mut sink)
    }

    #[test]
    fn primitives_positional_boxing() {
        assert_eq!(tr(&Ty::Int, TyPosition::TopLevel), "long");
        assert_eq!(tr(&Ty::Int, TyPosition::Boxed), "java.lang.Long");
        assert_eq!(tr(&Ty::Float, TyPosition::TopLevel), "double");
        assert_eq!(tr(&Ty::Bool, TyPosition::Boxed), "java.lang.Boolean");
        assert_eq!(tr(&Ty::String, TyPosition::TopLevel), "java.lang.String");
        assert_eq!(
            tr(&Ty::Bigint, TyPosition::TopLevel),
            "java.math.BigInteger"
        );
        assert_eq!(tr(&Ty::Uint8Array, TyPosition::TopLevel), "byte[]");
        assert_eq!(tr(&Ty::Null, TyPosition::TopLevel), "java.lang.Void");
    }

    #[test]
    fn unit_is_void_only_at_top_level() {
        assert_eq!(tr(&Ty::Unit, TyPosition::TopLevel), "void");
        assert_eq!(tr(&Ty::Unit, TyPosition::Boxed), "java.lang.Void");
    }

    #[test]
    fn containers_box_their_elements() {
        assert_eq!(
            tr(&Ty::List(Box::new(Ty::Int)), TyPosition::TopLevel),
            "java.util.List<java.lang.Long>"
        );
        assert_eq!(
            tr(
                &Ty::Map {
                    key: Box::new(Ty::String),
                    value: Box::new(Ty::Bool),
                },
                TyPosition::TopLevel
            ),
            "java.util.Map<java.lang.String, java.lang.Boolean>"
        );
    }

    #[test]
    fn class_and_enum_are_fqn() {
        let c = Ty::Class(name("user", &["lorem"], "Resume"), vec![]);
        assert_eq!(tr(&c, TyPosition::TopLevel), "baml_sdk.lorem.Resume");
        let e = Ty::Enum(name("user", &["ipsum"], "Sentiment"));
        assert_eq!(tr(&e, TyPosition::TopLevel), "baml_sdk.ipsum.Sentiment");
        let s = Ty::Class(name("user", &["lorem"], "Resume$stream"), vec![]);
        assert_eq!(tr(&s, TyPosition::TopLevel), "baml_sdk.lorem.Resume$stream");
    }

    #[test]
    fn generic_class_args_box() {
        let c = Ty::Class(name("user", &["generics"], "Wrapper"), vec![Ty::Int]);
        assert_eq!(
            tr(&c, TyPosition::TopLevel),
            "baml_sdk.generics.Wrapper<java.lang.Long>"
        );
    }

    #[test]
    fn typevar_renders_bare() {
        assert_eq!(
            tr(&Ty::TypeVar(BaseName::new("T")), TyPosition::TopLevel),
            "T"
        );
    }

    #[test]
    fn media_types() {
        assert_eq!(
            tr(
                &Ty::Media(baml_base::MediaKind::Image),
                TyPosition::TopLevel
            ),
            "baml_sdk.baml.media.Image"
        );
        assert_eq!(
            tr(
                &Ty::Media(baml_base::MediaKind::Generic),
                TyPosition::TopLevel
            ),
            "java.lang.Object"
        );
    }

    #[test]
    fn optional_collapses_to_boxed_inner() {
        let opt_int = Ty::Union(vec![Ty::Int, Ty::Null]);
        assert_eq!(tr(&opt_int, TyPosition::TopLevel), "java.lang.Long");
        let opt_class = Ty::Union(vec![
            Ty::Class(name("user", &["lorem"], "Resume"), vec![]),
            Ty::Null,
        ]);
        assert_eq!(
            tr(&opt_class, TyPosition::TopLevel),
            "baml_sdk.lorem.Resume"
        );
    }

    #[test]
    fn multi_arm_union_mints_nominal_type_and_records_it() {
        let aliases = AliasTable::new();
        let ctx = ctx_in(&["unions"], &aliases);
        let mut sink = UnionSink::default();
        let u = Ty::Union(vec![Ty::Int, Ty::String]);
        let expr = translate_ty(&u, TyPosition::TopLevel, &ctx, &mut sink);
        assert_eq!(expr, "baml_sdk.unions.UnionIntOrString");
        let key = (
            PackagePath {
                segments: vec!["unions".to_string()],
            },
            "UnionIntOrString".to_string(),
        );
        assert_eq!(sink.unions.get(&key), Some(&vec![Ty::Int, Ty::String]));
    }

    #[test]
    fn nullable_multi_arm_union_still_mints_without_null_arm() {
        let u = Ty::Union(vec![Ty::Int, Ty::String, Ty::Null]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_sdk.unions.UnionIntOrString"
        );
    }

    #[test]
    fn union_name_uses_declaration_order_and_class_idents() {
        let t = Ty::Class(name("user", &["unions"], "T"), vec![]);
        let u = Ty::Union(vec![t, Ty::String]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_sdk.unions.UnionTOrString"
        );
    }

    #[test]
    fn literal_union_same_base_erases() {
        let u = Ty::Union(vec![
            Ty::Literal(baml_base::Literal::String("draft".into())),
            Ty::Literal(baml_base::Literal::String("sent".into())),
        ]);
        assert_eq!(tr(&u, TyPosition::TopLevel), "java.lang.String");
    }

    #[test]
    fn literal_union_mixed_base_mints_k_arms() {
        let u = Ty::Union(vec![
            Ty::Literal(baml_base::Literal::Int(1)),
            Ty::Literal(baml_base::Literal::String("draft".into())),
        ]);
        assert_eq!(
            tr(&u, TyPosition::TopLevel),
            "baml_sdk.unions.UnionIntK1OrKdraft"
        );
    }

    #[test]
    fn alias_erases_nonrecursive_and_names_recursive() {
        let alias_name = name("user", &["aliases"], "StringList");
        let mut aliases = AliasTable::new();
        aliases.insert(alias_name.clone(), (Ty::List(Box::new(Ty::String)), false));
        let rec_name = name("user", &["aliases"], "RecList");
        aliases.insert(
            rec_name.clone(),
            (
                Ty::Union(vec![
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(rec_name.clone()))),
                ]),
                true,
            ),
        );
        let ctx = ctx_in(&["aliases"], &aliases);
        let mut sink = UnionSink::default();
        assert_eq!(
            translate_ty(
                &Ty::TypeAlias(alias_name),
                TyPosition::TopLevel,
                &ctx,
                &mut sink
            ),
            "java.util.List<java.lang.String>"
        );
        assert_eq!(
            translate_ty(
                &Ty::TypeAlias(rec_name),
                TyPosition::TopLevel,
                &ctx,
                &mut sink
            ),
            "baml_sdk.aliases.RecList"
        );
    }

    #[test]
    fn callable_maps_to_java_util_function() {
        let f = Ty::Callable {
            params: vec![CallableParam {
                name: Some(BaseName::new("x")),
                ty: Ty::Int,
                mode: CodegenFunctionParamMode::Required,
            }],
            ret: Box::new(Ty::String),
        };
        assert_eq!(
            tr(&f, TyPosition::TopLevel),
            "java.util.function.Function<java.lang.Long, java.lang.String>"
        );
        let c = Ty::Callable {
            params: vec![CallableParam {
                name: Some(BaseName::new("x")),
                ty: Ty::Int,
                mode: CodegenFunctionParamMode::Required,
            }],
            ret: Box::new(Ty::Unit),
        };
        assert_eq!(
            tr(&c, TyPosition::TopLevel),
            "java.util.function.Consumer<java.lang.Long>"
        );
    }

    #[test]
    fn misc_types() {
        assert_eq!(
            tr(&Ty::BuiltinUnknown, TyPosition::TopLevel),
            "java.lang.Object"
        );
        assert_eq!(
            tr(&Ty::RustType, TyPosition::TopLevel),
            "baml_bridge.BamlHandle"
        );
        assert_eq!(
            tr(&Ty::BamlOptions, TyPosition::TopLevel),
            "baml_bridge.BamlOptions"
        );
        assert_eq!(
            tr(
                &Ty::Literal(baml_base::Literal::String("draft".into())),
                TyPosition::TopLevel
            ),
            "java.lang.String"
        );
    }
}
