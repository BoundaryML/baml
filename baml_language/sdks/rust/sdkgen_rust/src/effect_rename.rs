//! Rename synthetic effect params before codegen.
//!
//! A callback parameter whose `throws` is inferred carries a
//! compiler-synthesized effect param (`__effect_param_N`). Left as-is that
//! internal name leaks into user-facing Rust — the function's generic, and
//! (for a `throws E | …` contract) the synthesized union enum's name, generic,
//! and variant. This pass rewrites each such param to a readable name derived
//! from the owning callback parameter (`cb` → `CbError`), once, at the pool
//! boundary, so every downstream stage (analysis, union synthesis, emission)
//! produces the nice name with no further plumbing.
//!
//! The rename is Rust-cosmetic only: the effect param is realized from the
//! callback at dispatch and never rides the wire (it is not in the function's
//! sent `type_args`), so the engine never sees these names.

use std::collections::{HashMap, HashSet};

use baml_codegen_types::{CallableParam, Class, Function, ParamTy, Symbol, SymbolPool, Ty};

/// Rewrite every function / method signature in `pool` with its synthetic
/// effect params renamed, returning the rewritten pool.
pub(crate) fn rename_effect_params(pool: &SymbolPool) -> SymbolPool {
    pool.iter()
        .map(|(name, symbol)| (name.clone(), rename_symbol(symbol)))
        .collect()
}

fn rename_symbol(symbol: &Symbol) -> Symbol {
    match symbol {
        Symbol::Function(function) => Symbol::Function(rename_function(function, &[])),
        Symbol::Class(class) => Symbol::Class(rename_class(class)),
        Symbol::Enum(_) | Symbol::TypeAlias(_) => symbol.clone(),
    }
}

fn rename_class(class: &Class) -> Class {
    let class_params: Vec<String> = class
        .generic_params
        .iter()
        .map(|p| p.as_str().to_string())
        .collect();
    let mut class = class.clone();
    for method in &mut class.static_methods {
        *method = rename_function(method, &class_params);
    }
    for method in &mut class.instance_methods {
        *method = rename_function(method, &class_params);
    }
    class
}

/// Rewrite a single function's synthetic effect params, returning a modified
/// clone (or the original when it has none). `class_params` are the enclosing
/// class's generics (for a method), reserved so a nice name never shadows one.
fn rename_function(function: &Function, class_params: &[String]) -> Function {
    let renames = effect_renames(function, class_params);
    if renames.is_empty() {
        return function.clone();
    }
    let mut function = function.clone();
    for arg in &mut function.arguments {
        arg.ty = rename_typevars(&arg.ty, &renames);
    }
    function.return_type = rename_typevars(&function.return_type, &renames);
    function.throws = function
        .throws
        .as_ref()
        .map(|t| rename_typevars(t, &renames));
    function
}

/// The callback function type at a parameter's root: the parameter type
/// itself, or the sole function member of an optional callback (`cb: ((v:
/// int) -> int)?`, which reaches codegen as a `Function | null` union). These
/// are exactly the shapes the compiler opens to a synthetic effect param, so
/// they are exactly the ones that need a readable name here.
pub(crate) fn callback_root(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Function { .. } => Some(ty),
        Ty::Union(members, _) => {
            let mut callback = None;
            for member in members {
                match member {
                    Ty::Null { .. } => {}
                    Ty::Function { .. } if callback.is_none() => callback = Some(member),
                    _ => return None,
                }
            }
            callback
        }
        _ => None,
    }
}

/// Build the `__effect_param_N` → readable-name map: each direct callback
/// parameter whose inferred `throws` is a typevar absent from the declared
/// generics contributes `<PascalCase(param)>Error`, de-collided against the
/// declared generics and each other.
fn effect_renames(function: &Function, class_params: &[String]) -> HashMap<String, String> {
    let declared: HashSet<String> = function
        .generic_params
        .iter()
        .map(|p| p.as_str().to_string())
        .chain(class_params.iter().cloned())
        .collect();
    let mut renames: HashMap<String, String> = HashMap::new();
    let mut taken: HashSet<String> = declared.clone();
    for arg in &function.arguments {
        let Some(Ty::Function { throws, .. }) = callback_root(&arg.ty) else {
            continue;
        };
        let Ty::TypeVar(effect, _) = throws.as_ref() else {
            continue;
        };
        let effect = effect.as_str();
        // Only synthetic effect params — a callback-throws typevar absent from
        // the declared generics — are renamed; a callback whose `throws` names
        // a user generic keeps that name.
        if declared.contains(effect) || renames.contains_key(effect) {
            continue;
        }
        let mut nice = format!("{}Error", pascal_case(arg.name.as_str()));
        while taken.contains(&nice) {
            nice.push('_');
        }
        taken.insert(nice.clone());
        renames.insert(effect.to_string(), nice);
    }
    renames
}

