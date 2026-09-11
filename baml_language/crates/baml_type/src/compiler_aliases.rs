//! Canonical compiler spellings and the declarations that carry builtin members.
//!
//! A carrier is a declaration anchor, never a second nominal runtime type. Keep
//! lookup, lowering, documentation and construction policy together so adding a
//! builtin does not require independent name tables in each consumer.
//!
//! Carriers live in the language packages the compiler knows by identity
//! ([`LangPackage`]); which root each one is installed at is the caller's
//! [`LangRoots`], so a compile-time head ([`DeclName`]) is matched by root and
//! a wire name ([`TypeName`]) by the package's manifest spelling. Neither
//! lookup compares a package name string of its own.

use baml_base::{LangPackage, LangRoots, Name};

use crate::{DeclName, PrimitiveType, QualifiedTypeName, TypeName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AliasTarget {
    Primitive(PrimitiveType),
    List,
    Map,
    Future,
    Type,
    Json,
    Void,
    Never,
    Unknown,
}

/// Arrays are syntax, not a bare `array` type name. Named constructors such as
/// `map` carry their arity separately from their unparameterized spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spelling {
    Named(&'static str),
    PostfixArray,
}

/// The declaration that carries a builtin's members: a language package and
/// the `[...namespace, name]` path inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Definition {
    pub package: LangPackage,
    pub path: &'static [&'static str],
}

impl Definition {
    /// The declaration's compile-time head, or `None` when `lang` has no
    /// installation of its package (a database without the standard library).
    pub fn decl_name(self, lang: LangRoots) -> Option<DeclName> {
        let (name, namespace) = self.split_path();
        Some(DeclName::in_root(
            lang.get(self.package)?,
            namespace.iter().map(Name::new).collect(),
            Name::new(name),
        ))
    }

    /// The declaration's wire name (`baml.media.Image`, `reflect.Type`): a
    /// dependency edge names a language package by its manifest spelling.
    pub fn type_name(self) -> TypeName {
        let (name, namespace) = self.split_path();
        TypeName::new(
            Name::new(self.package.manifest_name()),
            namespace.iter().map(Name::new).collect(),
            Name::new(name),
        )
    }

    /// The declaration as source spells it, `[package, ...path]` — what a
    /// desugaring (`json` → `baml.json.json`) writes into the AST for the
    /// ordinary path resolver to look up through the package's edges.
    pub fn source_segments(self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.package.manifest_name()).chain(self.path.iter().copied())
    }

    /// Whether `name` is this declaration under `lang`'s installation.
    pub fn matches(self, lang: LangRoots, name: &DeclName) -> bool {
        lang.is(self.package, name.root()) && self.matches_path(name)
    }

    /// [`matches`](Self::matches) for a wire name.
    pub fn matches_type_name(self, name: &TypeName) -> bool {
        !name.is_local()
            && name.package().as_str() == self.package.manifest_name()
            && self.matches_path(name)
    }

    fn matches_path<P>(self, name: &QualifiedTypeName<P>) -> bool {
        name.namespace()
            .iter()
            .chain(std::iter::once(name.name()))
            .map(Name::as_str)
            .eq(self.path.iter().copied())
    }

    fn split_path(self) -> (&'static str, &'static [&'static str]) {
        let (name, namespace) = self
            .path
            .split_last()
            .expect("builtin definition paths are nonempty");
        (name, namespace)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construction {
    /// A builtin value cannot be made by constructing the carrier's fields.
    Builtin {
        origin: &'static str,
        carries_methods: bool,
    },
    /// An alias expands through the ordinary alias resolver, not a constructor.
    Alias,
    /// A compiler type with no value constructor or addressable declaration.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilerAlias {
    pub target: AliasTarget,
    pub spelling: Spelling,
    pub arity: usize,
    pub definition: Option<Definition>,
    pub construction: Construction,
}

impl CompilerAlias {
    /// Diagnostic spelling; arrays have no standalone keyword.
    pub fn display_name(&self) -> &'static str {
        match self.spelling {
            Spelling::Named(name) => name,
            Spelling::PostfixArray => "array",
        }
    }

    /// The structural type this entry's class declaration denotes when it is
    /// applied to `arity` arguments: `class baml.media.Image` IS `image`,
    /// `class baml.Array<T>` IS `T[]`. Json stays on the alias resolver path,
    /// and intrinsics without declarations are not class carriers. Callers
    /// diagnose bad generic arity before constructing a class type, so a
    /// mismatched arity simply keeps the nominal class.
    pub fn class_carrier(&self, arity: usize) -> Option<AliasTarget> {
        match self.construction {
            Construction::Builtin { .. } if arity == self.arity => Some(self.target),
            _ => None,
        }
    }

    /// [`class_carrier`](Self::class_carrier) in the interned vocabulary: the
    /// type the carrier's class spelling denotes, or `None` to stay nominal.
    pub fn lower_class(&self, args: &[crate::interned::Ty]) -> Option<crate::interned::Ty> {
        use crate::interned::{InferTy, Ty};
        let attr = crate::TyAttr::default();
        let kind = match self.class_carrier(args.len())? {
            AliasTarget::Primitive(primitive) => {
                return Some(Ty::from_plain(&crate::Ty::from_primitive(primitive, attr)));
            }
            AliasTarget::List => InferTy::List(args[0].clone(), attr),
            AliasTarget::Map => InferTy::Map {
                key: args[0].clone(),
                value: args[1].clone(),
                attr,
            },
            AliasTarget::Future => InferTy::Future(args[0].clone(), args[1].clone(), attr),
            AliasTarget::Type => InferTy::Type { attr },
            AliasTarget::Json | AliasTarget::Void | AliasTarget::Never | AliasTarget::Unknown => {
                return None;
            }
        };
        Some(Ty::intern(kind))
    }
}

const fn primitive(
    primitive: PrimitiveType,
    spelling: &'static str,
    path: &'static [&'static str],
    origin: &'static str,
    carries_methods: bool,
) -> CompilerAlias {
    CompilerAlias {
        target: AliasTarget::Primitive(primitive),
        spelling: Spelling::Named(spelling),
        arity: 0,
        definition: Some(Definition {
            package: LangPackage::Baml,
            path,
        }),
        construction: Construction::Builtin {
            origin,
            carries_methods,
        },
    }
}

