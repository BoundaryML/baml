//! Code generation for IO (`$rust_io_function`) builtins.
//!
//! Generates:
//! - `SysOp` enum with `path()`, `allowed_error_categories()`,
//!   `allowed_panic_categories()`, `Display`, and `sys_op_for_path()`
//! - View structs (`view::{ns}::{ClassName}<'a>`) wrapping `BexClass<'a>`
//! - Owned structs (`owned::{ns}::{ClassName}`) with `AsBexExternalValue`
//! - Class traits (`IoClass{Ns}{Class}`) with clean methods, glue, dispatch
//! - Namespace traits (`IoNamespace{Ns}`) composing class traits + free functions
//! - Root trait (`IoPackageBaml`) composing all namespace traits
//! - `SysOps` struct with `get()`, `unsupported()`, `all_unsupported()`, `from_impl()`

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use crate::types::{BamlType, NativeBuiltin, NativeClassDef};

// ============================================================================
// Namespace tree for IO builtins
// ============================================================================

struct IoNamespaceNode<'a> {
    free_fns: Vec<&'a NativeBuiltin>,
    classes: BTreeMap<String, Vec<&'a NativeBuiltin>>,
}

impl IoNamespaceNode<'_> {
    fn new() -> Self {
        Self {
            free_fns: Vec::new(),
            classes: BTreeMap::new(),
        }
    }
}

/// Extract the namespace name from an IO builtin path.
///
/// All IO builtins start with "baml." and have a namespace as the second segment:
/// - "baml.fs.open" → "fs"
/// - "baml.fs.File.read" → "fs"
/// - `"baml.llm.get_client"` → `"llm"`
fn io_namespace_name(builtin: &NativeBuiltin) -> &str {
    let after_baml = builtin.path.strip_prefix("baml.").unwrap_or(&builtin.path);
    after_baml.split('.').next().unwrap_or("")
}

/// Extract the method name (last segment) from an IO builtin path.
fn io_method_name(builtin: &NativeBuiltin) -> &str {
    builtin.path.rsplit('.').next().unwrap_or("")
}

fn build_io_namespace_tree<'a>(
    io_builtins: &'a [NativeBuiltin],
) -> BTreeMap<String, IoNamespaceNode<'a>> {
    let mut tree: BTreeMap<String, IoNamespaceNode<'a>> = BTreeMap::new();

    for builtin in io_builtins {
        let ns = io_namespace_name(builtin).to_string();
        let node = tree.entry(ns).or_insert_with(IoNamespaceNode::new);

        if let Some(ref receiver) = builtin.receiver {
            node.classes
                .entry(receiver.class_name.clone())
                .or_default()
                .push(builtin);
        } else {
            node.free_fns.push(builtin);
        }
    }

    tree
}

// ============================================================================
// IO class defs filtering
// ============================================================================

/// Collect the set of namespace prefixes that contain IO builtins.
fn io_namespace_prefixes(io_builtins: &[NativeBuiltin]) -> BTreeSet<String> {
    io_builtins
        .iter()
        .map(|b| {
            if b.receiver.is_some() {
                // Class method: strip ".ClassName.method" → namespace prefix
                let last = b.path.rfind('.').unwrap();
                let before = &b.path[..last];
                let second_last = before.rfind('.').unwrap();
                b.path[..second_last].to_string()
            } else {
                // Free function: strip ".function" → namespace prefix
                let last = b.path.rfind('.').unwrap();
                b.path[..last].to_string()
            }
        })
        .collect()
}

/// Filter class defs to those in IO-relevant namespaces.
fn filter_io_class_defs<'a>(
    io_builtins: &[NativeBuiltin],
    class_defs: &'a [NativeClassDef],
) -> Vec<&'a NativeClassDef> {
    let ns_prefixes = io_namespace_prefixes(io_builtins);
    class_defs
        .iter()
        .filter(|cd| ns_prefixes.contains(&cd.namespace_prefix))
        .collect()
}

/// Build a map from class name → namespace segment (e.g., "File" → "fs").
fn build_class_ns_map(io_class_defs: &[&NativeClassDef]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for cd in io_class_defs {
        let ns = cd
            .namespace_prefix
            .strip_prefix("baml.")
            .unwrap_or(&cd.namespace_prefix);
        map.insert(cd.name.clone(), ns.to_string());
    }
    map
}

/// Group IO class defs by namespace segment.
fn group_class_defs_by_ns<'a>(
    io_class_defs: &[&'a NativeClassDef],
) -> BTreeMap<String, Vec<&'a NativeClassDef>> {
    let mut map: BTreeMap<String, Vec<&NativeClassDef>> = BTreeMap::new();
    for cd in io_class_defs {
        let ns = cd
            .namespace_prefix
            .strip_prefix("baml.")
            .unwrap_or(&cd.namespace_prefix);
        map.entry(ns.to_string()).or_default().push(cd);
    }
    map
}

// ============================================================================
// Type mapping helpers
// ============================================================================

/// Map a `BamlType` to the Rust type string for an owned struct field.
fn owned_rust_type(ty: &BamlType, class_ns_map: &BTreeMap<String, String>) -> String {
    match ty {
        BamlType::String => "String".into(),
        BamlType::Int => "i64".into(),
        BamlType::Float => "f64".into(),
        BamlType::Bool => "bool".into(),
        BamlType::Null => "()".into(),
        BamlType::RustType => "std::sync::Arc<dyn std::any::Any + Send + Sync>".into(),
        BamlType::List(inner) => format!("Vec<{}>", owned_rust_type(inner, class_ns_map)),
        BamlType::Map(k, v) => format!(
            "indexmap::IndexMap<{}, {}>",
            owned_rust_type(k, class_ns_map),
            owned_rust_type(v, class_ns_map)
        ),
        BamlType::Optional(inner) => {
            format!("Option<{}>", owned_rust_type(inner, class_ns_map))
        }
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                format!("owned::{ns}::{name}")
            } else {
                match name.as_str() {
                    "unknown" => "BexExternalValue".into(),
                    "type" => "baml_type::Ty".into(),
                    "function" => "BexExternalValue".into(),
                    _ => "BexExternalValue".into(),
                }
            }
        }
        BamlType::Generic(_) | BamlType::Media(_) => "BexExternalValue".into(),
    }
}

