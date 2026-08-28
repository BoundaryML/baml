//! `baml_codegen_types::Ty` → Rust type expression.
//!
//! Emitted types are fully qualified (`::std::string::String`, never bare
//! `String`; nominal types as absolute `crate::…` paths) so user-declared
//! BAML symbols can never shadow them under `PreserveCase`.
//!
//! Types the Rust SDK cannot represent yet return [`Unsupported`]; the
//! caller skips the enclosing symbol and reports it — never erasing to a
//! catch-all type.

use baml_codegen_types::{CodegenFunctionParamMode, Name, Ty};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    analyze::Analysis,
    idents, routing,
    unions::{self, UnionRegistry},
};

/// A type the generator cannot translate yet. The reason names the
/// missing capability for the skip warning.
#[derive(Debug)]
pub(crate) struct Unsupported {
    pub(crate) reason: String,
}

fn unsupported(what: &str) -> Unsupported {
    Unsupported {
        reason: format!("unsupported type: {what}"),
    }
}

/// Context for one type translation.
pub(crate) struct TyCtx<'a> {
    pub(crate) analysis: &'a Analysis,
    /// Synthesized union enums, and the (renamed) leaf the translated
    /// symbol lives in — union references resolve to that leaf's enums.
    pub(crate) unions: &'a UnionRegistry,
    pub(crate) leaf: &'a [String],
    /// Set when translating the fields of a class: that class's name.
    /// References to classes in the same containment SCC are boxed at
    /// the field site (outside heap-indirected containers) so recursive
    /// classes stay finite-sized.
    pub(crate) boxing_for: Option<&'a Name>,
    /// `TypeVar` names in scope — the enclosing generic function's own
    /// `<...>` params, in declaration order. A `Ty::TypeVar` naming one of
    /// these translates to that Rust generic parameter; a `TypeVar` not in
    /// scope is unsupported (fail closed). Empty everywhere except inside a
    /// generic function's signature (class fields, non-generic functions,
    /// and — for now — methods have no `TypeVar`s in scope).
    pub(crate) generic_params: &'a [String],
}

/// Translate a resolved BAML type to its Rust type expression.
pub(crate) fn translate(ty: &Ty, ctx: &TyCtx<'_>) -> Result<TokenStream, Unsupported> {
    translate_inner(ty, ctx, false)
}

