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

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::types::{BamlType, NativeBuiltin, NativeClassDef};

// ============================================================================
// Path configuration for generated code
// ============================================================================

/// Controls the module path prefixes for `owned::` and `view::` references in
/// generated code. When structs and traits live in the same file, use
/// `CodegenPaths::inline()`. When structs are generated in a separate crate,
/// use `CodegenPaths::external("sys_types::generated")`.
struct CodegenPaths {
    owned: syn::Path,
    view: syn::Path,
}

impl CodegenPaths {
    /// Structs are emitted in the same file: `owned::ns::Type`, `view::ns::Type`.
    fn inline() -> Self {
        Self {
            owned: syn::parse_str("owned").unwrap(),
            view: syn::parse_str("view").unwrap(),
        }
    }

    /// Structs live in an external crate: `path::owned::ns::Type`, `path::view::ns::Type`.
    fn external(path: &str) -> Self {
        Self {
            owned: syn::parse_str(&format!("{path}::owned")).unwrap(),
            view: syn::parse_str(&format!("{path}::view")).unwrap(),
        }
    }
}

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

/// Map a `BamlType` to the Rust type tokens for an owned struct field.
fn owned_rust_type(
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    match ty {
        BamlType::String => quote! { String },
        BamlType::Int => quote! { i64 },
        BamlType::Float => quote! { f64 },
        BamlType::Bool => quote! { bool },
        BamlType::Null => quote! { () },
        BamlType::RustType => quote! { std::sync::Arc<dyn std::any::Any + Send + Sync> },
        BamlType::List(inner) => {
            let inner_ty = owned_rust_type(inner, class_ns_map, paths);
            quote! { Vec<#inner_ty> }
        }
        BamlType::Map(k, v) => {
            let k_ty = owned_rust_type(k, class_ns_map, paths);
            let v_ty = owned_rust_type(v, class_ns_map, paths);
            quote! { indexmap::IndexMap<#k_ty, #v_ty> }
        }
        BamlType::Optional(inner) => {
            let inner_ty = owned_rust_type(inner, class_ns_map, paths);
            quote! { Option<#inner_ty> }
        }
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                let owned = &paths.owned;
                let ns_ident = format_ident!("{}", ns);
                let name_ident = format_ident!("{}", name);
                quote! { #owned::#ns_ident::#name_ident }
            } else {
                match name.as_str() {
                    "unknown" => quote! { BexExternalValue },
                    "type" => quote! { baml_type::Ty },
                    "function" => quote! { BexExternalValue },
                    _ => quote! { BexExternalValue },
                }
            }
        }
        BamlType::Generic(_) | BamlType::Media(_) => quote! { BexExternalValue },
    }
}

/// Map a `BamlType` to the return type tokens for a view struct accessor.
fn view_return_type(ty: &BamlType, needs_heap: &mut bool) -> TokenStream {
    match ty {
        BamlType::Int => quote! { Result<i64, AccessError> },
        BamlType::Float => quote! { Result<f64, AccessError> },
        BamlType::Bool => quote! { Result<bool, AccessError> },
        BamlType::String => {
            *needs_heap = true;
            quote! { Result<&'a String, AccessError> }
        }
        BamlType::RustType => {
            *needs_heap = true;
            quote! { Result<std::sync::Arc<dyn std::any::Any + Send + Sync>, AccessError> }
        }
        _ => {
            *needs_heap = true;
            quote! { Result<BexExternalValue, AccessError> }
        }
    }
}

/// Generate the accessor body for a view struct field.
fn view_accessor_body(field_name: &str, ty: &BamlType) -> TokenStream {
    let field_lit = field_name;
    match ty {
        BamlType::Int => quote! { self.cls.field(#field_lit)?.as_int() },
        BamlType::Float => quote! { self.cls.field(#field_lit)?.as_float() },
        BamlType::Bool => quote! { self.cls.field(#field_lit)?.as_bool() },
        BamlType::String => quote! { self.cls.field(#field_lit)?.as_string(heap) },
        BamlType::RustType => quote! { self.cls.field(#field_lit)?.as_rust_data(heap) },
        _ => quote! { self.cls.field(#field_lit)?.as_owned_but_very_slow(heap) },
    }
}

/// Generate a Rust expression that converts a `BexExternalValue` (`val_expr`)
/// into the owned Rust type for `ty`, returning `Result<T, AccessError>`.
fn external_to_typed_expr(
    val_expr: TokenStream,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    match ty {
        BamlType::String => quote! {
            match #val_expr {
                BexExternalValue::String(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "string",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        BamlType::Int => quote! {
            match #val_expr {
                BexExternalValue::Int(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "int",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        BamlType::Float => quote! {
            match #val_expr {
                BexExternalValue::Float(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "float",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        BamlType::Bool => quote! {
            match #val_expr {
                BexExternalValue::Bool(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "bool",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        BamlType::RustType => quote! {
            match #val_expr {
                BexExternalValue::RustData(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "rust_data",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        BamlType::List(inner) => {
            let inner_conv = external_to_typed_expr(quote! { __v }, inner, class_ns_map, paths);
            quote! {
                match #val_expr {
                    BexExternalValue::Array { items, .. } => {
                        items.into_iter()
                            .map(|__v| { #inner_conv })
                            .collect::<Result<Vec<_>, AccessError>>()
                    }
                    other => Err(AccessError::TypeMismatch {
                        expected: "array",
                        actual: other.type_name().to_string(),
                    }),
                }
            }
        }
        BamlType::Map(_k, v) => {
            let v_conv = external_to_typed_expr(quote! { __v }, v, class_ns_map, paths);
            quote! {
                match #val_expr {
                    BexExternalValue::Map { entries, .. } => {
                        entries.into_iter()
                            .map(|(__k, __v)| { Ok((__k, (#v_conv)?)) })
                            .collect::<Result<indexmap::IndexMap<_, _>, AccessError>>()
                    }
                    other => Err(AccessError::TypeMismatch {
                        expected: "map",
                        actual: other.type_name().to_string(),
                    }),
                }
            }
        }
        BamlType::Optional(inner) => {
            let inner_conv = external_to_typed_expr(quote! { __v }, inner, class_ns_map, paths);
            quote! {
                match #val_expr {
                    BexExternalValue::Null => Ok(None),
                    __v => Ok(Some((#inner_conv)?)),
                }
            }
        }
        BamlType::Named(name) if class_ns_map.contains_key(name.as_str()) => {
            let ns = &class_ns_map[name.as_str()];
            let owned = &paths.owned;
            let ns_ident = format_ident!("{}", ns);
            let name_ident = format_ident!("{}", name);
            quote! { #owned::#ns_ident::#name_ident::from_external(#val_expr) }
        }
        _ => quote! { Ok(#val_expr) },
    }
}

/// Generate the `into_owned` conversion expression for a view field.
fn into_owned_expr(
    field_name: &str,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let field_ident = format_ident!("{}", field_name);
    match ty {
        BamlType::Int | BamlType::Float | BamlType::Bool => {
            quote! { self.#field_ident()? }
        }
        BamlType::String => quote! { self.#field_ident(heap)?.clone() },
        BamlType::RustType => quote! { self.#field_ident(heap)? },
        BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let val = quote! { self.#field_ident(heap)? };
            let conv = external_to_typed_expr(val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        BamlType::Named(name) if class_ns_map.contains_key(name.as_str()) => {
            let val = quote! { self.#field_ident(heap)? };
            let conv = external_to_typed_expr(val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        _ => quote! { self.#field_ident(heap)? },
    }
}

/// Generate the `BexExternalValue` conversion expression for an owned field.
#[allow(clippy::only_used_in_recursion)]
fn owned_to_external_expr(
    field_expr: TokenStream,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
) -> TokenStream {
    match ty {
        BamlType::Int => quote! { BexExternalValue::Int(#field_expr) },
        BamlType::Float => quote! { BexExternalValue::Float(#field_expr) },
        BamlType::Bool => quote! { BexExternalValue::Bool(#field_expr) },
        BamlType::String => quote! { BexExternalValue::String(#field_expr) },
        BamlType::RustType => quote! { BexExternalValue::RustData(#field_expr) },
        BamlType::Null => quote! { BexExternalValue::Null },
        BamlType::List(inner) => {
            let inner_conv = owned_to_external_expr(quote! { __v }, inner, class_ns_map);
            quote! {
                BexExternalValue::Array {
                    element_type: baml_type::Ty::unknown(),
                    items: #field_expr.into_iter().map(|__v| #inner_conv).collect(),
                }
            }
        }
        BamlType::Map(_k, v) => {
            let v_conv = owned_to_external_expr(quote! { __v }, v, class_ns_map);
            quote! {
                BexExternalValue::Map {
                    key_type: baml_type::Ty::string(),
                    value_type: baml_type::Ty::unknown(),
                    entries: #field_expr.into_iter().map(|(__k, __v)| (__k, #v_conv)).collect(),
                }
            }
        }
        BamlType::Optional(inner) => {
            let inner_conv = owned_to_external_expr(quote! { __v }, inner, class_ns_map);
            quote! { #field_expr.map(|__v| #inner_conv).unwrap_or(BexExternalValue::Null) }
        }
        BamlType::Named(_name) => {
            quote! { #field_expr.into_bex_external_value() }
        }
        _ => quote! { #field_expr.into_bex_external_value() },
    }
}

/// Map a `BamlType` to the Rust type for a clean trait method param/return.
fn clean_rust_type(
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    match ty {
        BamlType::String => quote! { String },
        BamlType::Int => quote! { i64 },
        BamlType::Float => quote! { f64 },
        BamlType::Bool => quote! { bool },
        BamlType::Null => quote! { () },
        BamlType::RustType => quote! { std::sync::Arc<dyn std::any::Any + Send + Sync> },
        BamlType::List(inner) => {
            let inner_ty = clean_rust_type(inner, class_ns_map, paths);
            quote! { Vec<#inner_ty> }
        }
        BamlType::Map(k, v) => {
            let k_ty = clean_rust_type(k, class_ns_map, paths);
            let v_ty = clean_rust_type(v, class_ns_map, paths);
            quote! { indexmap::IndexMap<#k_ty, #v_ty> }
        }
        BamlType::Optional(inner) => {
            let inner_ty = clean_rust_type(inner, class_ns_map, paths);
            quote! { Option<#inner_ty> }
        }
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                let owned = &paths.owned;
                let ns_ident = format_ident!("{}", ns);
                let name_ident = format_ident!("{}", name);
                quote! { #owned::#ns_ident::#name_ident }
            } else {
                match name.as_str() {
                    "type" => quote! { baml_type::Ty },
                    "unknown" => quote! { BexExternalValue },
                    "function" => quote! { BexExternalValue },
                    _ => quote! { BexExternalValue },
                }
            }
        }
        BamlType::Generic(_) | BamlType::Media(_) => quote! { BexExternalValue },
    }
}

/// Generate the arg extraction expression for a glue method parameter.
fn glue_extract_expr(
    arg_ident: &syn::Ident,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    is_receiver: bool,
    paths: &CodegenPaths,
) -> TokenStream {
    if is_receiver {
        return quote! { /* receiver extracted below */ };
    }
    match ty {
        BamlType::String => quote! { #arg_ident.as_string(&__p)?.to_string() },
        BamlType::Int => quote! { #arg_ident.as_int()? },
        BamlType::Float => quote! { #arg_ident.as_float()? },
        BamlType::Bool => quote! { #arg_ident.as_bool()? },
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                let view = &paths.view;
                let ns_ident = format_ident!("{}", ns);
                let name_ident = format_ident!("{}", name);
                quote! {
                    #arg_ident.as_builtin_class::<#view::#ns_ident::#name_ident>(&__p)?.into_owned(&__p)?
                }
            } else {
                match name.as_str() {
                    "type" => quote! { #arg_ident.as_baml_type_owned(&__p)? },
                    _ => quote! { #arg_ident.as_owned_but_very_slow(&__p)? },
                }
            }
        }
        BamlType::RustType => quote! { #arg_ident.as_rust_data(&__p)? },
        BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let val = quote! { #arg_ident.as_owned_but_very_slow(&__p)? };
            let conv = external_to_typed_expr(val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        _ => quote! { #arg_ident.as_owned_but_very_slow(&__p)? },
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

fn ns_trait_ident(ns: &str) -> syn::Ident {
    format_ident!("IoNamespace{}", capitalize_first(ns))
}

fn class_trait_ident(ns: &str, class: &str) -> syn::Ident {
    format_ident!("IoClass{}{}", capitalize_first(ns), class)
}

// ============================================================================
// Generate SysOp Enum
// ============================================================================

pub fn generate_sys_op_enum(io_builtins: &[NativeBuiltin]) -> String {
    let variant_idents: Vec<_> = io_builtins
        .iter()
        .map(|b| format_ident!("{}", b.sys_op_variant_name()))
        .collect();
    let paths: Vec<&str> = io_builtins.iter().map(|b| b.path.as_str()).collect();

    let error_cat_arms: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let variant = format_ident!("{}", b.sys_op_variant_name());
            if b.throws.is_empty() {
                quote! { SysOp::#variant => &[] }
            } else {
                let cats: Vec<_> = b.throws.iter().map(|t| format_ident!("{}", t)).collect();
                quote! { SysOp::#variant => &[#(SysOpErrorCategory::#cats),*] }
            }
        })
        .collect();

    let panic_cat_arms: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let variant = format_ident!("{}", b.sys_op_variant_name());
            if b.sys_op_variant_name() == "BamlSysPanic" {
                quote! { SysOp::#variant => &[SysOpPanicCategory::HostPanic] }
            } else {
                quote! { SysOp::#variant => &[] }
            }
        })
        .collect();

    let path_to_variant_arms: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let path = &b.path;
            let variant = format_ident!("{}", b.sys_op_variant_name());
            quote! { #path => Some(SysOp::#variant) }
        })
        .collect();

    let tokens = quote! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum SysOp {
            #(#variant_idents,)*
        }

        impl SysOp {
            pub const fn path(&self) -> &'static str {
                match self {
                    #(SysOp::#variant_idents => #paths,)*
                }
            }

            pub fn allowed_error_categories(&self) -> &'static [SysOpErrorCategory] {
                match self {
                    #(#error_cat_arms,)*
                }
            }

            /// Hardcoded, not extracted from .baml
            pub fn allowed_panic_categories(&self) -> &'static [SysOpPanicCategory] {
                match self {
                    #(#panic_cat_arms,)*
                }
            }
        }

        impl std::fmt::Display for SysOp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.path())
            }
        }

        pub fn sys_op_for_path(path: &str) -> Option<SysOp> {
            match path {
                #(#path_to_variant_arms,)*
                // Legacy aliases (baml_builtins paths)
                "env.get" | "env.get_or_panic" => Some(SysOp::BamlEnvGet),
                "baml.http.Response.ok" => Some(SysOp::BamlHttpResponseText),
                _ => None,
            }
        }
    };

    crate::format_tokens(tokens)
}

// ============================================================================
// Generate IO Traits (main entry point for sys_ops codegen)
// ============================================================================

/// Generate view + owned struct modules from IO class definitions.
///
/// This is the structs-only half of the codegen. Use this in crates that need
/// the data types but not the IO trait hierarchy (e.g. `sys_types`).
pub fn generate_io_structs(io_builtins: &[NativeBuiltin], class_defs: &[NativeClassDef]) -> String {
    let io_class_defs = filter_io_class_defs(io_builtins, class_defs);
    let class_ns_map = build_class_ns_map(&io_class_defs);
    let class_defs_by_ns = group_class_defs_by_ns(&io_class_defs);
    let paths = CodegenPaths::inline();

    let view_mod = emit_view_module(&io_class_defs, &class_ns_map, &class_defs_by_ns, &paths);
    let owned_mod = emit_owned_module(&io_class_defs, &class_ns_map, &class_defs_by_ns, &paths);

    let tokens = quote! {
        #view_mod
        #owned_mod
    };

    crate::format_tokens(tokens)
}

/// Generate IO trait hierarchy and `SysOps` dispatch struct.
///
/// `structs_path` controls where struct references (both `view::` and `owned::`)
/// point. Pass `"self"` to include the struct modules inline, or an external
/// path like `"sys_types::generated"` to reference structs from another crate.
pub fn generate_io_traits(
    io_builtins: &[NativeBuiltin],
    class_defs: &[NativeClassDef],
    structs_path: &str,
) -> String {
    let tree = build_io_namespace_tree(io_builtins);
    let io_class_defs = filter_io_class_defs(io_builtins, class_defs);
    let class_ns_map = build_class_ns_map(&io_class_defs);
    let class_defs_by_ns = group_class_defs_by_ns(&io_class_defs);

    let paths = if structs_path == "self" {
        CodegenPaths::inline()
    } else {
        CodegenPaths::external(structs_path)
    };

    let struct_mods = if structs_path == "self" {
        let view_mod = emit_view_module(&io_class_defs, &class_ns_map, &class_defs_by_ns, &paths);
        let owned_mod = emit_owned_module(&io_class_defs, &class_ns_map, &class_defs_by_ns, &paths);
        quote! { #view_mod #owned_mod }
    } else {
        quote! {}
    };

    let class_traits = emit_class_traits(&tree, &class_ns_map, &paths);
    let ns_traits = emit_namespace_traits(&tree, &class_ns_map, &paths);
    let root_trait = emit_root_trait(&tree);
    let sys_ops = emit_sys_ops_struct(io_builtins);

    let tokens = quote! {
        #struct_mods
        #class_traits
        #ns_traits
        #root_trait
        #sys_ops
    };

    crate::format_tokens(tokens)
}

// ============================================================================
// View module
// ============================================================================

fn emit_view_module(
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
    paths: &CodegenPaths,
) -> TokenStream {
    let ns_modules: Vec<TokenStream> = class_defs_by_ns
        .iter()
        .map(|(ns, classes)| {
            let ns_ident = format_ident!("{}", ns);
            let structs: Vec<TokenStream> = classes
                .iter()
                .map(|cd| emit_view_struct(cd, class_ns_map, ns, paths))
                .collect();
            quote! {
                pub mod #ns_ident {
                    use super::super::*;
                    #(#structs)*
                }
            }
        })
        .collect();

    quote! {
        pub mod view {
            #(#ns_modules)*
        }
    }
}

fn emit_view_struct(
    cd: &NativeClassDef,
    class_ns_map: &BTreeMap<String, String>,
    ns: &str,
    paths: &CodegenPaths,
) -> TokenStream {
    let name_ident = format_ident!("{}", cd.name);
    let full_path = format!("{}.{}", cd.namespace_prefix, cd.name);
    let source_comment = format!("Generated from `{}`", cd.source_file);

    // Field accessors
    let accessors: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let mut needs_heap = false;
            let ret_type = view_return_type(&field.field_type, &mut needs_heap);
            let body = view_accessor_body(&field.name, &field.field_type);
            let field_ident = format_ident!("{}", field.name);

            if needs_heap {
                quote! {
                    pub fn #field_ident(&self, heap: &'a GcProtectedHeap<'a>) -> #ret_type {
                        #body
                    }
                }
            } else {
                quote! {
                    pub fn #field_ident(&self) -> #ret_type {
                        #body
                    }
                }
            }
        })
        .collect();

    // into_owned()
    let owned = &paths.owned;
    let ns_ident = format_ident!("{}", ns);
    let owned_path = quote! { #owned::#ns_ident::#name_ident };

    let into_owned_fields: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let field_ident = format_ident!("{}", field.name);
            let expr = into_owned_expr(&field.name, &field.field_type, class_ns_map, paths);
            quote! { #field_ident: #expr }
        })
        .collect();

    quote! {
        #[doc = #source_comment]
        pub struct #name_ident<'a> {
            cls: BexClass<'a>,
        }

        impl<'a> From<BexClass<'a>> for #name_ident<'a> {
            fn from(cls: BexClass<'a>) -> Self {
                Self { cls }
            }
        }

        impl<'a> BuiltinClass<'a> for #name_ident<'a> {
            fn name() -> &'static str {
                #full_path
            }
        }

        impl<'a> #name_ident<'a> {
            #(#accessors)*

            pub fn into_owned(self, heap: &'a GcProtectedHeap<'a>) -> Result<#owned_path, AccessError> {
                Ok(#owned_path {
                    #(#into_owned_fields,)*
                })
            }
        }
    }
}

// ============================================================================
// Owned module
// ============================================================================

fn emit_owned_module(
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
    paths: &CodegenPaths,
) -> TokenStream {
    let ns_modules: Vec<TokenStream> = class_defs_by_ns
        .iter()
        .map(|(ns, classes)| {
            let ns_ident = format_ident!("{}", ns);
            let structs: Vec<TokenStream> = classes
                .iter()
                .map(|cd| emit_owned_struct(cd, class_ns_map, ns, paths))
                .collect();
            quote! {
                pub mod #ns_ident {
                    use super::super::*;
                    #(#structs)*
                }
            }
        })
        .collect();

    quote! {
        pub mod owned {
            #(#ns_modules)*
        }
    }
}

fn emit_owned_struct(
    cd: &NativeClassDef,
    class_ns_map: &BTreeMap<String, String>,
    _ns: &str,
    paths: &CodegenPaths,
) -> TokenStream {
    let name_ident = format_ident!("{}", cd.name);
    let full_path = format!("{}.{}", cd.namespace_prefix, cd.name);
    let source_comment = format!("Generated from `{}`", cd.source_file);

    let has_rust_type = cd
        .fields
        .iter()
        .any(|f| matches!(f.field_type, BamlType::RustType));

    let derives = if has_rust_type {
        quote! { #[derive(Clone, Debug)] }
    } else {
        quote! { #[derive(Clone, Debug, Default)] }
    };

    // Struct fields
    let struct_fields: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let field_ident = format_ident!("{}", field.name);
            let rust_ty = owned_rust_type(&field.field_type, class_ns_map, paths);
            quote! { pub #field_ident: #rust_ty }
        })
        .collect();

    // AsBexExternalValue impl — indexmap entries
    let as_bex_entries: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let field_name_str = &field.name;
            let field_ident = format_ident!("{}", field.name);
            let conv = owned_to_external_expr(
                quote! { self.#field_ident },
                &field.field_type,
                class_ns_map,
            );
            quote! { #field_name_str.to_string() => #conv }
        })
        .collect();

    // from_external — field extraction
    let from_external_fields: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let field_ident = format_ident!("{}", field.name);
            let field_name_str = &field.name;
            let field_val = quote! {
                fields.swap_remove(#field_name_str).unwrap_or(BexExternalValue::Null)
            };
            let conv = external_to_typed_expr(field_val, &field.field_type, class_ns_map, paths);
            quote! { #field_ident: (#conv)? }
        })
        .collect();

    let name_str = &cd.name;

    quote! {
        #[doc = #source_comment]
        #derives
        pub struct #name_ident {
            #(#struct_fields,)*
        }

        impl AsBexExternalValue for #name_ident {
            fn into_bex_external_value(self) -> BexExternalValue {
                BexExternalValue::Instance {
                    class_name: #full_path.to_string(),
                    fields: indexmap::indexmap! {
                        #(#as_bex_entries,)*
                    },
                }
            }
        }

        impl #name_ident {
            pub fn from_external(__val: BexExternalValue) -> Result<Self, AccessError> {
                match __val {
                    BexExternalValue::Instance { mut fields, .. } => Ok(Self {
                        #(#from_external_fields,)*
                    }),
                    __other => Err(AccessError::TypeMismatch {
                        expected: #name_str,
                        actual: __other.type_name().to_string(),
                    }),
                }
            }
        }
    }
}

// ============================================================================
// Class traits
// ============================================================================

fn emit_class_traits(
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let traits: Vec<TokenStream> = tree
        .iter()
        .flat_map(|(ns, node)| {
            node.classes.iter().map(move |(class_name, methods)| {
                emit_one_class_trait(ns, class_name, methods, class_ns_map, paths)
            })
        })
        .collect();

    quote! { #(#traits)* }
}

fn emit_one_class_trait(
    ns: &str,
    class_name: &str,
    methods: &[&NativeBuiltin],
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let trait_ident = class_trait_ident(ns, class_name);
    let dispatch_fn_ident = format_ident!("__dispatch_{}_{}", ns, class_name.to_lowercase());

    let source_comment = methods
        .first()
        .map(|m| format!("Generated from `{}`", m.source_file))
        .unwrap_or_default();

    // Clean methods
    let clean_methods: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            let method_ident = format_ident!("{}", io_method_name(m));
            let ret_ty = clean_rust_type(&m.return_type, class_ns_map, paths);
            let owned = &paths.owned;
            let ns_ident = format_ident!("{}", ns);
            let class_ident = format_ident!("{}", class_name);
            let receiver_param_ident = format_ident!("{}", class_name.to_lowercase());
            let receiver_ty = quote! { #owned::#ns_ident::#class_ident };

            let extra_params: Vec<TokenStream> = m
                .params
                .iter()
                .map(|p| {
                    let p_ident = format_ident!("{}", p.name);
                    let p_ty = clean_rust_type(&p.ty, class_ns_map, paths);
                    quote! { #p_ident: #p_ty }
                })
                .collect();

            quote! {
                fn #method_ident(
                    &self,
                    heap: &std::sync::Arc<BexHeap>,
                    call_id: CallId,
                    #receiver_param_ident: #receiver_ty,
                    #(#extra_params,)*
                    ctx: &SysOpContext,
                ) -> SysOpOutput<#ret_ty>;
            }
        })
        .collect();

    // Glue methods
    let glue_methods: Vec<TokenStream> = methods
        .iter()
        .map(|m| emit_glue_method(m, ns, class_name, class_ns_map, paths))
        .collect();

    // Dispatch method — match arms
    let dispatch_arms: Vec<TokenStream> = methods
        .iter()
        .map(|m| {
            let method_name_str = io_method_name(m);
            let glue_ident = format_ident!("__glue_{}", m.fn_name);
            quote! {
                #method_name_str => Some(self.#glue_ident(heap, args, ctx, call_id))
            }
        })
        .collect();

    quote! {
        #[doc = #source_comment]
        pub trait #trait_ident {
            #(#clean_methods)*

            #(#glue_methods)*

            fn #dispatch_fn_ident(
                &self,
                method: &str,
                heap: &std::sync::Arc<BexHeap>,
                args: Vec<BexValue<'_>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> Option<SysOpResult> {
                match method {
                    #(#dispatch_arms,)*
                    _ => None,
                }
            }
        }
    }
}

fn emit_glue_method(
    builtin: &NativeBuiltin,
    ns: &str,
    class_name: &str,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let glue_ident = format_ident!("__glue_{}", builtin.fn_name);
    let variant_ident = format_ident!("{}", builtin.sys_op_variant_name());
    let clean_method_ident = format_ident!("{}", io_method_name(builtin));

    let view = &paths.view;
    let ns_ident = format_ident!("{}", ns);
    let class_ident = format_ident!("{}", class_name);

    // Arg extraction lets
    let arg_idents: Vec<syn::Ident> = (0..builtin.params.len())
        .map(|i| format_ident!("__arg{}", i))
        .collect();
    let arg_lets: Vec<TokenStream> = arg_idents
        .iter()
        .map(|id| quote! { let #id = __args.next().unwrap(); })
        .collect();

    // Extraction inside gc protection
    let param_extractions: Vec<TokenStream> = builtin
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let arg_id = &arg_idents[i];
            let param_ident = format_ident!("__{}", p.name);
            let extract = glue_extract_expr(arg_id, &p.ty, class_ns_map, false, paths);
            quote! { let #param_ident = #extract; }
        })
        .collect();

    // Tuple elements for Ok return
    let tuple_idents: Vec<syn::Ident> = std::iter::once(format_ident!("__receiver"))
        .chain(builtin.params.iter().map(|p| format_ident!("__{}", p.name)))
        .collect();

    // Call args for clean method
    let call_param_idents: Vec<syn::Ident> = std::iter::once(format_ident!("__receiver"))
        .chain(builtin.params.iter().map(|p| format_ident!("__{}", p.name)))
        .collect();

    quote! {
        fn #glue_ident(
            &self,
            heap: &std::sync::Arc<BexHeap>,
            args: Vec<BexValue<'_>>,
            ctx: &SysOpContext,
            call_id: CallId,
        ) -> SysOpResult {
            let mut __args = args.into_iter();
            let __arg_self = __args.next().unwrap();
            #(#arg_lets)*

            let __extraction = heap.with_gc_protection(move |__p| {
                let __receiver = __arg_self
                    .as_builtin_class::<#view::#ns_ident::#class_ident>(&__p)?
                    .into_owned(&__p)?;
                #(#param_extractions)*
                Ok::<_, AccessError>((#(#tuple_idents),*))
            });

            match __extraction {
                Ok((#(#tuple_idents),*)) => {
                    self.#clean_method_ident(heap, call_id, #(#call_param_idents,)* ctx)
                        .into_result(SysOp::#variant_ident)
                }
                Err(e) => SysOpResult::Ready(Err(OpError::new(
                    SysOp::#variant_ident,
                    OpErrorKind::AccessError(e),
                ))),
            }
        }
    }
}

// ============================================================================
// Namespace traits
// ============================================================================

fn emit_namespace_traits(
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let traits: Vec<TokenStream> = tree
        .iter()
        .map(|(ns, node)| emit_one_namespace_trait(ns, node, class_ns_map, paths))
        .collect();

    quote! { #(#traits)* }
}

fn emit_one_namespace_trait(
    ns: &str,
    node: &IoNamespaceNode,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let trait_ident = ns_trait_ident(ns);
    let dispatch_fn_ident = format_ident!("__dispatch_{}", ns);

    // Supertraits: class traits in this namespace
    let class_trait_idents: Vec<syn::Ident> = node
        .classes
        .keys()
        .map(|cn| class_trait_ident(ns, cn))
        .collect();

    let supertrait_bound = if class_trait_idents.is_empty() {
        quote! {}
    } else {
        quote! { : #(#class_trait_idents)+* }
    };

    // Clean methods for free functions
    let free_fn_clean: Vec<TokenStream> = node
        .free_fns
        .iter()
        .map(|f| {
            let fn_ident = format_ident!("{}", io_method_name(f));
            let ret_ty = clean_rust_type(&f.return_type, class_ns_map, paths);

            let extra_params: Vec<TokenStream> = f
                .params
                .iter()
                .map(|p| {
                    let p_ident = format_ident!("{}", p.name);
                    let p_ty = clean_rust_type(&p.ty, class_ns_map, paths);
                    quote! { #p_ident: #p_ty }
                })
                .collect();

            quote! {
                fn #fn_ident(
                    &self,
                    heap: &std::sync::Arc<BexHeap>,
                    call_id: CallId,
                    #(#extra_params,)*
                    ctx: &SysOpContext,
                ) -> SysOpOutput<#ret_ty>;
            }
        })
        .collect();

    // Glue methods for free functions
    let free_fn_glues: Vec<TokenStream> = node
        .free_fns
        .iter()
        .map(|f| emit_free_fn_glue(f, class_ns_map, paths))
        .collect();

    // Dispatch method body
    let dispatch_body = if node.classes.is_empty() {
        // Only free functions — match directly
        let arms: Vec<TokenStream> = node
            .free_fns
            .iter()
            .map(|f| {
                let fn_name_str = io_method_name(f);
                let glue_ident = format_ident!("__glue_{}", f.fn_name);
                quote! { #fn_name_str => Some(self.#glue_ident(heap, args, ctx, call_id)) }
            })
            .collect();

        quote! {
            match rest {
                #(#arms,)*
                _ => None,
            }
        }
    } else {
        // Mix of classes and free functions — use split_once to route
        let class_arms: Vec<TokenStream> = node
            .classes
            .keys()
            .map(|cn| {
                let cn_str = cn.as_str();
                let dispatch = format_ident!("__dispatch_{}_{}", ns, cn.to_lowercase());
                quote! { Some((#cn_str, method)) => self.#dispatch(method, heap, args, ctx, call_id) }
            })
            .collect();

        let free_fn_arms: Vec<TokenStream> = node
            .free_fns
            .iter()
            .map(|f| {
                let fn_name_str = io_method_name(f);
                let glue_ident = format_ident!("__glue_{}", f.fn_name);
                quote! { #fn_name_str => Some(self.#glue_ident(heap, args, ctx, call_id)) }
            })
            .collect();

        quote! {
            match rest.split_once('.') {
                #(#class_arms,)*
                None => match rest {
                    #(#free_fn_arms,)*
                    _ => None,
                },
                _ => None,
            }
        }
    };

    quote! {
        pub trait #trait_ident #supertrait_bound {
            #(#free_fn_clean)*

            #(#free_fn_glues)*

            fn #dispatch_fn_ident(
                &self,
                rest: &str,
                heap: &std::sync::Arc<BexHeap>,
                args: Vec<BexValue<'_>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> Option<SysOpResult> {
                #dispatch_body
            }
        }
    }
}

fn emit_free_fn_glue(
    builtin: &NativeBuiltin,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let glue_ident = format_ident!("__glue_{}", builtin.fn_name);
    let variant_ident = format_ident!("{}", builtin.sys_op_variant_name());
    let clean_ident = format_ident!("{}", io_method_name(builtin));

    if builtin.params.is_empty() {
        return quote! {
            fn #glue_ident(
                &self,
                heap: &std::sync::Arc<BexHeap>,
                args: Vec<BexValue<'_>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> SysOpResult {
                self.#clean_ident(heap, call_id, ctx).into_result(SysOp::#variant_ident)
            }
        };
    }

    let arg_idents: Vec<syn::Ident> = (0..builtin.params.len())
        .map(|i| format_ident!("__arg{}", i))
        .collect();
    let arg_lets: Vec<TokenStream> = arg_idents
        .iter()
        .map(|id| quote! { let #id = __args.next().unwrap(); })
        .collect();

    let param_extractions: Vec<TokenStream> = builtin
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let arg_id = &arg_idents[i];
            let param_ident = format_ident!("__{}", p.name);
            let extract = glue_extract_expr(arg_id, &p.ty, class_ns_map, false, paths);
            quote! { let #param_ident = #extract; }
        })
        .collect();

    let param_idents: Vec<syn::Ident> = builtin
        .params
        .iter()
        .map(|p| format_ident!("__{}", p.name))
        .collect();

    let ok_pattern = if param_idents.len() == 1 {
        let id = &param_idents[0];
        quote! { #id }
    } else {
        quote! { (#(#param_idents),*) }
    };

    let extraction_return = if param_idents.len() == 1 {
        let id = &param_idents[0];
        quote! { Ok::<_, AccessError>(#id) }
    } else {
        quote! { Ok::<_, AccessError>((#(#param_idents),*)) }
    };

    quote! {
        fn #glue_ident(
            &self,
            heap: &std::sync::Arc<BexHeap>,
            args: Vec<BexValue<'_>>,
            ctx: &SysOpContext,
            call_id: CallId,
        ) -> SysOpResult {
            let mut __args = args.into_iter();
            #(#arg_lets)*

            let __extraction = heap.with_gc_protection(move |__p| {
                #(#param_extractions)*
                #extraction_return
            });

            match __extraction {
                Ok(#ok_pattern) => {
                    self.#clean_ident(heap, call_id, #(#param_idents,)* ctx)
                        .into_result(SysOp::#variant_ident)
                }
                Err(e) => SysOpResult::Ready(Err(OpError::new(
                    SysOp::#variant_ident,
                    OpErrorKind::AccessError(e),
                ))),
            }
        }
    }
}

// ============================================================================
// Root trait
// ============================================================================

fn emit_root_trait(tree: &BTreeMap<String, IoNamespaceNode>) -> TokenStream {
    let ns_trait_idents: Vec<syn::Ident> = tree.keys().map(|ns| ns_trait_ident(ns)).collect();

    let dispatch_arms: Vec<TokenStream> = tree
        .keys()
        .map(|ns| {
            let ns_str = ns.as_str();
            let dispatch_fn_ident = format_ident!("__dispatch_{}", ns);
            quote! {
                Some((#ns_str, rest)) => self.#dispatch_fn_ident(rest, heap, args, ctx, call_id)
            }
        })
        .collect();

    quote! {
        pub trait IoPackageBaml: #(#ns_trait_idents)+* {
            fn get_sys_op_fn(
                &self,
                path: &str,
                heap: &std::sync::Arc<BexHeap>,
                args: Vec<BexValue<'_>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> Option<SysOpResult> {
                match path.split_once('.') {
                    Some(("baml", rest)) => {
                        match rest.split_once('.') {
                            #(#dispatch_arms,)*
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
        }
    }
}

// ============================================================================
// SysOps struct
// ============================================================================

fn emit_sys_ops_struct(io_builtins: &[NativeBuiltin]) -> TokenStream {
    let field_idents: Vec<syn::Ident> = io_builtins
        .iter()
        .map(|b| format_ident!("{}", b.fn_name))
        .collect();
    let variant_idents: Vec<syn::Ident> = io_builtins
        .iter()
        .map(|b| format_ident!("{}", b.sys_op_variant_name()))
        .collect();

    let from_impl_fields: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let field_ident = format_ident!("{}", b.fn_name);
            let variant_ident = format_ident!("{}", b.sys_op_variant_name());
            let path_str = &b.path;
            quote! {
                #field_ident: {
                    let t = t.clone();
                    std::sync::Arc::new(move |heap, args, ctx, call_id| {
                        t.get_sys_op_fn(#path_str, heap, args, ctx, call_id)
                            .unwrap_or_else(|| SysOpResult::Ready(Err(OpError::new(
                                SysOp::#variant_ident,
                                OpErrorKind::Unsupported,
                            ))))
                    })
                }
            }
        })
        .collect();

    quote! {
        #[derive(Clone)]
        pub struct SysOps {
            #(pub #field_idents: SysOpFn,)*
        }

        impl SysOps {
            pub fn get(&self, op: SysOp) -> &SysOpFn {
                match op {
                    #(SysOp::#variant_idents => &self.#field_idents,)*
                }
            }

            pub fn unsupported(operation: SysOp) -> SysOpFn {
                std::sync::Arc::new(move |_, _, _, _| {
                    SysOpResult::Ready(Err(OpError::new(
                        operation,
                        OpErrorKind::Unsupported,
                    )))
                })
            }

            pub fn all_unsupported() -> Self {
                Self {
                    #(#field_idents: Self::unsupported(SysOp::#variant_idents),)*
                }
            }

            pub fn from_impl<T: IoPackageBaml + Send + Sync + 'static>(t: T) -> Self {
                let t = std::sync::Arc::new(t);
                Self {
                    #(#from_impl_fields,)*
                }
            }
        }
    }
}
