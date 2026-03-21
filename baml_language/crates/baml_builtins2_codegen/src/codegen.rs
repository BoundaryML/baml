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

use std::collections::BTreeMap;

use crate::types::{BamlType, NativeBuiltin, NativeClassDef, Receiver, VmUsage};

// ============================================================================
// Fallibility
// ============================================================================

/// Returns `true` if the clean trait method for this path should return
/// `Result<T, VmError>` instead of plain `T`.
fn is_fallible(path: &str) -> bool {
    path.starts_with("baml.unstable.")
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

impl<'a> NamespaceNode<'a> {
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

impl<'a> ClassNamespaceNode<'a> {
    fn new() -> Self {
        Self {
            classes: BTreeMap::new(),
            sub_namespaces: BTreeMap::new(),
        }
    }
}

fn build_class_namespace_tree<'a>(class_defs: &'a [NativeClassDef]) -> ClassNamespaceNode<'a> {
    let mut root = ClassNamespaceNode::new();
    for def in class_defs {
        // namespace_prefix is e.g. "baml.media", "baml.errors", "baml"
        let rest = def.namespace_prefix.strip_prefix("baml.").unwrap_or("");
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
/// - `baml.math.trunc` → namespace `math`, free function `trunc`
fn build_namespace_tree<'a>(builtins: &'a [NativeBuiltin]) -> NamespaceNode<'a> {
    let mut root = NamespaceNode::new();

    for b in builtins {
        let rest = b.path.strip_prefix("baml.").unwrap_or(&b.path);
        let segments: Vec<&str> = rest.split('.').collect();
        let baml_method_name = segments.last().unwrap().to_string();
        let rust_method_name = camel_to_snake(&baml_method_name);

        let entry = BuiltinEntry {
            builtin: b,
            baml_method_name,
            rust_method_name,
        };

        let prefix_segments = &segments[..segments.len() - 1];

        let mut ns_segments: Vec<&str> = Vec::new();
        let mut class_name: Option<&str> = None;

        for &seg in prefix_segments {
            if seg.starts_with(|c: char| c.is_uppercase()) {
                class_name = Some(seg);
                break;
            } else {
                ns_segments.push(seg);
            }
        }

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
        out.push_str(&format!("{indent}pub mod {ns_name} {{\n"));
        out.push_str(&format!("{indent}    use super::super::*;\n\n"));
        emit_view_namespace_contents(out, sub_node, depth + 1);
        out.push_str(&format!("{indent}}}\n\n"));
    }
}

