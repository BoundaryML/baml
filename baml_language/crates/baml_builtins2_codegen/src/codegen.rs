//! Code generation for `NativeFunctions` trait from extracted `NativeBuiltin` records.
//!
//! `generate_native_trait` takes the output of `extract_native_builtins()` and emits
//! a Rust source `String` containing the three-tier `NativeFunctions` trait:
//!
//! - **Tier 1**: Required methods with clean Rust types (the developer implements these).
//! - **Tier 2**: Default glue methods (`__baml_*`) that extract `&[Value]` args and
//!   convert results back to `Value`.
//! - **Tier 3**: `get_native_fn(path)` match that routes a path string to a glue fn.

use crate::types::{BamlType, NativeBuiltin, Receiver};

// ============================================================================
// Path aliasing — maps `.baml`-derived paths to the legacy paths that
// `baml_compiler_emit` puts into `Function` objects. Required until the
// compiler is updated to use `.baml`-derived paths directly.
//
// The `.baml` stdlib defines per-class media methods (e.g. `baml.media.Pdf.url`)
// but the legacy DSL used a single consolidated `Media` struct
// (`baml.Media.as_url`). We emit match arms for both so that existing
// bytecode compiled by `baml_compiler_emit` continues to resolve correctly.
// ============================================================================

/// Returns the legacy path alias for a `.baml`-derived path, if one exists.
///
/// Returns `None` if the path has no alias (most paths).
fn legacy_path_alias(path: &str) -> Option<&'static str> {
    match path {
        "baml.media.Pdf.url"
        | "baml.media.Audio.url"
        | "baml.media.Video.url"
        | "baml.media.Image.url" => Some("baml.Media.as_url"),
        "baml.media.Pdf.file"
        | "baml.media.Audio.file"
        | "baml.media.Video.file"
        | "baml.media.Image.file" => Some("baml.Media.as_file"),
        "baml.media.Pdf.base64"
        | "baml.media.Audio.base64"
        | "baml.media.Video.base64"
        | "baml.media.Image.base64" => Some("baml.Media.as_base64"),
        "baml.media.Pdf.mime_type"
        | "baml.media.Audio.mime_type"
        | "baml.media.Video.mime_type"
        | "baml.media.Image.mime_type" => Some("baml.Media.mime_type"),
        _ => None,
    }
}

// ============================================================================
// Fallibility — methods that return `Result<T, VmError>` in the clean
// signature rather than the plain `T` inferred from the `.baml` type.
//
// The `.baml` stdlib does not yet encode fallibility (it would need a
// dedicated `Result<T>` type or a `#[fallible]` attribute). Until then we
// maintain an explicit allowlist.
// ============================================================================

/// Returns `true` if the clean trait method for this path should return
/// `Result<T, VmError>` instead of plain `T`.
fn is_fallible(path: &str) -> bool {
    matches!(
        path,
        "baml.Array.at"
            | "baml.deep_copy"
            | "baml.unstable.string"
    )
}

// ============================================================================
// Public entry point
// ============================================================================

