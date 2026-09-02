//! C#-owned normalization for generator-facing semantic types.

use baml_codegen_types::{
    Class, ClassProperty, CodegenFunctionParamMode, Enum, EnumVariant, Function, FunctionArgument,
    Symbol, Ty, TypeAlias,
};

use crate::model::CodegenModel;

/// Clone the compiler model into the single normalized representation consumed
/// by every C# allocation and rendering pass.
pub(crate) fn normalize_model(model: &CodegenModel) -> CodegenModel {
    CodegenModel {
        symbols: model
            .symbols
            .iter()
            .map(|(name, symbol)| (name.clone(), normalize_symbol(symbol)))
            .collect(),
        callables: model.callables.clone(),
    }
}

fn normalize_symbol(symbol: &Symbol) -> Symbol {
    match symbol {
        Symbol::Function(function) => Symbol::Function(normalize_function(function)),
        Symbol::Class(class) => Symbol::Class(Class {
            name: class.name.clone(),
            generic_params: class.generic_params.clone(),
            docstring: class.docstring.clone(),
            properties: class
                .properties
                .iter()
                .map(|property| ClassProperty {
                    name: property.name.clone(),
                    docstring: property.docstring.clone(),
                    ty: normalize_ty(&property.ty),
                })
                .collect(),
            static_methods: class
                .static_methods
                .iter()
                .map(normalize_function)
                .collect(),
            instance_methods: class
                .instance_methods
                .iter()
                .map(normalize_function)
                .collect(),
            origin: class.origin.clone(),
        }),
        Symbol::Enum(enumeration) => Symbol::Enum(Enum {
            name: enumeration.name.clone(),
            docstring: enumeration.docstring.clone(),
            variants: enumeration
                .variants
                .iter()
                .map(|variant| EnumVariant {
                    name: variant.name.clone(),
                    docstring: variant.docstring.clone(),
                    value: variant.value.clone(),
                })
                .collect(),
            origin: enumeration.origin.clone(),
        }),
        Symbol::TypeAlias(alias) => Symbol::TypeAlias(TypeAlias {
            name: alias.name.clone(),
            resolves_to: normalize_ty(&alias.resolves_to),
            recursive: alias.recursive,
            origin: alias.origin.clone(),
        }),
    }
}

/// The compiler-injected `on_event` listener parameter (`injected` provenance
/// set by the symbol pool). Its type reaches into the `ai.events.Event` union
/// (and through it `ai.content.*` and the media classes), none of which is
/// classified for C# yet, so the whole function would be rejected as
/// unsupported. The parameter is optional with a null default, so omitting it
/// from the C# surface keeps every call valid — the VM fills the default,
/// exactly like the pool-stripped `client` override. User-declared parameters
/// are never `injected`, whatever their name or shape. Remove this filter
/// when the events family gets a C# projection.
fn is_unrepresentable_on_event(argument: &FunctionArgument) -> bool {
    argument.injected
}

fn normalize_function(function: &Function) -> Function {
    Function {
        name: function.name.clone(),
        generic_params: function.generic_params.clone(),
        docstring: function.docstring.clone(),
        arguments: function
            .arguments
            .iter()
            .filter(|argument| !is_unrepresentable_on_event(argument))
            .map(|argument| FunctionArgument {
                injected: argument.injected,
                name: argument.name.clone(),
                docstring: argument.docstring.clone(),
                ty: normalize_ty(&argument.ty),
                default: argument.default.clone(),
            })
            .collect(),
        return_type: normalize_ty(&function.return_type),
        throws: function.throws.as_ref().map(normalize_ty),
        watchers: function
            .watchers
            .iter()
            .map(|(name, ty)| (name.clone(), normalize_ty(ty)))
            .collect(),
        origin: function.origin.clone(),
    }
}

/// Normalize a type for C# identity allocation and rendering.
///
/// Shared canonicalization performs structural cleanup while preserving
/// discovery order. C# additionally sorts union members by their typed
/// semantic identity, keeping null last for nullable projections.
pub(crate) fn normalize_ty(ty: &Ty) -> Ty {
    normalize_canonical(ty.clone().canonicalize())
}