/// Map a `BamlType` to the return type string for a view struct accessor.
fn view_return_type(ty: &BamlType, needs_heap: &mut bool) -> String {
    match ty {
        BamlType::Int => "Result<i64, AccessError>".into(),
        BamlType::Float => "Result<f64, AccessError>".into(),
        BamlType::Bool => "Result<bool, AccessError>".into(),
        BamlType::String => {
            *needs_heap = true;
            "Result<&'a String, AccessError>".into()
        }
        BamlType::RustType => {
            *needs_heap = true;
            "Result<std::sync::Arc<dyn std::any::Any + Send + Sync>, AccessError>".into()
        }
        BamlType::Map(_, _) => {
            *needs_heap = true;
            "Result<BexExternalValue, AccessError>".into()
        }
        BamlType::List(_) => {
            *needs_heap = true;
            "Result<BexExternalValue, AccessError>".into()
        }
        BamlType::Optional(_) => {
            *needs_heap = true;
            "Result<BexExternalValue, AccessError>".into()
        }
        _ => {
            *needs_heap = true;
            "Result<BexExternalValue, AccessError>".into()
        }
    }
}

/// Generate the accessor body for a view struct field.
fn view_accessor_body(field_name: &str, ty: &BamlType) -> String {
    match ty {
        BamlType::Int => format!("self.cls.field(\"{field_name}\")?.as_int()"),
        BamlType::Float => format!("self.cls.field(\"{field_name}\")?.as_float()"),
        BamlType::Bool => format!("self.cls.field(\"{field_name}\")?.as_bool()"),
        BamlType::String => format!("self.cls.field(\"{field_name}\")?.as_string(heap)"),
        BamlType::RustType => format!("self.cls.field(\"{field_name}\")?.as_rust_data(heap)"),
        _ => format!("self.cls.field(\"{field_name}\")?.as_owned_but_very_slow(heap)"),
    }
}

/// Generate a Rust expression that converts a `BexExternalValue` (`val_expr`)
/// into the owned Rust type for `ty`, returning `Result<T, AccessError>`.
fn external_to_typed_expr(
    val_expr: &str,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
) -> String {
    match ty {
        BamlType::String => format!(
            "match {val_expr} {{ BexExternalValue::String(v) => Ok(v), \
             other => Err(AccessError::TypeMismatch {{ expected: \"string\", \
             actual: other.type_name().to_string() }}) }}"
        ),
        BamlType::Int => format!(
            "match {val_expr} {{ BexExternalValue::Int(v) => Ok(v), \
             other => Err(AccessError::TypeMismatch {{ expected: \"int\", \
             actual: other.type_name().to_string() }}) }}"
        ),
        BamlType::Float => format!(
            "match {val_expr} {{ BexExternalValue::Float(v) => Ok(v), \
             other => Err(AccessError::TypeMismatch {{ expected: \"float\", \
             actual: other.type_name().to_string() }}) }}"
        ),
        BamlType::Bool => format!(
            "match {val_expr} {{ BexExternalValue::Bool(v) => Ok(v), \
             other => Err(AccessError::TypeMismatch {{ expected: \"bool\", \
             actual: other.type_name().to_string() }}) }}"
        ),
        BamlType::RustType => format!(
            "match {val_expr} {{ BexExternalValue::RustData(v) => Ok(v), \
             other => Err(AccessError::TypeMismatch {{ expected: \"rust_data\", \
             actual: other.type_name().to_string() }}) }}"
        ),
        BamlType::List(inner) => {
            let inner_conv = external_to_typed_expr("__v", inner, class_ns_map);
            format!(
                "match {val_expr} {{ BexExternalValue::Array {{ items, .. }} => \
                 items.into_iter().map(|__v| {{ {inner_conv} }}).collect::<Result<Vec<_>, AccessError>>(), \
                 other => Err(AccessError::TypeMismatch {{ expected: \"array\", \
                 actual: other.type_name().to_string() }}) }}"
            )
        }
        BamlType::Map(_k, v) => {
            let v_conv = external_to_typed_expr("__v", v, class_ns_map);
            format!(
                "match {val_expr} {{ BexExternalValue::Map {{ entries, .. }} => \
                 entries.into_iter().map(|(__k, __v)| {{ Ok((__k, ({v_conv})?)) }}).collect::<Result<indexmap::IndexMap<_, _>, AccessError>>(), \
                 other => Err(AccessError::TypeMismatch {{ expected: \"map\", \
                 actual: other.type_name().to_string() }}) }}"
            )
        }
        BamlType::Optional(inner) => {
            let inner_conv = external_to_typed_expr("__v", inner, class_ns_map);
            format!(
                "match {val_expr} {{ BexExternalValue::Null => Ok(None), \
                 __v => Ok(Some(({inner_conv})?)) }}"
            )
        }
        BamlType::Named(name) if class_ns_map.contains_key(name.as_str()) => {
            let ns = &class_ns_map[name.as_str()];
            format!("owned::{ns}::{name}::from_external({val_expr})")
        }
        _ => format!("Ok({val_expr})"),
    }
}

/// Generate the `into_owned` conversion expression for a view field.
fn into_owned_expr(
    field_name: &str,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
) -> String {
    match ty {
        BamlType::Int | BamlType::Float | BamlType::Bool => {
            format!("self.{field_name}()?")
        }
        BamlType::String => format!("self.{field_name}(heap)?.clone()"),
        BamlType::RustType => format!("self.{field_name}(heap)?"),
        BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let conv =
                external_to_typed_expr(&format!("self.{field_name}(heap)?"), ty, class_ns_map);
            format!("({conv})?")
        }
        BamlType::Named(name) if class_ns_map.contains_key(name.as_str()) => {
            let conv =
                external_to_typed_expr(&format!("self.{field_name}(heap)?"), ty, class_ns_map);
            format!("({conv})?")
        }
        _ => format!("self.{field_name}(heap)?"),
    }
}

