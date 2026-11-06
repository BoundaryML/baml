//! Typed v2 adapters for the implemented phase-one builtin subset.
//! Keep interpreter glue unchanged. A declaration change must fail generation,
//! rather than silently changing a native call's layout.
use crate::{BamlType, BuiltinPipeline, NativeBuiltin, ReceiverType};

pub fn supports_v2(path: &str) -> bool {
    matches!(path, "baml.String.char_count" | "baml.String.slice")
}

/// Generate only requested adapters, checked against extracted stdlib declarations.
/// Unsupported requests are errors, never interpreter fallbacks.
pub fn generate_v2(
    declarations: &[NativeBuiltin],
    requested: &std::collections::BTreeSet<String>,
) -> Result<String, String> {
    let mut output = String::new();
    for path in requested {
        if !supports_v2(path) {
            return Err(format!("unsupported in runtime v2: builtin {path}"));
        }
        let declaration = declarations
            .iter()
            .find(|d| d.path == *path)
            .ok_or_else(|| format!("runtime v2 builtin declaration missing: {path}"))?;
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
                "unsupported in runtime v2: changed builtin signature {path}"
            ));
        }
        if count {
            output.push_str("#[inline(always)] fn v2_string_char_count(cx: &rt::strings::Cx, value: rt::strings::Str) -> i64 { rt::strings::length(cx,value) }\n");
        } else {
            // A string result allocates in v2 even when the heap-neutral source
            // declaration throws never. Allocation panics remain Result errors.
            output.push_str("#[inline(always)] fn v2_string_slice(cx: &mut rt::strings::Cx, value: rt::strings::Str, start:i64, end:i64) -> rt::R<rt::strings::Str> { rt::strings::slice(cx,value,start,end) }\n");
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_declarations_and_signature_drift() {
        let (mut declarations, _, _) = crate::extract_native_builtins().unwrap();
        let requested = ["baml.String.char_count".into(), "baml.String.slice".into()].into();
        let source = generate_v2(&declarations, &requested).unwrap();
        syn::parse_file(&source).unwrap();
        let slice = declarations
            .iter_mut()
            .find(|d| d.path == "baml.String.slice")
            .unwrap();
        slice.params[0].ty = BamlType::Float;
        assert!(
            generate_v2(&declarations, &requested)
                .unwrap_err()
                .contains("changed builtin signature")
        );
        assert!(
            generate_v2(&declarations, &["baml.String.trim".into()].into())
                .unwrap_err()
                .contains("unsupported in runtime v2")
        );
    }
}