const fn intrinsic(target: AliasTarget, spelling: &'static str) -> CompilerAlias {
    CompilerAlias {
        target,
        spelling: Spelling::Named(spelling),
        arity: 0,
        definition: None,
        construction: Construction::None,
    }
}

pub const ALL: &[CompilerAlias] = &[
    primitive(PrimitiveType::Int, "int", &["Int"], "literals", true),
    primitive(
        PrimitiveType::Bigint,
        "bigint",
        &["Bigint"],
        "literals",
        true,
    ),
    primitive(PrimitiveType::Float, "float", &["Float"], "literals", true),
    primitive(
        PrimitiveType::String,
        "string",
        &["String"],
        "literals",
        true,
    ),
    primitive(PrimitiveType::Bool, "bool", &["Bool"], "literals", false),
    primitive(
        PrimitiveType::Null,
        "null",
        &["Null"],
        "the `null` literal",
        false,
    ),
    primitive(
        PrimitiveType::Uint8Array,
        "uint8array",
        &["Uint8Array"],
        "byte-string literals",
        true,
    ),
    primitive(
        PrimitiveType::Image,
        "image",
        &["media", "Image"],
        "image factory methods",
        true,
    ),
    primitive(
        PrimitiveType::Audio,
        "audio",
        &["media", "Audio"],
        "audio factory methods",
        true,
    ),
    primitive(
        PrimitiveType::Video,
        "video",
        &["media", "Video"],
        "video factory methods",
        true,
    ),
    primitive(
        PrimitiveType::Pdf,
        "pdf",
        &["media", "Pdf"],
        "pdf factory methods",
        true,
    ),
    CompilerAlias {
        target: AliasTarget::List,
        spelling: Spelling::PostfixArray,
        arity: 1,
        definition: Some(Definition {
            package: LangPackage::Baml,
            path: &["Array"],
        }),
        construction: Construction::Builtin {
            origin: "array literals",
            carries_methods: true,
        },
    },
    CompilerAlias {
        target: AliasTarget::Map,
        spelling: Spelling::Named("map"),
        arity: 2,
        definition: Some(Definition {
            package: LangPackage::Baml,
            path: &["Map"],
        }),
        construction: Construction::Builtin {
            origin: "map literals",
            carries_methods: true,
        },
    },
    CompilerAlias {
        target: AliasTarget::Future,
        spelling: Spelling::Named("baml.future.Future"),
        arity: 2,
        definition: Some(Definition {
            package: LangPackage::Baml,
            path: &["future", "Future"],
        }),
        construction: Construction::Builtin {
            origin: "spawn expressions",
            carries_methods: true,
        },
    },
    CompilerAlias {
        target: AliasTarget::Type,
        spelling: Spelling::Named("reflect.Type"),
        arity: 0,
        definition: Some(Definition {
            package: LangPackage::Reflect,
            path: &["Type"],
        }),
        construction: Construction::Builtin {
            origin: "`reflect.Type.of<T>()` and reflection",
            carries_methods: true,
        },
    },
    CompilerAlias {
        target: AliasTarget::Json,
        spelling: Spelling::Named("json"),
        arity: 0,
        definition: Some(Definition {
            package: LangPackage::Baml,
            path: &["json", "json"],
        }),
        construction: Construction::Alias,
    },
    intrinsic(AliasTarget::Void, "void"),
    intrinsic(AliasTarget::Never, "never"),
    intrinsic(AliasTarget::Unknown, "unknown"),
];