/// Generate the `BexExternalValue` conversion expression for an owned field.
#[allow(clippy::only_used_in_recursion)]
fn owned_to_external_expr(
    field_expr: &str,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
) -> String {
    match ty {
        BamlType::Int => format!("BexExternalValue::Int({field_expr})"),
        BamlType::Float => format!("BexExternalValue::Float({field_expr})"),
        BamlType::Bool => format!("BexExternalValue::Bool({field_expr})"),
        BamlType::String => format!("BexExternalValue::String({field_expr})"),
        BamlType::RustType => format!("BexExternalValue::RustData({field_expr})"),
        BamlType::Null => "BexExternalValue::Null".into(),
        BamlType::List(inner) => {
            let inner_conv = owned_to_external_expr("__v", inner, class_ns_map);
            format!(
                "BexExternalValue::Array {{ element_type: baml_type::Ty::unknown(), \
                 items: {field_expr}.into_iter().map(|__v| {inner_conv}).collect() }}"
            )
        }
        BamlType::Map(_k, v) => {
            let v_conv = owned_to_external_expr("__v", v, class_ns_map);
            format!(
                "BexExternalValue::Map {{ key_type: baml_type::Ty::string(), \
                 value_type: baml_type::Ty::unknown(), \
                 entries: {field_expr}.into_iter().map(|(__k, __v)| (__k, {v_conv})).collect() }}"
            )
        }
        BamlType::Optional(inner) => {
            let inner_conv = owned_to_external_expr("__v", inner, class_ns_map);
            format!("{field_expr}.map(|__v| {inner_conv}).unwrap_or(BexExternalValue::Null)")
        }
        BamlType::Named(_name) => {
            format!("{field_expr}.into_bex_external_value()")
        }
        _ => format!("{field_expr}.into_bex_external_value()"),
    }
}

/// Map a `BamlType` to the Rust type for a clean trait method param/return.
fn clean_rust_type(ty: &BamlType, class_ns_map: &BTreeMap<String, String>) -> String {
    match ty {
        BamlType::String => "String".into(),
        BamlType::Int => "i64".into(),
        BamlType::Float => "f64".into(),
        BamlType::Bool => "bool".into(),
        BamlType::Null => "()".into(),
        BamlType::RustType => "std::sync::Arc<dyn std::any::Any + Send + Sync>".into(),
        BamlType::List(inner) => {
            format!("Vec<{}>", clean_rust_type(inner, class_ns_map))
        }
        BamlType::Map(k, v) => format!(
            "indexmap::IndexMap<{}, {}>",
            clean_rust_type(k, class_ns_map),
            clean_rust_type(v, class_ns_map)
        ),
        BamlType::Optional(inner) => {
            format!("Option<{}>", clean_rust_type(inner, class_ns_map))
        }
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                format!("owned::{ns}::{name}")
            } else {
                match name.as_str() {
                    "type" => "baml_type::Ty".into(),
                    "unknown" => "BexExternalValue".into(),
                    "function" => "BexExternalValue".into(),
                    _ => "BexExternalValue".into(),
                }
            }
        }
        BamlType::Generic(_) | BamlType::Media(_) => "BexExternalValue".into(),
    }
}

/// Generate the arg extraction expression for a glue method parameter.
fn glue_extract_expr(
    arg_var: &str,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    is_receiver: bool,
) -> String {
    if is_receiver {
        // Receiver (self) params are always IO classes → extract via view + into_owned
        // The receiver class is determined by the method's class context
        return "/* receiver extracted below */".to_string();
    }
    match ty {
        BamlType::String => format!("{arg_var}.as_string(&__p)?.to_string()"),
        BamlType::Int => format!("{arg_var}.as_int()?"),
        BamlType::Float => format!("{arg_var}.as_float()?"),
        BamlType::Bool => format!("{arg_var}.as_bool()?"),
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                format!("{arg_var}.as_builtin_class::<view::{ns}::{name}>(&__p)?.into_owned(&__p)?")
            } else {
                match name.as_str() {
                    "type" => format!("{arg_var}.as_baml_type_owned(&__p)?"),
                    _ => format!("{arg_var}.as_owned_but_very_slow(&__p)?"),
                }
            }
        }
        BamlType::RustType => format!("{arg_var}.as_rust_data(&__p)?"),
        BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let conv = external_to_typed_expr(
                &format!("{arg_var}.as_owned_but_very_slow(&__p)?"),
                ty,
                class_ns_map,
            );
            format!("({conv})?")
        }
        _ => format!("{arg_var}.as_owned_but_very_slow(&__p)?"),
    }
}

// ============================================================================
// PascalCase helpers
// ============================================================================

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn ns_trait_name(ns: &str) -> String {
    format!("IoNamespace{}", capitalize_first(ns))
}

fn class_trait_name(ns: &str, class: &str) -> String {
    format!("IoClass{}{}", capitalize_first(ns), class)
}

// ============================================================================
// Generate SysOp Enum
// ============================================================================

