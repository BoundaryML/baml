use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};
pub(crate) fn supports(path: &str) -> bool {
    matches!(
        path.strip_prefix("baml.Uint8Array."),
        Some(
            "length"
                | "at"
                | "push"
                | "pop"
                | "concat"
                | "includes"
                | "index_of"
                | "reverse"
                | "slice"
                | "zeroes"
                | "from_array"
                | "to_array"
                | "from_hex"
                | "to_hex"
                | "from_base64"
                | "to_base64"
                | "_to_string_impl"
                | "sort"
        )
    )
}
pub(crate) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let method = d.path.rsplit('.').next().unwrap();
    let optional = || T::Optional(Box::new(T::Int));
    let list = || T::List(Box::new(T::Int));
    let (params, ret, receiver) = match method {
        "length" => (vec![], T::Int, true),
        "at" | "index_of" => (vec![T::Int], optional(), true),
        "push" => (vec![T::Int], T::Int, true),
        "pop" => (vec![], optional(), true),
        "concat" => (vec![T::Uint8Array], T::Uint8Array, true),
        "includes" => (vec![T::Int], T::Bool, true),
        "reverse" => (vec![], T::Uint8Array, true),
        "slice" => (vec![T::Int, T::Int], T::Uint8Array, true),
        "zeroes" => (vec![T::Int], T::Uint8Array, false),
        "from_array" => (vec![list()], T::Uint8Array, false),
        "to_array" => (vec![], list(), true),
        "from_hex" | "from_base64" => (vec![T::String], T::Uint8Array, false),
        "to_hex" | "to_base64" | "_to_string_impl" => (vec![], T::String, true),
        "sort" => (vec![], T::Null, true),
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
    let mut types = if receiver {
        vec![T::Uint8Array]
    } else {
        vec![]
    };
    types.extend(params);
    let mut params = types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "a{i}:{}",
                match t {
                    T::String => "rt::strings::Str",
                    T::Int => "i64",
                    _ => "usize",
                }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    if method == "to_array" {
        params.push_str(",desc:u32");
    }
    let args = types
        .iter()
        .enumerate()
        .map(|(i, t)| match t {
            T::Uint8Array => format!("&rt::native::bytes_owned(a{i})"),
            T::String => format!("a{i}.try_as_str(&rt::strings::Cx::current())?"),
            T::List(_) => format!("&rt::native::int_list_owned(a{i})"),
            _ => format!("a{i}"),
        })
        .collect::<Vec<_>>()
        .join(",");
    let call = format!("rt::native::bytes_body::{method}({args})");
    let call = if matches!(method, "zeroes" | "from_array" | "from_hex" | "from_base64") {
        format!("{call}.map_err(rt::scalars::error)?")
    } else {
        call
    };
    let (rust, result) = if matches!(method, "push" | "pop" | "sort") {
        let args = if method == "push" { ",a1" } else { "" };
        let call =
            format!("rt::native::with_bytes(a0,|v|rt::native::bytes_body::{method}(v{args}))?");
        if method == "pop" {
            ("rt::Dyn", format!("Ok(rt::native::optional_int({call}))"))
        } else if method == "sort" {
            ("usize", format!("{call};Ok(0usize)"))
        } else {
            ("i64", format!("Ok({call})"))
        }
    } else {
        match ret {
            T::Uint8Array => ("usize", format!("rt::native::bytes({call})")),
            T::String => (
                "usize",
                format!("rt::strings::Cx::current().string({call}.as_str()).map(|s|s.as_raw())"),
            ),
            T::List(_) => ("usize", format!("rt::native::int_list(desc,{call})")),
            T::Optional(_) => ("rt::Dyn", format!("Ok(rt::native::optional_int({call}))")),
            T::Int => ("i64", format!("Ok({call})")),
            T::Bool => ("bool", format!("Ok({call})")),
            _ => unreachable!(),
        }
    };
    Ok(format!(
        "#[inline(always)] fn rt_{}({params})->rt::R<{rust}>{{unsafe{{{result}}}}}\n",
        d.fn_name
    ))
}
