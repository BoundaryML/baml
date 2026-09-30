//! Shared BAML types exposed to every code generator.
//!
//! The representation itself belongs to `baml_type`: generators consume the
//! same compiler-owned qualified names and the same structurally narrowed type
//! family instead of maintaining a parallel enum with lossy conversions.

pub use baml_type::{
    CodegenFunctionParamTy as CallableParam, CodegenTy as Ty, Freshness,
    FunctionParamMode as CodegenFunctionParamMode, ParamTy, QualifiedTypeName as Name,
};

/// Test each immediate structural child in declaration order, stopping on true.
/// Alias definitions are not expanded: callers own symbol lookup and cycle policy.
/// The exhaustive match makes new type variants require a traversal decision.
pub fn any_type_child(ty: &Ty, mut predicate: impl FnMut(&Ty) -> bool) -> bool {
    match ty {
        Ty::Class(_, args) | Ty::Union(args) => args.iter().any(predicate),
        Ty::Interface(_, args, associated) => {
            args.iter().any(&mut predicate) || associated.iter().any(|(_, ty)| predicate(ty))
        }
        Ty::List(inner) => predicate(inner),
        Ty::Map { key, value } => predicate(key) || predicate(value),
        Ty::Function {
            params,
            ret,
            throws,
        } => params.iter().any(|p| predicate(&p.ty)) || predicate(ret) || predicate(throws),
        Ty::Future(value, error) => predicate(value) || predicate(error),
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(..)
        | Ty::TypeVar(..)
        | Ty::RustType
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Void
        | Ty::Unknown
        | Ty::Never => false,
    }
}

/// Visit every node in pre-order without expanding named aliases.
pub fn visit_type(ty: &Ty, visitor: &mut impl FnMut(&Ty)) {
    visitor(ty);
    any_type_child(ty, |child| {
        visit_type(child, visitor);
        false
    });
}

#[cfg(test)]
mod traversal_tests {
    use super::*;
    fn var(name: &str) -> Ty {
        Ty::TypeVar(ParamTy::new(0, name.into()))
    }

    #[test]
    fn visits_interface_associated_types_and_both_future_and_callable_results() {
        let interface = Ty::Interface(
            Name::new("user".into(), vec![], "Container".into()),
            Box::new([var("Generic")]),
            Box::new([("Item".into(), var("Associated"))]),
        );
        let ty = Ty::Function {
            params: Box::new([CallableParam::required(
                None,
                Ty::Map {
                    key: Box::new(var("Key")),
                    value: Box::new(Ty::List(Box::new(interface))),
                },
            )]),
            ret: Box::new(Ty::Future(Box::new(var("Value")), Box::new(var("Error")))),
            throws: Box::new(var("Throws")),
        };
        let mut names = Vec::new();
        visit_type(&ty, &mut |ty| {
            if let Ty::TypeVar(param) = ty {
                names.push(param.as_str().to_owned());
            }
        });
        assert_eq!(
            names,
            ["Key", "Generic", "Associated", "Value", "Error", "Throws"]
        );
    }

    #[test]
    fn child_predicate_short_circuits_without_visiting_later_children() {
        let ty = Ty::Union(Box::new([var("First"), var("Second")]));
        let mut visits = 0;
        assert!(any_type_child(&ty, |_| {
            visits += 1;
            true
        }));
        assert_eq!(visits, 1);
        assert!(!any_type_child(&var("Leaf"), |_| panic!(
            "a variable has no children"
        )));
    }
}