pub fn generate_sys_op_enum(io_builtins: &[NativeBuiltin]) -> String {
    let mut out = String::new();

    // Enum definition
    out.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n");
    out.push_str("pub enum SysOp {\n");
    for b in io_builtins {
        let variant = b.sys_op_variant_name();
        writeln!(out, "    {variant},").unwrap();
    }
    out.push_str("}\n\n");

    // SysOp impl block
    out.push_str("impl SysOp {\n");

    // path()
    out.push_str("    pub const fn path(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for b in io_builtins {
        writeln!(
            out,
            "            SysOp::{} => {:?},",
            b.sys_op_variant_name(),
            b.path
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n\n");

    // allowed_error_categories()
    out.push_str("    pub fn allowed_error_categories(&self) -> &'static [SysOpErrorCategory] {\n");
    out.push_str("        match self {\n");
    for b in io_builtins {
        let variant = b.sys_op_variant_name();
        if b.throws.is_empty() {
            writeln!(out, "            SysOp::{variant} => &[],").unwrap();
        } else {
            let cats: Vec<String> = b
                .throws
                .iter()
                .map(|t| format!("SysOpErrorCategory::{t}"))
                .collect();
            writeln!(
                out,
                "            SysOp::{variant} => &[{}],",
                cats.join(", ")
            )
            .unwrap();
        }
    }
    out.push_str("        }\n    }\n\n");

    // allowed_panic_categories() — hardcoded, not extracted from .baml
    out.push_str("    pub fn allowed_panic_categories(&self) -> &'static [SysOpPanicCategory] {\n");
    out.push_str("        match self {\n");
    for b in io_builtins {
        let variant = b.sys_op_variant_name();
        if variant == "BamlSysPanic" {
            writeln!(
                out,
                "            SysOp::{variant} => &[SysOpPanicCategory::HostPanic],"
            )
            .unwrap();
        } else {
            writeln!(out, "            SysOp::{variant} => &[],").unwrap();
        }
    }
    out.push_str("        }\n    }\n");

    out.push_str("}\n\n");

    // Display impl
    out.push_str("impl std::fmt::Display for SysOp {\n");
    out.push_str("    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("        write!(f, \"{}\", self.path())\n");
    out.push_str("    }\n}\n\n");

    // sys_op_for_path() with legacy backward-compat aliases.
    out.push_str("pub fn sys_op_for_path(path: &str) -> Option<SysOp> {\n");
    out.push_str("    match path {\n");
    for b in io_builtins {
        writeln!(
            out,
            "        {:?} => Some(SysOp::{}),",
            b.path,
            b.sys_op_variant_name()
        )
        .unwrap();
    }
    out.push_str("        // Legacy aliases (baml_builtins paths)\n");
    out.push_str("        \"env.get\" | \"env.get_or_panic\" => Some(SysOp::BamlEnvGet),\n");
    out.push_str("        \"baml.http.Response.ok\" => Some(SysOp::BamlHttpResponseText),\n");
    out.push_str("        _ => None,\n");
    out.push_str("    }\n}\n");

    out
}

// ============================================================================
// Generate standalone owned structs for specific classes
// ============================================================================

/// Generate owned Rust structs (with `from_external` and `AsBexExternalValue`)
/// for a specific set of class names. Used by `sys_types` to generate provider
/// option types from `llm_types.baml` without pulling in the full IO trait system.
pub fn generate_owned_structs(class_defs: &[NativeClassDef], class_names: &[&str]) -> String {
    let filtered: Vec<&NativeClassDef> = class_defs
        .iter()
        .filter(|cd| class_names.contains(&cd.name.as_str()))
        .collect();
    let class_ns_map = build_class_ns_map(&filtered);

    let mut out = String::new();
    out.push_str("// Generated from llm_types.baml. Do not edit.\n\n");
    out.push_str("use super::*;\n\n");

    for cd in &filtered {
        emit_owned_struct(&mut out, cd, &class_ns_map, "");
    }

    out
}

// ============================================================================
// Generate IO Traits (main entry point for sys_ops codegen)
// ============================================================================

/// Generate IO traits. `owned_path` controls where owned struct references point.
/// Pass `"owned"` to include the owned module inline, or an external path like
/// `"sys_types::generated::owned"` to reference structs from another crate
/// (skipping the owned module generation).
pub fn generate_io_traits(
    io_builtins: &[NativeBuiltin],
    class_defs: &[NativeClassDef],
    owned_path: &str,
) -> String {
    let tree = build_io_namespace_tree(io_builtins);
    let io_class_defs = filter_io_class_defs(io_builtins, class_defs);
    let class_ns_map = build_class_ns_map(&io_class_defs);
    let class_defs_by_ns = group_class_defs_by_ns(&io_class_defs);

    let mut out = String::new();

    emit_view_module(&mut out, &io_class_defs, &class_ns_map, &class_defs_by_ns);
    if owned_path == "owned" {
        emit_owned_module(&mut out, &io_class_defs, &class_ns_map, &class_defs_by_ns);
    }
    emit_class_traits(&mut out, &tree, &class_ns_map);
    emit_namespace_traits(&mut out, &tree, &class_ns_map);
    emit_root_trait(&mut out, &tree);
    emit_sys_ops_struct(&mut out, io_builtins);

    if owned_path != "owned" {
        out.replace("owned::", &format!("{owned_path}::"))
    } else {
        out
    }
}

// ============================================================================
// View module
// ============================================================================

fn emit_view_module(
    out: &mut String,
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
) {
    out.push_str("pub mod view {\n");

    for (ns, classes) in class_defs_by_ns {
        writeln!(out, "    pub mod {ns} {{").unwrap();
        out.push_str("        use super::super::*;\n\n");

        for cd in classes {
            emit_view_struct(out, cd, class_ns_map, ns);
        }

        out.push_str("    }\n");
    }

    out.push_str("}\n\n");
}

fn emit_view_struct(
    out: &mut String,
    cd: &NativeClassDef,
    class_ns_map: &BTreeMap<String, String>,
    ns: &str,
) {
    let name = &cd.name;
    let full_path = format!("{}.{}", cd.namespace_prefix, cd.name);

    writeln!(out, "        /// Generated from `{}`", cd.source_file).unwrap();
    writeln!(out, "        pub struct {name}<'a> {{").unwrap();
    out.push_str("            cls: BexClass<'a>,\n");
    out.push_str("        }\n\n");

    // From<BexClass<'a>>
    writeln!(out, "        impl<'a> From<BexClass<'a>> for {name}<'a> {{").unwrap();
    writeln!(
        out,
        "            fn from(cls: BexClass<'a>) -> Self {{ Self {{ cls }} }}"
    )
    .unwrap();
    out.push_str("        }\n\n");

    // BuiltinClass<'a>
    writeln!(out, "        impl<'a> BuiltinClass<'a> for {name}<'a> {{").unwrap();
    writeln!(
        out,
        "            fn name() -> &'static str {{ {full_path:?} }}"
    )
    .unwrap();
    out.push_str("        }\n\n");

    // Field accessors
    writeln!(out, "        impl<'a> {name}<'a> {{").unwrap();
    for field in &cd.fields {
        let mut needs_heap = false;
        let ret_type = view_return_type(&field.field_type, &mut needs_heap);
        let heap_param = if needs_heap {
            "heap: &'a GcProtectedHeap<'a>"
        } else {
            ""
        };
        let sep = if needs_heap { ", " } else { "" };
        let body = view_accessor_body(&field.name, &field.field_type);

        writeln!(
            out,
            "            pub fn {}(&self{sep}{heap_param}) -> {ret_type} {{",
            field.name
        )
        .unwrap();
        writeln!(out, "                {body}").unwrap();
        out.push_str("            }\n\n");
    }

    // into_owned()
    let owned_path = format!("owned::{ns}::{name}");
    writeln!(out,
        "            pub fn into_owned(self, heap: &'a GcProtectedHeap<'a>) -> Result<{owned_path}, AccessError> {{"
    ).unwrap();

    writeln!(out, "                Ok({owned_path} {{").unwrap();
    for field in &cd.fields {
        let expr = into_owned_expr(&field.name, &field.field_type, class_ns_map);
        writeln!(out, "                    {}: {expr},", field.name).unwrap();
    }
    out.push_str("                })\n");
    out.push_str("            }\n");

    out.push_str("        }\n\n");
}

// ============================================================================
// Owned module
// ============================================================================

fn emit_owned_module(
    out: &mut String,
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
) {
    out.push_str("pub mod owned {\n");

    for (ns, classes) in class_defs_by_ns {
        writeln!(out, "    pub mod {ns} {{").unwrap();
        out.push_str("        use super::super::*;\n\n");

        for cd in classes {
            emit_owned_struct(out, cd, class_ns_map, ns);
        }

        out.push_str("    }\n");
    }

    out.push_str("}\n\n");
}

fn emit_owned_struct(
    out: &mut String,
    cd: &NativeClassDef,
    class_ns_map: &BTreeMap<String, String>,
    _ns: &str,
) {
    let name = &cd.name;
    let full_path = format!("{}.{}", cd.namespace_prefix, cd.name);

    // Struct definition
    let has_rust_type = cd
        .fields
        .iter()
        .any(|f| matches!(f.field_type, BamlType::RustType));
    let derives = if has_rust_type {
        "#[derive(Clone, Debug)]"
    } else {
        "#[derive(Clone, Debug, Default)]"
    };
    writeln!(out, "        /// Generated from `{}`", cd.source_file).unwrap();
    writeln!(out, "        {derives}").unwrap();
    writeln!(out, "        pub struct {name} {{").unwrap();
    for field in &cd.fields {
        let rust_ty = owned_rust_type(&field.field_type, class_ns_map);
        writeln!(out, "            pub {}: {rust_ty},", field.name).unwrap();
    }
    out.push_str("        }\n\n");

    // AsBexExternalValue impl
    writeln!(out, "        impl AsBexExternalValue for {name} {{").unwrap();
    out.push_str("            fn into_bex_external_value(self) -> BexExternalValue {\n");
    out.push_str("                BexExternalValue::Instance {\n");
    writeln!(
        out,
        "                    class_name: {full_path:?}.to_string(),"
    )
    .unwrap();
    out.push_str("                    fields: indexmap::indexmap! {\n");
    for field in &cd.fields {
        let field_expr = format!("self.{}", field.name);
        let conv = owned_to_external_expr(&field_expr, &field.field_type, class_ns_map);
        writeln!(
            out,
            "                        {:?}.to_string() => {conv},",
            field.name
        )
        .unwrap();
    }
    out.push_str("                    },\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n\n");

    // from_external() — convert BexExternalValue::Instance back to this owned struct
    writeln!(out, "        impl {name} {{").unwrap();
    writeln!(
        out,
        "            pub fn from_external(__val: BexExternalValue) -> Result<Self, AccessError> {{"
    )
    .unwrap();
    out.push_str("                match __val {\n");
    out.push_str(
        "                    BexExternalValue::Instance { mut fields, .. } => Ok(Self {\n",
    );
    for field in &cd.fields {
        let field_val = format!(
            "fields.swap_remove({:?}).unwrap_or(BexExternalValue::Null)",
            field.name
        );
        let conv = external_to_typed_expr(&field_val, &field.field_type, class_ns_map);
        writeln!(out, "                        {}: ({conv})?,", field.name).unwrap();
    }
    out.push_str("                    }),\n");
    writeln!(
        out,
        "                    __other => Err(AccessError::TypeMismatch {{ \
         expected: {name:?}, actual: __other.type_name().to_string() }}),",
    )
    .unwrap();
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n\n");
}

// ============================================================================
// Class traits
// ============================================================================

fn emit_class_traits(
    out: &mut String,
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
) {
    for (ns, node) in tree {
        for (class_name, methods) in &node.classes {
            emit_one_class_trait(out, ns, class_name, methods, class_ns_map);
        }
    }
}

fn emit_one_class_trait(
    out: &mut String,
    ns: &str,
    class_name: &str,
    methods: &[&NativeBuiltin],
    class_ns_map: &BTreeMap<String, String>,
) {
    let trait_name = class_trait_name(ns, class_name);
    let dispatch_fn = format!("__dispatch_{ns}_{}", class_name.to_lowercase());

    if let Some(first) = methods.first() {
        writeln!(out, "/// Generated from `{}`", first.source_file).unwrap();
    }
    writeln!(out, "pub trait {trait_name} {{").unwrap();

    // Clean methods
    for m in methods {
        let method_name = io_method_name(m);
        let ret_ty = clean_rust_type(&m.return_type, class_ns_map);
        let receiver_ty = format!("owned::{ns}::{class_name}");

        // Build param list: &self, heap, call_id, receiver, then other params, then ctx
        let mut param_strs = vec![
            "&self".to_string(),
            "heap: &std::sync::Arc<BexHeap>".to_string(),
            "call_id: CallId".to_string(),
            format!("{}: {receiver_ty}", class_name.to_lowercase()),
        ];
        for p in &m.params {
            let pty = clean_rust_type(&p.ty, class_ns_map);
            param_strs.push(format!("{}: {pty}", p.name));
        }
        param_strs.push("ctx: &SysOpContext".to_string());

        write!(
            out,
            "    fn {method_name}({}) -> SysOpOutput<{ret_ty}>;\n\n",
            param_strs.join(", ")
        )
        .unwrap();
    }

    // Glue methods
    for m in methods {
        emit_glue_method(out, m, ns, class_name, class_ns_map);
    }

    // Dispatch method
    write!(
        out,
        "    fn {dispatch_fn}(&self, method: &str, heap: &std::sync::Arc<BexHeap>,\n\
         \x20       args: Vec<BexValue<'_>>, ctx: &SysOpContext, call_id: CallId,\n\
         \x20   ) -> Option<SysOpResult> {{\n"
    )
    .unwrap();
    out.push_str("        match method {\n");
    for m in methods {
        let method_name = io_method_name(m);
        let glue_name = format!("__glue_{}", m.fn_name);
        writeln!(
            out,
            "            \"{method_name}\" => Some(self.{glue_name}(heap, args, ctx, call_id)),"
        )
        .unwrap();
    }
    out.push_str("            _ => None,\n");
    out.push_str("        }\n    }\n");

    out.push_str("}\n\n");
}

fn emit_glue_method(
    out: &mut String,
    builtin: &NativeBuiltin,
    ns: &str,
    class_name: &str,
    class_ns_map: &BTreeMap<String, String>,
) {
    let method_name = io_method_name(builtin);
    let glue_name = format!("__glue_{}", builtin.fn_name);
    let variant = builtin.sys_op_variant_name();
    let clean_method = method_name;

    write!(
        out,
        "    fn {glue_name}(&self, heap: &std::sync::Arc<BexHeap>, args: Vec<BexValue<'_>>,\n\
         \x20       ctx: &SysOpContext, call_id: CallId,\n\
         \x20   ) -> SysOpResult {{\n"
    )
    .unwrap();

    // Extract args
    out.push_str("        let mut __args = args.into_iter();\n");

    // Receiver arg (self/class instance)
    out.push_str("        let __arg_self = __args.next().unwrap();\n");

    // Other args
    let mut arg_names = Vec::new();
    for (i, _p) in builtin.params.iter().enumerate() {
        writeln!(out, "        let __arg{i} = __args.next().unwrap();").unwrap();
        arg_names.push(format!("__arg{i}"));
    }

    // GC protection block
    out.push_str("        let __extraction = heap.with_gc_protection(move |__p| {\n");
    writeln!(out,
        "            let __receiver = __arg_self.as_builtin_class::<view::{ns}::{class_name}>(&__p)?.into_owned(&__p)?;"
    ).unwrap();

    for (i, p) in builtin.params.iter().enumerate() {
        let extract = glue_extract_expr(&format!("__arg{i}"), &p.ty, class_ns_map, false);
        writeln!(out, "            let __{} = {extract};", p.name).unwrap();
    }

    // Return tuple
    let mut tuple_elems = vec!["__receiver".to_string()];
    for p in &builtin.params {
        tuple_elems.push(format!("__{}", p.name));
    }
    writeln!(
        out,
        "            Ok::<_, AccessError>(({}))",
        tuple_elems.join(", ")
    )
    .unwrap();
    out.push_str("        });\n");

    // Handle extraction result
    out.push_str("        match __extraction {\n");

    // Build the clean method call args
    let mut call_args = vec!["heap".to_string(), "call_id".to_string()];
    call_args.push("__receiver".to_string());
    for p in &builtin.params {
        call_args.push(format!("__{}", p.name));
    }
    call_args.push("ctx".to_string());

    let destructure_names: Vec<String> = tuple_elems.clone();
    writeln!(
        out,
        "            Ok(({d})) => self.{clean_method}({c}).into_result(SysOp::{variant}),",
        d = destructure_names.join(", "),
        c = call_args.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "            Err(e) => SysOpResult::Ready(Err(OpError::new(\
SysOp::{variant}, OpErrorKind::AccessError(e)))),"
    )
    .unwrap();
    out.push_str("        }\n");
    out.push_str("    }\n\n");
}

// ============================================================================
// Namespace traits
// ============================================================================

fn emit_namespace_traits(
    out: &mut String,
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
) {
    for (ns, node) in tree {
        emit_one_namespace_trait(out, ns, node, class_ns_map);
    }
}

fn emit_one_namespace_trait(
    out: &mut String,
    ns: &str,
    node: &IoNamespaceNode,
    class_ns_map: &BTreeMap<String, String>,
) {
    let trait_name = ns_trait_name(ns);
    let dispatch_fn = format!("__dispatch_{ns}");

    // Supertraits: class traits in this namespace
    let class_traits: Vec<String> = node
        .classes
        .keys()
        .map(|cn| class_trait_name(ns, cn))
        .collect();

    if class_traits.is_empty() {
        writeln!(out, "pub trait {trait_name} {{").unwrap();
    } else {
        writeln!(
            out,
            "pub trait {trait_name}: {} {{",
            class_traits.join(" + ")
        )
        .unwrap();
    }

    // Clean methods for free functions
    for f in &node.free_fns {
        let fn_name = io_method_name(f);
        let ret_ty = clean_rust_type(&f.return_type, class_ns_map);

        let mut param_strs = vec![
            "&self".to_string(),
            "heap: &std::sync::Arc<BexHeap>".to_string(),
            "call_id: CallId".to_string(),
        ];
        for p in &f.params {
            let pty = clean_rust_type(&p.ty, class_ns_map);
            param_strs.push(format!("{}: {pty}", p.name));
        }
        // Some functions need ctx
        param_strs.push("ctx: &SysOpContext".to_string());

        write!(
            out,
            "    fn {fn_name}({}) -> SysOpOutput<{ret_ty}>;\n\n",
            param_strs.join(", ")
        )
        .unwrap();
    }

    // Glue methods for free functions
    for f in &node.free_fns {
        emit_free_fn_glue(out, f, class_ns_map);
    }

    // Dispatch method
    write!(
        out,
        "    fn {dispatch_fn}(&self, rest: &str, heap: &std::sync::Arc<BexHeap>,\n\
         \x20       args: Vec<BexValue<'_>>, ctx: &SysOpContext, call_id: CallId,\n\
         \x20   ) -> Option<SysOpResult> {{\n"
    )
    .unwrap();

    if node.classes.is_empty() {
        // Only free functions — match directly
        out.push_str("        match rest {\n");
        for f in &node.free_fns {
            let fn_name = io_method_name(f);
            let glue_name = format!("__glue_{}", f.fn_name);
            writeln!(
                out,
                "            \"{fn_name}\" => Some(self.{glue_name}(heap, args, ctx, call_id)),"
            )
            .unwrap();
        }
        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
    } else {
        // Mix of classes and free functions — use split_once to route
        out.push_str("        match rest.split_once('.') {\n");
        for cn in node.classes.keys() {
            let dispatch = format!("__dispatch_{ns}_{}", cn.to_lowercase());
            writeln!(out,
                "            Some((\"{cn}\", method)) => self.{dispatch}(method, heap, args, ctx, call_id),"
            ).unwrap();
        }
        // Free functions (no dot)
        out.push_str("            None => match rest {\n");
        for f in &node.free_fns {
            let fn_name = io_method_name(f);
            let glue_name = format!("__glue_{}", f.fn_name);
            writeln!(
                out,
                "                \"{fn_name}\" => Some(self.{glue_name}(heap, args, ctx, call_id)),"
            )
            .unwrap();
        }
        out.push_str("                _ => None,\n");
        out.push_str("            },\n");
        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
    }

    out.push_str("    }\n");
    out.push_str("}\n\n");
}

fn emit_free_fn_glue(
    out: &mut String,
    builtin: &NativeBuiltin,
    class_ns_map: &BTreeMap<String, String>,
) {
    let glue_name = format!("__glue_{}", builtin.fn_name);
    let variant = builtin.sys_op_variant_name();
    let clean_name = io_method_name(builtin);

    write!(
        out,
        "    fn {glue_name}(&self, heap: &std::sync::Arc<BexHeap>, args: Vec<BexValue<'_>>,\n\
         \x20       ctx: &SysOpContext, call_id: CallId,\n\
         \x20   ) -> SysOpResult {{\n"
    )
    .unwrap();

    if builtin.params.is_empty() {
        // No params to extract
        writeln!(
            out,
            "        self.{clean_name}(heap, call_id, ctx).into_result(SysOp::{variant})"
        )
        .unwrap();
    } else {
        // Extract args
        out.push_str("        let mut __args = args.into_iter();\n");
        for (i, _p) in builtin.params.iter().enumerate() {
            writeln!(out, "        let __arg{i} = __args.next().unwrap();").unwrap();
        }

        out.push_str("        let __extraction = heap.with_gc_protection(move |__p| {\n");
        let mut param_names = Vec::new();
        for (i, p) in builtin.params.iter().enumerate() {
            let extract = glue_extract_expr(&format!("__arg{i}"), &p.ty, class_ns_map, false);
            writeln!(out, "            let __{} = {extract};", p.name).unwrap();
            param_names.push(format!("__{}", p.name));
        }

        if param_names.len() == 1 {
            writeln!(out, "            Ok::<_, AccessError>({})", param_names[0]).unwrap();
        } else {
            writeln!(
                out,
                "            Ok::<_, AccessError>(({}))",
                param_names.join(", ")
            )
            .unwrap();
        }
        out.push_str("        });\n");

        // Handle result
        out.push_str("        match __extraction {\n");

        let mut call_args = vec!["heap".to_string(), "call_id".to_string()];
        for p in &builtin.params {
            call_args.push(format!("__{}", p.name));
        }
        call_args.push("ctx".to_string());

        if param_names.len() == 1 {
            writeln!(
                out,
                "            Ok({}) => self.{clean_name}({}).into_result(SysOp::{variant}),",
                param_names[0],
                call_args.join(", ")
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "            Ok(({d})) => self.{clean_name}({c}).into_result(SysOp::{variant}),",
                d = param_names.join(", "),
                c = call_args.join(", ")
            )
            .unwrap();
        }
        writeln!(
            out,
            "            Err(e) => SysOpResult::Ready(Err(OpError::new(\
SysOp::{variant}, OpErrorKind::AccessError(e)))),"
        )
        .unwrap();
        out.push_str("        }\n");
    }

    out.push_str("    }\n\n");
}

// ============================================================================
// Root trait
// ============================================================================

fn emit_root_trait(out: &mut String, tree: &BTreeMap<String, IoNamespaceNode>) {
    let ns_traits: Vec<String> = tree.keys().map(|ns| ns_trait_name(ns)).collect();

    writeln!(out, "pub trait IoPackageBaml: {} {{", ns_traits.join(" + ")).unwrap();

    // get_sys_op_fn dispatch
    out.push_str(
        "    fn get_sys_op_fn(&self, path: &str, heap: &std::sync::Arc<BexHeap>,\n\
         \x20       args: Vec<BexValue<'_>>, ctx: &SysOpContext, call_id: CallId,\n\
         \x20   ) -> Option<SysOpResult> {\n",
    );
    out.push_str("        match path.split_once('.') {\n");
    out.push_str("            Some((\"baml\", rest)) => {\n");
    out.push_str("                match rest.split_once('.') {\n");
    for ns in tree.keys() {
        let dispatch_fn = format!("__dispatch_{ns}");
        writeln!(out,
            "                    Some((\"{ns}\", rest)) => self.{dispatch_fn}(rest, heap, args, ctx, call_id),"
        ).unwrap();
    }
    out.push_str("                    _ => None,\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("            _ => None,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
}

// ============================================================================
// SysOps struct
// ============================================================================

fn emit_sys_ops_struct(out: &mut String, io_builtins: &[NativeBuiltin]) {
    // Struct definition
    out.push_str("#[derive(Clone)]\n");
    out.push_str("pub struct SysOps {\n");
    for b in io_builtins {
        writeln!(out, "    pub {}: SysOpFn,", b.fn_name).unwrap();
    }
    out.push_str("}\n\n");

    out.push_str("impl SysOps {\n");

    // get()
    out.push_str("    pub fn get(&self, op: SysOp) -> &SysOpFn {\n");
    out.push_str("        match op {\n");
    for b in io_builtins {
        writeln!(
            out,
            "            SysOp::{} => &self.{},",
            b.sys_op_variant_name(),
            b.fn_name
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n\n");

    // unsupported()
    out.push_str("    pub fn unsupported(operation: SysOp) -> SysOpFn {\n");
    out.push_str("        std::sync::Arc::new(move |_, _, _, _| {\n");
    out.push_str("            SysOpResult::Ready(Err(OpError::new(\n");
    out.push_str("                operation,\n");
    out.push_str("                OpErrorKind::Unsupported,\n");
    out.push_str("            )))\n");
    out.push_str("        })\n");
    out.push_str("    }\n\n");

    // all_unsupported()
    out.push_str("    pub fn all_unsupported() -> Self {\n");
    out.push_str("        Self {\n");
    for b in io_builtins {
        writeln!(
            out,
            "            {}: Self::unsupported(SysOp::{}),",
            b.fn_name,
            b.sys_op_variant_name()
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n\n");

    // from_impl()
    out.push_str(
        "    pub fn from_impl<T: IoPackageBaml + Send + Sync + 'static>(t: T) -> Self {\n",
    );
    out.push_str("        let t = std::sync::Arc::new(t);\n");
    out.push_str("        Self {\n");
    for b in io_builtins {
        let variant = b.sys_op_variant_name();
        writeln!(out, "            {}: {{", b.fn_name).unwrap();
        out.push_str("                let t = t.clone();\n");
        out.push_str("                std::sync::Arc::new(move |heap, args, ctx, call_id| {\n");
        writeln!(
            out,
            "                    t.get_sys_op_fn({:?}, heap, args, ctx, call_id)",
            b.path
        )
        .unwrap();
        write!(
            out,
            "                        .unwrap_or_else(|| SysOpResult::Ready(Err(OpError::new(\n\
             \x20                           SysOp::{variant}, OpErrorKind::Unsupported,\n\
             \x20                       ))))\n"
        )
        .unwrap();
        out.push_str("                })\n");
        out.push_str("            },\n");
    }
    out.push_str("        }\n    }\n");

    out.push_str("}\n");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_native_builtins;

    #[test]
    fn test_sys_op_enum_paths() {
        let (_vm, io, _cd) = extract_native_builtins().unwrap();
        let code = generate_sys_op_enum(&io);

        assert!(code.contains("SysOp::BamlFsOpen => \"baml.fs.open\""));
        assert!(code.contains("SysOp::BamlEnvGet => \"baml.env.get\""));
        assert!(code.contains("SysOp::BamlSysPanic => \"baml.sys.panic\""));
    }

    #[test]
    fn test_sys_op_enum_error_categories() {
        let (_vm, io, _cd) = extract_native_builtins().unwrap();
        let code = generate_sys_op_enum(&io);

        assert!(code.contains("SysOp::BamlFsOpen => &[SysOpErrorCategory::Io]"));
        assert!(code.contains(
            "SysOp::BamlHttpFetch => &[SysOpErrorCategory::Io, SysOpErrorCategory::Timeout]"
        ));
        assert!(code.contains("SysOp::BamlSysPanic => &[]"));
    }

    #[test]
    fn test_sys_op_enum_panic_categories() {
        let (_vm, io, _cd) = extract_native_builtins().unwrap();
        let code = generate_sys_op_enum(&io);

        assert!(code.contains("SysOp::BamlSysPanic => &[SysOpPanicCategory::HostPanic]"));
        assert!(code.contains("SysOp::BamlFsOpen => &[]"));
    }

    #[test]
    fn test_sys_op_for_path() {
        let (_vm, io, _cd) = extract_native_builtins().unwrap();
        let code = generate_sys_op_enum(&io);

        assert!(code.contains("\"baml.fs.open\" => Some(SysOp::BamlFsOpen)"));
        assert!(code.contains("\"baml.env.get\" => Some(SysOp::BamlEnvGet)"));
    }

    #[test]
    fn test_sys_ops_struct_field_names() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        let expected_fields = [
            "pub baml_fs_open: SysOpFn",
            "pub baml_fs_file_read: SysOpFn",
            "pub baml_fs_file_close: SysOpFn",
            "pub baml_net_connect: SysOpFn",
            "pub baml_http_fetch: SysOpFn",
            "pub baml_http_send: SysOpFn",
            "pub baml_sys_shell: SysOpFn",
            "pub baml_sys_sleep: SysOpFn",
            "pub baml_sys_panic: SysOpFn",
            "pub baml_env_get: SysOpFn",
        ];

        for f in &expected_fields {
            assert!(code.contains(f), "Missing field: {f}");
        }
    }

    #[test]
    fn test_owned_fs_file() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(code.contains("pub mod fs {"));
        assert!(code.contains("pub struct File {"));
        assert!(code.contains("pub _handle: std::sync::Arc<dyn std::any::Any + Send + Sync>"));
    }

    #[test]
    fn test_owned_http_response() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(code.contains("pub mod http {"));
        assert!(
            code.contains("pub status_code: i64"),
            "Missing status_code field"
        );
        assert!(
            code.contains("pub headers: indexmap::IndexMap<String, String>"),
            "Missing headers field"
        );
        assert!(code.contains("pub url: String"), "Missing url field");
        assert!(
            code.contains("pub _body: std::sync::Arc<dyn std::any::Any + Send + Sync>"),
            "Missing _body field"
        );
    }

    #[test]
    fn test_view_fs_file() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(code.contains("pub struct File<'a>"));
        assert!(code.contains("cls: BexClass<'a>"));
        assert!(code.contains("impl<'a> From<BexClass<'a>> for File<'a>"));
        assert!(code.contains("impl<'a> BuiltinClass<'a> for File<'a>"));
        assert!(code.contains("fn _handle(&self"));
        assert!(code.contains("as_rust_data(heap)"));
        assert!(code.contains("fn into_owned"));
    }

    #[test]
    fn test_class_trait_llm_primitive_client() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(
            code.contains("pub trait IoClassLlmPrimitiveClient"),
            "Missing IoClassLlmPrimitiveClient trait"
        );
        assert!(
            code.contains("fn render_prompt("),
            "Missing render_prompt method"
        );
        assert!(
            code.contains("fn specialize_prompt("),
            "Missing specialize_prompt method"
        );
        assert!(
            code.contains("fn build_request("),
            "Missing build_request method"
        );
        assert!(code.contains("fn parse("), "Missing parse method");
    }

    #[test]
    fn test_namespace_traits() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(
            code.contains("pub trait IoNamespaceFs: IoClassFsFile"),
            "Missing IoNamespaceFs"
        );
        assert!(
            code.contains("pub trait IoNamespaceNet: IoClassNetSocket"),
            "Missing IoNamespaceNet"
        );
        assert!(
            code.contains("pub trait IoNamespaceHttp: IoClassHttpResponse"),
            "Missing IoNamespaceHttp"
        );
        assert!(
            code.contains("pub trait IoNamespaceSys {"),
            "Missing IoNamespaceSys"
        );
        assert!(
            code.contains("pub trait IoNamespaceEnv {"),
            "Missing IoNamespaceEnv"
        );
        assert!(
            code.contains("pub trait IoNamespaceLlm: IoClassLlmClient + IoClassLlmPrimitiveClient"),
            "Missing IoNamespaceLlm"
        );
    }

    #[test]
    fn test_root_trait() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(
            code.contains("pub trait IoPackageBaml:"),
            "Missing IoPackageBaml"
        );
        assert!(
            code.contains("IoNamespaceFs"),
            "Missing IoNamespaceFs in supertraits"
        );
        assert!(
            code.contains("IoNamespaceLlm"),
            "Missing IoNamespaceLlm in supertraits"
        );
        assert!(
            code.contains("fn get_sys_op_fn("),
            "Missing get_sys_op_fn dispatch"
        );
    }

    #[test]
    fn test_sys_ops_from_impl() {
        let (_vm, io, cd) = extract_native_builtins().unwrap();
        let code = generate_io_traits(&io, &cd, "owned");

        assert!(
            code.contains("pub fn from_impl<T: IoPackageBaml + Send + Sync + 'static>"),
            "Missing from_impl"
        );
        assert!(
            code.contains("pub fn all_unsupported() -> Self"),
            "Missing all_unsupported"
        );
    }
}
