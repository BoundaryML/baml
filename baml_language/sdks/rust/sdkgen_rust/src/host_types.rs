use baml_base::qualified_name::BAML_JSON_JSON;
use baml_sdkgen_types::{CallableParam, Class, Function, Name, Symbol, SymbolPool, Ty};

const REFLECT_ANY_FUNCTION: &str = "reflect.AnyFunction";

pub(crate) fn lower_unrepresentable_literals(pool: &SymbolPool) -> SymbolPool {
    pool.iter()
        .map(|(name, symbol)| (name.clone(), lower_symbol(symbol)))
        .collect()
}

fn lower_symbol(symbol: &Symbol) -> Symbol {
    match symbol {
        Symbol::Function(function) => Symbol::Function(lower_function(function)),
        Symbol::Class(class) => Symbol::Class(lower_class(class)),
        Symbol::Enum(_) | Symbol::TypeAlias(_) => symbol.clone(),
    }
}

fn lower_class(class: &Class) -> Class {
    let mut class = class.clone();
    class.static_methods = class.static_methods.iter().map(lower_function).collect();
    class.instance_methods = class.instance_methods.iter().map(lower_function).collect();
    class
}

fn lower_function(function: &Function) -> Function {
    let mut function = function.clone();
    function.throws = function.throws.as_ref().map(widen_literals);
    function
}

/// The Rust type a host fills a parameter with: the part of the declared type
/// it can supply, with `unknown` taken as JSON and a function-value arm dropped.
/// The argument is still stated under the declared type, which is what the
/// engine checks it against.
pub(crate) fn narrow_argument(ty: &Ty) -> Ty {
    match ty {
        Ty::Unknown => Ty::TypeAlias(Name::from_dotted_path(BAML_JSON_JSON)),
        Ty::List(inner) => Ty::List(Box::new(narrow_argument(inner))),
        Ty::Map { key, value } => Ty::Map {
            key: key.clone(),
            value: Box::new(narrow_argument(value)),
        },
        Ty::Union(members) if members.iter().any(|member| !is_any_function(member)) => Ty::Union(
            members
                .iter()
                .filter(|member| !is_any_function(member))
                .map(narrow_argument)
                .collect(),
        )
        .canonicalize(),
        _ => ty.clone(),
    }
}

fn is_any_function(ty: &Ty) -> bool {
    matches!(ty, Ty::Interface(name, ..) if name == &Name::from_dotted_path(REFLECT_ANY_FUNCTION))
}

fn widen_literals(ty: &Ty) -> Ty {
    let widened = match ty {
        Ty::Literal(literal, _) => match literal {
            baml_base::Literal::Int(_) => Ty::Int,
            baml_base::Literal::Bigint(_) => Ty::Bigint,
            baml_base::Literal::Float(_) => Ty::Float,
            baml_base::Literal::String(_) => Ty::String,
            baml_base::Literal::Bool(_) => Ty::Bool,
        },
        Ty::List(inner) => Ty::List(Box::new(widen_literals(inner))),
        Ty::Map { key, value } => Ty::Map {
            key: Box::new(widen_literals(key)),
            value: Box::new(widen_literals(value)),
        },
        Ty::Union(members) => Ty::Union(members.iter().map(widen_literals).collect()),
        Ty::Class(name, args) => Ty::Class(name.clone(), args.iter().map(widen_literals).collect()),
        Ty::Interface(name, generics, associated) => Ty::Interface(
            name.clone(),
            generics.iter().map(widen_literals).collect(),
            associated
                .iter()
                .map(|(name, ty)| (name.clone(), widen_literals(ty)))
                .collect(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| CallableParam {
                    name: param.name.clone(),
                    ty: widen_literals(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(widen_literals(ret)),
            throws: Box::new(widen_literals(throws)),
        },
        Ty::Future(value, error) => Ty::Future(
            Box::new(widen_literals(value)),
            Box::new(widen_literals(error)),
        ),
        _ => ty.clone(),
    };
    widened.canonicalize()
}