/// Generate a Rust source `String` containing the `NativeFunctions` trait.
///
/// The generated code is intended to be written to a file in `OUT_DIR` and
/// `include!`-ed into `bex_vm/src/native.rs` instead of the legacy
/// `baml_builtins::generate_native_trait!()` invocation.
///
/// # Assumptions
///
/// The caller (`native.rs`) is responsible for having the following in scope:
/// - `BexVm` type
/// - `Value`, `IndexMap`, `MediaValue` from `bex_vm_types`
/// - `NativeFunctionResult`, `NativeFunction` type aliases
/// - `VmError`, `InternalError`, `RuntimeError` from `crate::errors`
/// - `Type` from `bex_vm_types`
/// - `MediaKind` from `baml_type`
pub fn generate_native_trait(builtins: &[NativeBuiltin]) -> String {
    let mut out = String::new();

    out.push_str("/// Trait for implementing native BAML functions.\n");
    out.push_str("///\n");
    out.push_str("/// Implement the `baml_*` methods — they have clean Rust types.\n");
    out.push_str("/// The `__baml_*` glue methods and `get_native_fn` are auto-generated.\n");
    out.push_str("pub trait NativeFunctions {\n");

    out.push_str("    // ========== Required methods (implement these) ==========\n");
    for b in builtins {
        emit_required_method(&mut out, b);
    }
    out.push('\n');

    out.push_str("    // ========== Glue methods (default implementations) ==========\n");
    for b in builtins {
        emit_glue_method(&mut out, b);
    }
    out.push('\n');

    out.push_str("    // ========== Lookup method (default implementation) ==========\n");
    out.push_str("    fn get_native_fn(path: &str) -> Option<NativeFunction> {\n");
    out.push_str("        match path {\n");

    // Track which legacy aliases have already been emitted (to avoid duplicates
    // when multiple media classes share the same legacy path).
    let mut emitted_aliases: std::collections::HashSet<&'static str> =
        std::collections::HashSet::new();

    for b in builtins {
        let glue = format!("__baml_{}", escape_fn_name(&b.fn_name));
        out.push_str(&format!(
            "            {:?} => Some(Self::{}),\n",
            b.path, glue
        ));

        if let Some(alias) = legacy_path_alias(&b.path) {
            if emitted_aliases.insert(alias) {
                // Only emit the first media class's glue fn for the alias
                // (Pdf is the canonical one; all four use equivalent implementations).
                // We pick the first encountered per alias.
                out.push_str(&format!(
                    "            {:?} => Some(Self::{}),\n",
                    alias, glue
                ));
            }
        }
    }

    out.push_str("            _ => None,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    out
}

// ============================================================================
// Tier 1 — Required method signatures
// ============================================================================

fn emit_required_method(out: &mut String, b: &NativeBuiltin) {
    let return_type = clean_return_type(b);
    let params = clean_param_list(b);

    let has_mut_receiver = b.receiver.as_ref().is_some_and(|r| r.is_mut);
    if has_mut_receiver {
        // Mutable receiver methods cannot also receive `vm` — the mutable borrow of
        // the receiver (e.g. `&mut Vec<Value>`) already ties up `vm`, so passing
        // `vm` a second time would violate borrow rules at the call site.
        out.push_str(&format!(
            "    fn baml_{}({}) -> {};\n",
            escape_fn_name(&b.fn_name),
            params,
            return_type
        ));
    } else {
        out.push_str(&format!(
            "    fn baml_{}(vm: &mut BexVm, {}) -> {};\n",
            escape_fn_name(&b.fn_name),
            params,
            return_type
        ));
    }
}

/// Build the clean parameter list (after the `vm` param).
///
/// Receiver always comes first in the clean signature (for API clarity).
/// The *extraction* order in the glue method is different for mutable receivers
/// (params extracted first to avoid borrow conflicts), but the signature itself
/// always has receiver → params.
fn clean_param_list(b: &NativeBuiltin) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(recv) = &b.receiver {
        // Receiver always first in the clean signature.
        parts.push(format!(
            "{}: {}",
            receiver_param_name(recv),
            receiver_input_type(recv)
        ));
        for p in &b.params {
            parts.push(format!("{}: {}", p.name, baml_type_to_input(&p.ty, false)));
        }
    } else {
        // Free function: just params.
        for p in &b.params {
            parts.push(format!("{}: {}", p.name, baml_type_to_input(&p.ty, false)));
        }
    }

    parts.join(", ")
}

/// Clean return type for a trait method.
fn clean_return_type(b: &NativeBuiltin) -> String {
    let inner = baml_type_to_output(&b.return_type);
    if is_fallible(&b.path) {
        format!("Result<{inner}, VmError>")
    } else {
        inner
    }
}

// ============================================================================
// Tier 2 — Glue methods
// ============================================================================

fn emit_glue_method(out: &mut String, b: &NativeBuiltin) {
    let fn_name = escape_fn_name(&b.fn_name);
    let glue_name = format!("__baml_{fn_name}");
    let fallible = is_fallible(&b.path);

    out.push_str(&format!(
        "    fn {glue_name}(vm: &mut BexVm, args: &[Value]) -> NativeFunctionResult {{\n"
    ));

    // Emit arg extraction.
    emit_arg_extractions(out, b);

    // Emit the call.
    let call_args = call_arg_list(b);
    let has_mut_receiver = b.receiver.as_ref().is_some_and(|r| r.is_mut);
    let returns_null = matches!(b.return_type, BamlType::Null);

    // For void/null returns, don't bind to `result` — avoids unused variable warning.
    let binding = if returns_null { "        " } else { "        let result = " };
    let suffix = if fallible { "?;\n" } else { ";\n" };

    if has_mut_receiver {
        out.push_str(&format!(
            "{binding}Self::baml_{fn_name}({call_args}){suffix}"
        ));
    } else {
        out.push_str(&format!(
            "{binding}Self::baml_{fn_name}(vm, {call_args}){suffix}"
        ));
    }

    // Emit result conversion.
    emit_result_conversion(out, &b.return_type);

    out.push_str("    }\n");
}

