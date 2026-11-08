//! Adapters for the shared, heap-neutral scalar implementations.
use crate::{BamlType, BuiltinPipeline, NativeBuiltin, ReceiverType};

pub(crate) fn supports(path: &str) -> bool {
    let Some((class, method)) = path.rsplit_once('.') else {
        return false;
    };
    match class {
        "baml.Int" => matches!(
            method,
            "abs"
                | "isqrt"
                | "pow"
                | "parse"
                | "_random_in_range"
                | "ilog"
                | "leading_zeros"
                | "leading_ones"
                | "trailing_zeros"
                | "trailing_ones"
                | "count_zeros"
                | "count_ones"
        ),
        "baml.Float" => matches!(
            method,
            "is_nan"
                | "is_infinite"
                | "abs"
                | "signum"
                | "floor"
                | "ceil"
                | "round"
                | "trunc"
                | "fract"
                | "ifloor"
                | "iceil"
                | "iround"
                | "itrunc"
                | "sqrt"
                | "pow"
                | "log"
                | "log10"
                | "cbrt"
                | "hypot"
                | "sin"
                | "cos"
                | "tan"
                | "asin"
                | "acos"
                | "atan"
                | "atan2"
                | "sinh"
                | "cosh"
                | "tanh"
                | "asinh"
                | "acosh"
                | "atanh"
                | "parse"
                | "_unit_from_draw"
                | "pi"
                | "e"
                | "golden_ratio"
                | "nan"
                | "inf"
                | "max_finite"
                | "min_finite"
                | "epsilon"
        ),
        _ => false,
    }
}

pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let changed = || {
        format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        )
    };
    let (class, method) = d.path.rsplit_once('.').ok_or_else(changed)?;
    let float = class == "baml.Float";
    let scalar = if float {
        BamlType::Float
    } else {
        BamlType::Int
    };
    let fallible = matches!(
        method,
        "parse" | "ifloor" | "iceil" | "iround" | "itrunc" | "isqrt" | "ilog"
    ) || (!float && method == "abs");
    let static_method = matches!(
        method,
        "parse"
            | "_random_in_range"
            | "_unit_from_draw"
            | "pi"
            | "e"
            | "golden_ratio"
            | "nan"
            | "inf"
            | "max_finite"
            | "min_finite"
            | "epsilon"
    );
    let params = match method {
        "parse" => vec![BamlType::String],
        "_random_in_range" => vec![BamlType::Int; 3],
        "_unit_from_draw" => vec![BamlType::Int],
        "pow" | "log" | "hypot" | "atan2" => vec![scalar.clone()],
        "ilog" => vec![BamlType::Int],
        _ => vec![],
    };
    let ret = if matches!(method, "is_nan" | "is_infinite") {
        BamlType::Bool
    } else if matches!(method, "ifloor" | "iceil" | "iround" | "itrunc") {
        BamlType::Int
    } else {
        scalar.clone()
    };
    if d.pipeline != BuiltinPipeline::Vm
        || !d.generics.is_empty()
        || d.may_yield
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
        || d.receiver.as_ref().is_none_or(|r| {
            r.receiver_type
                != if static_method {
                    ReceiverType::Static
                } else {
                    ReceiverType::RefSelf
                }
        })
        || (d.fallible || d.throws.as_ref().is_some_and(|t| !t.is_empty())) != fallible
    {
        return Err(changed());
    }
    fn rust(ty: &BamlType) -> &'static str {
        match ty {
            BamlType::Int => "i64",
            BamlType::Float => "f64",
            BamlType::Bool => "bool",
            BamlType::String => "rt::strings::Str",
            _ => unreachable!(),
        }
    }
    let mut types = Vec::new();
    if !static_method {
        types.push(scalar);
    }
    types.extend(params);
    let arguments = types
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("a{i}:{}", rust(ty)))
        .collect::<Vec<_>>()
        .join(",");
    let values = types
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            if *ty == BamlType::String {
                format!("a{i}.as_str(&cx)")
            } else {
                format!("a{i}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let result = format!(
        "rt::scalars::{}::{method}({values})",
        if float { "float" } else { "int" }
    );
    let result = if fallible {
        format!("{result}.map_err(rt::scalars::error)")
    } else {
        format!("Ok({result})")
    };
    let context = if types.contains(&BamlType::String) {
        "let cx=unsafe{rt::strings::Cx::current()};"
    } else {
        ""
    };
    Ok(format!(
        "#[inline(always)] fn rt_{}({arguments})->rt::R<{}>{{{context}{result}}}\n",
        d.fn_name,
        rust(&ret)
    ))
}
