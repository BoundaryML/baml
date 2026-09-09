//! Canonical compiler spellings and the declarations that carry builtin members.
//!
//! A carrier is a declaration anchor, never a second nominal runtime type. Keep
//! lookup, lowering, documentation and construction policy together so adding a
//! builtin does not require independent name tables in each consumer.

use crate::{PrimitiveType, QualifiedTypeName};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Definition {
    pub package: &'static str,
    pub path: &'static [&'static str],
}

impl Definition {
    pub fn qualified_name(self) -> QualifiedTypeName {
        let (name, namespace) = self
            .path
            .split_last()
            .expect("builtin definition paths are nonempty");
        QualifiedTypeName::new(
            crate::Name::new(self.package),
            namespace.iter().map(crate::Name::new).collect(),
            crate::Name::new(name),
        )
    }

    pub fn matches(self, name: &QualifiedTypeName) -> bool {
        !name.is_local()
            && name.package().as_str() == self.package
            && name
                .namespace()
                .iter()
                .chain(std::iter::once(name.name()))
                .map(crate::Name::as_str)
                .eq(self.path.iter().copied())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionVisibility {
    /// The compiler may refer to the carrier, but public source uses its alias.
    Internal,
    /// The definition's qualified name is itself the canonical public spelling.
    Canonical,
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
    pub visibility: DefinitionVisibility,
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

    /// Lower a class carrier to its actual type. Json stays on the alias
    /// resolver path; intrinsics without declarations are not class carriers.
    /// Callers diagnose bad generic arity before constructing a class type.
    pub fn lower_class(&self, args: &[crate::interned::Ty]) -> Option<crate::interned::Ty> {
        use crate::interned::{Ty, TyKind};
        if self.definition.is_none() || args.len() != self.arity {
            return None;
        }
        let attr = crate::TyAttr::default();
        let kind = match self.target {
            AliasTarget::Primitive(primitive) => {
                return Some(Ty::from_plain(&crate::Ty::from_primitive(primitive, attr)));
            }
            AliasTarget::List => TyKind::List(args[0].clone(), attr),
            AliasTarget::Map => TyKind::Map {
                key: args[0].clone(),
                value: args[1].clone(),
                attr,
            },
            AliasTarget::Future => TyKind::Future(args[0].clone(), args[1].clone(), attr),
            AliasTarget::Type => TyKind::Type { attr },
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
            package: "baml",
            path,
        }),
        visibility: DefinitionVisibility::Internal,
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
        visibility: DefinitionVisibility::Canonical,
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
            package: "baml",
            path: &["Array"],
        }),
        visibility: DefinitionVisibility::Internal,
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
            package: "baml",
            path: &["Map"],
        }),
        visibility: DefinitionVisibility::Internal,
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
            package: "baml",
            path: &["future", "Future"],
        }),
        visibility: DefinitionVisibility::Canonical,
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
            package: "reflect",
            path: &["Type"],
        }),
        visibility: DefinitionVisibility::Canonical,
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
            package: "baml",
            path: &["json", "json"],
        }),
        visibility: DefinitionVisibility::Internal,
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
    use crate::{MediaKind, interned::TyKind};
    let scalar = |primitive| (AliasTarget::Primitive(primitive), Vec::new());
    let (target, args) = match receiver.kind() {
        TyKind::Int { .. } => scalar(PrimitiveType::Int),
        TyKind::Bigint { .. } => scalar(PrimitiveType::Bigint),
        TyKind::Float { .. } => scalar(PrimitiveType::Float),
        TyKind::String { .. } => scalar(PrimitiveType::String),
        TyKind::Bool { .. } => scalar(PrimitiveType::Bool),
        TyKind::Null { .. } => scalar(PrimitiveType::Null),
        TyKind::Uint8Array { .. } => scalar(PrimitiveType::Uint8Array),
        TyKind::Literal(literal, _, _) => scalar(PrimitiveType::from_literal(literal)),
        TyKind::Media(kind, _) => scalar(match kind {
            MediaKind::Image => PrimitiveType::Image,
            MediaKind::Audio => PrimitiveType::Audio,
            MediaKind::Video => PrimitiveType::Video,
            MediaKind::Pdf => PrimitiveType::Pdf,
            MediaKind::Generic => return None,
        }),
        TyKind::List(element, _) => (AliasTarget::List, vec![element.clone()]),
        TyKind::Map { key, value, .. } => (AliasTarget::Map, vec![key.clone(), value.clone()]),
        TyKind::Future(value, error, _) => {
            (AliasTarget::Future, vec![value.clone(), error.clone()])
        }
        TyKind::Type { .. } => (AliasTarget::Type, Vec::new()),
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

pub fn by_definition(name: &QualifiedTypeName) -> Option<&'static CompilerAlias> {
    ALL.iter().find(|entry| {
        entry
            .definition
            .is_some_and(|definition| definition.matches(name))
    })
}

pub fn by_definition_path(package: &str, path: &[&str]) -> Option<&'static CompilerAlias> {
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
        interned::{Ty, TyKind},
    };

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
        let element = Ty::intern(TyKind::String {
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
            assert_eq!(by_definition(&definition.qualified_name()), Some(alias));
        }
    }

    #[test]
    fn lookup_checks_package_and_entire_namespace() {
        for name in [
            "user.Image",
            "user.String",
            "baml.Image",
            "baml.other.String",
            "reflect.class.Type",
        ] {
            assert!(
                by_definition(&QualifiedTypeName::from_dotted_path(name)).is_none(),
                "{name}"
            );
        }
        assert_eq!(
            by_definition(&QualifiedTypeName::from_dotted_path("baml.media.Image"))
                .unwrap()
                .target,
            AliasTarget::Primitive(PrimitiveType::Image)
        );
    }

    #[test]
    fn carrier_lowering_preserves_media_and_container_identity() {
        let image = by_target(AliasTarget::Primitive(PrimitiveType::Image))
            .lower_class(&[])
            .unwrap();
        assert!(matches!(image.kind(), TyKind::Media(MediaKind::Image, _)));
        let list = by_target(AliasTarget::List)
            .lower_class(&[image.clone()])
            .unwrap();
        assert!(matches!(list.kind(), TyKind::List(element, _) if element == &image));
        assert!(by_target(AliasTarget::List).lower_class(&[]).is_none());
        assert!(
            by_target(AliasTarget::Primitive(PrimitiveType::Image))
                .lower_class(&[image])
                .is_none()
        );
        // JSON's recursive expansion belongs to ordinary alias lowering.
        assert!(by_target(AliasTarget::Json).lower_class(&[]).is_none());
        let error = Ty::intern(TyKind::Never {
            attr: TyAttr::default(),
        });
        let result = Ty::intern(TyKind::String {
            attr: TyAttr::default(),
        });
        assert!(
            matches!(by_target(AliasTarget::Future).lower_class(&[result.clone(), error.clone()]).unwrap().kind(), TyKind::Future(value, thrown, _) if value == &result && thrown == &error)
        );
    }
}