/// Emit `let var = ...;` extraction statements for all method arguments.
fn emit_arg_extractions(out: &mut String, b: &NativeBuiltin) {
    if let Some(recv) = &b.receiver {
        if recv.is_mut {
            // Mutable receiver: extract non-receiver params first (cloning), then
            // take the mutable borrow of the receiver last to avoid borrow conflicts.
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1; // args[0] is the receiver
                emit_single_extraction(out, &p.name, arg_idx, &p.ty);
            }
            // Receiver last (mutable borrow of vm).
            let recv_name = receiver_param_name(recv);
            emit_mut_receiver_extraction(out, &recv_name, recv);
        } else {
            // Immutable receiver first.
            let recv_name = receiver_param_name(recv);
            emit_immut_receiver_extraction(out, &recv_name, 0, recv);
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1;
                emit_single_extraction(out, &p.name, arg_idx, &p.ty);
            }
        }
    } else {
        // Free function.
        for (i, p) in b.params.iter().enumerate() {
            emit_single_extraction(out, &p.name, i, &p.ty);
        }
    }
}

/// Emit a single `let {name} = {extraction_expr};`.
fn emit_single_extraction(out: &mut String, name: &str, idx: usize, ty: &BamlType) {
    let rhs = extraction_expr(&format!("&args[{idx}]"), ty, false);
    out.push_str(&format!("        let {name} = {rhs};\n"));
}

/// Emit extraction for an immutable receiver (args[0]).
fn emit_immut_receiver_extraction(
    out: &mut String,
    name: &str,
    idx: usize,
    recv: &Receiver,
) {
    let rhs = receiver_immut_extraction_expr(&format!("&args[{idx}]"), recv);
    out.push_str(&format!("        let {name} = {rhs};\n"));
}

/// Emit extraction for a mutable receiver (`as_array_mut`, `as_map_mut`).
fn emit_mut_receiver_extraction(out: &mut String, name: &str, recv: &Receiver) {
    let expr = match recv.class_name.as_str() {
        "Array" => "vm.as_array_mut(&args[0])?".to_string(),
        "Map" => "vm.as_map_mut(&args[0])?".to_string(),
        "String" => "vm.as_string_mut(&args[0])?".to_string(),
        _ => "vm.as_value_mut(&args[0])?".to_string(),
    };
    out.push_str(&format!("        let {name} = {expr};\n"));
}

// ============================================================================
// Argument extraction expressions
// ============================================================================

/// Build the extraction expression for an immutable receiver.
fn receiver_immut_extraction_expr(val: &str, recv: &Receiver) -> String {
    match recv.class_name.as_str() {
        "Array" => format!("vm.as_array({val})?.to_vec()"),
        "Map" => format!("vm.as_map({val})?.clone()"),
        "String" => format!("vm.as_string({val})?.clone()"),
        "Pdf" | "Audio" | "Video" | "Image" => {
            // Determine MediaKind from class name.
            let kind = media_kind_expr(&recv.class_name);
            format!("vm.as_media({val}, {kind})?.clone()")
        }
        _ => format!("{val}.clone()"),
    }
}

