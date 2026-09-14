//! Generator-independent normalization for [`CodegenTy`].

use std::fmt;

use crate::{CodegenFunctionParamTy, CodegenTy, Freshness, Ty, TyAttr};

impl CodegenTy {
    /// Normalize a compiler type at the code-generation boundary.
    ///
    /// The codegen family deliberately preserves nominal aliases and generic
    /// type variables and SAP/streaming attributes. This pass canonicalizes
    /// structure recursively, including inside generic arguments, interfaces,
    /// functions, futures, lists, and maps.
    #[must_use]
    pub fn canonicalize(self) -> Self {
        match self {
            Self::Int { attr } => Self::Int { attr },
            Self::Bigint { attr } => Self::Bigint { attr },
            Self::Float { attr } => Self::Float { attr },
            Self::String { attr } => Self::String { attr },
            Self::Bool { attr } => Self::Bool { attr },
            Self::Null { attr } => Self::Null { attr },
            Self::Uint8Array { attr } => Self::Uint8Array { attr },
            Self::Media(kind, attr) => Self::Media(kind, attr),
            Self::Literal(literal, _, attr) => Self::Literal(literal, Freshness::Regular, attr),
            Self::Class(name, args, attr) => Self::Class(
                name,
                args.into_iter().map(Self::canonicalize).collect(),
                attr,
            ),
            Self::Interface(name, generics, associated_types, attr) => Self::Interface(
                name,
                generics.into_iter().map(Self::canonicalize).collect(),
                associated_types
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize()))
                    .collect(),
                attr,
            ),
            Self::Enum(name, attr) => Self::Enum(name, attr),
            Self::EnumVariant(name, variant, attr) => Self::EnumVariant(name, variant, attr),
            Self::List(inner, attr) => Self::List(Box::new(inner.canonicalize()), attr),
            Self::Map { key, value, attr } => Self::Map {
                key: Box::new(key.canonicalize()),
                value: Box::new(value.canonicalize()),
                attr,
            },
            Self::Union(members, attr) => canonical_union(members, attr),
            Self::Function {
                params,
                ret,
                throws,
                attr,
            } => Self::Function {
                params: params
                    .into_iter()
                    .map(|param| CodegenFunctionParamTy {
                        name: param.name,
                        ty: param.ty.canonicalize(),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(ret.canonicalize()),
                throws: Box::new(throws.canonicalize()),
                attr,
            },
            Self::Future(value, error, attr) => Self::Future(
                Box::new(value.canonicalize()),
                Box::new(error.canonicalize()),
                attr,
            ),
            Self::RustType { attr } => Self::RustType { attr },
            Self::Type { attr } => Self::Type { attr },
            Self::Resource { attr } => Self::Resource { attr },
            Self::PromptAst { attr } => Self::PromptAst { attr },
            Self::Void { attr } => Self::Void { attr },
            Self::TypeAlias(name, attr) => Self::TypeAlias(name, attr),
            Self::TypeVar(name, attr) => Self::TypeVar(name, attr),
            Self::Unknown { attr } => Self::Unknown { attr },
            Self::Never { attr } => Self::Never { attr },
        }
    }
}

fn merge_attr(inner: &TyAttr, outer: &TyAttr) -> TyAttr {
    TyAttr {
        sap_parse_without_null: inner
            .sap_parse_without_null
            .or(outer.sap_parse_without_null),
        sap_pending_never: inner.sap_pending_never.or(outer.sap_pending_never),
        sap_in_progress_never: inner.sap_in_progress_never.or(outer.sap_in_progress_never),
    }
}

fn canonical_union(members: Box<[CodegenTy]>, attr: TyAttr) -> CodegenTy {
    let mut canonical = Vec::new();
    for member in members {
        match member.canonicalize() {
            CodegenTy::Union(nested, _) => {
                for nested_member in nested {
                    push_canonical_member(&mut canonical, nested_member, &attr);
                }
            }
            member => push_canonical_member(&mut canonical, member, &attr),
        }
    }

    if let Some(null_index) = canonical
        .iter()
        .position(|member| matches!(member, CodegenTy::Null { .. }))
    {
        let null = canonical.remove(null_index);
        canonical.push(null);
    }

    match canonical.len() {
        0 => CodegenTy::Never { attr },
        1 => canonical.pop().expect("singleton union has one member"),
        _ => CodegenTy::Union(canonical.into(), attr),
    }
}

