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
        Ty::Literal(literal, _, attr) => match literal {
            baml_base::Literal::Int(_) => Ty::Int { attr: attr.clone() },
            baml_base::Literal::Bigint(_) => Ty::Bigint { attr: attr.clone() },
            baml_base::Literal::Float(_) => Ty::Float { attr: attr.clone() },
            baml_base::Literal::String(_) => Ty::String { attr: attr.clone() },
            baml_base::Literal::Bool(_) => Ty::Bool { attr: attr.clone() },
        },
        Ty::List(inner, attr) => Ty::List(Box::new(widen_literals(inner)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(widen_literals(key)),
            value: Box::new(widen_literals(value)),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => {
            Ty::Union(members.iter().map(widen_literals).collect(), attr.clone())
        }
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(widen_literals).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, generics, associated, attr) => Ty::Interface(
            name.clone(),
            generics.iter().map(widen_literals).collect(),
            associated
                .iter()
                .map(|(name, ty)| (name.clone(), widen_literals(ty)))
                .collect(),
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
                .map(|param| CallableParam {
                    name: param.name.clone(),
                    ty: widen_literals(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(widen_literals(ret)),
            throws: Box::new(widen_literals(throws)),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(widen_literals(value)),
            Box::new(widen_literals(error)),
            attr.clone(),
        ),
        _ => ty.clone(),
    };
    widened.canonicalize()
}