/// Build the extraction expression for a parameter value.
fn extraction_expr(val: &str, ty: &BamlType, is_mut: bool) -> String {
    match ty {
        BamlType::String => {
            if is_mut {
                format!("vm.as_string_mut({val})?")
            } else {
                format!("vm.as_string({val})?.clone()")
            }
        }
        BamlType::Int => format!(
            "match {val} {{ Value::Int(i) => *i, other => return Err(InternalError::TypeError {{ expected: Type::Int, got: vm.type_of(other) }}.into()) }}"
        ),
        BamlType::Float => format!(
            "match {val} {{ Value::Float(f) => *f, other => return Err(InternalError::TypeError {{ expected: Type::Float, got: vm.type_of(other) }}.into()) }}"
        ),
        BamlType::Bool => format!(
            "match {val} {{ Value::Bool(b) => *b, other => return Err(InternalError::TypeError {{ expected: Type::Bool, got: vm.type_of(other) }}.into()) }}"
        ),
        BamlType::List(_) => {
            if is_mut {
                format!("vm.as_array_mut({val})?")
            } else {
                format!("vm.as_array({val})?.to_vec()")
            }
        }
        BamlType::Map(_, _) => {
            if is_mut {
                format!("vm.as_map_mut({val})?")
            } else {
                format!("vm.as_map({val})?.clone()")
            }
        }
        BamlType::Optional(inner) => {
            let inner_expr = extraction_expr("other", inner, false);
            format!("match {val} {{ Value::Null => None, other => Some({inner_expr}) }}")
        }
        BamlType::Generic(_) => {
            // Pass through as &Value — will be referenced by the call arg.
            format!("{val}")
        }
        BamlType::Media(name) => {
            let kind = media_kind_expr(name);
            format!("vm.as_media({val}, {kind})?.clone()")
        }
        BamlType::Named(_) | BamlType::Null => {
            format!("{val}")
        }
    }
}

/// Build the call-arg list (args passed to the clean `baml_*` method, after `vm`).
///
/// The call order matches the clean signature: receiver first, then params.
/// (The extraction order in the glue body is different for mutable receivers,
/// but the call site always uses receiver-first order.)
fn call_arg_list(b: &NativeBuiltin) -> String {
    let mut args: Vec<String> = Vec::new();

    if let Some(recv) = &b.receiver {
        let name = receiver_param_name(recv);
        // Receiver always first in the call.
        if recv.is_mut {
            args.push(name); // already a &mut ref from extraction
        } else {
            args.push(call_arg_for_type(&name, &receiver_baml_type(recv)));
        }
        for p in &b.params {
            args.push(call_arg_for_type(&p.name, &p.ty));
        }
    } else {
        for p in &b.params {
            args.push(call_arg_for_type(&p.name, &p.ty));
        }
    }

    args.join(", ")
}

/// Determine the right way to pass an extracted variable to the clean method.
fn call_arg_for_type(name: &str, ty: &BamlType) -> String {
    match ty {
        // These were extracted as owned values; pass by reference.
        BamlType::String => format!("&{name}"),
        BamlType::List(_) => format!("&{name}"),
        BamlType::Map(_, _) => format!("&{name}"),
        BamlType::Media(_) => format!("&{name}"),
        // Optional<String> extracted as Option<String>; pass as Option<&str>.
        BamlType::Optional(inner) => match inner.as_ref() {
            BamlType::String => format!("{name}.as_deref()"),
            BamlType::List(_) => format!("{name}.as_deref()"),
            _ => {
                // Check if inner needs a reference.
                if call_arg_needs_ref(inner) {
                    format!("{name}.as_ref()")
                } else {
                    name.to_string()
                }
            }
        },
        // Scalars and generics are passed by value.
        BamlType::Int | BamlType::Float | BamlType::Bool | BamlType::Null => name.to_string(),
        // Generic extracted as &Value — already a reference.
        BamlType::Generic(_) => name.to_string(),
        BamlType::Named(_) => name.to_string(),
    }
}

fn call_arg_needs_ref(ty: &BamlType) -> bool {
    matches!(ty, BamlType::String | BamlType::List(_) | BamlType::Map(_, _) | BamlType::Media(_))
}

// ============================================================================
// Tier 2 — Result conversion
// ============================================================================

fn emit_result_conversion(out: &mut String, ty: &BamlType) {
    let conversion = result_conversion_expr("result", ty);
    out.push_str(&format!("        Ok({conversion})\n"));
}

fn result_conversion_expr(name: &str, ty: &BamlType) -> String {
    match ty {
        BamlType::String => format!("vm.alloc_string({name})"),
        BamlType::Int => format!("Value::Int({name})"),
        BamlType::Float => format!("Value::Float({name})"),
        BamlType::Bool => format!("Value::Bool({name})"),
        BamlType::Null => "Value::Null".to_string(),
        BamlType::List(_) => format!("vm.alloc_array({name})"),
        BamlType::Map(_, _) => format!("vm.alloc_map({name})"),
        BamlType::Optional(inner) => {
            let inner_conversion = result_conversion_expr("v", inner);
            format!("match {name} {{ Some(v) => {inner_conversion}, None => Value::Null }}")
        }
        BamlType::Generic(_) => name.to_string(),
        BamlType::Named(_) => name.to_string(),
        BamlType::Media(_) => name.to_string(),
    }
}

