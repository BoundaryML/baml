//! Checked adapters for heap-neutral bigint bodies.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};
pub(crate) fn supports(path: &str) -> bool {
    matches!(
        path.strip_prefix("baml.Bigint."),
        Some(
            "abs"
                | "isqrt"
                | "pow"
                | "ilog"
                | "to_int"
                | "parse"
                | "_random_byte_count"
                | "_random_in_range"
        )
    )
}
pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let method = d.path.rsplit('.').next().unwrap();
    let (params, ret, receiver) = match method {
        "abs" | "isqrt" => (vec![], T::Bigint, true),
        "pow" | "ilog" => (vec![T::Bigint], T::Bigint, true),
        "to_int" => (vec![], T::Int, true),
        "parse" => (vec![T::String], T::Bigint, false),
        "_random_byte_count" => (vec![T::Bigint, T::Bigint], T::Int, false),
        "_random_in_range" => (vec![T::Uint8Array, T::Bigint, T::Bigint], T::Bigint, false),
        _ => return Err(format!("unsupported in baml_rt: {}", d.path)),
    };
    if d.pipeline != BuiltinPipeline::Vm
        || !d.generics.is_empty()
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
        || d.receiver
            .as_ref()
            .is_some_and(|r| r.receiver_type != ReceiverType::Static)
            != receiver
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        ));
    }
    let mut types = if receiver { vec![T::Bigint] } else { vec![] };
    types.extend(params);
    let params = types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "a{i}:{}",
                if *t == T::String {
                    "rt::strings::Str"
                } else {
                    "usize"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let args = types
        .iter()
        .enumerate()
        .map(|(i, t)| match t {
            T::Bigint => format!("rt::native::bigint_owned(a{i})"),
            T::Uint8Array => format!("&rt::native::bytes_owned(a{i})"),
            _ => format!("a{i}.as_str(&rt::strings::Cx::current())"),
        })
        .collect::<Vec<_>>()
        .join(",");
    let call = format!("rt::native::bigint_body::{method}({args})");
    let call = if matches!(method, "isqrt" | "pow" | "ilog" | "to_int" | "parse") {
        format!("{call}.map_err(rt::scalars::error)?")
    } else {
        call
    };
    let result = if ret == T::Bigint {
        format!("rt::native::bigint({call})")
    } else {
        format!("Ok({call})")
    };
    Ok(format!(
        "#[inline(always)] fn rt_{}({params})->rt::R<{}>{{unsafe{{{result}}}}}\n",
        d.fn_name,
        if ret == T::Int { "i64" } else { "usize" }
    ))
}
