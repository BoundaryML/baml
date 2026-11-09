//! Regex natives keep the VM's engine, with typed managed arguments.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};
pub(super) fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.regex._compile"
            | "baml.regex.escape"
            | "baml.regex._index_of_from"
            | "baml.Map.get"
            | "baml.Map.has"
            | "baml.Map.keys"
            | "baml.Map.length"
    ) || path.strip_prefix("baml.regex.Regex.").is_some_and(|m| {
        matches!(
            m,
            "has_match"
                | "match_once"
                | "match_all"
                | "match_full"
                | "split"
                | "_replace_template"
                | "_replace_all_template"
        )
    })
}
pub(super) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    let method = d.path.rsplit('.').next().unwrap();
    let map = d.path.starts_with("baml.Map.");
    let instance = d.path.starts_with("baml.regex.Regex.");
    let (params, ret) = if map {
        match method {
            "get" => (
                vec![T::Generic("K".into())],
                T::Optional(Box::new(T::Generic("V".into()))),
            ),
            "has" => (vec![T::Generic("K".into())], T::Bool),
            "keys" => (vec![], T::List(Box::new(T::Generic("K".into())))),
            "length" => (vec![], T::Int),
            _ => unreachable!(),
        }
    } else {
        match method {
            "_compile" => (vec![T::String, T::Bool], T::Named("Regex".into())),
            "escape" => (vec![T::String], T::String),
            "_index_of_from" => (
                vec![T::String, T::String, T::Int],
                T::Optional(Box::new(T::Int)),
            ),
            "has_match" => (vec![T::String], T::Bool),
            "match_once" | "match_full" => (
                vec![T::String],
                T::Optional(Box::new(T::Named("Match".into()))),
            ),
            "match_all" => (vec![T::String], T::List(Box::new(T::Named("Match".into())))),
            "split" => (vec![T::String], T::List(Box::new(T::String))),
            "_replace_template" | "_replace_all_template" => {
                (vec![T::String, T::String], T::String)
            }
            _ => unreachable!(),
        }
    };
    let throws = if method == "_compile" {
        vec!["Error".to_owned()]
    } else {
        vec![]
    };
    let valid = d.pipeline == BuiltinPipeline::Vm
        && d.generics == if map { vec!["K", "V"] } else { vec![] }
        && d.may_yield == (map && matches!(method, "get" | "has"))
        && d.fallible == instance
        && d.throws.as_deref() == Some(throws.as_slice())
        && d.params.iter().map(|p| &p.ty).eq(params.iter())
        && d.return_type == ret
        && if instance || map {
            d.receiver.as_ref().is_some_and(|r| {
                r.receiver_type == ReceiverType::RefSelf
                    && r.builtin_name() == if map { "Map" } else { "Regex" }
            })
        } else {
            d.receiver.is_none()
        };
    if !valid {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}: params={:?}, return={:?}, generics={:?}, receiver={:?}, fallible={}, may_yield={}, throws={:?}",
            d.path,
            d.params.iter().map(|p| &p.ty).collect::<Vec<_>>(),
            d.return_type,
            d.generics,
            d.receiver.is_some(),
            d.fallible,
            d.may_yield,
            d.throws
        ));
    }
    // Map adapters are lowered directly by the regex feature after this generic
    // signature check; existing string/int map adapters retain their ABI.
    if map {
        return Ok(String::new());
    }
    Ok(match method{
        "_compile"=>"#[inline(always)] fn rt_regex_compile(cx:&mut rt::strings::Cx,pattern:rt::strings::Str,backtracking:bool,schema:&rt::regex::Schema)->rt::R<usize>{rt::regex::compile(cx,pattern,backtracking,schema)}\n".into(),
        "escape"=>"#[inline(always)] fn rt_regex_escape(cx:&mut rt::strings::Cx,text:rt::strings::Str)->rt::R<rt::strings::Str>{rt::regex::escape(cx,text)}\n".into(),
        "_index_of_from"=>"#[inline(always)] fn rt_regex_index_from(cx:&rt::strings::Cx,haystack:rt::strings::Str,search:rt::strings::Str,start:i64)->rt::R<rt::Dyn>{rt::regex::index_from(cx,haystack,search,start)}\n".into(),
        "has_match"=>"#[inline(always)] unsafe fn rt_regex_has_match(cx:&rt::strings::Cx,regex:usize,text:rt::strings::Str,schema:&rt::regex::Schema)->rt::R<bool>{unsafe{rt::regex::has_match(cx,regex,text,schema)}}\n".into(),
        "match_once"|"match_all"|"match_full"=>format!("#[inline(always)] unsafe fn rt_regex_{method}(cx:&mut rt::strings::Cx,regex:usize,text:rt::strings::Str,schema:&rt::regex::Schema)->rt::R<usize>{{unsafe{{rt::regex::matches(cx,regex,text,{method:?},schema)}}}}\n"),
        "split"=>"#[inline(always)] unsafe fn rt_regex_split(cx:&mut rt::strings::Cx,regex:usize,text:rt::strings::Str,desc:u32,schema:&rt::regex::Schema)->rt::R<usize>{unsafe{rt::regex::split(cx,regex,text,desc,schema)}}\n".into(),
        "_replace_template"|"_replace_all_template"=>format!("#[inline(always)] unsafe fn rt_regex_{method}(cx:&mut rt::strings::Cx,regex:usize,text:rt::strings::Str,template:rt::strings::Str,schema:&rt::regex::Schema)->rt::R<rt::strings::Str>{{unsafe{{rt::regex::replace(cx,regex,text,template,{},schema)}}}}\n",method=="_replace_all_template"),_=>unreachable!(),
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn signatures_and_adapters_cover_regex_and_utf8() {
        let (declarations, _, _) = crate::extract_native_builtins().unwrap();
        let requested = declarations
            .iter()
            .filter(|d| super::supports(&d.path) || crate::codegen_rt_bytes::supports(&d.path) || crate::codegen_rt_string_bytes::supports(&d.path))
            .map(|d| d.path.clone())
            .collect();
        let source = crate::generate_rt(&declarations, &requested).unwrap();
        syn::parse_file(&source).unwrap();
        assert!(source.contains("rt_regex_compile"));
        assert!(source.contains("rt_string_from_utf8"));
    }
}
