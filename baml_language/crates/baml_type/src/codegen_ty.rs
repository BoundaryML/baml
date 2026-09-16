//! Generator-independent normalization for [`CodegenTy`].

use std::fmt;

use crate::{CodegenFunctionParamTy, CodegenTy, Freshness, Ty};

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
            Self::Int => Self::Int,
            Self::Bigint => Self::Bigint,
            Self::Float => Self::Float,
            Self::String => Self::String,
            Self::Bool => Self::Bool,
            Self::Null => Self::Null,
            Self::Uint8Array => Self::Uint8Array,
            Self::Media(kind) => Self::Media(kind),
            Self::Literal(literal, _) => Self::Literal(literal, Freshness::Regular),
            Self::Class(name, args) => {
                Self::Class(name, args.into_iter().map(Self::canonicalize).collect())
            }
            Self::Interface(name, generics, associated_types) => Self::Interface(
                name,
                generics.into_iter().map(Self::canonicalize).collect(),
                associated_types
                    .into_iter()
                    .map(|(name, ty)| (name, ty.canonicalize()))
                    .collect(),
            ),
            Self::Enum(name) => Self::Enum(name),
            Self::EnumVariant(name, variant) => Self::EnumVariant(name, variant),
            Self::List(inner) => Self::List(Box::new(inner.canonicalize())),
            Self::Map { key, value } => Self::Map {
                key: Box::new(key.canonicalize()),
                value: Box::new(value.canonicalize()),
            },
            Self::Union(members) => canonical_union(members),
            Self::Function {
                params,
                ret,
                throws,
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
            },
            Self::Future(value, error) => Self::Future(
                Box::new(value.canonicalize()),
                Box::new(error.canonicalize()),
            ),
            Self::RustType => Self::RustType,
            Self::Type => Self::Type,
            Self::Resource => Self::Resource,
            Self::PromptAst => Self::PromptAst,
            Self::Void => Self::Void,
            Self::TypeAlias(name) => Self::TypeAlias(name),
            Self::TypeVar(name) => Self::TypeVar(name),
            Self::Unknown => Self::Unknown,
            Self::Never => Self::Never,
        }
    }
}

fn canonical_union(members: Box<[CodegenTy]>) -> CodegenTy {
    let mut canonical = Vec::new();
    for member in members {
        match member.canonicalize() {
            CodegenTy::Union(nested) => {
                for nested_member in nested {
                    push_canonical_member(&mut canonical, nested_member);
                }
            }
            member => push_canonical_member(&mut canonical, member),
        }
    }

    if let Some(null_index) = canonical
        .iter()
        .position(|member| matches!(member, CodegenTy::Null))
    {
        let null = canonical.remove(null_index);
        canonical.push(null);
    }

    match canonical.len() {
        0 => CodegenTy::Never,
        1 => canonical.pop().expect("singleton union has one member"),
        _ => CodegenTy::Union(canonical.into()),
    }
}

fn push_canonical_member(canonical: &mut Vec<CodegenTy>, member: CodegenTy) {
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
    use crate::{CodegenFunctionParamTy, CodegenTy, FunctionParamMode, Name, TypeName};

    fn nullable_string_with_repeated_nulls() -> CodegenTy {
        CodegenTy::Union(Box::new([
            CodegenTy::Null,
            CodegenTy::Union(Box::new([CodegenTy::String, CodegenTy::Null])),
            CodegenTy::Null,
        ]))
    }

    #[test]
    fn canonicalization_is_recursive_in_every_container() {
        let ty = CodegenTy::Function {
            params: Box::new([CodegenFunctionParamTy::required(
                Some(Name::new("value")),
                CodegenTy::Class(
                    TypeName::local(Name::new("Box")),
                    Box::new([CodegenTy::List(Box::new(
                        nullable_string_with_repeated_nulls(),
                    ))]),
                ),
            )]),
            ret: Box::new(CodegenTy::Map {
                key: Box::new(CodegenTy::String),
                value: Box::new(nullable_string_with_repeated_nulls()),
            }),
            throws: Box::new(nullable_string_with_repeated_nulls()),
        };
        let nullable = CodegenTy::Union(Box::new([CodegenTy::String, CodegenTy::Null]));

        assert_eq!(
            ty.canonicalize(),
            CodegenTy::Function {
                params: Box::new([CodegenFunctionParamTy {
                    name: Some(Name::new("value")),
                    ty: CodegenTy::Class(
                        TypeName::local(Name::new("Box")),
                        Box::new([CodegenTy::List(Box::new(nullable.clone()))])
                    ),
                    mode: FunctionParamMode::Required,
                }]),
                ret: Box::new(CodegenTy::Map {
                    key: Box::new(CodegenTy::String),
                    value: Box::new(nullable.clone()),
                }),
                throws: Box::new(nullable),
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
            CodegenTy::TypeAlias(alias.clone()).canonicalize(),
            CodegenTy::TypeAlias(alias)
        );
    }
}
