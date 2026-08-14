//! Code generation for modular `BamlClass*` / `BamlNamespace*` / `BamlPackageBaml`
//! traits from extracted `NativeBuiltin` records.
//!
//! `generate_native_trait` takes the output of `extract_native_builtins()?` and emits
//! a Rust source `String` containing a hierarchy of traits:
//!
//! - **`BamlClass*`** traits (leaf): required methods with bare names, `__glue_*`
//!   defaults, and a `__dispatch_*` method that maps method names to glue fns.
//! - **`BamlNamespace*`** traits (aggregators): supertraits of child classes,
//!   own free-function methods, and a `__dispatch_*` that routes via `split_once('.')`.
//! - **`BamlPackageBaml`** (root): supertraits of all top-level classes and namespaces,
//!   root free functions, and `get_native_fn(path)` entry point.

use std::{collections::BTreeMap, fmt::Write};

use crate::{
    rust_ident::{rust_field_ident, rust_field_value_ident},
    types::{BamlType, NativeBuiltin, NativeClassDef, Receiver, VmUsage},
};

// ============================================================================
// Fallibility
// ============================================================================

/// Returns `true` if the clean trait method for this builtin should return
/// `Result<T, VmError>` instead of plain `T`.
///
/// A builtin is fallible (its trait method returns `Result<T, VmRustFnError>`) when
/// it declares a non-empty `throws` clause, or is explicitly `//baml:fallible`.
///
/// `//baml:fallible` covers builtins that raise a *panic* (e.g. `AllocFailure`)
/// rather than a catchable `throws` error, so `throws never` wouldn't flag them on
/// its own. A missing `throws` clause (`None`) is rejected during extraction
/// (`extract_native_builtins`), so here it's treated as infallible.
fn is_fallible(b: &NativeBuiltin) -> bool {
    b.fallible || matches!(&b.throws, Some(cats) if !cats.is_empty())
}

// ============================================================================
// camelCase → snake_case conversion
// ============================================================================

fn camel_to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.extend(c.to_lowercase());
    }
    result
}

// ============================================================================
// Namespace tree
// ============================================================================

struct BuiltinEntry<'a> {
    builtin: &'a NativeBuiltin,
    baml_method_name: String,
    rust_method_name: String,
}

struct NamespaceNode<'a> {
    free_fns: Vec<BuiltinEntry<'a>>,
    classes: BTreeMap<String, Vec<BuiltinEntry<'a>>>,
    sub_namespaces: BTreeMap<String, NamespaceNode<'a>>,
}

impl NamespaceNode<'_> {
    fn new() -> Self {
        Self {
            free_fns: Vec::new(),
            classes: BTreeMap::new(),
            sub_namespaces: BTreeMap::new(),
        }
    }

    fn get_or_create_namespace(&mut self, segments: &[&str]) -> &mut Self {
        let mut current = self;
        for &seg in segments {
            current = current
                .sub_namespaces
                .entry(seg.to_string())
                .or_insert_with(NamespaceNode::new);
        }
        current
    }
}

// ============================================================================
// Class namespace tree (for view/copy generation)
// ============================================================================

/// Namespace tree for class definitions (used for view/copy generation).
struct ClassNamespaceNode<'a> {
    classes: BTreeMap<String, &'a NativeClassDef>,
    sub_namespaces: BTreeMap<String, ClassNamespaceNode<'a>>,
}

impl ClassNamespaceNode<'_> {
    fn new() -> Self {
        Self {
            classes: BTreeMap::new(),
            sub_namespaces: BTreeMap::new(),
        }
    }
}

fn build_class_namespace_tree<'a>(
    class_defs: &'a [NativeClassDef],
    package: &str,
) -> ClassNamespaceNode<'a> {
    let mut root = ClassNamespaceNode::new();
    let prefix = format!("{package}.");
    for def in class_defs {
        // namespace_prefix is e.g. "baml.media", "baml.errors", "baml"
        let rest = def.namespace_prefix.strip_prefix(&prefix).unwrap_or("");
        if rest.is_empty() {
            // Root-level class
            root.classes.insert(def.name.clone(), def);
        } else {
            // Namespaced class — split segments and navigate
            let segments: Vec<&str> = rest.split('.').collect();
            let mut node = &mut root;
            for seg in &segments {
                node = node
                    .sub_namespaces
                    .entry(seg.to_string())
                    .or_insert_with(ClassNamespaceNode::new);
            }
            node.classes.insert(def.name.clone(), def);
        }
    }
    root
}

/// Group builtins into a tree based on their dotted paths.
///
/// Path convention: uppercase segments are class names, lowercase are namespaces.
/// - `baml.Array.length` → class `Array` at root, method `length`
/// - `baml.media.Pdf.url` → namespace `media`, class `Pdf`, method `url`
/// - `baml.deep_copy` → root free function
/// - `baml.sys.argv` → namespace `sys`, free function `argv`
fn build_namespace_tree<'a>(builtins: &'a [NativeBuiltin], package: &str) -> NamespaceNode<'a> {
    let mut root = NamespaceNode::new();
    let prefix = format!("{package}.");

    for b in builtins {
        let rest = b.path.strip_prefix(&prefix).unwrap_or(&b.path);
        let segments: Vec<&str> = rest.split('.').collect();
        let last_idx = segments.len() - 1;

        // The class is the first uppercase segment before the final method
        // segment (if any). Everything after the class is the dispatch key:
        // a method declared inside an `implements I { ... }` block keeps the
        // interface segment in its runtime path (`...{Class}.I.method`), so the
        // class dispatch must match on `I.method`. The Rust method name comes
        // from the final segment alone so it stays a valid identifier.
        let class_idx = segments[..last_idx]
            .iter()
            .position(|s| s.starts_with(|c: char| c.is_uppercase()));

        let (ns_segments, class_name, baml_method_name): (Vec<&str>, Option<&str>, String) =
            match class_idx {
                Some(ci) => (
                    segments[..ci].to_vec(),
                    Some(segments[ci]),
                    segments[ci + 1..].join("."),
                ),
                None => (
                    segments[..last_idx].to_vec(),
                    None,
                    segments[last_idx].to_string(),
                ),
            };

        let entry = BuiltinEntry {
            builtin: b,
            baml_method_name,
            rust_method_name: camel_to_snake(segments[last_idx]),
        };

        let node = root.get_or_create_namespace(&ns_segments);

        if let Some(cls) = class_name {
            node.classes.entry(cls.to_string()).or_default().push(entry);
        } else {
            node.free_fns.push(entry);
        }
    }

    root
}

// ============================================================================
// View module emission
// ============================================================================

fn emit_view_module(out: &mut String, root: &ClassNamespaceNode) {
    out.push_str("#[allow(dead_code, unused_imports, unused_variables)]\n");
    out.push_str("pub mod view {\n");
    out.push_str("    use super::*;\n\n");
    emit_view_namespace_contents(out, root, 1);
    out.push_str("}\n\n");
}

fn emit_view_namespace_contents(out: &mut String, node: &ClassNamespaceNode, depth: usize) {
    let indent = "    ".repeat(depth);

    // Emit classes directly in this namespace
    for (class_name, class_def) in &node.classes {
        emit_view_struct(out, class_name, class_def, depth);
    }

    // Emit sub-namespace modules
    for (ns_name, sub_node) in &node.sub_namespaces {
        writeln!(out, "{indent}pub mod {ns_name} {{").unwrap();
        write!(out, "{indent}    use super::super::*;\n\n").unwrap();
        emit_view_namespace_contents(out, sub_node, depth + 1);
        writeln!(out, "{indent}}}\n").unwrap();
    }
}

