//! Typed rt adapters for the implemented builtin subset.
//! Keep interpreter glue unchanged. A declaration change must fail generation,
//! rather than silently changing a native call's layout.
use crate::{BamlType, BuiltinPipeline, NativeBuiltin, ReceiverType};

pub fn supports_rt(path: &str) -> bool {
    crate::supports_rt_calendar(path)
        || crate::codegen_rt_provider::supports(path)
        || crate::codegen_rt_random::supports(path)
        || crate::codegen_rt_clock::supports(path)
        || crate::codegen_rt_bytes::supports(path)
        || crate::codegen_rt_bigint::supports(path)
        || crate::codegen_rt_io::supports(path)
        || crate::codegen_rt_scalar::supports(path)
        || matches!(
            path,
            "baml._to_string_primitive"
                | "baml._to_string_default"
                | "baml.String.to_code_points"
                | "baml.Map.set"
                | "baml.Map.values"
                | "baml.String.char_count"
                | "baml.String.slice"
                | "baml.Array.push"
                | "baml.Array.join"
                | "baml.Array.length"
        )
}

/// Generate only requested adapters, checked against extracted stdlib declarations.
/// Unsupported requests are errors, never interpreter fallbacks.
pub fn generate_rt(
    declarations: &[NativeBuiltin],
    requested: &std::collections::BTreeSet<String>,
) -> Result<String, String> {
    let mut output = String::new();
    for path in requested {
        if !supports_rt(path) {
            return Err(format!("unsupported in baml_rt: builtin {path}"));
        }
        let declaration = declarations
            .iter()
            .find(|d| d.path == *path)
            .ok_or_else(|| format!("baml_rt builtin declaration missing: {path}"))?;
        if crate::supports_rt_calendar(path) {
            crate::codegen_rt_calendar::validate(declaration)?;
            continue;
        }
        if crate::codegen_rt_provider::supports(path) {
            crate::codegen_rt_provider::validate(declaration)?;
            continue;
        }
        if crate::codegen_rt_random::supports(path) {
            output.push_str(&crate::codegen_rt_random::generate(declaration)?);
            continue;
        }
        if crate::codegen_rt_clock::supports(path) {
            output.push_str(&crate::codegen_rt_clock::generate(declaration)?);
            continue;
        }
        if crate::codegen_rt_bytes::supports(path) {
            output.push_str(&crate::codegen_rt_bytes::generate(declaration)?);
            continue;
        }
        if crate::codegen_rt_bigint::supports(path) {
            output.push_str(&crate::codegen_rt_bigint::generate(declaration)?);
            continue;
        }
        if path == "baml._to_string_primitive" {
            if declaration.pipeline != BuiltinPipeline::Vm
                || declaration.params.len() != 1
                || declaration.return_type != BamlType::String
                || !declaration.generics.is_empty()
                || declaration.may_yield
            {
                return Err(format!(
                    "unsupported in baml_rt: changed builtin signature {path}"
                ));
            }
            continue;
        }
        if crate::codegen_rt_io::supports(path) {
            output.push_str(&crate::codegen_rt_io::generate(declaration)?);
            continue;
        }
        if crate::codegen_rt_scalar::supports(path) {
            output.push_str(&crate::codegen_rt_scalar::generate(declaration)?);
            continue;
        }
        if path == "baml._to_string_default" || path == "baml.String.to_code_points" {
            let integer = path == "baml._to_string_default";
            let params = if integer {
                vec![BamlType::Generic("T".into())]
            } else {
                vec![]
            };
            let ret = if integer {
                BamlType::String
            } else {
                BamlType::List(Box::new(BamlType::Int))
            };
            if declaration.pipeline != BuiltinPipeline::Vm
                || declaration.params.iter().map(|p| &p.ty).ne(params.iter())
                || declaration.return_type != ret
                || declaration.generics != if integer { vec!["T"] } else { vec![] }
                || declaration.may_yield != integer
                || declaration.fallible
                || declaration.throws.as_ref().is_none_or(|t| !t.is_empty())
                || (integer && declaration.receiver.is_some())
                || (!integer
                    && declaration.receiver.as_ref().is_none_or(|r| {
                        r.builtin_name() != "String" || r.receiver_type != ReceiverType::RefSelf
                    }))
            {
                return Err(format!(
                    "unsupported in baml_rt: changed builtin signature {path}"
                ));
            }
            output.push_str(if integer {
                "#[inline(always)] fn rt_int_to_string(cx:&mut rt::strings::Cx,value:i64)->rt::R<rt::strings::Str>{rt::strings::integer_to_string(cx,value)}\n"
            }else{
                "#[inline(always)] unsafe fn rt_code_points(cx:&mut rt::strings::Cx,value:rt::strings::Str,desc:u32)->rt::R<usize>{unsafe{rt::strings::to_code_points(cx,value,desc)}}\n"
            });
            continue;
        }
        if path.starts_with("baml.Map.") {
            let set = path == "baml.Map.set";
            let params = if set {
                vec![BamlType::Generic("K".into()), BamlType::Generic("V".into())]
            } else {
                vec![]
            };
            let ret = if set {
                BamlType::Optional(Box::new(BamlType::Generic("V".into())))
            } else {
                BamlType::List(Box::new(BamlType::Generic("V".into())))
            };
            if declaration.pipeline != BuiltinPipeline::Vm
                || declaration.generics != ["K", "V"]
                || declaration.params.iter().map(|p| &p.ty).ne(params.iter())
                || declaration.return_type != ret
                || declaration.may_yield != set
                || declaration.fallible
                || declaration.throws.as_ref().is_none_or(|t| !t.is_empty())
                || declaration.receiver.as_ref().is_none_or(|r| {
                    r.builtin_name() != "Map"
                        || r.receiver_type
                            != if set {
                                ReceiverType::MutSelf
                            } else {
                                ReceiverType::RefSelf
                            }
                })
            {
                return Err(format!(
                    "unsupported in baml_rt: changed builtin signature {path}"
                ));
            }
            output.push_str(if set {
                "#[inline(always)] fn rt_map_set(cx:&mut rt::strings::Cx,map:rt::maps::StringIntMap,key:rt::strings::Str,value:i64)->rt::R<rt::Dyn>{map.set(cx,key,value)}\n"
            } else {
                "#[inline(always)] unsafe fn rt_map_values(cx:&mut rt::strings::Cx,map:rt::maps::StringIntMap,desc:u32)->rt::R<usize>{unsafe{map.values(cx,desc)}}\n"
            });
            continue;
        }
        if path.starts_with("baml.Array.") {
            let (params, ret, receiver, yielding, body) = match path.as_str() {
                "baml.Array.push" => (
                    vec![BamlType::Generic("T".into())],
                    BamlType::Int,
                    ReceiverType::MutSelf,
                    false,
                    "#[inline(always)] fn rt_array_push<T:rt::lists::Slot>(cx:&mut rt::strings::Cx,list:rt::lists::List<T>,value:T)->rt::R<i64>{list.push(cx,value)}\n",
                ),
                "baml.Array.length" => (
                    vec![],
                    BamlType::Int,
                    ReceiverType::RefSelf,
                    false,
                    "#[inline(always)] fn rt_array_length<T:rt::lists::Slot>(cx:&rt::strings::Cx,list:rt::lists::List<T>)->i64{list.len(cx)}\n",
                ),
                "baml.Array.join" => (
                    vec![BamlType::String],
                    BamlType::String,
                    ReceiverType::RefSelf,
                    true,
                    "#[inline(always)] fn rt_array_join(cx:&mut rt::strings::Cx,list:rt::lists::List<rt::strings::Str>,separator:rt::strings::Str)->rt::R<rt::strings::Str>{rt::lists::join(cx,list,separator)}\n",
                ),
                _ => unreachable!(),
            };
            if declaration.pipeline != BuiltinPipeline::Vm
                || declaration.generics != ["T"]
                || declaration.return_type != ret
                || declaration.params.iter().map(|p| &p.ty).ne(params.iter())
                || declaration.may_yield != yielding
                || declaration.fallible
                || declaration.throws.as_ref().is_none_or(|t| !t.is_empty())
                || declaration
                    .receiver
                    .as_ref()
                    .is_none_or(|r| r.builtin_name() != "Array" || r.receiver_type != receiver)
            {
                return Err(format!(
                    "unsupported in baml_rt: changed builtin signature {path}"
                ));
            }
            output.push_str(body);
            continue;
        }
        let count = path == "baml.String.char_count";
        let expected_params = if count {
            vec![]
        } else {
            vec![BamlType::Int, BamlType::Int]
        };
        let expected_return = if count {
            BamlType::Int
        } else {
            BamlType::String
        };
        if declaration.pipeline != BuiltinPipeline::Vm
            || !declaration.generics.is_empty()
            || declaration.may_yield
            || declaration.fallible
            || declaration
                .throws
                .as_ref()
                .is_none_or(|throws| !throws.is_empty())
            || declaration.return_type != expected_return
            || declaration
                .params
                .iter()
                .map(|p| &p.ty)
                .ne(expected_params.iter())
            || declaration.receiver.as_ref().is_none_or(|r| {
                r.builtin_name() != "String" || r.receiver_type != ReceiverType::RefSelf
            })
        {
            return Err(format!(
                "unsupported in baml_rt: changed builtin signature {path}"
            ));
        }
        if count {
            output.push_str("#[inline(always)] fn rt_string_char_count(cx: &rt::strings::Cx, value: rt::strings::Str) -> i64 { rt::strings::length(cx,value) }\n");
        } else {
            // A string result allocates in rt even when the heap-neutral source
            // declaration throws never. Allocation panics remain Result errors.
            output.push_str("#[inline(always)] fn rt_string_slice(cx: &mut rt::strings::Cx, value: rt::strings::Str, start:i64, end:i64) -> rt::R<rt::strings::Str> { rt::strings::slice(cx,value,start,end) }\n");
        }
    }
    let providers: std::collections::BTreeSet<String> = requested
        .iter()
        .filter(|p| crate::codegen_rt_provider::supports(p))
        .cloned()
        .collect();
    let io = declarations
        .iter()
        .filter(|d| d.pipeline == BuiltinPipeline::Io)
        .cloned()
        .collect::<Vec<_>>();
    if !providers.is_empty() {
        let (_, _, classes) = crate::extract_native_builtins().map_err(|e| e.to_string())?;
        output.push_str(&crate::codegen_io::generate_rt_provider(
            &io, &classes, &providers,
        )?);
    }
    // Native adapters are hidden. They have no observation state or atomic
    // loads on the owner thread; observed BAML wrappers own their telemetry.
    output.push_str("fn init_builtin_observation(){}\n");
    let syntax = syn::parse_file(&output).map_err(|e| e.to_string())?;
    Ok(prettyplease::unparse(&syntax))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_declarations_and_signature_drift() {
        let (mut declarations, _, _) = crate::extract_native_builtins().unwrap();
        let requested = [
            "baml.String.char_count".into(),
            "baml.String.slice".into(),
            "baml.Array.push".into(),
            "baml.Array.join".into(),
            "baml.Array.length".into(),
            "baml.Map.set".into(),
            "baml._to_string_default".into(),
            "baml.String.to_code_points".into(),
            "baml.Map.values".into(),
        ]
        .into();
        let source = generate_rt(&declarations, &requested).unwrap();
        syn::parse_file(&source).unwrap();
        let slice = declarations
            .iter_mut()
            .find(|d| d.path == "baml.String.slice")
            .unwrap();
        slice.params[0].ty = BamlType::Float;
        assert!(
            generate_rt(&declarations, &requested)
                .unwrap_err()
                .contains("changed builtin signature")
        );
        assert!(
            generate_rt(&declarations, &["baml.String.trim".into()].into())
                .unwrap_err()
                .contains("unsupported in baml_rt")
        );
    }
}