fn push_canonical_member(canonical: &mut Vec<CodegenTy>, member: CodegenTy, outer: &TyAttr) {
    let merged = merge_attr(member.attr(), outer);
    let member = member.with_attr(merged);

    if matches!(member, CodegenTy::Null { .. })
        && let Some(existing) = canonical
            .iter_mut()
            .find(|existing| matches!(existing, CodegenTy::Null { .. }))
    {
        let merged = merge_attr(existing.attr(), member.attr());
        *existing = existing.clone().with_attr(merged);
        return;
    }

    if !canonical.contains(&member) {
        canonical.push(member);
    }
}

impl fmt::Display for CodegenTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ty::from(self).fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CodegenFunctionParamTy, CodegenTy, FunctionParamMode, Name, TyAttr, TyAttrValue, TypeName,
    };

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn nullable_string_with_repeated_nulls() -> CodegenTy {
        CodegenTy::Union(
            Box::new([
                CodegenTy::Null { attr: a() },
                CodegenTy::Union(
                    Box::new([
                        CodegenTy::String { attr: a() },
                        CodegenTy::Null { attr: a() },
                    ]),
                    a(),
                ),
                CodegenTy::Null { attr: a() },
            ]),
            a(),
        )
    }

    #[test]
    fn canonicalization_is_recursive_in_every_container() {
        let ty = CodegenTy::Function {
            params: Box::new([CodegenFunctionParamTy::required(
                Some(Name::new("value")),
                CodegenTy::Class(
                    TypeName::local(Name::new("Box")),
                    Box::new([CodegenTy::List(
                        Box::new(nullable_string_with_repeated_nulls()),
                        a(),
                    )]),
                    a(),
                ),
            )]),
            ret: Box::new(CodegenTy::Map {
                key: Box::new(CodegenTy::String { attr: a() }),
                value: Box::new(nullable_string_with_repeated_nulls()),
                attr: a(),
            }),
            throws: Box::new(nullable_string_with_repeated_nulls()),
            attr: a(),
        };
        let nullable = CodegenTy::Union(
            Box::new([
                CodegenTy::String { attr: a() },
                CodegenTy::Null { attr: a() },
            ]),
            a(),
        );

        assert_eq!(
            ty.canonicalize(),
            CodegenTy::Function {
                params: Box::new([CodegenFunctionParamTy {
                    name: Some(Name::new("value")),
                    ty: CodegenTy::Class(
                        TypeName::local(Name::new("Box")),
                        Box::new([CodegenTy::List(Box::new(nullable.clone()), a())]),
                        a(),
                    ),
                    mode: FunctionParamMode::Required,
                }]),
                ret: Box::new(CodegenTy::Map {
                    key: Box::new(CodegenTy::String { attr: a() }),
                    value: Box::new(nullable.clone()),
                    attr: a(),
                }),
                throws: Box::new(nullable),
                attr: a(),
            }
        );
    }

    #[test]
    fn aliases_remain_nominal_during_canonicalization() {
        let alias = TypeName::new(
            Name::new("vendor"),
            vec![Name::new("models")],
            Name::new("Text"),
        );
        assert_eq!(
            CodegenTy::TypeAlias(alias.clone(), a()).canonicalize(),
            CodegenTy::TypeAlias(alias, a())
        );
    }

    #[test]
    fn canonicalization_preserves_and_distributes_attributes() {
        let outer = TyAttr {
            sap_pending_never: TyAttrValue::Set,
            ..a()
        };
        let nested = TyAttr {
            sap_parse_without_null: TyAttrValue::Set,
            ..a()
        };
        let string = TyAttr {
            sap_in_progress_never: TyAttrValue::Set,
            ..a()
        };
        let ty = CodegenTy::Union(
            Box::new([
                CodegenTy::Union(
                    Box::new([
                        CodegenTy::String { attr: string },
                        CodegenTy::Null { attr: a() },
                    ]),
                    nested,
                ),
                CodegenTy::Null { attr: a() },
            ]),
            outer.clone(),
        );

        let CodegenTy::Union(members, attr) = ty.canonicalize() else {
            panic!("two distinct members must remain a union");
        };
        assert_eq!(attr, outer);
        assert_eq!(members.len(), 2);
        assert_eq!(
            members[0].attr().attr_names(),
            vec![
                "sap.parse_without_null",
                "sap.pending_never",
                "sap.in_progress_never"
            ]
        );
        assert!(matches!(members[1], CodegenTy::Null { .. }));
        assert_eq!(
            members[1].attr().attr_names(),
            vec!["sap.parse_without_null", "sap.pending_never"]
        );
    }
}