// ============================================================================
// Type mapping helpers
// ============================================================================

/// Map a `BamlType` to a Rust input type (used in trait method signatures).
fn baml_type_to_input(ty: &BamlType, is_mut: bool) -> String {
    match ty {
        BamlType::String => {
            if is_mut {
                "&mut String".to_string()
            } else {
                "&str".to_string()
            }
        }
        BamlType::Int => "i64".to_string(),
        BamlType::Float => "f64".to_string(),
        BamlType::Bool => "bool".to_string(),
        BamlType::Null => "()".to_string(),
        BamlType::List(_) => {
            if is_mut {
                "&mut Vec<Value>".to_string()
            } else {
                "&[Value]".to_string()
            }
        }
        BamlType::Map(_, _) => {
            if is_mut {
                "&mut IndexMap<String, Value>".to_string()
            } else {
                "&IndexMap<String, Value>".to_string()
            }
        }
        BamlType::Optional(inner) => {
            let inner_str = baml_type_to_input(inner, false);
            format!("Option<{inner_str}>")
        }
        BamlType::Generic(_) => "&Value".to_string(),
        BamlType::Named(_) => "&Value".to_string(),
        BamlType::Media(_) => {
            if is_mut {
                "&mut MediaValue".to_string()
            } else {
                "&MediaValue".to_string()
            }
        }
    }
}

/// Map a `BamlType` to a Rust output type (used in trait method return types).
fn baml_type_to_output(ty: &BamlType) -> String {
    match ty {
        BamlType::String => "String".to_string(),
        BamlType::Int => "i64".to_string(),
        BamlType::Float => "f64".to_string(),
        BamlType::Bool => "bool".to_string(),
        BamlType::Null => "()".to_string(),
        BamlType::List(_) => "Vec<Value>".to_string(),
        BamlType::Map(_, _) => "IndexMap<String, Value>".to_string(),
        BamlType::Optional(inner) => {
            let inner_str = baml_type_to_output(inner);
            format!("Option<{inner_str}>")
        }
        BamlType::Generic(_) => "Value".to_string(),
        BamlType::Named(_) => "Value".to_string(),
        BamlType::Media(_) => "Value".to_string(),
    }
}

// ============================================================================
// Receiver helpers
// ============================================================================

/// The parameter name for a receiver (snake_case of the class name).
fn receiver_param_name(recv: &Receiver) -> String {
    recv.class_name.to_lowercase()
}

/// The Rust input type for a receiver in the clean signature.
fn receiver_input_type(recv: &Receiver) -> String {
    match recv.class_name.as_str() {
        "Array" => {
            if recv.is_mut {
                "&mut Vec<Value>".to_string()
            } else {
                "&[Value]".to_string()
            }
        }
        "Map" => {
            if recv.is_mut {
                "&mut IndexMap<String, Value>".to_string()
            } else {
                "&IndexMap<String, Value>".to_string()
            }
        }
        "String" => {
            if recv.is_mut {
                "&mut String".to_string()
            } else {
                "&str".to_string()
            }
        }
        "Pdf" | "Audio" | "Video" | "Image" => "&MediaValue".to_string(),
        _ => "&Value".to_string(),
    }
}

/// Return a synthetic `BamlType` representing the receiver type (for call-arg generation).
fn receiver_baml_type(recv: &Receiver) -> BamlType {
    match recv.class_name.as_str() {
        "Array" => BamlType::List(Box::new(BamlType::Generic("T".to_string()))),
        "Map" => BamlType::Map(
            Box::new(BamlType::Generic("K".to_string())),
            Box::new(BamlType::Generic("V".to_string())),
        ),
        "String" => BamlType::String,
        "Pdf" | "Audio" | "Video" | "Image" => {
            BamlType::Media(recv.class_name.clone())
        }
        _ => BamlType::Named(recv.class_name.clone()),
    }
}

/// The `MediaKind::*` expression for a given media class name.
fn media_kind_expr(class_name: &str) -> String {
    match class_name {
        "Pdf" => "MediaKind::Pdf".to_string(),
        "Audio" => "MediaKind::Audio".to_string(),
        "Video" => "MediaKind::Video".to_string(),
        "Image" => "MediaKind::Image".to_string(),
        _ => "MediaKind::Generic".to_string(),
    }
}