fn normalize_canonical(ty: Ty) -> Ty {
    match ty {
        Ty::Class(name, arguments, attr) => {
            Ty::Class(name, arguments.iter().map(normalize_ty).collect(), attr)
        }
        Ty::Interface(name, generics, associated_types, attr) => Ty::Interface(
            name,
            generics.iter().map(normalize_ty).collect(),
            associated_types
                .into_iter()
                .map(|(name, ty)| (name, normalize_ty(&ty)))
                .collect(),
            attr,
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(normalize_ty(&inner)), attr),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(normalize_ty(&key)),
            value: Box::new(normalize_ty(&value)),
            attr,
        },
        Ty::Union(members, attr) => {
            let mut members = members.iter().map(normalize_ty).collect::<Vec<_>>();
            members.sort();
            members.dedup();
            if let Some(index) = members
                .iter()
                .position(|member| matches!(member, Ty::Null { .. }))
            {
                let null = members.remove(index);
                members.push(null);
            }
            match members.len() {
                0 => Ty::Never { attr },
                1 => members.pop().expect("singleton union has one member"),
                _ => Ty::Union(members.into(), attr),
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .into_iter()
                .map(|param| baml_codegen_types::CallableParam {
                    name: param.name,
                    ty: normalize_ty(&param.ty),
                    mode: match param.mode {
                        CodegenFunctionParamMode::Required => CodegenFunctionParamMode::Required,
                        CodegenFunctionParamMode::Optional => CodegenFunctionParamMode::Optional,
                    },
                })
                .collect(),
            ret: Box::new(normalize_ty(&ret)),
            throws: Box::new(normalize_ty(&throws)),
            attr,
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(normalize_ty(&value)),
            Box::new(normalize_ty(&error)),
            attr,
        ),
        leaf => leaf,
    }
}

#[cfg(test)]
mod tests {
    use baml_base::{Name, TyAttr};
    use baml_codegen_types::{Name as TypeName, Ty};

    use super::normalize_ty;

    fn attr() -> TyAttr {
        TyAttr::EMPTY
    }

    #[test]
    fn union_normalization_is_repeatable_across_discovery_permutations() {
        let members = vec![
            Ty::String { attr: attr() },
            Ty::Class(TypeName::local(Name::new("Person")), Box::new([]), attr()),
            Ty::Int { attr: attr() },
            Ty::Null { attr: attr() },
            Ty::Bool { attr: attr() },
        ];
        let expected = normalize_ty(&Ty::Union(members.clone().into(), attr()));
        let factorial = [1, 1, 2, 6, 24, 120];

        for rank in 0..100 {
            let mut remaining = members.clone();
            let mut permutation = Vec::with_capacity(remaining.len());
            let mut remainder = rank;
            for width in (1..=members.len()).rev() {
                let block = factorial[width - 1];
                let selected = remainder / block;
                remainder %= block;
                permutation.push(remaining.remove(selected));
            }
            assert_eq!(
                normalize_ty(&Ty::Union(permutation.into(), attr())),
                expected
            );
        }

        let Ty::Union(ordered, _) = expected else {
            panic!("five distinct members must remain a union");
        };
        assert!(matches!(ordered.last(), Some(Ty::Null { .. })));
    }

    #[test]
    fn normalization_reaches_nested_union_positions() {
        let union = |members| Ty::Union(members, attr());
        let left = union(Box::new([
            Ty::String { attr: attr() },
            Ty::Int { attr: attr() },
        ]));
        let right = union(Box::new([
            Ty::Int { attr: attr() },
            Ty::String { attr: attr() },
        ]));
        let first = Ty::Map {
            key: Box::new(Ty::String { attr: attr() }),
            value: Box::new(Ty::List(Box::new(left), attr())),
            attr: attr(),
        };
        let second = Ty::Map {
            key: Box::new(Ty::String { attr: attr() }),
            value: Box::new(Ty::List(Box::new(right), attr())),
            attr: attr(),
        };
        assert_eq!(normalize_ty(&first), normalize_ty(&second));
    }
}
