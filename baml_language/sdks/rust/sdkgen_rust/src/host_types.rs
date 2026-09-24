use baml_codegen_types::{CallableParam, Class, Function, Symbol, SymbolPool, Ty};

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