/// The member declaration and its type arguments for a structural receiver.
/// Aliases must be expanded by the caller's type-definition oracle first.
pub fn member_owner(
    receiver: &crate::interned::Ty,
) -> Option<(Definition, Vec<crate::interned::Ty>)> {
    use crate::{MediaKind, interned::InferTy};
    let scalar = |primitive| (AliasTarget::Primitive(primitive), Vec::new());
    let (target, args) = match receiver.kind() {
        InferTy::Int { .. } => scalar(PrimitiveType::Int),
        InferTy::Bigint { .. } => scalar(PrimitiveType::Bigint),
        InferTy::Float { .. } => scalar(PrimitiveType::Float),
        InferTy::String { .. } => scalar(PrimitiveType::String),
        InferTy::Bool { .. } => scalar(PrimitiveType::Bool),
        InferTy::Null { .. } => scalar(PrimitiveType::Null),
        InferTy::Uint8Array { .. } => scalar(PrimitiveType::Uint8Array),
        InferTy::Literal(literal, _, _) => scalar(PrimitiveType::from_literal(literal)),
        InferTy::Media(kind, _) => scalar(match kind {
            MediaKind::Image => PrimitiveType::Image,
            MediaKind::Audio => PrimitiveType::Audio,
            MediaKind::Video => PrimitiveType::Video,
            MediaKind::Pdf => PrimitiveType::Pdf,
            MediaKind::Generic => return None,
        }),
        InferTy::List(element, _) => (AliasTarget::List, vec![element.clone()]),
        InferTy::Map { key, value, .. } => (AliasTarget::Map, vec![key.clone(), value.clone()]),
        InferTy::Future(value, error, _) => {
            (AliasTarget::Future, vec![value.clone(), error.clone()])
        }
        InferTy::Type { .. } => (AliasTarget::Type, Vec::new()),
        _ => return None,
    };
    Some((by_target(target).definition?, args))
}

pub fn by_target(target: AliasTarget) -> &'static CompilerAlias {
    ALL.iter()
        .find(|entry| entry.target == target)
        .expect("every builtin target has a compiler alias")
}

pub fn by_spelling(spelling: &str) -> Option<&'static CompilerAlias> {
    ALL.iter()
        .find(|entry| matches!(entry.spelling, Spelling::Named(name) if name == spelling))
}

/// The entry whose carrier declaration is `name` under `lang`'s installed
/// language packages. Keyed on the full definition path, so a user's own
/// `class Int` and `reflect.class.Type` stay nominal.
pub fn by_definition(lang: LangRoots, name: &DeclName) -> Option<&'static CompilerAlias> {
    ALL.iter().find(|entry| {
        entry
            .definition
            .is_some_and(|definition| definition.matches(lang, name))
    })
}

/// [`by_definition`] for a wire name.
pub fn by_type_name(name: &TypeName) -> Option<&'static CompilerAlias> {
    ALL.iter().find(|entry| {
        entry
            .definition
            .is_some_and(|definition| definition.matches_type_name(name))
    })
}