fn emit_view_struct(out: &mut String, class_name: &str, def: &NativeClassDef, depth: usize) {
    let indent = "    ".repeat(depth);
    let inner = "    ".repeat(depth + 1);
    let inner2 = "    ".repeat(depth + 2);

    // Struct definition
    writeln!(out, "{indent}/// Generated from `{}`", def.source_file).unwrap();
    writeln!(out, "{indent}pub struct {class_name}<'a> {{").unwrap();
    writeln!(out, "{inner}pub instance: &'a Instance,").unwrap();
    writeln!(out, "{indent}}}\n").unwrap();

    // Impl block with typed accessors
    writeln!(out, "{indent}impl<'a> {class_name}<'a> {{").unwrap();

    for field in &def.fields {
        let raw_name = &field.name;
        let field_name = rust_field_ident(raw_name);
        match &field.field_type {
            BamlType::RustType => {
                // Generic downcast accessor: fn _data<T: 'static>(&self, vm: &BexVm) -> &T
                writeln!(
                    out,
                    "{inner}pub fn {field_name}<'v, T: 'static>(&self, vm: &'v BexVm) -> &'v T {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}vm.as_rust_data::<T>(&self.instance.load_field({}))",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .expect(\"{class_name}.{raw_name}: downcast failed\")"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Int => {
                writeln!(out, "{inner}pub fn {field_name}(&self) -> i64 {{").unwrap();
                writeln!(
                    out,
                    "{inner2}self.instance.load_field({}).as_int()",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .unwrap_or_else(|| panic!(\"{class_name}.{raw_name}: expected Int\"))"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Float => {
                writeln!(out, "{inner}pub fn {field_name}(&self) -> f64 {{").unwrap();
                writeln!(
                    out,
                    "{inner2}match self.instance.load_field({}).as_object_ptr() {{",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    Some(ptr) => match unsafe {{ ptr.get() }} {{"
                )
                .unwrap();
                writeln!(out, "{inner2}        bex_vm_types::Object::Float(f) => *f,").unwrap();
                writeln!(
                    out,
                    "{inner2}        _ => panic!(\"{class_name}.{raw_name}: expected Float\"),"
                )
                .unwrap();
                writeln!(out, "{inner2}    }},").unwrap();
                writeln!(
                    out,
                    "{inner2}    None => panic!(\"{class_name}.{raw_name}: expected Float\"),"
                )
                .unwrap();
                writeln!(out, "{inner2}}}").unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Bool => {
                writeln!(out, "{inner}pub fn {field_name}(&self) -> bool {{").unwrap();
                writeln!(
                    out,
                    "{inner2}self.instance.load_field({}).as_bool()",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .unwrap_or_else(|| panic!(\"{class_name}.{raw_name}: expected Bool\"))"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::String => {
                // Heap type — vm parameter needed
                writeln!(
                    out,
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> &'v str {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}vm.as_string(&self.instance.load_field({}))",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .expect(\"{class_name}.{raw_name}: expected String\")"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::List(_) => {
                writeln!(
                    out,
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> ArrayReadGuard<'v> {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}vm.as_array(&self.instance.load_field({}))",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .expect(\"{class_name}.{raw_name}: expected Array\")"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Map(_, _) => {
                writeln!(
                    out,
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> MapReadGuard<'v> {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}vm.as_map(&self.instance.load_field({}))",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    .expect(\"{class_name}.{raw_name}: expected Map\")"
                )
                .unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Optional(inner_ty) => {
                // For optional fields, return Option<T> with appropriate accessor
                let (ret_type, some_expr) =
                    view_optional_type_and_expr(class_name, raw_name, inner_ty, field.index);
                writeln!(
                    out,
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> {ret_type} {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}if self.instance.load_field({}).is_null() {{",
                    field.index
                )
                .unwrap();
                writeln!(out, "{inner2}    None").unwrap();
                writeln!(out, "{inner2}}} else {{").unwrap();
                writeln!(out, "{inner2}    Some({some_expr})").unwrap();
                writeln!(out, "{inner2}}}").unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            BamlType::Bigint => {
                // Bigints live behind a heap pointer (`Object::Bigint`); clone
                // the shared `Arc` out (cheap) so the caller gets an owned handle.
                writeln!(
                    out,
                    "{inner}pub fn {field_name}(&self) -> std::sync::Arc<num_bigint::BigInt> {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}match self.instance.load_field({}).as_object_ptr() {{",
                    field.index
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}    Some(ptr) => match unsafe {{ ptr.get() }} {{"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}        bex_vm_types::Object::Bigint(b) => b.clone(),"
                )
                .unwrap();
                writeln!(
                    out,
                    "{inner2}        _ => panic!(\"{class_name}.{raw_name}: expected Bigint\"),"
                )
                .unwrap();
                writeln!(out, "{inner2}    }},").unwrap();
                writeln!(
                    out,
                    "{inner2}    None => panic!(\"{class_name}.{raw_name}: expected Bigint\"),"
                )
                .unwrap();
                writeln!(out, "{inner2}}}").unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
            // Generic, Named, Media, Null — fallback to a copied Value.
            _ => {
                writeln!(out, "{inner}pub fn {field_name}(&self) -> Value {{").unwrap();
                writeln!(out, "{inner2}self.instance.load_field({})", field.index).unwrap();
                writeln!(out, "{inner}}}\n").unwrap();
            }
        }
    }

    writeln!(out, "{indent}}}\n").unwrap();
}

/// Returns (`return_type`, `some_expression`) for Optional field view accessors.
fn view_optional_type_and_expr(
    class_name: &str,
    field_name: &str,
    inner: &BamlType,
    field_index: usize,
) -> (String, String) {
    match inner {
        BamlType::Int => (
            "Option<i64>".to_string(),
            format!(
                "self.instance.load_field({field_index}).as_int().unwrap_or_else(|| panic!(\"{class_name}.{field_name}: expected Int\"))"
            ),
        ),
        BamlType::Float => (
            "Option<f64>".to_string(),
            format!(
                "{{ \
                    let Some(ptr) = self.instance.load_field({field_index}).as_object_ptr() \
                        else {{ panic!(\"{class_name}.{field_name}: expected Float\") }}; \
                    let bex_vm_types::Object::Float(f) = (unsafe {{ ptr.get() }}) \
                        else {{ panic!(\"{class_name}.{field_name}: expected Float\") }}; \
                    *f \
                }}"
            ),
        ),
        BamlType::Bool => (
            "Option<bool>".to_string(),
            format!(
                "self.instance.load_field({field_index}).as_bool().unwrap_or_else(|| panic!(\"{class_name}.{field_name}: expected Bool\"))"
            ),
        ),
        BamlType::String => (
            "Option<&'v str>".to_string(),
            format!(
                "vm.as_string(&self.instance.load_field({field_index})).expect(\"{class_name}.{field_name}: expected String\")"
            ),
        ),
        _ => (
            "Option<Value>".to_string(),
            format!("self.instance.load_field({field_index})"),
        ),
    }
}

// ============================================================================
// Copy module emission
// ============================================================================

fn emit_copy_module(out: &mut String, root: &ClassNamespaceNode) {
    out.push_str("#[allow(dead_code, unused_imports, non_snake_case)]\n");
    out.push_str("pub mod copy {\n");
    out.push_str("    use super::*;\n");
    out.push_str("    use std::sync::Arc;\n");
    out.push_str("    use std::any::Any;\n\n");
    emit_copy_namespace_contents(out, root, 1);
    out.push_str("}\n\n");
}

fn emit_copy_namespace_contents(out: &mut String, node: &ClassNamespaceNode, depth: usize) {
    let indent = "    ".repeat(depth);

    for (class_name, class_def) in &node.classes {
        emit_copy_struct(out, class_name, class_def, depth);
    }

    for (ns_name, sub_node) in &node.sub_namespaces {
        writeln!(out, "{indent}pub mod {ns_name} {{").unwrap();
        writeln!(out, "{indent}    use super::super::*;").unwrap();
        writeln!(out, "{indent}    use std::sync::Arc;").unwrap();
        write!(out, "{indent}    use std::any::Any;\n\n").unwrap();
        emit_copy_namespace_contents(out, sub_node, depth + 1);
        writeln!(out, "{indent}}}\n").unwrap();
    }
}

fn emit_copy_struct(out: &mut String, class_name: &str, def: &NativeClassDef, depth: usize) {
    let indent = "    ".repeat(depth);
    let inner = "    ".repeat(depth + 1);
    let inner2 = "    ".repeat(depth + 2);

    // Struct definition with owned fields
    writeln!(out, "{indent}/// Generated from `{}`", def.source_file).unwrap();
    writeln!(out, "{indent}pub struct {class_name} {{").unwrap();
    for field in &def.fields {
        let rust_type = copy_field_type(&field.field_type);
        writeln!(
            out,
            "{inner}pub {}: {rust_type},",
            rust_field_ident(&field.name)
        )
        .unwrap();
    }
    writeln!(out, "{indent}}}\n").unwrap();

    // Impl with to_value()
    let fqn = format!("{}.{}", def.namespace_prefix, def.name);
    writeln!(out, "{indent}impl {class_name} {{").unwrap();
    writeln!(
        out,
        "{inner}pub fn to_value(self, vm: &mut BexVm) -> Value {{"
    )
    .unwrap();
    writeln!(out, "{inner2}let class_ptr = vm.resolve_class({fqn:?});").unwrap();

    // Convert each field to a Value
    for field in &def.fields {
        let value_ident = rust_field_value_ident(&field.name);
        let conversion = copy_field_to_value(
            &rust_field_ident(&field.name).to_string(),
            &field.field_type,
        );
        writeln!(out, "{inner2}let {value_ident} = {conversion};").unwrap();
    }

    // Build the fields vec
    write!(
        out,
        "{inner2}Value::object(vm.alloc_instance(class_ptr, vec!["
    )
    .unwrap();
    for (i, field) in def.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "{}", rust_field_value_ident(&field.name)).unwrap();
    }
    out.push_str("]))\n");
    writeln!(out, "{inner}}}").unwrap();
    writeln!(out, "{indent}}}\n").unwrap();
}

/// Map `BamlType` to the owned Rust type used in copy structs.
fn copy_field_type(ty: &BamlType) -> String {
    match ty {
        BamlType::RustType => "Arc<dyn Any + Send + Sync>".to_string(),
        BamlType::Int => "i64".to_string(),
        BamlType::Bigint => "Arc<num_bigint::BigInt>".to_string(),
        BamlType::Float => "f64".to_string(),
        BamlType::Bool => "bool".to_string(),
        BamlType::Null => "()".to_string(),
        // Heap types stored as Value — caller creates them via vm helpers
        BamlType::String
        | BamlType::Uint8Array
        | BamlType::List(_)
        | BamlType::Map(_, _)
        | BamlType::Optional(_)
        | BamlType::Generic(_)
        | BamlType::Named(_)
        | BamlType::Media(_) => "Value".to_string(),
    }
}

/// Generate the expression to convert a copy struct field to a Value.
fn copy_field_to_value(field_name: &str, ty: &BamlType) -> String {
    match ty {
        BamlType::RustType => format!("Value::object(vm.alloc_rust_data(self.{field_name}))"),
        // `to_value` has no error channel (`fn to_value(self, vm) -> Value`),
        // so an out-of-i63 native i64 reaches this path only when caller-side
        // Rust constructed a struct field that violates the i63 BAML
        // contract. Fail loudly in *both* debug and release rather than
        // truncating silently via `Value::int`'s `debug_assert`.
        BamlType::Int => format!(
            "Value::try_int(self.{field_name}).unwrap_or_else(|| panic!(\
                \"`{field_name}: int` is outside BAML int range [{{}}, {{}}], got {{}}\", \
                Value::INT_MIN, Value::INT_MAX, self.{field_name}))"
        ),
        // Bigints are always heap-allocated, and allocation is fallible (the
        // value may exceed `MAX_BIGINT_BITS`). `to_value` has no error channel,
        // so — like the `int` range case above — fail loudly rather than
        // silently dropping the overflow. The panic reports the bit count from
        // `VmPanic::AllocFailure` (`{p}`), never the bigint itself, which could
        // be millions of digits long.
        BamlType::Bigint => format!(
            "vm.try_alloc_bigint(self.{field_name}).unwrap_or_else(|p| panic!(\
                \"failed to allocate bigint field `{field_name}`: {{p}}\"))"
        ),
        BamlType::Float => format!("Value::object(vm.alloc_float(self.{field_name}))"),
        BamlType::Bool => format!("Value::bool(self.{field_name})"),
        BamlType::Null => "Value::NULL".to_string(),
        // String, List, Map, Optional, Generic, Named, Media — already a Value
        _ => format!("self.{field_name}"),
    }
}

// ============================================================================
// Naming helpers
// ============================================================================

fn to_pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let mut result = c.to_uppercase().to_string();
            result.push_str(chars.as_str());
            result
        }
    }
}

/// Replace characters that are illegal in Rust identifiers with `_`. Synthetic
/// class names for `implement Interface for Type` blocks (e.g. `Equals$for$int`,
/// `Equals$for$map<K, V>`) contain `$`, `<`, `>`, `[`, `]`, `,`, and spaces; the
/// runtime dispatch still matches on the original (unsanitized) string, only the
/// generated Rust identifiers are sanitized.
fn sanitize_ident(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn class_trait_name(namespace_prefix: &str, class_name: &str) -> String {
    let class_name = sanitize_ident(class_name);
    if namespace_prefix.is_empty() {
        format!("BamlClass{class_name}")
    } else {
        let ns_pascal = to_pascal_case(namespace_prefix);
        format!("BamlClass{ns_pascal}{class_name}")
    }
}

fn class_dispatch_name(namespace_prefix: &str, class_name: &str) -> String {
    let class_lower = sanitize_ident(&class_name.to_lowercase());
    if namespace_prefix.is_empty() {
        format!("__dispatch_{class_lower}")
    } else {
        format!("__dispatch_{namespace_prefix}_{class_lower}")
    }
}

/// The root trait name for a stdlib package: `baml` → `BamlPackageBaml`,
/// `reflect` → `BamlPackageReflect`.
fn package_trait_name(package: &str) -> String {
    format!("BamlPackage{}", to_pascal_case(package))
}

fn namespace_trait_name(name: &str) -> String {
    let pascal = to_pascal_case(name);
    format!("BamlNamespace{pascal}")
}

fn namespace_dispatch_name(name: &str) -> String {
    format!("__dispatch_{name}")
}

// ============================================================================
// Public entry point
// ============================================================================

/// Generate Rust source containing the `BamlClass*`, `BamlNamespace*`, and
/// `BamlPackageBaml` trait hierarchy.
///
/// The generated code is intended to be written to a file in `OUT_DIR` and
/// `include!`-ed into `bex_vm/src/native.rs`.
///
/// # Assumptions
///
/// The caller (`native.rs`) is responsible for having the following in scope:
/// - `BexVm` type
/// - `Value`, `IndexMap`, `MediaValue` from `bex_vm_types`
/// - `NativeFunctionResult`, `NativeFunction` type aliases
/// - `VmError`, `VmPanic` from `crate::errors`
/// - `Type` from `bex_vm_types`
/// - `MediaKind` from `baml_type`
pub fn generate_native_trait(builtins: &[NativeBuiltin], class_defs: &[NativeClassDef]) -> String {
    generate_native_trait_for(baml_builtins2::PACKAGE_BAML, builtins, class_defs)
}

/// [`generate_native_trait`] for one stdlib package: emits that package's
/// `BamlPackage<Pascal>` root trait (with its `get_native_fn` dispatcher) over
/// the builtins extracted for it. Adding a `$rust_function` to the package's
/// `.baml` source adds a required trait method, so a missing Rust
/// implementation is a compile error rather than a runtime lookup miss.
pub fn generate_native_trait_for(
    package: &str,
    builtins: &[NativeBuiltin],
    class_defs: &[NativeClassDef],
) -> String {
    let tree = build_namespace_tree(builtins, package);
    let class_tree = build_class_namespace_tree(class_defs, package);
    let mut out = String::new();

    // Emit view and copy modules first (they are referenced by trait signatures later)
    emit_view_module(&mut out, &class_tree);
    emit_copy_module(&mut out, &class_tree);

    emit_subtree_traits(&mut out, &tree, "");
    emit_root_trait(&mut out, &tree, package);

    out
}

/// Recursively emit class traits and namespace traits (bottom-up).
fn emit_subtree_traits(out: &mut String, node: &NamespaceNode, namespace_prefix: &str) {
    for (class_name, entries) in &node.classes {
        let trait_name = class_trait_name(namespace_prefix, class_name);
        let dispatch_name = class_dispatch_name(namespace_prefix, class_name);
        emit_class_trait(out, &trait_name, &dispatch_name, entries);
    }

    for (ns_name, sub_node) in &node.sub_namespaces {
        emit_subtree_traits(out, sub_node, ns_name);
        emit_namespace_trait(out, ns_name, sub_node);
    }
}

// ============================================================================
// Class trait emission
// ============================================================================

fn emit_class_trait(
    out: &mut String,
    trait_name: &str,
    dispatch_name: &str,
    entries: &[BuiltinEntry],
) {
    if let Some(first) = entries.first() {
        writeln!(out, "/// Generated from `{}`", first.builtin.source_file).unwrap();
    }
    writeln!(out, "pub trait {trait_name} {{").unwrap();

    for entry in entries {
        emit_required_method(out, &entry.rust_method_name, entry.builtin);
    }
    out.push('\n');

    for entry in entries {
        emit_glue_method(out, &entry.rust_method_name, entry.builtin);
    }

    writeln!(
        out,
        "    fn {dispatch_name}(method: &str) -> Option<NativeFunction> {{"
    )
    .unwrap();
    out.push_str("        match method {\n");
    for entry in entries {
        writeln!(
            out,
            "            {:?} => Some(Self::__glue_{}),",
            entry.baml_method_name, entry.rust_method_name
        )
        .unwrap();
    }
    out.push_str("            _ => None,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");

    out.push_str("}\n\n");
}

// ============================================================================
// Namespace trait emission
// ============================================================================

fn emit_namespace_trait(out: &mut String, ns_name: &str, node: &NamespaceNode) {
    let trait_name = namespace_trait_name(ns_name);
    let dispatch_name = namespace_dispatch_name(ns_name);

    let mut supertraits: Vec<String> = Vec::new();
    for class_name in node.classes.keys() {
        supertraits.push(class_trait_name(ns_name, class_name));
    }
    for sub_ns in node.sub_namespaces.keys() {
        supertraits.push(namespace_trait_name(sub_ns));
    }

    if supertraits.is_empty() {
        writeln!(out, "pub trait {trait_name} {{").unwrap();
    } else {
        let bounds = supertraits.join(" + ");
        writeln!(out, "pub trait {trait_name}: {bounds} {{").unwrap();
    }

    for entry in &node.free_fns {
        emit_required_method(out, &entry.rust_method_name, entry.builtin);
    }

    if !node.free_fns.is_empty() {
        out.push('\n');
        for entry in &node.free_fns {
            emit_glue_method(out, &entry.rust_method_name, entry.builtin);
        }
    }

    let has_children = !node.classes.is_empty() || !node.sub_namespaces.is_empty();

    writeln!(
        out,
        "    fn {dispatch_name}(rest: &str) -> Option<NativeFunction> {{"
    )
    .unwrap();

    if has_children {
        out.push_str("        match rest.split_once('.') {\n");

        for class_name in node.classes.keys() {
            let child_dispatch = class_dispatch_name(ns_name, class_name);
            writeln!(
                out,
                "            Some(({class_name:?}, method)) => Self::{child_dispatch}(method),"
            )
            .unwrap();
        }

        for sub_ns in node.sub_namespaces.keys() {
            let child_dispatch = namespace_dispatch_name(sub_ns);
            writeln!(
                out,
                "            Some(({sub_ns:?}, rest)) => Self::{child_dispatch}(rest),",
            )
            .unwrap();
        }

        if !node.free_fns.is_empty() {
            out.push_str("            None => match rest {\n");
            for entry in &node.free_fns {
                writeln!(
                    out,
                    "                {:?} => Some(Self::__glue_{}),",
                    entry.baml_method_name, entry.rust_method_name
                )
                .unwrap();
            }
            out.push_str("                _ => None,\n");
            out.push_str("            },\n");
        }

        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
    } else {
        out.push_str("        match rest {\n");
        for entry in &node.free_fns {
            let baml_name = &entry.baml_method_name;
            let rust_name = &entry.rust_method_name;
            writeln!(
                out,
                "            {baml_name:?} => Some(Self::__glue_{rust_name}),",
            )
            .unwrap();
        }
        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
    }

    out.push_str("    }\n");
    out.push_str("}\n\n");
}

// ============================================================================
// Root trait emission (BamlPackageBaml)
// ============================================================================

fn emit_root_trait(out: &mut String, root: &NamespaceNode, package: &str) {
    let trait_name = package_trait_name(package);
    let mut supertraits: Vec<String> = Vec::new();

    for class_name in root.classes.keys() {
        supertraits.push(class_trait_name("", class_name));
    }
    for ns_name in root.sub_namespaces.keys() {
        supertraits.push(namespace_trait_name(ns_name));
    }

    if supertraits.is_empty() {
        writeln!(out, "pub trait {trait_name} {{").unwrap();
    } else {
        let bounds = supertraits.join(" + ");
        writeln!(out, "pub trait {trait_name}: {bounds} {{").unwrap();
    }

    for entry in &root.free_fns {
        emit_required_method(out, &entry.rust_method_name, entry.builtin);
    }

    if !root.free_fns.is_empty() {
        out.push('\n');
        for entry in &root.free_fns {
            emit_glue_method(out, &entry.rust_method_name, entry.builtin);
        }
    }

    out.push_str("    fn get_native_fn(path: &str) -> Option<NativeFunction> {\n");
    writeln!(
        out,
        "        let rest = path.strip_prefix({:?})?;",
        format!("{package}.")
    )
    .unwrap();
    out.push_str("        match rest.split_once('.') {\n");

    for class_name in root.classes.keys() {
        let dispatch = class_dispatch_name("", class_name);
        writeln!(
            out,
            "            Some(({class_name:?}, method)) => Self::{dispatch}(method),",
        )
        .unwrap();
    }

    for ns_name in root.sub_namespaces.keys() {
        let dispatch = namespace_dispatch_name(ns_name);
        writeln!(
            out,
            "            Some(({ns_name:?}, rest)) => Self::{dispatch}(rest),",
        )
        .unwrap();
    }

    if !root.free_fns.is_empty() {
        out.push_str("            None => match rest {\n");
        for entry in &root.free_fns {
            let baml_name = &entry.baml_method_name;
            let rust_name = &entry.rust_method_name;
            writeln!(
                out,
                "                {baml_name:?} => Some(Self::__glue_{rust_name}),",
            )
            .unwrap();
        }
        out.push_str("                _ => None,\n");
        out.push_str("            },\n");
    }

    out.push_str("            _ => None,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");
}

// ============================================================================
// Method emission helpers
// ============================================================================

fn emit_required_method(out: &mut String, method_name: &str, b: &NativeBuiltin) {
    if b.may_yield {
        // Yielding methods return NativeCallResult directly.
        // may_yield implies mut_vm, so always include vm parameter.
        let params = clean_param_list(b);
        writeln!(
            out,
            "    fn {method_name}(vm: &mut BexVm, {params}) -> NativeCallResult;",
        )
        .unwrap();
        return;
    }

    let return_type = clean_return_type(b);
    let params = clean_param_list(b);

    match b.vm_usage {
        VmUsage::None => {
            writeln!(out, "    fn {method_name}({params}) -> {return_type};").unwrap();
        }
        VmUsage::Ref => writeln!(
            out,
            "    fn {method_name}(vm: &BexVm, {params}) -> {return_type};",
        )
        .unwrap(),
        VmUsage::MutRef => writeln!(
            out,
            "    fn {method_name}(vm: &mut BexVm, {params}) -> {return_type};",
        )
        .unwrap(),
    }
}

fn emit_glue_method(out: &mut String, method_name: &str, b: &NativeBuiltin) {
    let glue_name = format!("__glue_{method_name}");
    // When the receiver is `&mut self`, parameter extractions run BEFORE the
    // mutable receiver extraction. If any parameter borrows VM state shared-ly
    // (e.g. `vm.as_array(&args[i])?` for a `T[]` param), that borrow conflicts
    // with the subsequent mutable borrow. Cloning param values up front frees
    // the immutable borrow before the mutable one.
    let receiver_is_mut_self = b
        .receiver
        .as_ref()
        .is_some_and(|r| r.receiver_type.is_mut());
    let needs_owned = matches!(b.vm_usage, VmUsage::MutRef) || b.may_yield || receiver_is_mut_self;
    // Container extractions return read/write guards that borrow `vm`. If
    // the glue function's result conversion needs `&mut vm` (alloc_string /
    // alloc_array / alloc_map / alloc_uint8array), the guard's borrow would
    // conflict — so clone the container up front in those cases. This is a
    // separate flag from `needs_owned` because it must NOT propagate to
    // other extraction paths (the media-class path uses `needs_owned` for
    // its own reasons and switching it on here would break the receiver
    // shape).
    //
    // Also force the owned path when the method reads two or more containers
    // (e.g. `T[].eq(self, other: T[])`): the container lock is a
    // non-reentrant exclusive spin-lock, so holding overlapping read guards
    // deadlocks the moment two operands alias the same heap container
    // (`a.eq(a)`), and recursion that re-locks a held container would too.
    // Snapshotting to owned values releases each lock before the next acquire.
    let arraymap_needs_owned =
        needs_owned || return_type_needs_alloc(&b.return_type) || reads_multiple_containers(b);

    writeln!(
        out,
        "    fn {glue_name}(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {{"
    )
    .unwrap();

    if b.may_yield {
        // Yielding method — returns NativeCallResult directly.
        // Use a closure returning Result<NativeCallResult, VmRustFnError> so `?`
        // operators in arg extractions work (VmInternalError -> VmRustFnError via From),
        // then flatten the result.
        out.push_str("        let __result: Result<NativeCallResult, VmRustFnError> = (|| {\n");
        emit_arg_extractions_indented(out, b, "            ", needs_owned, arraymap_needs_owned);
        let call_args = call_arg_list(b, needs_owned, arraymap_needs_owned);
        writeln!(out, "            Ok(Self::{method_name}(vm, {call_args}))").unwrap();
        out.push_str("        })();\n");
        out.push_str("        match __result {\n");
        out.push_str("            Ok(r) => r,\n");
        out.push_str("            Err(e) => NativeCallResult::Error(e),\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        return;
    }

    let fallible = is_fallible(b);

    // Use a closure returning NativeFunctionResult so `?` operators work inside.
    out.push_str("        let __result: NativeFunctionResult = (|| {\n");

    emit_arg_extractions_indented(out, b, "            ", needs_owned, arraymap_needs_owned);

    let call_args = call_arg_list(b, needs_owned, arraymap_needs_owned);
    let returns_null = matches!(b.return_type, BamlType::Null);

    let binding = if returns_null {
        "            "
    } else {
        "            let result = "
    };
    let suffix = if fallible { "?;\n" } else { ";\n" };

    match b.vm_usage {
        VmUsage::MutRef | VmUsage::Ref => {
            write!(out, "{binding}Self::{method_name}(vm, {call_args}){suffix}").unwrap();
        }
        VmUsage::None => {
            write!(out, "{binding}Self::{method_name}({call_args}){suffix}").unwrap();
        }
    }

    emit_result_conversion_ok(out, b, "            ");

    out.push_str("        })();\n");
    out.push_str("        match __result {\n");
    out.push_str("            Ok(v) => NativeCallResult::Done(v),\n");
    out.push_str("            Err(e) => NativeCallResult::Error(e),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
}

// ============================================================================
// Parameter list and return type helpers
// ============================================================================

/// Build the clean parameter list.
fn clean_param_list(b: &NativeBuiltin) -> String {
    let mut parts: Vec<String> = Vec::new();

    let has_instance_receiver = b
        .receiver
        .as_ref()
        .is_some_and(|r| !r.receiver_type.is_static());

    if has_instance_receiver {
        let recv = b.receiver.as_ref().unwrap();
        parts.push(format!(
            "{}: {}",
            receiver_param_name(recv),
            receiver_input_type_with_vm_usage(recv, b.vm_usage)
        ));
    }
    for p in &b.params {
        parts.push(format!("{}: {}", p.name, baml_type_to_input(&p.ty, false)));
    }

    parts.join(", ")
}

fn clean_return_type(b: &NativeBuiltin) -> String {
    // Static constructors on media classes return copy types
    if b.receiver
        .as_ref()
        .is_none_or(|r| r.receiver_type.is_static())
    {
        if let Some(class_name) = constructor_media_class(b) {
            let ns = constructor_media_namespace(b);
            let inner = format!("copy::{ns}::{class_name}");
            if is_fallible(b) {
                return format!("Result<{inner}, VmRustFnError>");
            }
            return inner;
        }
    }
    let inner = baml_type_to_output(&b.return_type);
    if is_fallible(b) {
        format!("Result<{inner}, VmRustFnError>")
    } else {
        inner
    }
}

/// If this is a static constructor for a media class (no instance receiver, path has a media class segment),
/// return the class name. Used to determine copy return type.
fn constructor_media_class(b: &NativeBuiltin) -> Option<&str> {
    if b.receiver
        .as_ref()
        .is_some_and(|r| !r.receiver_type.is_static())
    {
        return None;
    }
    // Indexed from the end, so this needs no package knowledge: the class is
    // always the segment before the method (`baml.media.Pdf.from_url`).
    let segments: Vec<&str> = b.path.split('.').collect();
    if segments.len() < 3 {
        return None;
    }
    let class_seg = segments[segments.len() - 2];
    // Only uppercase-starting segments are class names
    if !class_seg.starts_with(|c: char| c.is_uppercase()) {
        return None;
    }
    // Only for media classes
    if is_media_class(class_seg) {
        Some(class_seg)
    } else {
        None
    }
}

fn constructor_media_namespace(b: &NativeBuiltin) -> &str {
    // `baml.media.Pdf.from_url` → `media`: the segment before the class,
    // indexed from the end so no package prefix is assumed.
    let segments: Vec<&str> = b.path.split('.').collect();
    if segments.len() >= 4 {
        segments[segments.len() - 3]
    } else {
        ""
    }
}

// ============================================================================
// Argument extraction
// ============================================================================

fn emit_single_extraction_indented(
    out: &mut String,
    name: &str,
    idx: usize,
    ty: &BamlType,
    indent: &str,
    needs_owned: bool,
    arraymap_needs_owned: bool,
) {
    let rhs = extraction_expr(
        &format!("&args[{idx}]"),
        ty,
        false,
        needs_owned,
        arraymap_needs_owned,
    );
    writeln!(out, "{indent}let {name} = {rhs};").unwrap();
}

fn emit_immut_receiver_extraction_indented(
    out: &mut String,
    name: &str,
    idx: usize,
    recv: &Receiver,
    indent: &str,
    needs_owned: bool,
    arraymap_needs_owned: bool,
) {
    match recv.class_name.as_str() {
        // A read array receiver is passed as an `ArrayView` carrying its element
        // type alongside the data. The data is owned (`to_vec`) when the glue
        // later needs `&mut vm` (may_yield / mut_vm), else a cheap read guard;
        // both deref to `&[Value]` so slice-only builtins are unchanged.
        "Array" => {
            writeln!(
                out,
                "{indent}let __{name}_ty = vm.array_element_ty(&args[{idx}]);"
            )
            .unwrap();
            let data_expr = if arraymap_needs_owned {
                format!("vm.as_array(&args[{idx}])?.to_vec()")
            } else {
                format!("vm.as_array(&args[{idx}])?")
            };
            writeln!(out, "{indent}let __{name}_data = {data_expr};").unwrap();
            writeln!(
                out,
                "{indent}let {name} = ArrayView {{ ty: &__{name}_ty, data: &__{name}_data }};"
            )
            .unwrap();
        }
        // A read map receiver is passed as a `MapView` carrying its key/value
        // types alongside the data. The data is owned (`to_index_map`) when the
        // glue later needs `&mut vm` (may_yield / mut_vm), else a cheap read
        // guard; both deref to the `IndexMap` so map-only builtins are unchanged.
        "Map" => {
            writeln!(
                out,
                "{indent}let __{name}_kty = vm.map_key_ty(&args[{idx}]);"
            )
            .unwrap();
            writeln!(
                out,
                "{indent}let __{name}_vty = vm.map_value_ty(&args[{idx}]);"
            )
            .unwrap();
            let data_expr = if arraymap_needs_owned {
                format!("vm.as_map(&args[{idx}])?.to_index_map()")
            } else {
                format!("vm.as_map(&args[{idx}])?")
            };
            writeln!(out, "{indent}let __{name}_data = {data_expr};").unwrap();
            writeln!(
                out,
                "{indent}let {name} = MapView {{ key_ty: &__{name}_kty, value_ty: &__{name}_vty, data: &__{name}_data }};"
            )
            .unwrap();
        }
        cls if is_media_class(cls) => {
            if needs_owned {
                // `//baml:mut_vm` media methods: pass the raw `Value` copy so the
                // `vm` borrow is released before the mutable-vm call.  The view
                // struct (`view::media::Cls`) holds `&Instance` which borrows `vm`
                // and cannot coexist with `&mut BexVm`.
                writeln!(out, "{indent}let {name} = &args[{idx}];").unwrap();
            } else {
                writeln!(
                    out,
                    "{indent}let __instance = vm.as_instance(&args[{idx}])?;"
                )
                .unwrap();
                writeln!(
                    out,
                    "{indent}let {name} = view::media::{cls} {{ instance: __instance }};"
                )
                .unwrap();
            }
        }
        // Other instance-backed receivers (e.g. `time.Instant`): wrap the heap
        // instance in its `view::<ns>::<Class>` struct. `needs_owned` (set for
        // `//baml:mut_vm` methods) falls through to the raw `&Value` path below,
        // matching `receiver_input_type_with_vm_usage`.
        _ if recv.instance_backed && !needs_owned => {
            let view_path = receiver_view_path(recv);
            writeln!(
                out,
                "{indent}let __instance = vm.as_instance(&args[{idx}])?;"
            )
            .unwrap();
            writeln!(
                out,
                "{indent}let {name} = {view_path} {{ instance: __instance }};"
            )
            .unwrap();
        }
        _ => {
            let rhs = receiver_immut_extraction_expr(
                &format!("&args[{idx}]"),
                recv,
                needs_owned,
                arraymap_needs_owned,
            );
            writeln!(out, "{indent}let {name} = {rhs};").unwrap();
        }
    }
}

fn emit_mut_receiver_extraction_indented(
    out: &mut String,
    name: &str,
    recv: &Receiver,
    indent: &str,
    pass_raw_receiver: bool,
) {
    if pass_raw_receiver {
        writeln!(out, "{indent}let {name} = &args[0];").unwrap();
        return;
    }

    let expr = match recv.class_name.as_str() {
        "Array" => "vm.as_array_mut(&args[0])?".to_string(),
        "Map" => "vm.as_map_mut(&args[0])?".to_string(),
        "String" => "vm.as_string_mut(&args[0])?".to_string(),
        "Uint8Array" => "vm.as_uint8array_mut(&args[0])?".to_string(),
        _ => "vm.as_value_mut(&args[0])?".to_string(),
    };
    // Container receivers now return a guard rather than `&mut Vec/IndexMap`.
    // Bind mutably so the call site can take `&mut name` and let the DerefMut
    // impl coerce to the underlying container.
    let mut_kw = match recv.class_name.as_str() {
        "Array" | "Map" | "Uint8Array" => "mut ",
        _ => "",
    };
    writeln!(out, "{indent}let {mut_kw}{name} = {expr};").unwrap();
}

/// Like `emit_arg_extractions` but uses `indent` for each line.
fn emit_arg_extractions_indented(
    out: &mut String,
    b: &NativeBuiltin,
    indent: &str,
    needs_owned: bool,
    arraymap_needs_owned: bool,
) {
    if let Some(recv) = &b.receiver {
        if recv.receiver_type.is_static() {
            // Static methods: no receiver
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i;
                emit_single_extraction_indented(
                    out,
                    &p.name,
                    arg_idx,
                    &p.ty,
                    indent,
                    needs_owned,
                    arraymap_needs_owned,
                );
            }
        } else if recv.receiver_type.is_mut() {
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1;
                emit_single_extraction_indented(
                    out,
                    &p.name,
                    arg_idx,
                    &p.ty,
                    indent,
                    needs_owned,
                    arraymap_needs_owned,
                );
            }
            let recv_name = receiver_param_name(recv);
            emit_mut_receiver_extraction_indented(
                out,
                &recv_name,
                recv,
                indent,
                matches!(b.vm_usage, VmUsage::MutRef) && b.may_yield,
            );
        } else {
            let recv_name = receiver_param_name(recv);
            emit_immut_receiver_extraction_indented(
                out,
                &recv_name,
                0,
                recv,
                indent,
                needs_owned,
                arraymap_needs_owned,
            );
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1;
                emit_single_extraction_indented(
                    out,
                    &p.name,
                    arg_idx,
                    &p.ty,
                    indent,
                    needs_owned,
                    arraymap_needs_owned,
                );
            }
        }
    } else {
        for (i, p) in b.params.iter().enumerate() {
            emit_single_extraction_indented(
                out,
                &p.name,
                i,
                &p.ty,
                indent,
                needs_owned,
                arraymap_needs_owned,
            );
        }
    }
}

// ============================================================================
// Extraction expressions
// ============================================================================

fn receiver_immut_extraction_expr(
    val: &str,
    recv: &Receiver,
    needs_owned: bool,
    arraymap_needs_owned: bool,
) -> String {
    match recv.class_name.as_str() {
        // Clone for container read receivers when the glue function would
        // later need `&mut vm` after extraction (per `arraymap_needs_owned`).
        // Otherwise `vm.as_array(...)?` returns a guard whose lock is
        // released on drop — the cheap path.
        "Array" => {
            if arraymap_needs_owned {
                format!("vm.as_array({val})?.to_vec()")
            } else {
                format!("vm.as_array({val})?")
            }
        }
        "Map" => {
            if arraymap_needs_owned {
                format!("vm.as_map({val})?.to_index_map()")
            } else {
                format!("vm.as_map({val})?")
            }
        }
        "String" => {
            if needs_owned {
                format!("vm.as_string({val})?.clone()")
            } else {
                format!("vm.as_string({val})?")
            }
        }
        "Uint8Array" => {
            if arraymap_needs_owned {
                format!("vm.as_uint8array({val})?.clone()")
            } else {
                format!("vm.as_uint8array({val})?")
            }
        }
        // Primitive value receivers: extract the underlying scalar. `int` is
        // backed by `i64`, `float` by `f64` — both `Copy`, so `needs_owned` is
        // irrelevant. The local `__v: &Value` binding lets `.as_int()` /
        // `.as_object_ptr()` (which take `&self`) resolve without the explicit
        // borrow that would trip `clippy::needless_borrow`.
        "Int" => format!(
            "{{ \
                let __v = {val}; \
                let Some(i) = __v.as_int() \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Int, got: vm.type_of(__v) }}.into()); }}; \
                i \
            }}"
        ),
        "Float" => format!(
            "{{ \
                let __v = {val}; \
                let Some(ptr) = __v.as_object_ptr() \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Float, got: vm.type_of(__v) }}.into()); }}; \
                let bex_vm_types::Object::Float(f) = (unsafe {{ ptr.get() }}) \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Float, got: vm.type_of(__v) }}.into()); }}; \
                *f \
            }}"
        ),
        // Bigint is heap-allocated behind an Arc; always clone the Arc so the
        // trait method receives an owned `Arc<BigInt>` regardless of needs_owned.
        "Bigint" => format!("vm.as_bigint({val})?.clone()"),
        name if is_media_class(name) => {
            let kind = media_kind_expr(&recv.class_name);
            if needs_owned {
                format!("vm.as_media({val}, {kind})?.clone()")
            } else {
                format!("vm.as_media({val}, {kind})?")
            }
        }
        _ => {
            if needs_owned {
                format!("{val}.clone()")
            } else {
                val.to_string()
            }
        }
    }
}

fn extraction_expr(
    val: &str,
    ty: &BamlType,
    is_mut: bool,
    needs_owned: bool,
    arraymap_needs_owned: bool,
) -> String {
    match ty {
        BamlType::String => {
            if is_mut {
                format!("vm.as_string_mut({val})?")
            } else if needs_owned {
                format!("vm.as_string({val})?.clone()")
            } else {
                format!("vm.as_string({val})?")
            }
        }
        // The local `__v: &Value` binding lets the `Value::as_*` methods
        // (which take `&self`) resolve without an explicit borrow that
        // would trip `clippy::needless_borrow`.
        BamlType::Int => format!(
            "{{ \
                let __v = {val}; \
                let Some(i) = __v.as_int() \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Int, got: vm.type_of(__v) }}.into()); }}; \
                i \
            }}"
        ),
        BamlType::Bigint => format!("vm.as_bigint({val})?.clone()"),
        BamlType::Float => format!(
            "{{ \
                let __v = {val}; \
                let Some(ptr) = __v.as_object_ptr() \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Float, got: vm.type_of(__v) }}.into()); }}; \
                let bex_vm_types::Object::Float(f) = (unsafe {{ ptr.get() }}) \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Float, got: vm.type_of(__v) }}.into()); }}; \
                *f \
            }}"
        ),
        BamlType::Bool => format!(
            "{{ \
                let __v = {val}; \
                let Some(b) = __v.as_bool() \
                    else {{ return Err(VmInternalError::TypeError {{ expected: Type::Bool, got: vm.type_of(__v) }}.into()); }}; \
                b \
            }}"
        ),
        BamlType::List(_) => {
            if is_mut {
                format!("vm.as_array_mut({val})?")
            } else if arraymap_needs_owned {
                // `as_array` returns an `ArrayReadGuard` that holds a borrow
                // on `vm` via the container's lazy biased mutex. When the
                // glue function later needs `&mut vm` (e.g. for
                // `vm.alloc_array(result)`), the guard's lifetime would
                // conflict. `arraymap_needs_owned` flags those cases; we
                // then clone into an owned `Vec<Value>` and let the guard
                // drop immediately.
                format!("vm.as_array({val})?.to_vec()")
            } else {
                // No `&mut vm` access after extraction — keep the guard
                // alive (cheap fetch_add/fetch_sub pair, no allocation).
                format!("vm.as_array({val})?")
            }
        }
        BamlType::Map(_, _) => {
            if is_mut {
                format!("vm.as_map_mut({val})?")
            } else if arraymap_needs_owned {
                format!("vm.as_map({val})?.to_index_map()")
            } else {
                format!("vm.as_map({val})?")
            }
        }
        BamlType::Optional(inner) => {
            let inner_expr =
                extraction_expr("other", inner, false, needs_owned, arraymap_needs_owned);
            // Bind `other` so `.is_null()` (Value method) resolves through
            // the `&Value` without triggering `clippy::needless_borrow`.
            // An omitted optional argument arrives as the `OmittedArg`
            // sentinel; treat it the same as an explicit `null` (→ `None`) so
            // the inner extraction (e.g. `as_bool`) never runs on it.
            format!(
                "{{ let other = {val}; if other.is_null() || other.is_omitted() {{ None }} else {{ Some({inner_expr}) }} }}"
            )
        }
        BamlType::Uint8Array => {
            if is_mut {
                format!("vm.as_uint8array_mut({val})?")
            } else if arraymap_needs_owned {
                format!("vm.as_uint8array({val})?.clone()")
            } else {
                format!("vm.as_uint8array({val})?")
            }
        }
        BamlType::Generic(_) => val.to_string(),
        BamlType::Media(name) => {
            let kind = media_kind_expr(name);
            if needs_owned {
                format!("vm.as_media({val}, {kind})?.clone()")
            } else {
                format!("vm.as_media({val}, {kind})?")
            }
        }
        BamlType::Named(_) | BamlType::Null | BamlType::RustType => val.to_string(),
    }
}

fn call_arg_list(b: &NativeBuiltin, needs_owned: bool, arraymap_needs_owned: bool) -> String {
    let mut args: Vec<String> = Vec::new();
    // For guarded containers we use the dedicated flag — these may be owned
    // (cloned Vec/IndexMap) even when other types are passed by reference, because
    // the guard's vm borrow forced an early clone.
    let is_ref = !needs_owned;
    let arraymap_is_ref = !arraymap_needs_owned;

    if let Some(recv) = &b.receiver {
        if !recv.receiver_type.is_static() {
            let name = receiver_param_name(recv);
            if recv.receiver_type.is_mut() {
                // Container mut receivers are guards; take &mut name so
                // DerefMut coerces to the underlying mutable reference.
                let arg = if matches!(b.vm_usage, VmUsage::MutRef) && b.may_yield {
                    name
                } else {
                    match recv.class_name.as_str() {
                        "Array" | "Map" | "Uint8Array" => format!("&mut {name}"),
                        _ => name,
                    }
                };
                args.push(arg);
            } else if needs_owned && is_media_class(recv.class_name.as_str()) {
                // For `//baml:mut_vm` media methods the extraction emits
                // `let pdf = &args[0];` (a `&Value` copy).  Pass `name` directly —
                // it is already the `&Value` the trait method expects.
                args.push(name);
            } else if recv.instance_backed && !needs_owned {
                // Instance-backed receiver extracted as a `view::<ns>::<Class>`
                // value; the trait method takes it by reference.
                args.push(format!("&{name}"));
            } else if recv.class_name == "Array" || recv.class_name == "Map" {
                // A read array/map receiver is materialized as an `ArrayView` /
                // `MapView` value (element or key/value types + data) and the
                // trait method takes it by value.
                args.push(name);
            } else {
                let recv_is_ref = match recv.class_name.as_str() {
                    "Array" | "Map" | "Uint8Array" => arraymap_is_ref,
                    _ => is_ref,
                };
                args.push(call_arg_for_type(
                    &name,
                    &receiver_baml_type(recv),
                    recv_is_ref,
                ));
            }
        }
    }
    for p in &b.params {
        let p_is_ref = match &p.ty {
            BamlType::List(_) | BamlType::Map(_, _) | BamlType::Uint8Array => arraymap_is_ref,
            _ => is_ref,
        };
        args.push(call_arg_for_type(&p.name, &p.ty, p_is_ref));
    }

    args.join(", ")
}

fn call_arg_for_type(name: &str, ty: &BamlType, is_ref: bool) -> String {
    match ty {
        BamlType::List(_) | BamlType::Map(_, _) | BamlType::Uint8Array => {
            // Extraction returns a container guard on the borrowed path and an
            // owned container on the cloned path. In both cases `&name` lets
            // Deref/DerefMut coerce to the trait method's slice/map reference.
            format!("&{name}")
        }
        BamlType::String | BamlType::Media(_) => {
            if is_ref {
                // Extraction already returned a reference — don't double-ref
                name.to_string()
            } else {
                format!("&{name}")
            }
        }
        BamlType::Optional(inner) => {
            if is_ref {
                // Extraction returned Option<&T> — pass through (already the right type)
                match inner.as_ref() {
                    BamlType::Uint8Array => format!("{name}.as_deref().map(Vec::as_slice)"),
                    BamlType::List(_) => format!("{name}.as_deref().map(Vec::as_slice)"),
                    BamlType::Map(_, _) => format!("{name}.as_deref().map(Box::as_ref)"),
                    // Option<&BexStr>, Option<&MediaValue> — already correct
                    _ => name.to_string(),
                }
            } else {
                // Extraction returned Option<T> (owned) — current behavior
                match inner.as_ref() {
                    BamlType::String => format!("{name}.as_ref()"),
                    BamlType::List(_) => format!("{name}.as_deref()"),
                    BamlType::Uint8Array => format!("{name}.as_deref()"),
                    _ => {
                        if call_arg_needs_ref(inner) {
                            format!("{name}.as_ref()")
                        } else {
                            name.to_string()
                        }
                    }
                }
            }
        }
        BamlType::Int | BamlType::Float | BamlType::Bool | BamlType::Null => name.to_string(),
        BamlType::Bigint => name.to_string(),
        // Media class view types (Pdf, Audio, Video, Image) are Named and need to be passed by ref
        BamlType::Named(class_name) if is_media_class(class_name.as_str()) => {
            format!("&{name}")
        }
        BamlType::Generic(_) | BamlType::Named(_) | BamlType::RustType => name.to_string(),
    }
}

fn call_arg_needs_ref(ty: &BamlType) -> bool {
    matches!(
        ty,
        BamlType::String
            | BamlType::Uint8Array
            | BamlType::List(_)
            | BamlType::Map(_, _)
            | BamlType::Media(_)
    )
}

// ============================================================================
// Result conversion
// ============================================================================

/// Emit `Ok(...)` return for the inner closure body with the given indentation.
fn emit_result_conversion_ok(out: &mut String, b: &NativeBuiltin, indent: &str) {
    if b.receiver
        .as_ref()
        .is_none_or(|r| r.receiver_type.is_static())
        && constructor_media_class(b).is_some()
    {
        writeln!(out, "{indent}Ok(result.to_value(vm))").unwrap();
        return;
    }
    // List(String) needs two steps to avoid double-borrow of vm:
    // first map+collect into Value vec, then alloc_array.
    if matches!(&b.return_type, BamlType::List(inner) if matches!(inner.as_ref(), BamlType::String))
    {
        writeln!(
            out,
            "{indent}let result_values: Vec<Value> = result.into_iter().map(|s| Value::object(vm.alloc_string(s))).collect();"
        )
        .unwrap();
        writeln!(
            out,
            "{indent}Ok(Value::object(vm.alloc_array(baml_type::RealizedTy::string(), result_values)))"
        )
        .unwrap();
        return;
    }
    let conversion = result_conversion_expr("result", &b.return_type, &b.generics);
    writeln!(out, "{indent}Ok({conversion})").unwrap();
}

/// Returns `true` if `result_conversion_expr` for this return type emits
/// a `vm.alloc_*` call (which requires `&mut vm`). Used by the glue
/// emitter to decide whether Array/Map parameter extractions must be
/// cloned to release the read-guard's borrow before the result
/// conversion runs.
fn return_type_needs_alloc(ty: &BamlType) -> bool {
    match ty {
        BamlType::String
        | BamlType::Uint8Array
        | BamlType::Bigint
        | BamlType::Float
        | BamlType::List(_)
        | BamlType::Map(_, _) => true,
        BamlType::Optional(inner) => return_type_needs_alloc(inner),
        BamlType::Int
        | BamlType::Bool
        | BamlType::Null
        | BamlType::Generic(_)
        | BamlType::Named(_)
        | BamlType::Media(_)
        | BamlType::RustType => false,
    }
}

/// Returns `true` if this builtin reads two or more heap containers
/// (`Array` / `Map` / `Uint8Array`) via shared receiver/parameter access.
///
/// The per-container lock ([`bex_vm_types::lazy_biased_mutex`]) is a
/// non-reentrant exclusive spin-lock with no shared-read mode, so two
/// `vm.as_array(..)` guards held at once deadlock the instant the operands
/// alias the same container (the natural `a.eq(a)` call). When this returns
/// `true` the glue takes the owned-snapshot path, releasing each container's
/// lock before acquiring the next.
///
/// A mutating receiver is excluded: it already forces the owned path for
/// every parameter (`needs_owned`), and its own `as_*_mut` guard is acquired
/// last, after the snapshots, so no read guard overlaps it.
fn reads_multiple_containers(b: &NativeBuiltin) -> bool {
    // A parameter such as `T[]?` still reads a container under its lock, so an
    // `Optional` wrapper must be seen through. Recursing only collapses the
    // wrapper — it still resolves to a single leaf container, so this neither
    // over- nor under-counts. Receivers are never `Optional`, so the
    // receiver-counting below needs no equivalent.
    fn contains_container(ty: &BamlType) -> bool {
        match ty {
            BamlType::List(_) | BamlType::Map(_, _) | BamlType::Uint8Array => true,
            BamlType::Optional(inner) => contains_container(inner),
            _ => false,
        }
    }
    let receiver_is_mut = b
        .receiver
        .as_ref()
        .is_some_and(|r| r.receiver_type.is_mut());
    if receiver_is_mut {
        return false;
    }
    let receiver_containers = usize::from(
        b.receiver
            .as_ref()
            .filter(|r| !r.receiver_type.is_static())
            .is_some_and(|r| matches!(r.class_name.as_str(), "Array" | "Map" | "Uint8Array")),
    );
    let param_containers = b
        .params
        .iter()
        .filter(|p| contains_container(&p.ty))
        .count();
    receiver_containers + param_containers >= 2
}

/// Emit a Rust expression that constructs the `baml_type::RealizedTy` describing
/// `ty` at VM runtime, used to tag `vm.alloc_array` / `vm.alloc_map`
/// allocations with their declared element/key/value types.
///
/// - Leaf primitives map to their exact `RealizedTy` constructor.
/// - A **generic parameter** resolves to the call's instantiated type argument,
///   read from `vm.current_call_type_args()` at the parameter's declaration
///   index (the call-instruction handler records the leading `LoadType`s there,
///   in `generics` order). This is sound for these non-yielding result
///   conversions: the native body makes no nested call that could overwrite the
///   pending args before the result is built.
/// - **Media / named / `$rust_type`** positions map to their concrete
///   `RealizedTy`.
///
/// `unknown` appears only when a generic name is absent from `generics` or its
/// type argument was not supplied at the call — never as a lazy fallback for a
/// type that is statically recoverable.
fn runtime_ty_expr(ty: &BamlType, generics: &[String]) -> String {
    match ty {
        BamlType::String => "baml_type::RealizedTy::string()".to_string(),
        BamlType::Int => "baml_type::RealizedTy::int()".to_string(),
        BamlType::Bigint => "baml_type::RealizedTy::bigint()".to_string(),
        BamlType::Float => "baml_type::RealizedTy::float()".to_string(),
        BamlType::Bool => "baml_type::RealizedTy::bool()".to_string(),
        BamlType::Null => "baml_type::RealizedTy::null()".to_string(),
        BamlType::Uint8Array => "baml_type::RealizedTy::uint8array()".to_string(),
        BamlType::List(inner) => {
            format!(
                "baml_type::RealizedTy::list({})",
                runtime_ty_expr(inner, generics)
            )
        }
        BamlType::Map(key, value) => format!(
            "baml_type::RealizedTy::map({}, {})",
            runtime_ty_expr(key, generics),
            runtime_ty_expr(value, generics)
        ),
        BamlType::Optional(inner) => {
            format!(
                "baml_type::RealizedTy::optional({})",
                runtime_ty_expr(inner, generics)
            )
        }
        BamlType::Generic(name) => match generics.iter().position(|g| g == name) {
            // The frame's call type args are populated by MIR's
            // receiver-class-type-arg prepend, which fires for `Class`/`List`/
            // `Map` receivers. If a container method is ever reached with the arg
            // absent (an unforeseen dispatch / unknown-typed receiver), degrade to
            // `unknown` rather than aborting the VM — the element type is a
            // best-effort tag, not a correctness invariant worth a hard panic.
            Some(idx) => format!(
                "vm.current_call_type_args().get({idx}).cloned()\
                    .unwrap_or_else(baml_type::RealizedTy::unknown)"
            ),
            None => format!("compile_error!(\"unknown type arg `{name}`\")"),
        },
        BamlType::Media(kind) => format!(
            "baml_type::RealizedTy::Media({}, baml_type::TyAttr::default())",
            media_kind_path(kind)
        ),
        BamlType::RustType => {
            "baml_type::RealizedTy::RustType { attr: baml_type::TyAttr::default() }".to_string()
        }
        // `Named` is a lossy catch-all: the type parser discards a class's
        // generic arguments (`Box<int>` → `Named("Box")`) and also funnels
        // unions/unresolved types through it (`Named("union")`,
        // `Named("unknown")`). Reconstructing `RuntimeTy::class(name)` would both
        // drop generics and fabricate a class for the placeholder names, so this
        // static-`BamlType` path cannot recover a named type — the complete type
        // lives only on the runtime value's stored `element_ty`.
        BamlType::Named(_) => "baml_type::RealizedTy::unknown()".to_string(),
    }
}

/// The `baml_base::MediaKind` path for a `Media(name)` BAML type. Unknown names
/// degrade to the catch-all `Generic` media kind (never an erased type).
fn media_kind_path(name: &str) -> &'static str {
    match name {
        "Image" => "baml_base::MediaKind::Image",
        "Audio" => "baml_base::MediaKind::Audio",
        "Video" => "baml_base::MediaKind::Video",
        "Pdf" => "baml_base::MediaKind::Pdf",
        _ => "baml_base::MediaKind::Generic",
    }
}

/// Emit a Rust expression that converts a native method's return value into a
/// `Value` for the surrounding `Ok({conversion})` line in the glue closure.
///
/// **Invariant**: the closure this expression lands in returns
/// `NativeFunctionResult` (= `Result<Value, VmRustFnError>`, see
/// `bex_vm::package_baml::NativeFunctionResult`), so any `?` inside the
/// emitted expression propagates via `VmRustFnError`. In particular the
/// `Bigint` arm uses `try_alloc_bigint(...)?` so a `VmPanic::AllocFailure`
/// from a too-large bigint is promoted to `VmRustFnError::Panic` via the
/// `#[from]` on `VmRustFnError`.
fn result_conversion_expr(name: &str, ty: &BamlType, generics: &[String]) -> String {
    match ty {
        BamlType::String => format!("Value::object(vm.alloc_string({name}))"),
        BamlType::Uint8Array => format!("Value::object(vm.alloc_uint8array({name}))"),
        // Surface out-of-i63 native returns as a normal VM error instead
        // of silently truncating via the bare `Value::int` constructor's
        // `debug_assert` (which is a no-op in release).
        BamlType::Int => format!(
            "Value::try_int({name}).ok_or_else(|| VmBamlError::InvalidArgument {{ \
                message: format!(\"native int return value {{}} is outside the BAML int range [{{}}, {{}}]\", \
                    {name}, Value::INT_MIN, Value::INT_MAX) \
            }})?"
        ),
        BamlType::Bigint => format!("vm.try_alloc_bigint({name})?"),
        BamlType::Float => format!("Value::object(vm.alloc_float({name}))"),
        BamlType::Bool => format!("Value::bool({name})"),
        BamlType::Null => "Value::NULL".to_string(),
        BamlType::List(inner) => {
            let element_ty = runtime_ty_expr(inner, generics);
            format!("Value::object(vm.alloc_array({element_ty}, {name}))")
        }
        BamlType::Map(key, value) => {
            let key_ty = runtime_ty_expr(key, generics);
            let value_ty = runtime_ty_expr(value, generics);
            format!("Value::object(vm.alloc_map({key_ty}, {value_ty}, {name}))")
        }
        BamlType::Optional(inner) => {
            let inner_conversion = result_conversion_expr("v", inner, generics);
            format!("match {name} {{ Some(v) => {inner_conversion}, None => Value::NULL }}")
        }
        BamlType::Generic(_) | BamlType::Named(_) | BamlType::Media(_) | BamlType::RustType => {
            name.to_string()
        }
    }
}

// ============================================================================
// Type mapping helpers
// ============================================================================

fn baml_type_to_input(ty: &BamlType, is_mut: bool) -> String {
    match ty {
        BamlType::String => {
            if is_mut {
                "&mut bex_str::BexStr".to_string()
            } else {
                "&bex_str::BexStr".to_string()
            }
        }
        BamlType::Int => "i64".to_string(),
        BamlType::Bigint => "std::sync::Arc<num_bigint::BigInt>".to_string(),
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
                "&mut IndexMap<bex_str::BexStr, Value>".to_string()
            } else {
                "&IndexMap<bex_str::BexStr, Value>".to_string()
            }
        }
        BamlType::Optional(inner) => {
            let inner_str = baml_type_to_input(inner, false);
            format!("Option<{inner_str}>")
        }
        BamlType::Uint8Array => "&[u8]".to_string(),
        BamlType::Generic(_) | BamlType::Named(_) | BamlType::RustType => "&Value".to_string(),
        BamlType::Media(_) => {
            if is_mut {
                "&mut MediaValue".to_string()
            } else {
                "&MediaValue".to_string()
            }
        }
    }
}

fn baml_type_to_output(ty: &BamlType) -> String {
    match ty {
        BamlType::String => "bex_str::BexStr".to_string(),
        BamlType::Int => "i64".to_string(),
        BamlType::Bigint => "std::sync::Arc<num_bigint::BigInt>".to_string(),
        BamlType::Float => "f64".to_string(),
        BamlType::Bool => "bool".to_string(),
        BamlType::Null => "()".to_string(),
        BamlType::List(inner) => match inner.as_ref() {
            BamlType::String => "Vec<bex_str::BexStr>".to_string(),
            _ => "Vec<Value>".to_string(),
        },
        BamlType::Map(_, _) => "IndexMap<bex_str::BexStr, Value>".to_string(),
        BamlType::Optional(inner) => {
            let inner_str = baml_type_to_output(inner);
            format!("Option<{inner_str}>")
        }
        BamlType::Uint8Array => "Vec<u8>".to_string(),
        BamlType::Generic(_) | BamlType::Named(_) | BamlType::Media(_) | BamlType::RustType => {
            "Value".to_string()
        }
    }
}

// ============================================================================
// Receiver helpers
// ============================================================================

fn receiver_param_name(recv: &Receiver) -> String {
    recv.class_name.to_lowercase()
}

/// Path to the generated `view::` struct for an instance-backed receiver, e.g.
/// `view::time::Instant` or (for a root-level class) `view::Foo`.
fn receiver_view_path(recv: &Receiver) -> String {
    if recv.namespace.is_empty() {
        format!("view::{}", recv.class_name)
    } else {
        format!(
            "view::{}::{}",
            recv.namespace.replace('.', "::"),
            recv.class_name
        )
    }
}

#[allow(dead_code)]
fn receiver_input_type(recv: &Receiver) -> String {
    receiver_input_type_with_vm_usage(recv, VmUsage::None)
}

/// Like `receiver_input_type` but switches media class receivers to `&Value`
/// when `vm_usage == MutRef` — the `view::media::Cls<'_>` view struct holds a
/// `&Instance` borrowed from `vm`, which would conflict with the `&mut BexVm`
/// parameter required for mutating-VM methods.  Passing the raw `Value` (which
/// is `Copy`) instead avoids the split-borrow.
fn receiver_input_type_with_vm_usage(recv: &Receiver, vm_usage: VmUsage) -> String {
    match recv.class_name.as_str() {
        "Array" => {
            if recv.receiver_type.is_mut() && matches!(vm_usage, VmUsage::MutRef) {
                "&Value".to_string()
            } else if recv.receiver_type.is_mut() {
                "&mut Vec<Value>".to_string()
            } else {
                // A read receiver carries its element type via `ArrayView`, which
                // derefs to `[Value]` so slice-only builtins are unaffected.
                "ArrayView<'_>".to_string()
            }
        }
        "Map" => {
            if recv.receiver_type.is_mut() && matches!(vm_usage, VmUsage::MutRef) {
                "&Value".to_string()
            } else if recv.receiver_type.is_mut() {
                "&mut IndexMap<bex_str::BexStr, Value>".to_string()
            } else {
                // A read receiver carries its key/value types via `MapView`,
                // which derefs to `IndexMap` so map-only builtins are unaffected.
                "MapView<'_>".to_string()
            }
        }
        "String" => {
            if recv.receiver_type.is_mut() && matches!(vm_usage, VmUsage::MutRef) {
                "&Value".to_string()
            } else if recv.receiver_type.is_mut() {
                "&mut bex_str::BexStr".to_string()
            } else {
                "&bex_str::BexStr".to_string()
            }
        }
        "Uint8Array" => {
            if recv.receiver_type.is_mut() && matches!(vm_usage, VmUsage::MutRef) {
                "&Value".to_string()
            } else if recv.receiver_type.is_mut() {
                "&mut Vec<u8>".to_string()
            } else {
                "&[u8]".to_string()
            }
        }
        // Primitive value receivers: pass by value, since the underlying type
        // (`i64` / `f64` / `bool`) is `Copy` and that's the natural Rust idiom.
        "Int" => "i64".to_string(),
        "Float" => "f64".to_string(),
        // Bigint is heap-allocated; the glue extracts and clones the Arc so the
        // trait method receives an owned `Arc<num_bigint::BigInt>`.
        "Bigint" => "std::sync::Arc<num_bigint::BigInt>".to_string(),
        name if is_media_class(name) => {
            // For `//baml:mut_vm` methods the view struct cannot coexist with
            // `&mut BexVm` (split-borrow).  Use the raw `Value` instead so the
            // trait impl can extract / clone what it needs from `vm` before the
            // mutable allocation calls.
            if matches!(vm_usage, VmUsage::MutRef) {
                "&Value".to_string()
            } else {
                match name {
                    "Pdf" => "&view::media::Pdf<'_>".to_string(),
                    "Audio" => "&view::media::Audio<'_>".to_string(),
                    "Video" => "&view::media::Video<'_>".to_string(),
                    "Image" => "&view::media::Image<'_>".to_string(),
                    _ => "&Value".to_string(),
                }
            }
        }
        // Other instance-backed classes (e.g. `time.Instant`) are passed as a
        // borrowed `view::<ns>::<Class>` for typed field access. As with media,
        // `//baml:mut_vm` methods fall back to the `Copy` `&Value` because the
        // view's `&Instance` borrow can't coexist with `&mut BexVm`. Opaque
        // classes and primitives stay type-erased.
        _ if recv.instance_backed && !matches!(vm_usage, VmUsage::MutRef) => {
            format!("&{}<'_>", receiver_view_path(recv))
        }
        _ => "&Value".to_string(),
    }
}

fn receiver_baml_type(recv: &Receiver) -> BamlType {
    match recv.class_name.as_str() {
        "Array" => BamlType::List(Box::new(BamlType::Generic("T".to_string()))),
        "Map" => BamlType::Map(
            Box::new(BamlType::Generic("K".to_string())),
            Box::new(BamlType::Generic("V".to_string())),
        ),
        "String" => BamlType::String,
        "Uint8Array" => BamlType::Uint8Array,
        "Int" => BamlType::Int,
        "Bigint" => BamlType::Bigint,
        "Float" => BamlType::Float,
        "Pdf" | "Audio" | "Video" | "Image" => BamlType::Named(recv.class_name.clone()),
        _ => BamlType::Named(recv.class_name.clone()),
    }
}

fn is_media_class(name: &str) -> bool {
    matches!(name, "Pdf" | "Audio" | "Video" | "Image")
}

fn media_kind_expr(class_name: &str) -> String {
    match class_name {
        "Pdf" => "MediaKind::Pdf".to_string(),
        "Audio" => "MediaKind::Audio".to_string(),
        "Video" => "MediaKind::Video".to_string(),
        "Image" => "MediaKind::Image".to_string(),
        _ => "MediaKind::Generic".to_string(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_native_builtins;

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("toLowerCase"), "to_lower_case");
        assert_eq!(camel_to_snake("toUpperCase"), "to_upper_case");
        assert_eq!(camel_to_snake("startsWith"), "starts_with");
        assert_eq!(camel_to_snake("endsWith"), "ends_with");
        assert_eq!(camel_to_snake("indexOf"), "index_of");
        assert_eq!(camel_to_snake("charAt"), "char_at");
        assert_eq!(camel_to_snake("replaceAll"), "replace_all");
        assert_eq!(camel_to_snake("length"), "length");
        assert_eq!(camel_to_snake("from_url"), "from_url");
        assert_eq!(camel_to_snake("mime_type"), "mime_type");
        assert_eq!(camel_to_snake("deep_copy"), "deep_copy");
    }

    #[test]
    fn test_generate_produces_class_traits() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("pub trait BamlClassArray"),
            "missing BamlClassArray trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassMap"),
            "missing BamlClassMap trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassString"),
            "missing BamlClassString trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassMediaPdf"),
            "missing BamlClassMediaPdf trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassMediaAudio"),
            "missing BamlClassMediaAudio trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassMediaVideo"),
            "missing BamlClassMediaVideo trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlClassMediaImage"),
            "missing BamlClassMediaImage trait:\n{output}"
        );
    }

    #[test]
    fn test_generate_produces_namespace_traits() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("pub trait BamlNamespaceMedia"),
            "missing BamlNamespaceMedia trait:\n{output}"
        );
    }

    #[test]
    fn test_generate_produces_root_trait() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("pub trait BamlPackageBaml"),
            "missing BamlPackageBaml trait:\n{output}"
        );
        assert!(
            output.contains("fn get_native_fn(path: &str)"),
            "missing get_native_fn in output:\n{output}"
        );
    }

    #[test]
    fn test_bare_method_names_on_class_traits() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn length(array: ArrayView<'_>) -> i64;"),
            "BamlClassArray should have bare `length` method:\n{output}"
        );
        assert!(
            output.contains("fn to_lower_case(string: &bex_str::BexStr) -> bex_str::BexStr;"),
            "BamlClassString should have bare `to_lower_case` method:\n{output}"
        );
        assert!(
            output.contains("fn trunc(float: f64) -> f64;"),
            "BamlClassFloat should have bare `trunc` method:\n{output}"
        );
    }

    #[test]
    fn test_glue_methods_present() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn __glue_length(vm: &mut BexVm, args: &[Value])"),
            "missing __glue_length:\n{output}"
        );
        assert!(
            output.contains("fn __glue_to_lower_case(vm: &mut BexVm, args: &[Value])"),
            "missing __glue_to_lower_case:\n{output}"
        );
        assert!(
            output.contains("fn __glue_trunc(vm: &mut BexVm, args: &[Value])"),
            "missing __glue_trunc:\n{output}"
        );
    }

    #[test]
    fn test_dispatch_methods_present() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn __dispatch_array(method: &str)"),
            "missing __dispatch_array:\n{output}"
        );
        assert!(
            output.contains("fn __dispatch_map(method: &str)"),
            "missing __dispatch_map:\n{output}"
        );
        assert!(
            output.contains("fn __dispatch_string(method: &str)"),
            "missing __dispatch_string:\n{output}"
        );
        assert!(
            output.contains("fn __dispatch_media(rest: &str)"),
            "missing __dispatch_media:\n{output}"
        );
        assert!(
            output.contains("fn __dispatch_media_pdf(method: &str)"),
            "missing __dispatch_media_pdf:\n{output}"
        );
    }

    #[test]
    fn test_root_dispatches_to_children() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("(\"Array\", method)) => Self::__dispatch_array(method)"),
            "get_native_fn should dispatch Array to __dispatch_array:\n{output}"
        );
        assert!(
            output.contains("(\"media\", rest)) => Self::__dispatch_media(rest)"),
            "get_native_fn should dispatch media to __dispatch_media:\n{output}"
        );
    }

    #[test]
    fn test_root_free_fns_on_baml_package() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn deep_copy(vm: &mut BexVm, value: &Value) -> Value;"),
            "BamlPackageBaml should have deep_copy:\n{output}"
        );
    }

    #[test]
    fn test_array_push_mut_receiver() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn push(array: &mut Vec<Value>, item: &Value)"),
            "Array.push should have &mut Vec<Value> receiver:\n{output}"
        );
        assert!(
            !output.contains("fn push(vm: &mut BexVm,"),
            "Array.push should NOT have vm as first param:\n{output}"
        );
    }

    #[test]
    fn test_vm_param_matches_vm_usage() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        for b in &builtins {
            let rest = b.path.strip_prefix("baml.").unwrap_or(&b.path);
            let segments: Vec<&str> = rest.split('.').collect();
            let baml_name = segments.last().unwrap();
            let name = camel_to_snake(baml_name);
            let has_mut_receiver = b
                .receiver
                .as_ref()
                .is_some_and(|r| r.receiver_type.is_mut());

            // Build the expected full signature — params AND return type —
            // to avoid false matches when multiple classes have a method
            // with the same name but different VmUsage (e.g.
            // uint8array.to_string vs errors.StackTrace.to_string). The
            // return type matters for zero-param methods, where the params
            // alone cannot disambiguate (e.g. spawn.CancelToken.new(vm,) vs
            // baml.id.new()).
            let params = clean_param_list(b);
            let ret = if b.may_yield {
                "NativeCallResult".to_string()
            } else {
                clean_return_type(b)
            };
            let has_mut_vm =
                output.contains(&format!("fn {name}(vm: &mut BexVm, {params}) -> {ret};"));
            let has_ref_vm = output.contains(&format!("fn {name}(vm: &BexVm, {params}) -> {ret};"));

            if has_mut_receiver && b.may_yield {
                assert!(
                    has_mut_vm && !has_ref_vm,
                    "yielding mutable method {name} should have vm: &mut BexVm"
                );
            } else if has_mut_receiver {
                assert!(
                    !has_mut_vm && !has_ref_vm,
                    "method {name} should NOT have vm param (mutable receiver)"
                );
            } else {
                match b.vm_usage {
                    VmUsage::MutRef => {
                        assert!(has_mut_vm, "method {name} should have vm: &mut BexVm");
                    }
                    VmUsage::Ref => assert!(has_ref_vm, "method {name} should have vm: &BexVm"),
                    VmUsage::None => assert!(
                        !has_mut_vm && !has_ref_vm,
                        "method {name} should NOT have vm param"
                    ),
                }
            }
        }
    }

    #[test]
    fn test_namespace_media_aggregates_classes() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains(
                "pub trait BamlNamespaceMedia: BamlClassMediaAudio + BamlClassMediaImage + BamlClassMediaPdf + BamlClassMediaVideo"
            ),
            "BamlNamespaceMedia should have child class supertraits:\n{output}"
        );
    }

    #[test]
    fn test_media_dispatch_routes_to_class_dispatchers() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("(\"Pdf\", method)) => Self::__dispatch_media_pdf(method)"),
            "__dispatch_media should route Pdf to __dispatch_media_pdf:\n{output}"
        );
        assert!(
            output.contains("(\"Audio\", method)) => Self::__dispatch_media_audio(method)"),
            "__dispatch_media should route Audio to __dispatch_media_audio:\n{output}"
        );
    }

    #[test]
    fn test_static_constructors_on_class_trait() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Pdf;"),
            "BamlClassMediaPdf should have from_url static constructor returning copy::media::Pdf:\n{output}"
        );
    }

    #[test]
    fn test_view_module_generated() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(output.contains("pub mod view"), "missing view module");
        assert!(output.contains("pub mod copy"), "missing copy module");
    }

    #[test]
    fn test_view_media_pdf_struct() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        assert!(
            output.contains("pub struct Pdf<'a>"),
            "missing Pdf view struct:\n{output}"
        );
        assert!(
            output.contains("pub instance: &'a Instance"),
            "Pdf view should hold &Instance:\n{output}"
        );
        // _data accessor should be generic with downcast
        assert!(
            output.contains("fn _data<'v, T: 'static>"),
            "Pdf._data should be generic downcast accessor:\n{output}"
        );
    }

    #[test]
    fn test_copy_media_pdf_struct() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        // copy::media::Pdf should have _data: Arc<dyn Any + Send + Sync>
        assert!(
            output.contains("Arc<dyn Any + Send + Sync>"),
            "copy struct should have Arc<dyn Any> field:\n{output}"
        );
        // to_value method
        assert!(
            output.contains("fn to_value(self, vm: &mut BexVm) -> Value"),
            "copy struct should have to_value method:\n{output}"
        );
    }

    #[test]
    fn test_view_namespace_structure() {
        let (builtins, _io_builtins, class_defs) = extract_native_builtins().unwrap();
        let output = generate_native_trait(&builtins, &class_defs);

        // Check namespace sub-modules exist
        assert!(
            output.contains("pub mod media"),
            "missing media namespace in view"
        );
        assert!(
            output.contains("pub mod errors")
                || !class_defs
                    .iter()
                    .any(|c| c.namespace_prefix == "baml.errors"),
            "missing errors namespace in view (if error classes exist)"
        );
    }
}