/// Strip the `baml_` prefix from a fn_name produced by `path_to_fn_name` so
/// it matches the suffix used in the trait method names.
///
/// `extract.rs` produces names like `"baml_array_length"`. The trait emits
/// `fn baml_array_length(...)`, so we just use the full name as-is — but we
/// need to avoid double-prefixing `baml_`.
///
/// This helper is a no-op; it exists to make the intent explicit.
fn escape_fn_name(fn_name: &str) -> &str {
    // fn_name from extract.rs is already `baml_<rest>`.
    // We strip the `baml_` prefix because the caller writes `fn baml_{name}`.
    fn_name
        .strip_prefix("baml_")
        .unwrap_or(fn_name)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_native_builtins;

    #[test]
    fn test_generate_native_trait() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        assert!(output.contains("pub trait NativeFunctions"));
        assert!(
            output.contains("fn baml_array_length("),
            "missing baml_array_length in output:\n{output}"
        );
        assert!(
            output.contains("fn __baml_array_length("),
            "missing __baml_array_length in output:\n{output}"
        );
        assert!(
            output.contains("fn get_native_fn("),
            "missing get_native_fn in output:\n{output}"
        );
        assert!(
            output.contains("\"baml.Array.length\""),
            "missing path baml.Array.length in output:\n{output}"
        );
    }

    #[test]
    fn test_generate_all_required_methods_present() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        // Every extracted builtin should appear in the output.
        for b in &builtins {
            let name = escape_fn_name(&b.fn_name);
            assert!(
                output.contains(&format!("fn baml_{name}(")),
                "missing method baml_{name} (path={}) in generated trait",
                b.path
            );
            assert!(
                output.contains(&format!("fn __baml_{name}(")),
                "missing glue method __baml_{name} (path={}) in generated trait",
                b.path
            );
            assert!(
                output.contains(&format!("{:?} => Some(Self::__baml_{name})", b.path)),
                "missing path {:?} in get_native_fn match (method __baml_{name})",
                b.path
            );
        }
    }

    #[test]
    fn test_legacy_path_aliases_present() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        // Legacy media paths must be present so existing bytecode resolves.
        let expected_aliases = &[
            "baml.Media.as_url",
            "baml.Media.as_file",
            "baml.Media.as_base64",
            "baml.Media.mime_type",
        ];
        for alias in expected_aliases {
            assert!(
                output.contains(&format!("{alias:?} => Some")),
                "legacy alias {alias:?} missing from get_native_fn match"
            );
        }
    }

    #[test]
    fn test_array_push_mut_receiver() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        // Array.push has a mutable receiver — signature should use &mut Vec<Value>.
        // Mutable receiver methods do NOT have vm as first param (borrow conflict).
        assert!(
            output.contains("fn baml_array_push(array: &mut Vec<Value>, item: &Value)"),
            "Array.push should have &mut Vec<Value> receiver (no vm param) first:\n{output}"
        );
        // Must NOT have vm as first param.
        assert!(
            !output.contains("fn baml_array_push(vm: &mut BexVm,"),
            "Array.push should NOT have vm: &mut BexVm as first param:\n{output}"
        );
    }

    #[test]
    fn test_fallible_methods_return_result() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        // Array.at and deep_copy should return Result<...>.
        assert!(
            output.contains("fn baml_array_at(") && output.contains("Result<"),
            "Array.at should return Result<...>"
        );
    }

    #[test]
    fn test_vm_param_always_present() {
        let builtins = extract_native_builtins();
        let output = generate_native_trait(&builtins);

        // Every required method without a mutable receiver must have `vm: &mut BexVm` as first param.
        // Mutable receiver methods do NOT get vm (borrow conflict at call site).
        for b in &builtins {
            let name = escape_fn_name(&b.fn_name);
            let has_mut_receiver = b.receiver.as_ref().is_some_and(|r| r.is_mut);
            if has_mut_receiver {
                // Mutable receiver methods must NOT have vm as first param.
                assert!(
                    !output.contains(&format!("fn baml_{name}(vm: &mut BexVm,")),
                    "method baml_{name} should NOT have vm: &mut BexVm (mutable receiver)"
                );
            } else {
                assert!(
                    output.contains(&format!("fn baml_{name}(vm: &mut BexVm,")),
                    "method baml_{name} should have vm: &mut BexVm as first param"
                );
            }
        }
    }
}
