//! Checked typed adapters for the UTF-8 string boundary.
use crate::{BamlType as T, BuiltinPipeline, NativeBuiltin, ReceiverType};
pub(super) fn supports(path: &str) -> bool {
    matches!(path, "baml.String.to_utf8" | "baml.String.from_utf8")
}
pub(super) fn generate(d: &NativeBuiltin) -> Result<String, String> {
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
