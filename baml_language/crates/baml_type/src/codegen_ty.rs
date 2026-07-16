//! Generator-independent normalization for [`CodegenTy`].

use std::fmt;

use crate::{CodegenFunctionParamTy, CodegenTy, Freshness, Ty, TyAttr};

impl CodegenTy {
    /// Normalize a compiler type at the code-generation boundary.
    ///
    /// The codegen family deliberately preserves nominal aliases and generic
    /// type variables. This pass removes compiler-only type-expression
    /// attributes and canonicalizes structure recursively, including inside
    /// generic arguments, interfaces, functions, futures, lists, and maps.
    #[must_use]
    pub fn canonicalize(self) -> Self {
        let empty = TyAttr::default;
        match self {
            Self::Int { .. } => Self::Int { attr: empty() },
            Self::Bigint { .. } => Self::Bigint { attr: empty() },
            Self::Float { .. } => Self::Float { attr: empty() },
            Self::String { .. } => Self::String { attr: empty() },
            Self::Bool { .. } => Self::Bool { attr: empty() },
            Self::Null { .. } => Self::Null { attr: empty() },
            Self::Uint8Array { .. } => Self::Uint8Array { attr: empty() },
            Self::Media(kind, _) => Self::Media(kind, empty()),
            Self::Literal(literal, _, _) => Self::Literal(literal, Freshness::Regular, empty()),
            Self::Class(name, args, _) => Self::Class(
                name,
                args.into_iter().map(Self::canonicalize).collect(),
                empty(),
            ),
            Self::Interface(name, generics, associated_types, _) => Self::Interface(
                name,
                generics.into_iter().map(Self::canonicalize).collect(),
                associated_types
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize()))
                    .collect(),
                empty(),
            ),
            Self::Enum(name, _) => Self::Enum(name, empty()),
            Self::EnumVariant(name, variant, _) => Self::EnumVariant(name, variant, empty()),
            Self::List(inner, _) => Self::List(Box::new(inner.canonicalize()), empty()),
            Self::Map { key, value, .. } => Self::Map {
                key: Box::new(key.canonicalize()),
                value: Box::new(value.canonicalize()),
                attr: empty(),
            },
            Self::Union(members, _) => canonical_union(members),
            Self::Function {
                params,
                ret,
                throws,
                ..
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
                attr: empty(),
            },
            Self::Future(value, error, _) => Self::Future(
                Box::new(value.canonicalize()),
                Box::new(error.canonicalize()),
                empty(),
            ),
            Self::RustType { .. } => Self::RustType { attr: empty() },
            Self::Type { .. } => Self::Type { attr: empty() },
            Self::Resource { .. } => Self::Resource { attr: empty() },
            Self::PromptAst { .. } => Self::PromptAst { attr: empty() },
            Self::Void { .. } => Self::Void { attr: empty() },
            Self::TypeAlias(name, _) => Self::TypeAlias(name, empty()),
            Self::TypeVar(name, _) => Self::TypeVar(name, empty()),
            Self::BuiltinUnknown { .. } => Self::BuiltinUnknown { attr: empty() },
            Self::Never { .. } => Self::Never { attr: empty() },
        }
    }
}

fn canonical_union(members: Vec<CodegenTy>) -> CodegenTy {
    let mut canonical = Vec::new();
    for member in members {
        match member.canonicalize() {
            CodegenTy::Union(nested, _) => {
                for nested_member in nested {
                    if !canonical.contains(&nested_member) {
                        canonical.push(nested_member);
                    }
                }
            }
            member if !canonical.contains(&member) => canonical.push(member),
            _ => {}
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
        0 => CodegenTy::Never {
            attr: TyAttr::default(),
        },
        1 => canonical.pop().expect("singleton union has one member"),
        _ => CodegenTy::Union(canonical, TyAttr::default()),
    }
}

impl fmt::Display for CodegenTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ty::from(self).fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::{CodegenFunctionParamTy, CodegenTy, FunctionParamMode, Name, TyAttr, TypeName};

    fn a() -> TyAttr {
        TyAttr::default()
    }

    fn nullable_string_with_repeated_nulls() -> CodegenTy {
        CodegenTy::Union(
            vec![
                CodegenTy::Null { attr: a() },
                CodegenTy::Union(
                    vec![
                        CodegenTy::String { attr: a() },
                        CodegenTy::Null { attr: a() },
                    ],
                    a(),
                ),
                CodegenTy::Null { attr: a() },
            ],
            a(),
        )
    }

    #[test]
    fn canonicalization_is_recursive_in_every_container() {
        let ty = CodegenTy::Function {
            params: vec![CodegenFunctionParamTy::required(
                Some(Name::new("value")),
                CodegenTy::Class(
                    TypeName::local(Name::new("Box")),
                    vec![CodegenTy::List(
                        Box::new(nullable_string_with_repeated_nulls()),
                        a(),
                    )],
                    a(),
                ),
            )],
            ret: Box::new(CodegenTy::Map {
                key: Box::new(CodegenTy::String { attr: a() }),
                value: Box::new(nullable_string_with_repeated_nulls()),
                attr: a(),
            }),
            throws: Box::new(nullable_string_with_repeated_nulls()),
            attr: a(),
        };
        let nullable = CodegenTy::Union(
            vec![
                CodegenTy::String { attr: a() },
                CodegenTy::Null { attr: a() },
            ],
            a(),
        );

        assert_eq!(
            ty.canonicalize(),
            CodegenTy::Function {
                params: vec![CodegenFunctionParamTy {
                    name: Some(Name::new("value")),
                    ty: CodegenTy::Class(
                        TypeName::local(Name::new("Box")),
                        vec![CodegenTy::List(Box::new(nullable.clone()), a())],
                        a(),
                    ),
                    mode: FunctionParamMode::Required,
                }],
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
}