fn emit_view_struct(out: &mut String, class_name: &str, def: &NativeClassDef, depth: usize) {
    let indent = "    ".repeat(depth);
    let inner = "    ".repeat(depth + 1);
    let inner2 = "    ".repeat(depth + 2);

    // Struct definition
    out.push_str(&format!("{indent}pub struct {class_name}<'a> {{\n"));
    out.push_str(&format!("{inner}pub instance: &'a Instance,\n"));
    out.push_str(&format!("{indent}}}\n\n"));

    // Impl block with typed accessors
    out.push_str(&format!("{indent}impl<'a> {class_name}<'a> {{\n"));

    for field in &def.fields {
        let field_name = &field.name;
        match &field.field_type {
            BamlType::RustType => {
                // Generic downcast accessor: fn _data<T: 'static>(&self, vm: &BexVm) -> &T
                out.push_str(&format!(
                    "{inner}pub fn {field_name}<'v, T: 'static>(&self, vm: &'v BexVm) -> &'v T {{\n"
                ));
                out.push_str(&format!(
                    "{inner2}vm.as_rust_data::<T>(&self.instance.fields[{}])\n",
                    field.index
                ));
                out.push_str(&format!(
                    "{inner2}    .expect(\"{class_name}.{field_name}: downcast failed\")\n"
                ));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::Int => {
                out.push_str(&format!("{inner}pub fn {field_name}(&self) -> i64 {{\n"));
                out.push_str(&format!(
                    "{inner2}match self.instance.fields[{}] {{\n",
                    field.index
                ));
                out.push_str(&format!("{inner2}    Value::Int(i) => i,\n"));
                out.push_str(&format!(
                    "{inner2}    _ => panic!(\"{class_name}.{field_name}: expected Int\"),\n"
                ));
                out.push_str(&format!("{inner2}}}\n"));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::Float => {
                out.push_str(&format!("{inner}pub fn {field_name}(&self) -> f64 {{\n"));
                out.push_str(&format!(
                    "{inner2}match self.instance.fields[{}] {{\n",
                    field.index
                ));
                out.push_str(&format!("{inner2}    Value::Float(f) => f,\n"));
                out.push_str(&format!(
                    "{inner2}    _ => panic!(\"{class_name}.{field_name}: expected Float\"),\n"
                ));
                out.push_str(&format!("{inner2}}}\n"));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::Bool => {
                out.push_str(&format!("{inner}pub fn {field_name}(&self) -> bool {{\n"));
                out.push_str(&format!(
                    "{inner2}match self.instance.fields[{}] {{\n",
                    field.index
                ));
                out.push_str(&format!("{inner2}    Value::Bool(b) => b,\n"));
                out.push_str(&format!(
                    "{inner2}    _ => panic!(\"{class_name}.{field_name}: expected Bool\"),\n"
                ));
                out.push_str(&format!("{inner2}}}\n"));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::String => {
                // Heap type — vm parameter needed
                out.push_str(&format!(
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> &'v str {{\n"
                ));
                out.push_str(&format!(
                    "{inner2}vm.as_string(&self.instance.fields[{}])\n",
                    field.index
                ));
                out.push_str(&format!(
                    "{inner2}    .expect(\"{class_name}.{field_name}: expected String\")\n"
                ));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::List(_) => {
                out.push_str(&format!(
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> &'v [Value] {{\n"
                ));
                out.push_str(&format!(
                    "{inner2}vm.as_array(&self.instance.fields[{}])\n",
                    field.index
                ));
                out.push_str(&format!(
                    "{inner2}    .expect(\"{class_name}.{field_name}: expected Array\")\n"
                ));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::Map(_, _) => {
                out.push_str(&format!(
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> &'v IndexMap<String, Value> {{\n"
                ));
                out.push_str(&format!(
                    "{inner2}vm.as_map(&self.instance.fields[{}])\n",
                    field.index
                ));
                out.push_str(&format!(
                    "{inner2}    .expect(\"{class_name}.{field_name}: expected Map\")\n"
                ));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            BamlType::Optional(inner_ty) => {
                // For optional fields, return Option<T> with appropriate accessor
                let (ret_type, some_expr) =
                    view_optional_type_and_expr(class_name, field_name, inner_ty, field.index);
                out.push_str(&format!(
                    "{inner}pub fn {field_name}<'v>(&self, vm: &'v BexVm) -> {ret_type} {{\n"
                ));
                out.push_str(&format!(
                    "{inner2}match self.instance.fields[{}] {{\n",
                    field.index
                ));
                out.push_str(&format!("{inner2}    Value::Null => None,\n"));
                out.push_str(&format!("{inner2}    _ => Some({some_expr}),\n"));
                out.push_str(&format!("{inner2}}}\n"));
                out.push_str(&format!("{inner}}}\n\n"));
            }
            // Generic, Named, Media, Null — fallback to &Value
            _ => {
                out.push_str(&format!("{inner}pub fn {field_name}(&self) -> &Value {{\n"));
                out.push_str(&format!("{inner2}&self.instance.fields[{}]\n", field.index));
                out.push_str(&format!("{inner}}}\n\n"));
            }
        }
    }

    out.push_str(&format!("{indent}}}\n\n"));
}

/// Returns (return_type, some_expression) for Optional field view accessors.
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
                "match self.instance.fields[{field_index}] {{ Value::Int(i) => i, _ => panic!(\"{class_name}.{field_name}: expected Int\") }}"
            ),
        ),
        BamlType::Float => (
            "Option<f64>".to_string(),
            format!(
                "match self.instance.fields[{field_index}] {{ Value::Float(f) => f, _ => panic!(\"{class_name}.{field_name}: expected Float\") }}"
            ),
        ),
        BamlType::Bool => (
            "Option<bool>".to_string(),
            format!(
                "match self.instance.fields[{field_index}] {{ Value::Bool(b) => b, _ => panic!(\"{class_name}.{field_name}: expected Bool\") }}"
            ),
        ),
        BamlType::String => (
            "Option<&'v str>".to_string(),
            format!(
                "vm.as_string(&self.instance.fields[{field_index}]).expect(\"{class_name}.{field_name}: expected String\")"
            ),
        ),
        _ => (
            "Option<&Value>".to_string(),
            format!("&self.instance.fields[{field_index}]"),
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
        out.push_str(&format!("{indent}pub mod {ns_name} {{\n"));
        out.push_str(&format!("{indent}    use super::super::*;\n"));
        out.push_str(&format!("{indent}    use std::sync::Arc;\n"));
        out.push_str(&format!("{indent}    use std::any::Any;\n\n"));
        emit_copy_namespace_contents(out, sub_node, depth + 1);
        out.push_str(&format!("{indent}}}\n\n"));
    }
}

fn emit_copy_struct(out: &mut String, class_name: &str, def: &NativeClassDef, depth: usize) {
    let indent = "    ".repeat(depth);
    let inner = "    ".repeat(depth + 1);
    let inner2 = "    ".repeat(depth + 2);

    // Struct definition with owned fields
    out.push_str(&format!("{indent}pub struct {class_name} {{\n"));
    for field in &def.fields {
        let rust_type = copy_field_type(&field.field_type);
        out.push_str(&format!("{inner}pub {}: {rust_type},\n", field.name));
    }
    out.push_str(&format!("{indent}}}\n\n"));

    // Impl with to_value()
    let fqn = format!("{}.{}", def.namespace_prefix, def.name);
    out.push_str(&format!("{indent}impl {class_name} {{\n"));
    out.push_str(&format!(
        "{inner}pub fn to_value(self, vm: &mut BexVm) -> Value {{\n"
    ));
    out.push_str(&format!(
        "{inner2}let class_ptr = vm.resolve_class({fqn:?});\n"
    ));

    // Convert each field to a Value
    for field in &def.fields {
        let conversion = copy_field_to_value(&field.name, &field.field_type);
        out.push_str(&format!("{inner2}let f_{} = {conversion};\n", field.name));
    }

    // Build the fields vec
    out.push_str(&format!("{inner2}vm.alloc_instance(class_ptr, vec!["));
    for (i, field) in def.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("f_{}", field.name));
    }
    out.push_str("])\n");
    out.push_str(&format!("{inner}}}\n"));
    out.push_str(&format!("{indent}}}\n\n"));
}

/// Map BamlType to the owned Rust type used in copy structs.
fn copy_field_type(ty: &BamlType) -> String {
    match ty {
        BamlType::RustType => "Arc<dyn Any + Send + Sync>".to_string(),
        BamlType::Int => "i64".to_string(),
        BamlType::Float => "f64".to_string(),
        BamlType::Bool => "bool".to_string(),
        BamlType::Null => "()".to_string(),
        // Heap types stored as Value — caller creates them via vm helpers
        BamlType::String
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
        BamlType::RustType => format!("vm.alloc_rust_data(self.{field_name})"),
        BamlType::Int => format!("Value::Int(self.{field_name})"),
        BamlType::Float => format!("Value::Float(self.{field_name})"),
        BamlType::Bool => format!("Value::Bool(self.{field_name})"),
        BamlType::Null => "Value::Null".to_string(),
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

fn class_trait_name(namespace_prefix: &str, class_name: &str) -> String {
    if namespace_prefix.is_empty() {
        format!("BamlClass{class_name}")
    } else {
        let ns_pascal = to_pascal_case(namespace_prefix);
        format!("BamlClass{ns_pascal}{class_name}")
    }
}

fn class_dispatch_name(namespace_prefix: &str, class_name: &str) -> String {
    let class_lower = class_name.to_lowercase();
    if namespace_prefix.is_empty() {
        format!("__dispatch_{class_lower}")
    } else {
        format!("__dispatch_{namespace_prefix}_{class_lower}")
    }
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
/// - `VmError`, `InternalError`, `RuntimeError` from `crate::errors`
/// - `Type` from `bex_vm_types`
/// - `MediaKind` from `baml_type`
pub fn generate_native_trait(builtins: &[NativeBuiltin], class_defs: &[NativeClassDef]) -> String {
    let tree = build_namespace_tree(builtins);
    let class_tree = build_class_namespace_tree(class_defs);
    let mut out = String::new();

    // Emit view and copy modules first (they are referenced by trait signatures later)
    emit_view_module(&mut out, &class_tree);
    emit_copy_module(&mut out, &class_tree);

    emit_subtree_traits(&mut out, &tree, "");
    emit_root_trait(&mut out, &tree);

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
    out.push_str(&format!("pub trait {trait_name} {{\n"));

    for entry in entries {
        emit_required_method(out, &entry.rust_method_name, entry.builtin);
    }
    out.push('\n');

    for entry in entries {
        emit_glue_method(out, &entry.rust_method_name, entry.builtin);
    }

    out.push_str(&format!(
        "    fn {dispatch_name}(method: &str) -> Option<NativeFunction> {{\n"
    ));
    out.push_str("        match method {\n");
    for entry in entries {
        out.push_str(&format!(
            "            {:?} => Some(Self::__glue_{}),\n",
            entry.baml_method_name, entry.rust_method_name
        ));
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
        out.push_str(&format!("pub trait {trait_name} {{\n"));
    } else {
        let bounds = supertraits.join(" + ");
        out.push_str(&format!("pub trait {trait_name}: {bounds} {{\n"));
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

    out.push_str(&format!(
        "    fn {dispatch_name}(rest: &str) -> Option<NativeFunction> {{\n"
    ));

    if has_children {
        out.push_str("        match rest.split_once('.') {\n");

        for (class_name, _) in &node.classes {
            let child_dispatch = class_dispatch_name(ns_name, class_name);
            out.push_str(&format!(
                "            Some(({:?}, method)) => Self::{child_dispatch}(method),\n",
                class_name
            ));
        }

        for (sub_ns, _) in &node.sub_namespaces {
            let child_dispatch = namespace_dispatch_name(sub_ns);
            out.push_str(&format!(
                "            Some(({:?}, rest)) => Self::{child_dispatch}(rest),\n",
                sub_ns
            ));
        }

        if !node.free_fns.is_empty() {
            out.push_str("            None => match rest {\n");
            for entry in &node.free_fns {
                out.push_str(&format!(
                    "                {:?} => Some(Self::__glue_{}),\n",
                    entry.baml_method_name, entry.rust_method_name
                ));
            }
            out.push_str("                _ => None,\n");
            out.push_str("            },\n");
        }

        out.push_str("            _ => None,\n");
        out.push_str("        }\n");
    } else {
        out.push_str("        match rest {\n");
        for entry in &node.free_fns {
            out.push_str(&format!(
                "            {:?} => Some(Self::__glue_{}),\n",
                entry.baml_method_name, entry.rust_method_name
            ));
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

fn emit_root_trait(out: &mut String, root: &NamespaceNode) {
    let mut supertraits: Vec<String> = Vec::new();

    for class_name in root.classes.keys() {
        supertraits.push(class_trait_name("", class_name));
    }
    for ns_name in root.sub_namespaces.keys() {
        supertraits.push(namespace_trait_name(ns_name));
    }

    if supertraits.is_empty() {
        out.push_str("pub trait BamlPackageBaml {\n");
    } else {
        let bounds = supertraits.join(" + ");
        out.push_str(&format!("pub trait BamlPackageBaml: {bounds} {{\n"));
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
    out.push_str("        let rest = path.strip_prefix(\"baml.\")?;\n");
    out.push_str("        match rest.split_once('.') {\n");

    for (class_name, _) in &root.classes {
        let dispatch = class_dispatch_name("", class_name);
        out.push_str(&format!(
            "            Some(({:?}, method)) => Self::{dispatch}(method),\n",
            class_name
        ));
    }

    for (ns_name, _) in &root.sub_namespaces {
        let dispatch = namespace_dispatch_name(ns_name);
        out.push_str(&format!(
            "            Some(({:?}, rest)) => Self::{dispatch}(rest),\n",
            ns_name
        ));
    }

    if !root.free_fns.is_empty() {
        out.push_str("            None => match rest {\n");
        for entry in &root.free_fns {
            out.push_str(&format!(
                "                {:?} => Some(Self::__glue_{}),\n",
                entry.baml_method_name, entry.rust_method_name
            ));
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
    let return_type = clean_return_type(b);
    let params = clean_param_list(b);

    match b.vm_usage {
        VmUsage::None => out.push_str(&format!(
            "    fn {method_name}({params}) -> {return_type};\n",
        )),
        VmUsage::Ref => out.push_str(&format!(
            "    fn {method_name}(vm: &BexVm, {params}) -> {return_type};\n",
        )),
        VmUsage::MutRef => out.push_str(&format!(
            "    fn {method_name}(vm: &mut BexVm, {params}) -> {return_type};\n",
        )),
    }
}

fn emit_glue_method(out: &mut String, method_name: &str, b: &NativeBuiltin) {
    let glue_name = format!("__glue_{method_name}");
    let fallible = is_fallible(&b.path);

    out.push_str(&format!(
        "    fn {glue_name}(vm: &mut BexVm, args: &[Value]) -> NativeFunctionResult {{\n"
    ));

    emit_arg_extractions(out, b);

    let call_args = call_arg_list(b);
    let returns_null = matches!(b.return_type, BamlType::Null);

    let binding = if returns_null {
        "        "
    } else {
        "        let result = "
    };
    let suffix = if fallible { "?;\n" } else { ";\n" };

    match b.vm_usage {
        VmUsage::MutRef | VmUsage::Ref => {
            out.push_str(&format!(
                "{binding}Self::{method_name}(vm, {call_args}){suffix}"
            ));
        }
        VmUsage::None => {
            out.push_str(&format!(
                "{binding}Self::{method_name}({call_args}){suffix}"
            ));
        }
    }

    emit_result_conversion(out, b);

    out.push_str("    }\n");
}

// ============================================================================
// Parameter list and return type helpers
// ============================================================================

/// Build the clean parameter list.
fn clean_param_list(b: &NativeBuiltin) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(recv) = &b.receiver {
        parts.push(format!(
            "{}: {}",
            receiver_param_name(recv),
            receiver_input_type(recv)
        ));
        for p in &b.params {
            parts.push(format!("{}: {}", p.name, baml_type_to_input(&p.ty, false)));
        }
    } else {
        for p in &b.params {
            parts.push(format!("{}: {}", p.name, baml_type_to_input(&p.ty, false)));
        }
    }

    parts.join(", ")
}

fn clean_return_type(b: &NativeBuiltin) -> String {
    // Static constructors on media classes return copy types
    if b.receiver.is_none() {
        if let Some(class_name) = constructor_media_class(b) {
            let ns = constructor_media_namespace(b);
            let inner = format!("copy::{ns}::{class_name}");
            if is_fallible(&b.path) {
                return format!("Result<{inner}, VmError>");
            } else {
                return inner;
            }
        }
    }
    let inner = baml_type_to_output(&b.return_type);
    if is_fallible(&b.path) {
        format!("Result<{inner}, VmError>")
    } else {
        inner
    }
}

/// If this is a static constructor for a media class (no receiver, path has a media class segment),
/// return the class name. Used to determine copy return type.
fn constructor_media_class(b: &NativeBuiltin) -> Option<&str> {
    if b.receiver.is_some() {
        return None;
    }
    let rest = b.path.strip_prefix("baml.")?;
    let segments: Vec<&str> = rest.split('.').collect();
    // Need at least 2 segments for ClassName.method (e.g. "media.Pdf.from_url")
    if segments.len() < 2 {
        return None;
    }
    let class_seg = segments[segments.len() - 2];
    // Only uppercase-starting segments are class names
    if !class_seg.starts_with(|c: char| c.is_uppercase()) {
        return None;
    }
    // Only for media classes
    match class_seg {
        "Pdf" | "Audio" | "Video" | "Image" => Some(class_seg),
        _ => None,
    }
}

fn constructor_media_namespace(b: &NativeBuiltin) -> &str {
    let rest = b.path.strip_prefix("baml.").unwrap_or(&b.path);
    let segments: Vec<&str> = rest.split('.').collect();
    // segments = ["media", "Pdf", "from_url"] → namespace = "media"
    if segments.len() >= 3 { segments[0] } else { "" }
}

// ============================================================================
// Argument extraction
// ============================================================================

fn emit_arg_extractions(out: &mut String, b: &NativeBuiltin) {
    if let Some(recv) = &b.receiver {
        if recv.is_mut {
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1;
                emit_single_extraction(out, &p.name, arg_idx, &p.ty);
            }
            let recv_name = receiver_param_name(recv);
            emit_mut_receiver_extraction(out, &recv_name, recv);
        } else {
            let recv_name = receiver_param_name(recv);
            emit_immut_receiver_extraction(out, &recv_name, 0, recv);
            for (i, p) in b.params.iter().enumerate() {
                let arg_idx = i + 1;
                emit_single_extraction(out, &p.name, arg_idx, &p.ty);
            }
        }
    } else {
        for (i, p) in b.params.iter().enumerate() {
            emit_single_extraction(out, &p.name, i, &p.ty);
        }
    }
}

fn emit_single_extraction(out: &mut String, name: &str, idx: usize, ty: &BamlType) {
    let rhs = extraction_expr(&format!("&args[{idx}]"), ty, false);
    out.push_str(&format!("        let {name} = {rhs};\n"));
}

fn emit_immut_receiver_extraction(out: &mut String, name: &str, idx: usize, recv: &Receiver) {
    match recv.class_name.as_str() {
        "Pdf" | "Audio" | "Video" | "Image" => {
            let cls = &recv.class_name;
            out.push_str(&format!(
                "        let __instance = vm.as_instance(&args[{idx}])?;\n"
            ));
            out.push_str(&format!(
                "        let {name} = view::media::{cls} {{ instance: __instance }};\n"
            ));
        }
        _ => {
            let rhs = receiver_immut_extraction_expr(&format!("&args[{idx}]"), recv);
            out.push_str(&format!("        let {name} = {rhs};\n"));
        }
    }
}

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
// Extraction expressions
// ============================================================================

fn receiver_immut_extraction_expr(val: &str, recv: &Receiver) -> String {
    match recv.class_name.as_str() {
        "Array" => format!("vm.as_array({val})?.to_vec()"),
        "Map" => format!("vm.as_map({val})?.clone()"),
        "String" => format!("vm.as_string({val})?.clone()"),
        "Pdf" | "Audio" | "Video" | "Image" => {
            let kind = media_kind_expr(&recv.class_name);
            format!("vm.as_media({val}, {kind})?.clone()")
        }
        _ => format!("{val}.clone()"),
    }
}

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
        BamlType::Generic(_) => format!("{val}"),
        BamlType::Media(name) => {
            let kind = media_kind_expr(name);
            format!("vm.as_media({val}, {kind})?.clone()")
        }
        BamlType::Named(_) | BamlType::Null | BamlType::RustType => format!("{val}"),
    }
}

fn call_arg_list(b: &NativeBuiltin) -> String {
    let mut args: Vec<String> = Vec::new();

    if let Some(recv) = &b.receiver {
        let name = receiver_param_name(recv);
        if recv.is_mut {
            args.push(name);
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

fn call_arg_for_type(name: &str, ty: &BamlType) -> String {
    match ty {
        BamlType::String | BamlType::List(_) | BamlType::Map(_, _) | BamlType::Media(_) => {
            format!("&{name}")
        }
        BamlType::Optional(inner) => match inner.as_ref() {
            BamlType::String => format!("{name}.as_deref()"),
            BamlType::List(_) => format!("{name}.as_deref()"),
            _ => {
                if call_arg_needs_ref(inner) {
                    format!("{name}.as_ref()")
                } else {
                    name.to_string()
                }
            }
        },
        BamlType::Int | BamlType::Float | BamlType::Bool | BamlType::Null => name.to_string(),
        // Media class view types (Pdf, Audio, Video, Image) are Named and need to be passed by ref
        BamlType::Named(class_name)
            if matches!(class_name.as_str(), "Pdf" | "Audio" | "Video" | "Image") =>
        {
            format!("&{name}")
        }
        BamlType::Generic(_) | BamlType::Named(_) | BamlType::RustType => name.to_string(),
    }
}

fn call_arg_needs_ref(ty: &BamlType) -> bool {
    matches!(
        ty,
        BamlType::String | BamlType::List(_) | BamlType::Map(_, _) | BamlType::Media(_)
    )
}

// ============================================================================
// Result conversion
// ============================================================================

fn emit_result_conversion(out: &mut String, b: &NativeBuiltin) {
    // Static constructors on media classes: result is a copy struct, call .to_value(vm)
    if b.receiver.is_none() && constructor_media_class(b).is_some() {
        out.push_str("        Ok(result.to_value(vm))\n");
        return;
    }
    let conversion = result_conversion_expr("result", &b.return_type);
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
        "Pdf" => "&view::media::Pdf<'_>".to_string(),
        "Audio" => "&view::media::Audio<'_>".to_string(),
        "Video" => "&view::media::Video<'_>".to_string(),
        "Image" => "&view::media::Image<'_>".to_string(),
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
        "Pdf" | "Audio" | "Video" | "Image" => BamlType::Named(recv.class_name.clone()),
        _ => BamlType::Named(recv.class_name.clone()),
    }
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
            output.contains("pub trait BamlNamespaceMath"),
            "missing BamlNamespaceMath trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlNamespaceMedia"),
            "missing BamlNamespaceMedia trait:\n{output}"
        );
        assert!(
            output.contains("pub trait BamlNamespaceUnstable"),
            "missing BamlNamespaceUnstable trait:\n{output}"
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
            output.contains("fn length(array: &[Value]) -> i64;"),
            "BamlClassArray should have bare `length` method:\n{output}"
        );
        assert!(
            output.contains("fn to_lower_case(string: &str) -> String;"),
            "BamlClassString should have bare `to_lower_case` method:\n{output}"
        );
        assert!(
            output.contains("fn trunc(value: f64) -> i64;"),
            "BamlNamespaceMath should have bare `trunc` method:\n{output}"
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
            output.contains("fn __dispatch_math(rest: &str)"),
            "missing __dispatch_math:\n{output}"
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
        assert!(
            output.contains("(\"math\", rest)) => Self::__dispatch_math(rest)"),
            "get_native_fn should dispatch math to __dispatch_math:\n{output}"
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
        assert!(
            output.contains("fn deep_equals(vm: &BexVm, a: &Value, b: &Value) -> bool;"),
            "BamlPackageBaml should have deep_equals with &BexVm:\n{output}"
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
            let has_mut_receiver = b.receiver.as_ref().is_some_and(|r| r.is_mut);
            let has_mut_vm = output.contains(&format!("fn {name}(vm: &mut BexVm,"));
            let has_ref_vm = output.contains(&format!("fn {name}(vm: &BexVm,"));

            if has_mut_receiver {
                assert!(
                    !has_mut_vm && !has_ref_vm,
                    "method {name} should NOT have vm param (mutable receiver)"
                );
            } else {
                match b.vm_usage {
                    VmUsage::MutRef => {
                        assert!(has_mut_vm, "method {name} should have vm: &mut BexVm")
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
            output.contains("fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Pdf;"),
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
