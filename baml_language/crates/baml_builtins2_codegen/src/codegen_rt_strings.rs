//! String-only adapters, checked against the authoritative native signatures.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};

fn signature(method: &str) -> Option<(Vec<T>, T)> {
    let string = T::String;
    let int = T::Int;
    Some(match method {
        "byte_length" => (vec![], int),
        "to_lower_case" | "to_upper_case" | "trim" | "trim_start" | "trim_end" => (vec![], string),
        "includes" | "starts_with" | "ends_with" => (vec![string], T::Bool),
        "split" => (vec![string.clone()], T::List(Box::new(string))),
        "chars" | "lines" => (vec![], T::List(Box::new(string))),
        "_replace_literal" | "_replace_all_literal" => {
            (vec![string.clone(), string.clone()], string)
        }
        "index_of" | "last_index_of" => (vec![string], T::Optional(Box::new(int))),
        "at" => (vec![int], T::Optional(Box::new(string))),
        "code_point_at" => (vec![int.clone()], T::Optional(Box::new(int))),
        "repeat" => (vec![int], string),
        "is_numeric"
        | "is_alphabetic"
        | "is_alphanumeric"
        | "is_uppercase"
        | "is_lowercase"
        | "is_whitespace"
        | "is_control"
        | "is_graphic"
        | "is_ascii"
        | "is_ascii_numeric"
        | "is_ascii_alphabetic"
        | "is_ascii_alphanumeric"
        | "is_ascii_uppercase"
        | "is_ascii_lowercase"
        | "is_ascii_whitespace"
        | "is_ascii_control"
        | "is_ascii_graphic"
        | "is_ascii_hex" => (vec![], T::Bool),
        _ => return None,
    })
}
pub(super) fn supports(path: &str) -> bool {
    if matches!(
        path,
        "baml.Int.parse" | "baml.String.from_code_points" | "baml._to_string_primitive"
    ) {
        return true;
    }
    path.strip_prefix("baml.String.")
        .and_then(signature)
        .is_some()
}
pub(super) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    if d.path == "baml._to_string_primitive" {
        if d.pipeline != BuiltinPipeline::Vm
            || !d.generics.is_empty()
            || d.receiver.is_some()
            || d.may_yield
            || d.fallible
            || d.throws.as_ref().is_none_or(|t| !t.is_empty())
            || d.params
                .iter()
                .map(|p| &p.ty)
                .ne([T::Named("union".into())].iter())
            || d.return_type != T::String
        {
            return Err(format!(
                "unsupported in baml_rt: changed builtin signature {}",
                d.path
            ));
        }
        return Ok("#[inline(always)] fn rt_primitive_to_string(cx:&mut rt::strings::Cx,value:rt::Dyn)->rt::R<rt::strings::Str>{match value.tag{0=>rt::strings::integer_to_string(cx,value.payload as i64),1=>cx.string(if value.payload!=0{\"true\"}else{\"false\"}),2=>cx.string(\"null\"),_=>unreachable!(\"compiler-checked scalar string conversion\")}}\n".into());
    }

    if matches!(
        d.path.as_str(),
        "baml.Int.parse" | "baml.String.from_code_points"
    ) {
        let integer = d.path == "baml.Int.parse";
        let params = if integer {
            vec![T::String]
        } else {
            vec![T::List(Box::new(T::Int))]
        };
        let ret = if integer { T::Int } else { T::String };
        if d.pipeline != BuiltinPipeline::Vm
            || !d.generics.is_empty()
            || d.receiver.as_ref().is_none_or(|r| {
                r.receiver_type != ReceiverType::Static
                    || r.builtin_name() != if integer { "Int" } else { "String" }
            })
            || d.may_yield
            || d.fallible
            || d.throws.as_deref()
                != Some(&[if integer {
                    "ParseError".to_owned()
                } else {
                    "InvalidArgument".to_owned()
                }])
            || d.params.iter().map(|p| &p.ty).ne(params.iter())
            || d.return_type != ret
        {
            return Err(format!(
                "unsupported in baml_rt: changed builtin signature {}",
                d.path
            ));
        }
        return Ok(if integer{
            "#[inline(always)] fn rt_int_parse(cx:&mut rt::strings::Cx,value:rt::strings::Str,error_desc:u32)->rt::R<i64>{rt::strings::parse_int(cx,value,error_desc)}\n"
        }else{
            "#[inline(always)] unsafe fn rt_from_code_points(cx:&mut rt::strings::Cx,values:usize,error_desc:u32)->rt::R<rt::strings::Str>{unsafe{rt::strings::from_code_points(cx,values,error_desc)}}\n"
        }.into());
    }
    let method = d.path.strip_prefix("baml.String.").unwrap();
    let (params, ret) = signature(method).unwrap();
    if d.pipeline != BuiltinPipeline::Vm
        || !d.generics.is_empty()
        || d.may_yield
        || d.fallible
        || d.throws.as_ref().is_none_or(|t| !t.is_empty())
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
        || d.receiver.as_ref().is_none_or(|r| {
            r.builtin_name() != "String" || r.receiver_type != ReceiverType::RefSelf
        })
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        ));
    }
    let s = "rt::strings::Str";
    let (extra, output, body) = match method {
        "byte_length" => (
            String::new(),
            "i64".to_owned(),
            "Ok(rt::strings::byte_length(cx,value))".to_owned(),
        ),
        "to_lower_case" | "to_upper_case" | "trim" | "trim_start" | "trim_end" => (
            String::new(),
            s.to_owned(),
            format!("rt::strings::transform(cx,value,{method:?})"),
        ),
        "includes" | "starts_with" | "ends_with" => (
            format!(",needle:{s}"),
            "bool".to_owned(),
            format!("rt::strings::search(cx,value,needle,{method:?})"),
        ),
        "index_of" | "last_index_of" => (
            format!(",needle:{s}"),
            "rt::Dyn".to_owned(),
            format!(
                "rt::strings::index_of(cx,value,needle,{})",
                method == "last_index_of"
            ),
        ),
        "at" => (
            ",index:i64".into(),
            "usize".into(),
            "rt::strings::at(cx,value,index)".into(),
        ),
        "code_point_at" => (
            ",index:i64".into(),
            "rt::Dyn".into(),
            "rt::strings::code_point_at(cx,value,index)".into(),
        ),
        "repeat" => (
            ",count:i64".into(),
            s.into(),
            "rt::strings::repeat(cx,value,count)".into(),
        ),
        "_replace_literal" | "_replace_all_literal" => (
            format!(",search:{s},replacement:{s}"),
            s.into(),
            format!(
                "rt::strings::replace(cx,value,search,replacement,{})",
                method == "_replace_all_literal"
            ),
        ),
        "split" => (
            format!(",delimiter:{s},desc:u32"),
            "usize".into(),
            "unsafe{rt::strings::segments(cx,value,Some(delimiter),false,desc)}".into(),
        ),
        "chars" | "lines" => (
            ",desc:u32".into(),
            "usize".into(),
            format!(
                "unsafe{{rt::strings::segments(cx,value,None,{},desc)}}",
                method == "lines"
            ),
        ),
        _ => (
            String::new(),
            "bool".to_owned(),
            format!("rt::strings::predicate(cx,value,{method:?})"),
        ),
    };
    Ok(format!(
        "#[inline(always)] fn rt_string_{method}(cx:&mut rt::strings::Cx,value:{s}{extra})->rt::R<{output}>{{{body}}}\n"
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_string_adapters_match_stdlib_and_parse_as_rust() {
        let (declarations, _, _) = crate::extract_native_builtins().unwrap();
        let requested = declarations
            .iter()
            .filter(|d| super::supports(&d.path))
            .map(|d| d.path.clone())
            .collect();
        let source = crate::generate_rt(&declarations, &requested).unwrap();
        syn::parse_file(&source).unwrap();
        assert!(source.contains("rt_string_split"));
        assert!(source.contains("rt_int_parse"));
    }
}