/// [`by_definition`] for a path inside a language package.
pub fn by_definition_path(package: LangPackage, path: &[&str]) -> Option<&'static CompilerAlias> {
    ALL.iter().find(|entry| {
        entry
            .definition
            .is_some_and(|definition| definition.package == package && definition.path == path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MediaKind, TyAttr,
        interned::{InferTy, Ty},
        test_roots,
    };

    /// `path` as a compile-time head over the test roots (`user.X` is the
    /// nameless workspace root), the shape [`TypeName::from_dotted_path`]
    /// gives on the wire.
    fn decl(path: &str) -> DeclName {
        let wire = TypeName::from_dotted_path(path);
        test_roots::new(
            wire.package().clone(),
            wire.namespace().clone(),
            wire.name().clone(),
        )
    }

    #[test]
    fn registry_is_unambiguous_and_covers_every_primitive() {
        for (index, entry) in ALL.iter().enumerate() {
            for other in &ALL[..index] {
                assert_ne!(entry.target, other.target);
                assert_ne!(entry.spelling, other.spelling);
                if let Some(definition) = entry.definition {
                    assert_ne!(Some(definition), other.definition);
                }
            }
        }
        for primitive in PrimitiveType::ALL {
            assert_eq!(by_target(AliasTarget::Primitive(primitive)).arity, 0);
        }
        // Syntax is not a second, accidentally accepted name for T[].
        assert!(by_spelling("array").is_none());
        assert!(by_spelling("Array").is_none());
    }

    #[test]
    fn lowering_and_member_owners_agree_for_every_carrier() {
        let lang = test_roots::lang();
        let element = Ty::intern(InferTy::String {
            attr: TyAttr::default(),
        });
        for alias in ALL {
            if !matches!(alias.construction, Construction::Builtin { .. }) {
                continue;
            }
            let args = vec![element.clone(); alias.arity];
            let ty = alias.lower_class(&args).expect("class carrier lowers");
            let (definition, recovered_args) = member_owner(&ty).expect("carrier has members");
            assert_eq!(Some(definition), alias.definition);
            assert_eq!(recovered_args, args);
            let head = definition
                .decl_name(lang)
                .expect("language package installed");
            assert_eq!(by_definition(lang, &head), Some(alias));
            assert_eq!(by_type_name(&definition.type_name()), Some(alias));
            assert_eq!(
                by_definition_path(definition.package, definition.path),
                Some(alias)
            );
        }
    }

    #[test]
    fn lookup_checks_package_and_entire_namespace() {
        let lang = test_roots::lang();
        for path in [
            "user.Image",
            "user.String",
            "baml.Image",
            "baml.other.String",
            "reflect.class.Type",
        ] {
            assert!(by_definition(lang, &decl(path)).is_none(), "{path}");
            assert!(
                by_type_name(&TypeName::from_dotted_path(path)).is_none(),
                "{path}"
            );
        }
        let image = AliasTarget::Primitive(PrimitiveType::Image);
        assert_eq!(
            by_definition(lang, &decl("baml.media.Image"))
                .unwrap()
                .target,
            image
        );
        assert_eq!(
            by_type_name(&TypeName::from_dotted_path("baml.media.Image"))
                .unwrap()
                .target,
            image
        );
    }

    #[test]
    fn carriers_are_keyed_by_root_not_spelling() {
        // A package that merely spells itself `baml` is not the language's
        // `baml`: only the root `lang` installed as `LangPackage::Baml` is.
        let head = decl("baml.media.Image");
        assert!(by_definition(test_roots::lang(), &head).is_some());
        assert!(by_definition(LangRoots::default(), &head).is_none());
        let other = LangRoots::default().with(LangPackage::Baml, test_roots::root("other"));
        assert!(by_definition(other, &head).is_none());
        // And with no standard library installed, a carrier has no head at all.
        let image = by_target(AliasTarget::Primitive(PrimitiveType::Image));
        assert!(
            image
                .definition
                .unwrap()
                .decl_name(LangRoots::default())
                .is_none()
        );
    }

    #[test]
    fn carrier_lowering_preserves_media_and_container_identity() {
        let image = by_target(AliasTarget::Primitive(PrimitiveType::Image))
            .lower_class(&[])
            .unwrap();
        assert!(matches!(image.kind(), InferTy::Media(MediaKind::Image, _)));
        let list = by_target(AliasTarget::List)
            .lower_class(std::slice::from_ref(&image))
            .unwrap();
        assert!(matches!(list.kind(), InferTy::List(element, _) if element == &image));
        assert!(by_target(AliasTarget::List).lower_class(&[]).is_none());
        assert!(
            by_target(AliasTarget::Primitive(PrimitiveType::Image))
                .lower_class(&[image])
                .is_none()
        );
        // JSON's recursive expansion belongs to ordinary alias lowering.
        assert!(by_target(AliasTarget::Json).lower_class(&[]).is_none());
        let error = Ty::intern(InferTy::Never {
            attr: TyAttr::default(),
        });
        let result = Ty::intern(InferTy::String {
            attr: TyAttr::default(),
        });
        assert!(
            matches!(by_target(AliasTarget::Future).lower_class(&[result.clone(), error.clone()]).unwrap().kind(), InferTy::Future(value, thrown, _) if value == &result && thrown == &error)
        );
    }
}
