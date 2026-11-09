//! Checked typed adapters for the UTF-8 string boundary.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};
pub(super) fn supports(path: &str) -> bool {
    matches!(
        path,
        "baml.String.to_utf8"
            | "baml.String.from_utf8"
            | "baml.Uint8Array._to_string_impl"
            | "baml.Uint8Array.from_array"
            | "baml.Uint8Array.length"
    )
}
pub(super) fn generate(d: &NativeBuiltin) -> Result<String, String> {
    if matches!(
        d.path.as_str(),
        "baml.Uint8Array.from_array" | "baml.Uint8Array.length"
    ) {
        let from = d.path.ends_with("from_array");
        let params = if from {
            vec![T::List(Box::new(T::Int))]
        } else {
            vec![]
        };
        if d.pipeline != BuiltinPipeline::Vm
            || !d.generics.is_empty()
            || d.params.iter().map(|p| &p.ty).ne(params.iter())
            || d.return_type != if from { T::Uint8Array } else { T::Int }
            || d.receiver.as_ref().is_none_or(|r| {
                r.receiver_type
                    != if from {
                        ReceiverType::Static
                    } else {
                        ReceiverType::RefSelf
                    }
            })
        {
            return Err(format!(
                "unsupported in baml_rt: changed builtin signature {}",
                d.path
            ));
        }
        return Ok(if from{"#[inline(always)] unsafe fn rt_bytes_from_array(cx:&mut rt::strings::Cx,value:usize,error_desc:u32)->rt::R<usize>{unsafe{rt::bytes::from_array(cx,value,error_desc)}}\n"}else{"#[inline(always)] unsafe fn rt_bytes_length(_cx:&mut rt::strings::Cx,value:usize)->rt::R<i64>{Ok(unsafe{rt::bytes::data(value)}.len() as i64)}\n"}.into());
    }
    let decode = d.path == "baml.String.from_utf8";
    let encode = d.path == "baml.String.to_utf8";
    let params = if decode { vec![T::Uint8Array] } else { vec![] };
    let ret = if encode { T::Uint8Array } else { T::String };
    let throws = if decode {
        vec!["InvalidArgument".to_owned()]
    } else {
        vec![]
    };
    if d.pipeline != BuiltinPipeline::Vm
        || !d.generics.is_empty()
        || d.may_yield
        || d.fallible
        || d.params.iter().map(|p| &p.ty).ne(params.iter())
        || d.return_type != ret
        || d.throws.as_deref() != Some(throws.as_slice())
        || d.receiver.as_ref().is_none_or(|r| {
            r.receiver_type
                != if decode {
                    ReceiverType::Static
                } else {
                    ReceiverType::RefSelf
                }
                || r.builtin_name()
                    != if encode || decode {
                        "String"
                    } else {
                        "Uint8Array"
                    }
        })
    {
        return Err(format!(
            "unsupported in baml_rt: changed builtin signature {}",
            d.path
        ));
    }
    Ok(if encode{"#[inline(always)] fn rt_string_to_utf8(cx:&mut rt::strings::Cx,value:rt::strings::Str)->rt::R<usize>{rt::bytes::encode(cx,value)}\n"}
    else if decode{"#[inline(always)] unsafe fn rt_string_from_utf8(cx:&mut rt::strings::Cx,value:usize,error_desc:u32)->rt::R<rt::strings::Str>{unsafe{rt::bytes::decode(cx,value,error_desc)}}\n"}
    else{"#[inline(always)] unsafe fn rt_bytes_to_string(cx:&mut rt::strings::Cx,value:usize)->rt::R<rt::strings::Str>{cx.string(&String::from_utf8_lossy(unsafe{rt::bytes::data(value)}))}\n"}.into())
}