/// `snake_case` (or a bare ident) to `PascalCase`: `cb` → `Cb`,
/// `my_callback` → `MyCallback`.
fn pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Structurally rewrite `ty`, renaming any `TypeVar` whose name is in
/// `renames`. Everything else is cloned unchanged.
fn rename_typevars(ty: &Ty, renames: &HashMap<String, String>) -> Ty {
    match ty {
        Ty::TypeVar(name, attr) => match renames.get(name.as_str()) {
            Some(nice) => Ty::TypeVar(
                ParamTy::new(name.index(), baml_base::Name::new(nice.as_str())),
                attr.clone(),
            ),
            None => ty.clone(),
        },
        Ty::List(inner, attr) => Ty::List(Box::new(rename_typevars(inner, renames)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(rename_typevars(key, renames)),
            value: Box::new(rename_typevars(value, renames)),
            attr: attr.clone(),
        },
        Ty::Union(items, attr) => Ty::Union(
            items.iter().map(|t| rename_typevars(t, renames)).collect(),
            attr.clone(),
        ),
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(|t| rename_typevars(t, renames)).collect(),
            attr.clone(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|p| CallableParam {
                    name: p.name.clone(),
                    ty: rename_typevars(&p.ty, renames),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(rename_typevars(ret, renames)),
            throws: Box::new(rename_typevars(throws, renames)),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(rename_typevars(value, renames)),
            Box::new(rename_typevars(error, renames)),
            attr.clone(),
        ),
        Ty::Interface(name, generics, associated, attr) => Ty::Interface(
            name.clone(),
            generics
                .iter()
                .map(|t| rename_typevars(t, renames))
                .collect(),
            associated
                .iter()
                .map(|(n, t)| (n.clone(), rename_typevars(t, renames)))
                .collect(),
            attr.clone(),
        ),
        // Leaves — no nested `Ty` to rewrite.
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Void { .. }
        | Ty::Literal(..)
        | Ty::Uint8Array { .. }
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(..)
        | Ty::Media(..)
        | Ty::Unknown { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Never { .. }
        | Ty::RustType { .. } => ty.clone(),
    }
}

#[cfg(test)]
mod tests {
    use baml_base::{Name as BaseName, TyAttr};
    use baml_codegen_types::{CallableParam, Function, FunctionArgument, Origin, ParamTy, Ty};

    use super::{callback_root, rename_function};

    fn int() -> Ty {
        Ty::Int {
            attr: TyAttr::EMPTY,
        }
    }

    fn effect_var() -> Ty {
        Ty::TypeVar(
            ParamTy::new(0, BaseName::new("__effect_param_0")),
            TyAttr::EMPTY,
        )
    }

    fn callback_ty() -> Ty {
        Ty::Function {
            params: Box::new([CallableParam {
                name: Some(BaseName::new("value")),
                ty: int(),
                mode: baml_codegen_types::CodegenFunctionParamMode::Required,
            }]),
            ret: Box::new(int()),
            throws: Box::new(effect_var()),
            attr: TyAttr::EMPTY,
        }
    }

    fn function_taking(arg_name: &str, ty: Ty) -> Function {
        Function {
            name: BaseName::new("apply"),
            generic_params: Vec::new(),
            docstring: None,
            arguments: vec![FunctionArgument {
                injected: false,
                name: BaseName::new(arg_name),
                docstring: None,
                ty,
                default: None,
            }],
            return_type: int(),
            throws: Some(effect_var()),
            watchers: Vec::new(),
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        }
    }

    fn throws_name(function: &Function) -> String {
        match function.throws.as_ref().expect("throws") {
            Ty::TypeVar(param, _) => param.name().as_str().to_string(),
            other => panic!("expected a type var, got {other:?}"),
        }
    }

    #[test]
    fn optional_callback_is_a_callback_root() {
        let optional = Ty::Union(
            Box::new([
                callback_ty(),
                Ty::Null {
                    attr: TyAttr::EMPTY,
                },
            ]),
            TyAttr::EMPTY,
        );
        assert!(matches!(
            callback_root(&optional),
            Some(Ty::Function { .. })
        ));
    }

    #[test]
    fn a_list_of_callbacks_is_not_a_callback_root() {
        let listed = Ty::List(Box::new(callback_ty()), TyAttr::EMPTY);
        assert!(callback_root(&listed).is_none());
    }

    /// An optional callback's synthetic effect param gets the same readable
    /// name the immediate form gets — otherwise `__effect_param_0` would leak
    /// into the generated Rust generic.
    #[test]
    fn optional_callback_effect_param_is_renamed() {
        let immediate = rename_function(&function_taking("cb", callback_ty()), &[]);
        assert_eq!(throws_name(&immediate), "CbError");

        let optional_ty = Ty::Union(
            Box::new([
                callback_ty(),
                Ty::Null {
                    attr: TyAttr::EMPTY,
                },
            ]),
            TyAttr::EMPTY,
        );
        let optional = rename_function(&function_taking("cb", optional_ty), &[]);
        assert_eq!(throws_name(&optional), "CbError");
    }
}