/// The absolute `crate::…` path of an emitted nominal type.
pub(crate) fn type_path(name: &Name, analysis: &Analysis) -> TokenStream {
    let routed = routing::route(name).segments;
    let segments = analysis.renamed(&routed);
    let mods = segments.iter().map(|seg| idents::ident(seg));
    let type_ident = idents::ident(name.bare_name());
    quote! { crate::#(#mods::)*#type_ident }
}

fn translate_inner(ty: &Ty, ctx: &TyCtx<'_>, under_heap: bool) -> Result<TokenStream, Unsupported> {
    match ty {
        Ty::Int { .. } => Ok(quote! { ::core::primitive::i64 }),
        Ty::Bigint { .. } => Ok(quote! { ::baml_bridge::BigInt }),
        Ty::Float { .. } => Ok(quote! { ::core::primitive::f64 }),
        Ty::String { .. } => Ok(quote! { ::std::string::String }),
        Ty::Bool { .. } => Ok(quote! { ::core::primitive::bool }),
        // BAML `null` (as a type) and `void` both surface as unit: null
        // rides the wire as an absent value, a void function returns null.
        Ty::Null { .. } | Ty::Void { .. } => Ok(quote! { () }),
        // Rust cannot refine value-level literals in types; a literal type
        // widens to its base primitive (the same widening TS applies going
        // from `Literal[42]`-style types to `number`).
        Ty::Literal(lit, ..) => Ok(match lit {
            baml_base::Literal::Int(_) => quote! { ::core::primitive::i64 },
            baml_base::Literal::Bigint(_) => quote! { ::baml_bridge::BigInt },
            baml_base::Literal::Float(_) => quote! { ::core::primitive::f64 },
            baml_base::Literal::String(_) => quote! { ::std::string::String },
            baml_base::Literal::Bool(_) => quote! { ::core::primitive::bool },
        }),
        Ty::Uint8Array { .. } => Ok(quote! { ::std::vec::Vec<::core::primitive::u8> }),
        Ty::List(inner, _) => {
            let inner = translate_inner(inner, ctx, true)?;
            Ok(quote! { ::std::vec::Vec<#inner> })
        }
        Ty::Map { key, value, .. } => {
            // The language restricts map keys to strings (E0067); the
            // codegen-facing Ty is more permissive, so fail closed on
            // anything else rather than guessing a wire stringification.
            match key.as_ref() {
                Ty::String { .. } => {}
                other => return Err(unsupported(&format!("map key type ({other})"))),
            }
            let value = translate_inner(value, ctx, true)?;
            Ok(quote! { ::baml_bridge::Map<::std::string::String, #value> })
        }
        Ty::Union(items, _) => {
            // A `null` arm is optionality: strip it and wrap the rest in
            // `Option`. One remaining arm is the arm itself; several
            // become the leaf's synthesized union enum.
            let (arms, had_null) = unions::strip_null(items);
            let inner = match arms.as_slice() {
                [] => quote! { () },
                [only] => translate_inner(only, ctx, under_heap)?,
                arms => {
                    let Some(union_enum) = ctx.unions.lookup(ctx.leaf, arms) else {
                        return Err(Unsupported {
                            reason: unions::shape_error(arms).unwrap_or_else(|| {
                                "union references a skipped or unknown type".to_string()
                            }),
                        });
                    };
                    let enum_ident = idents::ident(&union_enum.rust_name);
                    let mods: Vec<_> = ctx.leaf.iter().map(|seg| idents::ident(seg)).collect();
                    // A generic union enum (a `TypeVar` arm) is referenced
                    // with its type parameters supplied — each must be a
                    // generic param in scope at this site, else fail closed.
                    let generics = if union_enum.generic_params.is_empty() {
                        TokenStream::new()
                    } else {
                        let mut args = Vec::new();
                        for param in &union_enum.generic_params {
                            if !ctx.generic_params.iter().any(|scoped| scoped == param) {
                                return Err(Unsupported {
                                    reason: format!(
                                        "union references type variable `{param}` not in scope"
                                    ),
                                });
                            }
                            args.push(idents::ident(param));
                        }
                        quote! { <#(#args),*> }
                    };
                    let path = quote! { crate::#(#mods::)*#enum_ident #generics };
                    // The enum holds class arms by value, so a same-SCC
                    // class arm makes the enum itself part of the
                    // containment cycle — box the enum reference.
                    let boxed = !under_heap
                        && ctx.boxing_for.is_some_and(|owner| {
                            arms.iter().any(|arm| match arm {
                                Ty::Class(name, _, _) => ctx.analysis.needs_box(owner, name),
                                _ => false,
                            })
                        });
                    if boxed {
                        quote! { ::std::boxed::Box<#path> }
                    } else {
                        path
                    }
                }
            };
            if had_null {
                Ok(quote! { ::std::option::Option<#inner> })
            } else {
                Ok(inner)
            }
        }
        Ty::Class(name, args, _) => {
            let builtin = name.to_string();
            if builtin == "ai.FunctionSpec" && args.len() == 1 {
                let output = translate_inner(&args[0], ctx, true)?;
                return Ok(quote! { ::baml_bridge::FunctionSpec<#output> });
            }
            if builtin == "ai.stream.Stream" && args.len() == 2 {
                let partial = translate_inner(&args[0], ctx, true)?;
                let output = translate_inner(&args[1], ctx, true)?;
                return Ok(quote! { ::baml_bridge::Stream<#partial, #output> });
            }
            if builtin == "ai.Prompt" && args.is_empty() {
                return Ok(quote! { ::baml_bridge::Prompt });
            }
            // The builtin opaque host-error class is the Rust surface of an
            // opaque host throw: it maps to `baml_bridge::HostCallable` (the
            // erased default), not an emitted class — it never appears in the
            // generated crate.
            if name.to_string() == "baml.errors.HostCallable" {
                return Ok(quote! { ::baml_bridge::HostCallable });
            }
            if !ctx.analysis.is_emitted(name) {
                return Err(Unsupported {
                    reason: format!("references skipped or unknown type `{name}`"),
                });
            }
            let mut path = type_path(name, ctx.analysis);
            // A generic instantiation carries its concrete type arguments as
            // `<A, B, …>`. They are stored inline in the class (not behind a
            // heap-indirected container), so a same-SCC argument still boxes.
            if !args.is_empty() {
                let translated = args
                    .iter()
                    .map(|arg| translate_inner(arg, ctx, false))
                    .collect::<Result<Vec<_>, _>>()?;
                path = quote! { #path<#(#translated),*> };
            }
            let boxed = !under_heap
                && ctx
                    .boxing_for
                    .is_some_and(|owner| ctx.analysis.needs_box(owner, name));
            if boxed {
                Ok(quote! { ::std::boxed::Box<#path> })
            } else {
                Ok(path)
            }
        }
        // A specific enum variant used as a type (`Sentiment.Positive`)
        // drops its variant tag and translates to the enum itself — Rust
        // has no variant-level types, and the value is a `Sentiment`.
        Ty::Enum(name, _) | Ty::EnumVariant(name, _, _) => {
            if !ctx.analysis.is_emitted(name) {
                return Err(Unsupported {
                    reason: format!("references skipped or unknown type `{name}`"),
                });
            }
            Ok(type_path(name, ctx.analysis))
        }
        Ty::Media(kind, _) => Err(unsupported(&format!("media ({kind})"))),
        // Opaque alias references (in-package non-recursive aliases are
        // inlined upstream, so these are recursive or cross-package ones).
        // A Rust `type` alias is transparent, so the reference resolves to
        // the underlying type's conversions; no boxing is needed because
        // package dependencies are acyclic, so a cross-package alias can
        // never sit on a containment cycle.
        Ty::TypeAlias(name, _) => {
            if !ctx.analysis.is_emitted(name) {
                return Err(Unsupported {
                    reason: format!("references skipped or unknown type `{name}`"),
                });
            }
            Ok(type_path(name, ctx.analysis))
        }
        // A TypeVar naming one of the enclosing generic function's own
        // `<...>` params translates to that Rust generic parameter. The
        // bound (`T: BamlValue`) is attached at the binding's signature,
        // not here. A TypeVar not in scope (e.g. a class-level param, or a
        // stray) is unsupported so the enclosing symbol skips loudly.
        Ty::TypeVar(name, _) => {
            let name = name.as_str();
            if ctx.generic_params.iter().any(|param| param == name) {
                let ident = idents::ident(name);
                Ok(quote! { #ident })
            } else {
                Err(unsupported(&format!(
                    "type variable `{name}` (not a function type parameter)"
                )))
            }
        }
        Ty::Unknown { .. } => Err(unsupported("unknown")),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let mut argument_types = Vec::with_capacity(params.len());
            for parameter in params {
                let translated = translate_inner(&parameter.ty, ctx, true)?;
                argument_types.push(
                    if matches!(parameter.mode, CodegenFunctionParamMode::Optional) {
                        quote! { ::baml_bridge::OptionalArg<#translated> }
                    } else {
                        translated
                    },
                );
            }
            let arguments = match argument_types.as_slice() {
                [] => quote! { () },
                values => quote! { (#(#values,)*) },
            };
            let ret = translate_inner(ret, ctx, true)?;
            let throws = match throws.as_ref() {
                Ty::Never { .. } => quote! { ::core::convert::Infallible },
                ty => translate_inner(ty, ctx, true)?,
            };
            Ok(quote! { ::baml_bridge::BamlFunction<#arguments, #ret, #throws> })
        }
        Ty::Future(..) => Err(unsupported("future handle")),
        Ty::Interface(..) => Err(unsupported("interface")),
        Ty::Type { .. } => Err(unsupported("type metatype")),
        Ty::Resource { .. } => Err(unsupported("resource handle")),
        Ty::PromptAst { .. } => Err(unsupported("prompt AST")),
        // The uninhabited type: never appears as a field/param/return type
        // (throws-nothing is `throws: None`, handled before translation).
        Ty::Never { .. } => Err(unsupported("never")),
        Ty::RustType { .. } => Err(unsupported("$rust_type handle")),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use baml_codegen_types::{Class, ClassProperty, Origin, Symbol, SymbolPool};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::analyze::analyze;

    /// Tests that exercise non-union translation share an empty registry
    /// and the crate-root leaf.
    static NO_UNIONS: LazyLock<UnionRegistry> = LazyLock::new(UnionRegistry::default);

    fn name(pkg: &str, ns: &[&str], leaf: &str) -> Name {
        Name::new(
            baml_base::Name::new(pkg),
            ns.iter().map(|s| baml_base::Name::new(*s)).collect(),
            baml_base::Name::new(leaf),
        )
    }

    fn class(n: &Name, fields: Vec<(&str, Ty)>) -> Symbol {
        Symbol::Class(Class {
            name: n.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: fields
                .into_iter()
                .map(|(field, ty)| ClassProperty {
                    name: baml_base::Name::new(field),
                    docstring: None,
                    ty,
                })
                .collect(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        })
    }

    fn empty_analysis() -> Analysis {
        analyze(&SymbolPool::default()).0
    }

    fn rendered(ty: &Ty) -> String {
        let analysis = empty_analysis();
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        translate(ty, &ctx)
            .map(|t| t.to_string())
            .unwrap_or_else(|u| panic!("expected {ty} to translate, got: {}", u.reason))
    }

    #[test]
    fn primitives() {
        assert_eq!(
            rendered(&Ty::Int {
                attr: baml_base::TyAttr::EMPTY
            }),
            ":: core :: primitive :: i64"
        );
        assert_eq!(
            rendered(&Ty::String {
                attr: baml_base::TyAttr::EMPTY
            }),
            ":: std :: string :: String"
        );
        assert_eq!(
            rendered(&Ty::Void {
                attr: baml_base::TyAttr::EMPTY
            }),
            "()"
        );
        assert_eq!(
            rendered(&Ty::Null {
                attr: baml_base::TyAttr::EMPTY
            }),
            "()"
        );
        assert_eq!(
            rendered(&Ty::Bigint {
                attr: baml_base::TyAttr::EMPTY
            }),
            ":: baml_bridge :: BigInt"
        );
        assert_eq!(
            rendered(&Ty::Uint8Array {
                attr: baml_base::TyAttr::EMPTY
            }),
            ":: std :: vec :: Vec < :: core :: primitive :: u8 >"
        );
    }

    #[test]
    fn literals_widen_to_their_base_primitive() {
        assert_eq!(
            rendered(&Ty::Literal(
                baml_base::Literal::String("hello world".into()),
                baml_codegen_types::Freshness::Regular,
                baml_base::TyAttr::EMPTY
            )),
            ":: std :: string :: String"
        );
        assert_eq!(
            rendered(&Ty::Literal(
                baml_base::Literal::Int(42),
                baml_codegen_types::Freshness::Regular,
                baml_base::TyAttr::EMPTY
            )),
            ":: core :: primitive :: i64"
        );
    }

    #[test]
    fn null_union_is_option_in_either_arm_order() {
        let expected = ":: std :: option :: Option < :: core :: primitive :: i64 >";
        assert_eq!(
            rendered(&Ty::Union(
                vec![
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY
                    },
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY
                    }
                ],
                baml_base::TyAttr::EMPTY
            )),
            expected
        );
        assert_eq!(
            rendered(&Ty::Union(
                vec![
                    Ty::Null {
                        attr: baml_base::TyAttr::EMPTY
                    },
                    Ty::Int {
                        attr: baml_base::TyAttr::EMPTY
                    }
                ],
                baml_base::TyAttr::EMPTY
            )),
            expected
        );
    }

    #[test]
    fn string_keyed_maps_translate_and_other_keys_fail_closed() {
        assert_eq!(
            rendered(&Ty::Map {
                key: Box::new(Ty::String {
                    attr: baml_base::TyAttr::EMPTY
                }),
                value: Box::new(Ty::Int {
                    attr: baml_base::TyAttr::EMPTY
                }),
                attr: baml_base::TyAttr::EMPTY,
            }),
            ":: baml_bridge :: Map < :: std :: string :: String , :: core :: primitive :: i64 >"
        );
        let analysis = empty_analysis();
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        let enum_keyed = Ty::Map {
            key: Box::new(Ty::Enum(
                name("user", &[], "Color"),
                baml_base::TyAttr::EMPTY,
            )),
            value: Box::new(Ty::Int {
                attr: baml_base::TyAttr::EMPTY,
            }),
            attr: baml_base::TyAttr::EMPTY,
        };
        assert!(translate(&enum_keyed, &ctx).is_err());
    }

    #[test]
    fn non_null_unions_are_unsupported() {
        let analysis = empty_analysis();
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        assert!(
            translate(
                &Ty::Union(
                    vec![
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY
                        },
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY
                        }
                    ],
                    baml_base::TyAttr::EMPTY
                ),
                &ctx
            )
            .is_err()
        );
        assert!(
            translate(
                &Ty::Union(
                    vec![
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY
                        },
                        Ty::String {
                            attr: baml_base::TyAttr::EMPTY
                        },
                        Ty::Null {
                            attr: baml_base::TyAttr::EMPTY
                        }
                    ],
                    baml_base::TyAttr::EMPTY
                ),
                &ctx
            )
            .is_err()
        );
    }

    #[test]
    fn emitted_nominals_render_as_absolute_paths() {
        let resume = name("user", &["lorem"], "Resume");
        let pool = SymbolPool::from([(
            resume.clone(),
            class(
                &resume,
                vec![(
                    "title",
                    Ty::String {
                        attr: baml_base::TyAttr::EMPTY,
                    },
                )],
            ),
        )]);
        let (analysis, warnings) = analyze(&pool);
        assert!(warnings.is_empty());
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        assert_eq!(
            translate(
                &Ty::Class(resume, Vec::new(), baml_base::TyAttr::EMPTY),
                &ctx
            )
            .unwrap()
            .to_string(),
            "crate :: lorem :: Resume"
        );
    }

    #[test]
    fn references_to_skipped_types_fail_closed() {
        // `Bad` has a media field, so it is skipped; a reference to it
        // must not translate.
        let bad = name("user", &[], "Bad");
        let pool = SymbolPool::from([(
            bad.clone(),
            class(
                &bad,
                vec![(
                    "m",
                    Ty::Media(baml_base::MediaKind::Image, baml_base::TyAttr::EMPTY),
                )],
            ),
        )]);
        let (analysis, warnings) = analyze(&pool);
        assert_eq!(warnings.len(), 1);
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        assert!(translate(&Ty::Class(bad, Vec::new(), baml_base::TyAttr::EMPTY), &ctx).is_err());
    }

    #[test]
    fn same_scc_fields_box_outside_heap_containers() {
        // tree.left: tree? boxes; tree.items: tree[] does not (Vec is
        // already heap-indirected).
        let tree = name("user", &[], "Tree");
        let optional_self = Ty::Union(
            vec![
                Ty::Class(tree.clone(), Vec::new(), baml_base::TyAttr::EMPTY),
                Ty::Null {
                    attr: baml_base::TyAttr::EMPTY,
                },
            ],
            baml_base::TyAttr::EMPTY,
        );
        let self_list = Ty::List(
            Box::new(Ty::Class(
                tree.clone(),
                Vec::new(),
                baml_base::TyAttr::EMPTY,
            )),
            baml_base::TyAttr::EMPTY,
        );
        let pool = SymbolPool::from([(
            tree.clone(),
            class(
                &tree,
                vec![
                    ("left", optional_self.clone()),
                    ("items", self_list.clone()),
                ],
            ),
        )]);
        let (analysis, warnings) = analyze(&pool);
        assert!(warnings.is_empty());
        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: Some(&tree),
            generic_params: &[],
        };
        assert_eq!(
            translate(&optional_self, &ctx).unwrap().to_string(),
            ":: std :: option :: Option < :: std :: boxed :: Box < crate :: Tree > >"
        );
        assert_eq!(
            translate(&self_list, &ctx).unwrap().to_string(),
            ":: std :: vec :: Vec < crate :: Tree >"
        );
    }

    #[test]
    fn mutually_recursive_classes_box_in_both_directions() {
        let a = name("user", &[], "A");
        let b = name("user", &[], "B");
        let a_field = Ty::Union(
            vec![
                Ty::Class(b.clone(), Vec::new(), baml_base::TyAttr::EMPTY),
                Ty::Null {
                    attr: baml_base::TyAttr::EMPTY,
                },
            ],
            baml_base::TyAttr::EMPTY,
        );
        let b_field = Ty::Union(
            vec![
                Ty::Class(a.clone(), Vec::new(), baml_base::TyAttr::EMPTY),
                Ty::Null {
                    attr: baml_base::TyAttr::EMPTY,
                },
            ],
            baml_base::TyAttr::EMPTY,
        );
        let pool = SymbolPool::from([
            (a.clone(), class(&a, vec![("b", a_field)])),
            (b.clone(), class(&b, vec![("a", b_field)])),
        ]);
        let (analysis, warnings) = analyze(&pool);
        assert!(warnings.is_empty());
        assert!(analysis.needs_box(&a, &b));
        assert!(analysis.needs_box(&b, &a));
    }

    #[test]
    fn acyclic_references_do_not_box() {
        let inner = name("user", &[], "Inner");
        let outer = name("user", &[], "Outer");
        let pool = SymbolPool::from([
            (
                inner.clone(),
                class(
                    &inner,
                    vec![(
                        "x",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    )],
                ),
            ),
            (
                outer.clone(),
                class(
                    &outer,
                    vec![(
                        "inner",
                        Ty::Class(inner.clone(), Vec::new(), baml_base::TyAttr::EMPTY),
                    )],
                ),
            ),
        ]);
        let (analysis, _) = analyze(&pool);
        assert!(!analysis.needs_box(&outer, &inner));
    }

    #[test]
    fn transitively_poisoned_classes_skip_with_a_warning() {
        // Bad (media field) poisons Holder, which references it.
        let bad = name("user", &[], "Bad");
        let holder = name("user", &[], "Holder");
        let pool = SymbolPool::from([
            (
                bad.clone(),
                class(
                    &bad,
                    vec![(
                        "m",
                        Ty::Media(baml_base::MediaKind::Image, baml_base::TyAttr::EMPTY),
                    )],
                ),
            ),
            (
                holder.clone(),
                class(
                    &holder,
                    vec![(
                        "bad",
                        Ty::Class(bad.clone(), Vec::new(), baml_base::TyAttr::EMPTY),
                    )],
                ),
            ),
        ]);
        let (analysis, warnings) = analyze(&pool);
        assert!(!analysis.is_emitted(&bad));
        assert!(!analysis.is_emitted(&holder));
        assert_eq!(warnings.len(), 2);
        assert!(
            warnings
                .iter()
                .any(|w| w.fqn == "user.Holder" && w.reason.contains("user.Bad")),
            "holder should name its poisoned dependency"
        );
    }

    fn alias(n: &Name, resolves_to: Ty, recursive: bool) -> Symbol {
        Symbol::TypeAlias(baml_codegen_types::TypeAlias {
            name: n.clone(),
            resolves_to,
            recursive,
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        })
    }

    #[test]
    fn emitted_alias_references_resolve_and_recursive_aliases_fail_closed() {
        let plain = name("user", &["aliases"], "StringList");
        let recursive = name("user", &["aliases"], "RecList");
        let pool = SymbolPool::from([
            (
                plain.clone(),
                alias(
                    &plain,
                    Ty::List(
                        Box::new(Ty::String {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                        baml_base::TyAttr::EMPTY,
                    ),
                    false,
                ),
            ),
            (
                recursive.clone(),
                alias(
                    &recursive,
                    Ty::List(
                        Box::new(Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                        baml_base::TyAttr::EMPTY,
                    ),
                    true,
                ),
            ),
        ]);
        let (analysis, warnings) = analyze(&pool);
        assert!(analysis.is_emitted(&plain));
        assert!(!analysis.is_emitted(&recursive));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].reason.contains("recursive"));

        let ctx = TyCtx {
            analysis: &analysis,
            unions: &NO_UNIONS,
            leaf: &[],
            boxing_for: None,
            generic_params: &[],
        };
        assert_eq!(
            translate(&Ty::TypeAlias(plain, baml_base::TyAttr::EMPTY), &ctx)
                .unwrap()
                .to_string(),
            "crate :: aliases :: StringList"
        );
        assert!(translate(&Ty::TypeAlias(recursive, baml_base::TyAttr::EMPTY), &ctx).is_err());
    }

    #[test]
    fn classes_with_recursive_alias_fields_skip_transitively() {
        let rec = name("user", &[], "RecList");
        let container = name("user", &[], "AliasContainer");
        let pool = SymbolPool::from([
            (
                rec.clone(),
                alias(
                    &rec,
                    Ty::List(
                        Box::new(Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        }),
                        baml_base::TyAttr::EMPTY,
                    ),
                    true,
                ),
            ),
            (
                container.clone(),
                class(
                    &container,
                    vec![(
                        "rec_field",
                        Ty::TypeAlias(rec.clone(), baml_base::TyAttr::EMPTY),
                    )],
                ),
            ),
        ]);
        let (analysis, warnings) = analyze(&pool);
        assert!(!analysis.is_emitted(&container));
        assert!(
            warnings
                .iter()
                .any(|w| w.fqn == "user.AliasContainer" && w.reason.contains("user.RecList")),
            "container should name its poisoned alias dependency"
        );
    }

    #[test]
    fn module_segment_colliding_with_type_name_is_renamed() {
        // Type `foo` at the root + namespace `foo` — the module segment
        // yields, keeping the type's name intact.
        let foo_type = name("user", &[], "foo");
        let in_foo_ns = name("user", &["foo"], "X");
        let pool = SymbolPool::from([
            (
                foo_type.clone(),
                class(
                    &foo_type,
                    vec![(
                        "x",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    )],
                ),
            ),
            (
                in_foo_ns.clone(),
                class(
                    &in_foo_ns,
                    vec![(
                        "y",
                        Ty::Int {
                            attr: baml_base::TyAttr::EMPTY,
                        },
                    )],
                ),
            ),
        ]);
        let (analysis, warnings) = analyze(&pool);
        assert!(warnings.is_empty());
        assert_eq!(
            translate(
                &Ty::Class(in_foo_ns, Vec::new(), baml_base::TyAttr::EMPTY),
                &TyCtx {
                    analysis: &analysis,
                    unions: &NO_UNIONS,
                    leaf: &[],
                    boxing_for: None,
                    generic_params: &[],
                }
            )
            .unwrap()
            .to_string(),
            "crate :: foo_ :: X"
        );
    }
}
