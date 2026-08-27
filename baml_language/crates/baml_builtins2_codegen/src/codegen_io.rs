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

use crate::{
    rust_ident::rust_field_ident,
    types::{BamlType, NativeBuiltin, NativeClassDef, Receiver},
};

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
    /// Stdlib package the builtins came from (`"baml"`, `"ai"`, …).
    package: String,
    /// Namespace segment inside that package (`"fs"`, `"internal"`, …).
    namespace: String,
    free_fns: Vec<&'a NativeBuiltin>,
    classes: BTreeMap<String, Vec<&'a NativeBuiltin>>,
}

impl IoNamespaceNode<'_> {
    fn new(package: &str, namespace: &str) -> Self {
        Self {
            package: package.to_string(),
            namespace: namespace.to_string(),
            free_fns: Vec::new(),
            classes: BTreeMap::new(),
        }
    }
}

/// Extract the package name from an IO builtin path (the first segment):
/// - `"baml.fs.open"` → `"baml"`
/// - `"ai.internal._gcp_access_token"` → `"ai"`
fn io_package_name(builtin: &NativeBuiltin) -> &str {
    builtin.path.split('.').next().unwrap_or("")
}

/// Extract the namespace name from an IO builtin path (the segments between
/// the package and the item):
/// - "baml.fs.open" → "fs"
/// - "baml.fs.File.read" → "fs"
/// - `"ai.internal._gcp_access_token"` → `"internal"`
/// - `"ai.OutputFormat._render"` -> `""` (top-level class method)
fn io_namespace_name(builtin: &NativeBuiltin) -> &str {
    // Carried structurally from extraction ([`NativeBuiltin::namespace`]):
    // the path alone cannot be split, because an impl-block class segment
    // embeds the WRITTEN interface target, which may itself be dotted
    // (`root.io.Read$for$FileHandle`).
    &builtin.namespace
}

/// Key a namespace node (and every identifier derived from it) by package +
/// namespace.
///
/// The `baml` package keeps the bare namespace as its key, so all pre-existing
/// generated names (`IoNamespaceFs`, `__dispatch_fs`, `IoClassFsFile`, …) and
/// the `class_ns_map`-keyed lookups into the tree are unchanged. Any other
/// package is package-qualified (`ai` + `internal` → `ai_internal` →
/// `IoNamespaceAiInternal`), so two packages' `ns_internal` folders can never
/// collide.
fn io_ns_key(package: &str, namespace: &str) -> String {
    if package == "baml" {
        namespace.to_string()
    } else if namespace.is_empty() {
        // Top-level items of a non-`baml` package (e.g. `ai.Context`) key on
        // the bare package name: `owned::ai::Context`, `IoClassAiContext`.
        package.to_string()
    } else {
        format!("{package}_{namespace}")
    }
}

/// Extract the method name (last segment) from an IO builtin path.
fn io_method_name(builtin: &NativeBuiltin) -> &str {
    builtin.path.rsplit('.').next().unwrap_or("")
}

/// The class-dispatch match key for an IO method: the final path segment.
///
/// A method declared inside an `implements I { ... }` block carries its
/// interface in the class segment (`...{I}$for${Class}.method`), so the
/// method name alone keys the dispatch — same as a direct method.
fn io_class_dispatch_key(builtin: &NativeBuiltin) -> String {
    io_method_name(builtin).to_string()
}

/// The class segment of an IO method path (the second-to-last segment): the
/// receiver's class name for a direct method (`baml.fs.File.read` → `"File"`),
/// the synthetic `{Iface}$for${Class}` for an implements-block method
/// (`baml.random.Rng$for$SystemRandom.random` → `"Rng$for$SystemRandom"`).
/// The namespace router matches the routed path segment against these, so a
/// class reached through both spellings needs every one as an arm key.
fn io_class_segment(builtin: &NativeBuiltin) -> &str {
    // Structural, like [`io_namespace_name`] — a dotted interface qualifier
    // makes the path unsplittable.
    builtin.class_segment.as_deref().unwrap_or("")
}

fn build_io_namespace_tree<'a>(
    io_builtins: &'a [NativeBuiltin],
) -> BTreeMap<String, IoNamespaceNode<'a>> {
    let mut tree: BTreeMap<String, IoNamespaceNode<'a>> = BTreeMap::new();

    for builtin in io_builtins {
        let package = io_package_name(builtin);
        let namespace = io_namespace_name(builtin);
        let node = tree
            .entry(io_ns_key(package, namespace))
            .or_insert_with(|| IoNamespaceNode::new(package, namespace));

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
        BamlType::Bigint => quote! { std::sync::Arc<num_bigint::BigInt> },
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
                    "type" => quote! { baml_type::RuntimeTy<baml_type::TaggedTypeName> },
                    "function" => quote! { BexExternalValue },
                    _ => quote! { BexExternalValue },
                }
            }
        }
        BamlType::Uint8Array => quote! { Vec<u8> },
        BamlType::Generic(_) | BamlType::Media(_) => quote! { BexExternalValue },
    }
}

/// Map a `BamlType` to the return type tokens for a view struct accessor.
fn view_return_type(ty: &BamlType, needs_heap: &mut bool) -> TokenStream {
    match ty {
        BamlType::Int => quote! { Result<i64, AccessError> },
        // Float is heap-boxed (`Object::Float`); the accessor needs a
        // `PermitProof` to deref the pointer soundly.
        BamlType::Float => {
            *needs_heap = true;
            quote! { Result<f64, AccessError> }
        }
        BamlType::Bool => quote! { Result<bool, AccessError> },
        // Bigints live behind a heap pointer (`Object::Bigint`) or by value in
        // an external value; either way `as_bigint` hands back an owned `Arc`,
        // so the accessor needs a `PermitProof` to deref the pointer soundly.
        BamlType::Bigint => {
            *needs_heap = true;
            quote! { Result<std::sync::Arc<num_bigint::BigInt>, AccessError> }
        }
        BamlType::String => {
            *needs_heap = true;
            quote! { Result<&'a bex_str::BexStr, AccessError> }
        }
        BamlType::RustType => {
            *needs_heap = true;
            quote! { Result<std::sync::Arc<dyn std::any::Any + Send + Sync>, AccessError> }
        }
        // The `type` metatype's owned representation is `RuntimeTy` (see the
        // owned-field mapping), so the accessor must return it too — not the
        // generic `BexExternalValue` fallback, which would not type-check.
        BamlType::Named(name) if name == "type" => {
            *needs_heap = true;
            quote! { Result<baml_type::RuntimeTy<baml_type::TaggedTypeName>, AccessError> }
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
        BamlType::Float => quote! { self.cls.field(#field_lit)?.as_float(heap, permit) },
        BamlType::Bool => quote! { self.cls.field(#field_lit)?.as_bool() },
        BamlType::Bigint => quote! { self.cls.field(#field_lit)?.as_bigint(heap, permit) },
        BamlType::String => quote! { self.cls.field(#field_lit)?.as_string(heap, permit) },
        BamlType::RustType => quote! { self.cls.field(#field_lit)?.as_rust_data(heap, permit) },
        BamlType::Named(name) if name == "type" => {
            quote! { self.cls.field(#field_lit)?.as_baml_type_owned(heap, permit) }
        }
        _ => quote! { self.cls.field(#field_lit)?.as_owned_but_very_slow(heap, permit) },
    }
}

/// Generate a Rust expression that converts a `BexExternalValue` (`val_expr`)
/// into the owned Rust type for `ty`, returning `Result<T, AccessError>`.
fn external_to_typed_expr(
    val_expr: &TokenStream,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    match ty {
        BamlType::String => quote! {
            match #val_expr {
                BexExternalValue::String(v) => Ok(v.to_string()),
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
        BamlType::Bigint => quote! {
            match #val_expr {
                BexExternalValue::Bigint(v) => Ok(std::sync::Arc::new(v)),
                other => Err(AccessError::TypeMismatch {
                    expected: "bigint",
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
            let inner_conv = external_to_typed_expr(&quote! { __v }, inner, class_ns_map, paths);
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
            let v_conv = external_to_typed_expr(&quote! { __v }, v, class_ns_map, paths);
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
            let inner_conv = external_to_typed_expr(&quote! { __v }, inner, class_ns_map, paths);
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
        BamlType::Uint8Array => quote! {
            match #val_expr {
                BexExternalValue::Uint8Array(v) => Ok(v),
                other => Err(AccessError::TypeMismatch {
                    expected: "uint8array",
                    actual: other.type_name().to_string(),
                }),
            }
        },
        // The `type` metatype's owned representation is `RuntimeTy`; unwrap it
        // from the external `Type` ADT rather than passing the raw
        // `BexExternalValue` through (which would not type-check against the
        // `RuntimeTy` owned field).
        BamlType::Named(name) if name == "type" => quote! {
            match #val_expr {
                BexExternalValue::Adt(bex_external_types::BexExternalAdt::Type(v)) => Ok(v),
                BexExternalValue::Adt(bex_external_types::BexExternalAdt::TypeDef(_)) => {
                    Err(AccessError::TypeMismatch {
                        expected: "a type value",
                        actual: "a portable type definition".to_string(),
                    })
                }
                other => Err(AccessError::TypeMismatch {
                    expected: "type",
                    actual: other.type_name().to_string(),
                }),
            }
        },
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
    let field_ident = rust_field_ident(field_name);
    match ty {
        BamlType::Int | BamlType::Bool => {
            quote! { self.#field_ident()? }
        }
        // Float accessor is now heap-aware (`Object::Float` deref).
        BamlType::Float => quote! { self.#field_ident(heap, permit)? },
        BamlType::String => quote! { self.#field_ident(heap, permit)?.to_string() },
        BamlType::RustType => quote! { self.#field_ident(heap, permit)? },
        BamlType::Uint8Array | BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let val = quote! { self.#field_ident(heap, permit)? };
            let conv = external_to_typed_expr(&val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        BamlType::Named(name) if class_ns_map.contains_key(name.as_str()) => {
            let val = quote! { self.#field_ident(heap, permit)? };
            let conv = external_to_typed_expr(&val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        _ => quote! { self.#field_ident(heap, permit)? },
    }
}

/// Emit a `baml_type::RuntimeTy` expression describing `ty`, for tagging a
/// `BexExternalValue::Array`/`Map` with its declared element/key/value type.
///
/// The owned→external conversion runs without a `vm` or call type-args in
/// scope, so a generic parameter cannot be resolved to its instantiation here
/// and degrades to `unknown`. Every concrete position — primitives, nested
/// containers, media, and named classes — is recovered exactly (never erased).
fn runtime_ty_tokens(ty: &BamlType) -> TokenStream {
    match ty {
        BamlType::String => quote! { baml_type::RuntimeTy::string() },
        BamlType::Int => quote! { baml_type::RuntimeTy::int() },
        BamlType::Bigint => quote! { baml_type::RuntimeTy::bigint() },
        BamlType::Float => quote! { baml_type::RuntimeTy::float() },
        BamlType::Bool => quote! { baml_type::RuntimeTy::bool() },
        BamlType::Null => quote! { baml_type::RuntimeTy::null() },
        BamlType::Uint8Array => quote! { baml_type::RuntimeTy::uint8array() },
        BamlType::List(inner) => {
            let inner = runtime_ty_tokens(inner);
            quote! { baml_type::RuntimeTy::list(#inner) }
        }
        BamlType::Map(key, value) => {
            let key = runtime_ty_tokens(key);
            let value = runtime_ty_tokens(value);
            quote! { baml_type::RuntimeTy::map(#key, #value) }
        }
        BamlType::Optional(inner) => {
            let inner = runtime_ty_tokens(inner);
            quote! { baml_type::RuntimeTy::optional(#inner) }
        }
        BamlType::Media(kind) => {
            let kind = media_kind_tokens(kind);
            quote! { baml_type::RuntimeTy::Media(#kind, baml_type::TyAttr::default()) }
        }
        BamlType::RustType => {
            quote! { baml_type::RuntimeTy::RustType { attr: baml_type::TyAttr::default() } }
        }
        // `Named` is a lossy catch-all: the type parser discards a class's
        // generic arguments (`Box<int>` → `Named("Box")`) and also funnels
        // unions/unresolved types through it (`Named("union")`,
        // `Named("unknown")`). Reconstructing a `RuntimeTy::class(name)` here
        // would both drop generics and fabricate a class for the placeholder
        // names, so this static-`BamlType` path cannot recover a named type — the
        // complete type lives only on the runtime value's stored `element_ty`.
        BamlType::Named(_) | BamlType::Generic(_) => {
            quote! { baml_type::RuntimeTy::unknown() }
        }
    }
}

/// The `baml_base::MediaKind` token for a `Media(name)` BAML type. Unknown names
/// degrade to the catch-all `Generic` media kind (never an erased type).
fn media_kind_tokens(name: &str) -> TokenStream {
    match name {
        "Image" => quote! { baml_base::MediaKind::Image },
        "Audio" => quote! { baml_base::MediaKind::Audio },
        "Video" => quote! { baml_base::MediaKind::Video },
        "Pdf" => quote! { baml_base::MediaKind::Pdf },
        _ => quote! { baml_base::MediaKind::Generic },
    }
}

/// Generate the `BexExternalValue` conversion expression for an owned field.
#[allow(clippy::only_used_in_recursion)]
fn owned_to_external_expr(
    field_expr: &TokenStream,
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
) -> TokenStream {
    match ty {
        BamlType::Int => quote! { BexExternalValue::Int(#field_expr) },
        BamlType::Bigint => quote! {
            BexExternalValue::Bigint(std::sync::Arc::unwrap_or_clone(#field_expr))
        },
        BamlType::Float => quote! { BexExternalValue::Float(#field_expr) },
        BamlType::Bool => quote! { BexExternalValue::Bool(#field_expr) },
        BamlType::String => quote! { BexExternalValue::String((#field_expr).into()) },
        BamlType::RustType => quote! { BexExternalValue::RustData(#field_expr) },
        BamlType::Null => quote! { BexExternalValue::Null },
        BamlType::List(inner) => {
            let inner_conv = owned_to_external_expr(&quote! { __v }, inner, class_ns_map);
            let element_type = runtime_ty_tokens(inner);
            quote! {
                BexExternalValue::Array {
                    element_type: #element_type,
                    items: #field_expr.into_iter().map(|__v| #inner_conv).collect(),
                }
            }
        }
        BamlType::Map(k, v) => {
            let v_conv = owned_to_external_expr(&quote! { __v }, v, class_ns_map);
            let key_type = runtime_ty_tokens(k);
            let value_type = runtime_ty_tokens(v);
            quote! {
                BexExternalValue::Map {
                    key_type: #key_type,
                    value_type: #value_type,
                    entries: #field_expr.into_iter().map(|(__k, __v)| (__k, #v_conv)).collect(),
                }
            }
        }
        BamlType::Optional(inner) => {
            let inner_conv = owned_to_external_expr(&quote! { __v }, inner, class_ns_map);
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
        BamlType::Bigint => quote! { std::sync::Arc<num_bigint::BigInt> },
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
                    "type" => quote! { baml_type::RuntimeTy<baml_type::TaggedTypeName> },
                    "unknown" => quote! { BexExternalValue },
                    "function" => quote! { BexExternalValue },
                    _ => quote! { BexExternalValue },
                }
            }
        }
        BamlType::Uint8Array => quote! { Vec<u8> },
        BamlType::Generic(_) | BamlType::Media(_) => quote! { BexExternalValue },
    }
}

/// True if `ty` is the builtin top-level `function` type — a callable BAML value
/// (function, closure, or bound method) passed to a sys-op, which crosses as a
/// rooted [`Handle`](bex_external_types::Handle) rather than a `BexExternalValue`.
///
/// Only matches a top-level `function` (and, via [`clean_param_type`], a
/// top-level-optional one). A `function` nested in a container (`list<…>` /
/// `map<…>`) or behind a type alias is NOT detected and would fall back to the
/// (closure-incompatible) `as_owned_but_very_slow` path — no such sys-op param
/// exists today; add handling here if one is introduced.
fn is_callable_param(ty: &BamlType, class_ns_map: &BTreeMap<String, String>) -> bool {
    matches!(ty, BamlType::Named(name) if name == "function" && !class_ns_map.contains_key(name.as_str()))
}

/// Like [`clean_rust_type`], but for a sys-op *parameter*. A `function`-typed
/// parameter is a callable BAML value passed into the op; it crosses as a rooted
/// heap [`Handle`](bex_external_types::Handle) (extracted via `as_callable_handle`)
/// that the op later invokes with `VmSpawner::spawn_with_callable`. An optional
/// callable (`((…) -> …)?`) becomes `Option<Handle>`. (A `function`-typed
/// *return*, by contrast, is a `FunctionRef` and stays `BexExternalValue` — so
/// this only diverges on the parameter side.)
fn clean_param_type(
    ty: &BamlType,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    if is_callable_param(ty, class_ns_map) {
        return quote! { bex_external_types::Handle };
    }
    if let BamlType::Optional(inner) = ty {
        if is_callable_param(inner, class_ns_map) {
            return quote! { Option<bex_external_types::Handle> };
        }
    }
    clean_rust_type(ty, class_ns_map, paths)
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
        BamlType::String => quote! { #arg_ident.as_string(heap.as_ref(), permit)?.to_string() },
        BamlType::Int => quote! { #arg_ident.as_int()? },
        BamlType::Bigint => quote! { #arg_ident.as_bigint(heap.as_ref(), permit)? },
        BamlType::Float => quote! { #arg_ident.as_float(heap.as_ref(), permit)? },
        BamlType::Bool => quote! { #arg_ident.as_bool()? },
        BamlType::Named(name) => {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                let view = &paths.view;
                let ns_ident = format_ident!("{}", ns);
                let name_ident = format_ident!("{}", name);
                quote! {
                    #arg_ident.as_builtin_class::<#view::#ns_ident::#name_ident>(heap.as_ref(), permit)?.into_owned(heap.as_ref(), permit)?
                }
            } else {
                match name.as_str() {
                    "type" => quote! { #arg_ident.as_baml_type_owned(heap.as_ref(), permit)? },
                    "function" => quote! { #arg_ident.as_callable_handle(heap.as_ref(), permit)? },
                    _ => quote! { #arg_ident.as_owned_but_very_slow(heap.as_ref(), permit)? },
                }
            }
        }
        BamlType::RustType => quote! { #arg_ident.as_rust_data(heap.as_ref(), permit)? },
        // An optional callable param (`((…) -> …)?`): `null` → `None`, else a
        // rooted handle. Must precede the general `Optional(_)` arm below.
        BamlType::Optional(inner) if is_callable_param(inner, class_ns_map) => {
            quote! { #arg_ident.as_optional_callable_handle(heap.as_ref(), permit)? }
        }
        BamlType::Uint8Array | BamlType::List(_) | BamlType::Map(_, _) | BamlType::Optional(_) => {
            let val = quote! { #arg_ident.as_owned_but_very_slow(heap.as_ref(), permit)? };
            let conv = external_to_typed_expr(&val, ty, class_ns_map, paths);
            quote! { (#conv)? }
        }
        _ => quote! { #arg_ident.as_owned_but_very_slow(heap.as_ref(), permit)? },
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

/// `PascalCase` a namespace key (`"fs"` → `"Fs"`, `"ai_internal"` → `"AiInternal"`).
/// No `baml` namespace contains an underscore, so this is identity-preserving
/// for every pre-existing generated name.
fn pascal_case_key(key: &str) -> String {
    key.split('_').map(capitalize_first).collect()
}

fn ns_trait_ident(ns_key: &str) -> syn::Ident {
    format_ident!("IoNamespace{}", pascal_case_key(ns_key))
}

fn class_trait_ident(ns_key: &str, class: &str) -> syn::Ident {
    format_ident!("IoClass{}{}", pascal_case_key(ns_key), class)
}

/// Whether the clean trait method and glue thread an *extracted* receiver value
/// to the impl.
///
/// True for a non-static receiver on an instance-backed (field-carrying) class:
/// the receiver is materialized as an `owned::{ns}::{Class}` and passed through.
///
/// A fieldless marker class (e.g. `random.SystemRandom`) still takes `self` in
/// BAML so it can satisfy an interface, but it carries no data and has no
/// generated `owned::`/`view::` struct. Its `self` arg slot is still consumed
/// by the glue (the VM pushes it), but the value is ignored and the clean
/// method takes no receiver parameter. `consumes_self_slot` covers that case.
fn receiver_is_extracted(receiver: &Receiver) -> bool {
    !receiver.receiver_type.is_static() && receiver.instance_backed
}

/// Whether the method consumes a leading `self` arg slot on the operand stack.
/// True for any non-static receiver, including fieldless marker classes whose
/// receiver value is consumed but ignored.
fn consumes_self_slot(receiver: &Receiver) -> bool {
    !receiver.receiver_type.is_static()
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
            // `None` (no clause) is rejected during extraction; `Some([])`
            // (`throws never`) and `None` both map to no error categories.
            // A `throws` entry that names one of the builtin's generic params
            // (e.g. `call_host_value<T, E> ... throws E`) is a *dynamic* error
            // contract, not a fixed `SysOpErrorCategory`. The static category
            // list cannot represent a mix of "concrete X" + "dynamic E" — the
            // dynamic component would be silently dropped, leaving
            // `validate_sys_op_error` to wrongly reject any `E`-shaped throw
            // as off-contract. Apply an all-or-nothing rule: if *any* throws
            // entry is generic, emit an empty list ("accept any category")
            // and defer the entire contract check to runtime. Concrete-only
            // contracts still emit a precise category list. (The `throws`
            // clause stays non-empty either way, so the builtin remains
            // `is_fallible`.)
            match &b.throws {
                Some(cats) => {
                    let has_generic_throw = cats.iter().any(|t| b.generics.iter().any(|g| g == t));
                    if has_generic_throw {
                        quote! { SysOp::#variant => &[] }
                    } else {
                        let cats: Vec<_> = cats
                            .iter()
                            .map(|t| {
                                format_ident!(
                                    "{}",
                                    t.rsplit('.').next().expect("throw type has a name")
                                )
                            })
                            .collect();
                        if cats.is_empty() {
                            quote! { SysOp::#variant => &[] }
                        } else {
                            quote! { SysOp::#variant => &[#(SysOpErrorCategory::#cats),*] }
                        }
                    }
                }
                None => quote! { SysOp::#variant => &[] },
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
        #[derive(
            Clone,
            Copy,
            Debug,
            PartialEq,
            Eq,
            ::borsh::BorshSerialize,
            ::borsh::BorshDeserialize,
        )]
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

    crate::format_tokens(&tokens)
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

    crate::format_tokens(&tokens)
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

    crate::format_tokens(&tokens)
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
            let field_ident = rust_field_ident(&field.name);

            if needs_heap {
                quote! {
                    pub fn #field_ident(
                        &self,
                        heap: &'a BexHeap,
                        permit: PermitProof<'a>,
                    ) -> #ret_type {
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
            let field_ident = rust_field_ident(&field.name);
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

            pub fn into_owned(
                self,
                heap: &'a BexHeap,
                permit: PermitProof<'a>,
            ) -> Result<#owned_path, AccessError> {
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

/// Compute the set of class names that cannot derive `Default`.
///
/// A class is non-defaultable if it directly contains a `$rust_type` field,
/// or if any of its fields transitively references a non-defaultable class.
///
/// Both a fully-qualified name (for example `baml.sap.ParseCache`) and its
/// short name are stored, because field type references may use
/// either form depending on whether the path was single- or multi-segment.
fn compute_non_defaultable_classes(
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
) -> std::collections::HashSet<String> {
    use crate::types::BamlType;

    fn references_non_defaultable(
        ty: &BamlType,
        non_defaultable: &std::collections::HashSet<String>,
    ) -> bool {
        match ty {
            BamlType::Named(name) => non_defaultable.contains(name),
            BamlType::List(inner) | BamlType::Optional(inner) => {
                references_non_defaultable(inner, non_defaultable)
            }
            BamlType::Map(k, v) => {
                references_non_defaultable(k, non_defaultable)
                    || references_non_defaultable(v, non_defaultable)
            }
            _ => false,
        }
    }

    // Collect all classes with both name forms.
    let all_classes: Vec<(&NativeClassDef, String)> = class_defs_by_ns
        .values()
        .flat_map(|classes| classes.iter().copied())
        .map(|cd| (cd, format!("{}.{}", cd.namespace_prefix, cd.name)))
        .collect();

    // Seed: classes with a direct non-`Default` field — insert both name forms.
    // `$rust_type` (`Arc<dyn Any>`) and the `type` metatype (`RuntimeTy`) are
    // both non-`Default`. (Container forms like `list<reflect.Type>`/`reflect.Type?` stay
    // defaultable — `Vec`/`Option` are `Default` — so only direct fields seed.)
    let mut non_defaultable: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (cd, full_name) in &all_classes {
        if cd.fields.iter().any(|f| {
            matches!(f.field_type, BamlType::RustType)
                || matches!(&f.field_type, BamlType::Named(name) if name == "type")
        }) {
            non_defaultable.insert(full_name.clone());
            non_defaultable.insert(cd.name.clone());
        }
    }

    // Fixed-point: propagate through Named references until stable.
    loop {
        let mut changed = false;
        for (cd, full_name) in &all_classes {
            if non_defaultable.contains(full_name) {
                continue;
            }
            if cd
                .fields
                .iter()
                .any(|f| references_non_defaultable(&f.field_type, &non_defaultable))
            {
                non_defaultable.insert(full_name.clone());
                non_defaultable.insert(cd.name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    non_defaultable
}

fn emit_owned_module(
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
    paths: &CodegenPaths,
) -> TokenStream {
    let non_defaultable = compute_non_defaultable_classes(class_defs_by_ns);

    let ns_modules: Vec<TokenStream> = class_defs_by_ns
        .iter()
        .map(|(ns, classes)| {
            let ns_ident = format_ident!("{}", ns);
            let structs: Vec<TokenStream> = classes
                .iter()
                .map(|cd| emit_owned_struct(cd, class_ns_map, ns, paths, &non_defaultable))
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
    non_defaultable: &std::collections::HashSet<String>,
) -> TokenStream {
    let name_ident = format_ident!("{}", cd.name);
    let full_path = format!("{}.{}", cd.namespace_prefix, cd.name);
    let source_comment = format!("Generated from `{}`", cd.source_file);

    // Struct definition
    let derives = if non_defaultable.contains(&full_path) {
        quote! { #[derive(Clone, Debug)] }
    } else {
        quote! { #[derive(Clone, Debug, Default)] }
    };

    // Struct fields
    let struct_fields: Vec<TokenStream> = cd
        .fields
        .iter()
        .map(|field| {
            let field_ident = rust_field_ident(&field.name);
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
            let field_ident = rust_field_ident(&field.name);
            let conv = owned_to_external_expr(
                &quote! { self.#field_ident },
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
            let field_ident = rust_field_ident(&field.name);
            let field_name_str = &field.name;
            let field_val = quote! {
                fields.swap_remove(#field_name_str).unwrap_or(BexExternalValue::Null)
            };
            let conv = external_to_typed_expr(&field_val, &field.field_type, class_ns_map, paths);
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
                    type_args: vec![],
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
        .flat_map(|(ns_key, node)| {
            node.classes.iter().map(move |(class_name, methods)| {
                emit_one_class_trait(
                    ns_key,
                    // Top-level classes of a non-`baml` package (empty
                    // namespace, e.g. `ai.Context`) live in the module named
                    // after the package, which equals their `ns_key`.
                    if node.namespace.is_empty() {
                        ns_key
                    } else {
                        &node.namespace
                    },
                    class_name,
                    methods,
                    class_ns_map,
                    paths,
                )
            })
        })
        .collect();

    quote! { #(#traits)* }
}

/// Returns the count of *function-level* generic type parameters for a builtin.
///
/// For IO class methods, the `NativeBuiltin.generics` list merges the enclosing
/// class's generics with the function's own generics.  Only the function-level
/// ones (those NOT contributed by the class) generate synthetic type-arg value
/// slots on the operand stack: class-level generics are part of the instance
/// type and are not threaded as extra stack args.
///
/// For free functions (no receiver), every generic is function-level, so this
/// just returns `generics.len()`.
fn fn_only_generic_count(builtin: &NativeBuiltin) -> usize {
    let class_generics: &[String] = builtin
        .receiver
        .as_ref()
        .map(|r| r.class_generics.as_slice())
        .unwrap_or(&[]);
    builtin
        .generics
        .iter()
        .filter(|g| !class_generics.contains(g))
        .count()
}

fn emit_one_class_trait(
    ns_key: &str,
    ns: &str,
    class_name: &str,
    methods: &[&NativeBuiltin],
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let trait_ident = class_trait_ident(ns_key, class_name);
    let dispatch_fn_ident = format_ident!("__dispatch_{}_{}", ns_key, class_name.to_lowercase());

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
            let Some(receiver) = &m.receiver else {
                return quote! {
                    compile_error!(concat!("missing receiver for method ", stringify!(#method_ident)));
                };
            };
            let receiver_param = if receiver_is_extracted(receiver) {
                let receiver_param_ident = format_ident!("{}", class_name.to_lowercase());
                let receiver_ty = quote! { #owned::#ns_ident::#class_ident };
                Some(quote! { #receiver_param_ident: #receiver_ty,})
            } else {
                None
            };

            let extra_params: Vec<TokenStream> = m
                .params
                .iter()
                .map(|p| {
                    let p_ident = format_ident!("{}", p.name);
                    let p_ty = clean_param_type(&p.ty, class_ns_map, paths);
                    quote! { #p_ident: #p_ty }
                })
                .collect();

            // Synthetic type-arg params appended after value params.
            // Only function-level generics (those NOT from the enclosing
            // class) generate type-arg slots — class-level generics are
            // part of the instance type and are not threaded as stack args.
            let fn_type_arg_count = fn_only_generic_count(m);
            let type_arg_params: Vec<TokenStream> = (0..fn_type_arg_count)
                .map(|i| {
                    let p_ident = format_ident!("type_arg_{}", i);
                    quote! { #p_ident: baml_type::RuntimeTy<baml_type::TaggedTypeName> }
                })
                .collect();

            quote! {
                fn #method_ident(
                    &self,
                    heap: &std::sync::Arc<BexHeap>,
                    call_id: CallId,
                    #receiver_param
                    #(#extra_params,)*
                    #(#type_arg_params,)*
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
            let method_name_str = io_class_dispatch_key(m);
            let glue_ident = format_ident!("__glue_{}", m.fn_name);
            quote! {
                #method_name_str => Some(self.#glue_ident(heap, permit, args, ctx, call_id))
            }
        })
        .collect();

    quote! {
        #[doc = #source_comment]
        pub trait #trait_ident {
            #(#clean_methods)*

            #(#glue_methods)*

            fn #dispatch_fn_ident<'a>(
                &self,
                method: &str,
                heap: &std::sync::Arc<BexHeap>,
                permit: PermitProof<'a>,
                args: Vec<BexValue<'a>>,
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
    let Some(receiver) = &builtin.receiver else {
        return quote! {
            compile_error!(concat!("missing receiver for glue method ", stringify!(#glue_ident)));
        };
    };
    let variant_ident = format_ident!("{}", builtin.sys_op_variant_name());
    let clean_method_ident = format_ident!("{}", io_method_name(builtin));

    let view = &paths.view;
    let ns_ident = format_ident!("{}", ns);
    let class_ident = format_ident!("{}", class_name);

    // Arg extraction lets
    let arg_self = if !consumes_self_slot(receiver) {
        None
    } else if receiver_is_extracted(receiver) {
        Some(quote! { let __arg_self = __args.next().unwrap(); })
    } else {
        // Fieldless marker receiver: consume the `self` slot but discard it —
        // the clean method takes no receiver and there is no `view::` struct.
        Some(quote! { let _ = __args.next().unwrap(); })
    };

    let arg_idents: Vec<syn::Ident> = (0..builtin.params.len())
        .map(|i| format_ident!("__arg{}", i))
        .collect();
    let arg_lets: Vec<TokenStream> = arg_idents
        .iter()
        .map(|id| quote! { let #id = __args.next().unwrap(); })
        .collect();

    // Synthetic type-arg slots: appended after all value args by the compiler.
    // Only function-level generics generate these slots; class-level generics
    // (from the enclosing class definition) do not.
    let fn_type_arg_count = fn_only_generic_count(builtin);
    let type_arg_idents: Vec<syn::Ident> = (0..fn_type_arg_count)
        .map(|i| format_ident!("__type_arg{}", i))
        .collect();
    let type_arg_lets: Vec<TokenStream> = type_arg_idents
        .iter()
        .map(|id| quote! { let #id = __args.next().unwrap(); })
        .collect();

    let receiver_extraction = if receiver_is_extracted(receiver) {
        Some(quote! {
            let __receiver = __arg_self
                .as_builtin_class::<#view::#ns_ident::#class_ident>(heap.as_ref(), permit)?
                .into_owned(heap.as_ref(), permit)?;
        })
    } else {
        None
    };

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

    // Extract synthetic type-arg slots as baml_type::RuntimeTy.
    let type_arg_extractions: Vec<TokenStream> = type_arg_idents
        .iter()
        .enumerate()
        .map(|(i, raw_id)| {
            let extracted_ident = format_ident!("__type_arg_val{}", i);
            quote! { let #extracted_ident = #raw_id.as_baml_type_owned(heap.as_ref(), permit)?; }
        })
        .collect();
    let type_arg_val_idents: Vec<syn::Ident> = (0..fn_type_arg_count)
        .map(|i| format_ident!("__type_arg_val{}", i))
        .collect();

    // Tuple elements for Ok return
    let receiver_ident = if receiver_is_extracted(receiver) {
        Some(quote! { __receiver, })
    } else {
        None
    };
    let tuple_idents: Vec<syn::Ident> = builtin
        .params
        .iter()
        .map(|p| format_ident!("__{}", p.name))
        .collect();

    // Call args for clean method
    let call_param_idents: Vec<syn::Ident> = builtin
        .params
        .iter()
        .map(|p| format_ident!("__{}", p.name))
        .collect();

    // Clean method type-arg call params (positional: type_arg_0, type_arg_1, ...)
    let clean_type_arg_call_idents: Vec<syn::Ident> = (0..fn_type_arg_count)
        .map(|i| format_ident!("type_arg_{}", i))
        .collect();

    // Bind extracted type-arg vals to the clean param names inside the match arm.
    let type_arg_bind_stmts: Vec<TokenStream> = type_arg_val_idents
        .iter()
        .zip(clean_type_arg_call_idents.iter())
        .map(|(val_id, param_id)| quote! { let #param_id = #val_id; })
        .collect();

    let call_expr = quote! {
        self.#clean_method_ident(heap, call_id, #receiver_ident #(#call_param_idents,)* #(#clean_type_arg_call_idents,)* ctx)
    };
    // Use the shared helper so a class method returning a class array
    // (`Vec<owned::ns::Class>`) is converted via `into_result_mapped` — the same
    // path free functions use — instead of an unconditional `into_result`
    // (which requires an `AsBexExternalValue` impl that `Vec<ClassName>` lacks).
    let into_result = emit_into_result_call(
        &builtin.return_type,
        &variant_ident,
        &call_expr,
        class_ns_map,
        paths,
    );

    quote! {
        fn #glue_ident<'a>(
            &self,
            heap: &std::sync::Arc<BexHeap>,
            permit: PermitProof<'a>,
            args: Vec<BexValue<'a>>,
            ctx: &SysOpContext,
            call_id: CallId,
        ) -> SysOpResult {
            let mut __args = args.into_iter();
            #arg_self
            #(#arg_lets)*
            // Synthetic type-arg slots (appended by lower_call for generic IO functions).
            #(#type_arg_lets)*

            let __extraction = (|| {
                #receiver_extraction
                #(#param_extractions)*
                #(#type_arg_extractions)*
                Ok::<_, AccessError>((#receiver_ident #(#tuple_idents,)* #(#type_arg_val_idents),*))
            })();

            match __extraction {
                Ok((#receiver_ident #(#tuple_idents,)* #(#type_arg_val_idents),*)) => {
                    #(#type_arg_bind_stmts)*
                    #into_result
                }
                Err(e) => SysOpResult::Ready(Err(OpError::new(
                    SysOp::#variant_ident,
                    bex_vm_types::errors::VmBamlError::AccessError { message: e.to_string() },
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
        .map(|(ns_key, node)| emit_one_namespace_trait(ns_key, node, class_ns_map, paths))
        .collect();

    quote! { #(#traits)* }
}

fn emit_one_namespace_trait(
    ns_key: &str,
    node: &IoNamespaceNode,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let trait_ident = ns_trait_ident(ns_key);
    let dispatch_fn_ident = format_ident!("__dispatch_{}", ns_key);

    // Supertraits: class traits in this namespace
    let class_trait_idents: Vec<syn::Ident> = node
        .classes
        .keys()
        .map(|cn| class_trait_ident(ns_key, cn))
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
                    let p_ty = clean_param_type(&p.ty, class_ns_map, paths);
                    quote! { #p_ident: #p_ty }
                })
                .collect();

            // Synthetic type-arg params appended after value params.
            let type_arg_params: Vec<TokenStream> = f
                .generics
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let p_ident = format_ident!("type_arg_{}", i);
                    quote! { #p_ident: baml_type::RuntimeTy<baml_type::TaggedTypeName> }
                })
                .collect();

            quote! {
                fn #fn_ident(
                    &self,
                    heap: &std::sync::Arc<BexHeap>,
                    call_id: CallId,
                    #(#extra_params,)*
                    #(#type_arg_params,)*
                    ctx: &SysOpContext,
                ) -> SysOpOutput<#ret_ty>;
            }
        })
        .collect();

    // Glue methods for free functions
    let free_fn_glues: Vec<TokenStream> = node
        .free_fns
        .iter()
        .map(|f| emit_free_fn_glue(f, &trait_ident, class_ns_map, paths))
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
                quote! { #fn_name_str => Some(self.#glue_ident(heap, permit, args, ctx, call_id)) }
            })
            .collect();

        quote! {
            match rest {
                #(#arms,)*
                _ => None,
            }
        }
    } else {
        // Mix of classes and free functions. A class's methods route by
        // stripping the class-segment prefix (with its trailing dot): the
        // bare class name for direct methods, `{Iface}$for${Class}` for
        // implements-block methods — the latter may itself be DOTTED
        // (`root.io.Read$for$File`), so a `split_once('.')` peel can never
        // match it and prefix-stripping is the one correct route. Longer
        // segments strip first so a segment that extends another
        // dot-for-dot cannot be shadowed.
        let mut segment_routes: Vec<(String, &str)> = Vec::new();
        for (cn, methods) in &node.classes {
            let segments: BTreeSet<&str> = methods.iter().map(|m| io_class_segment(m)).collect();
            for seg in segments {
                segment_routes.push((format!("{seg}."), cn.as_str()));
            }
        }
        segment_routes.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.0.cmp(&b.0)));
        let class_routes: Vec<TokenStream> = segment_routes
            .iter()
            .map(|(prefix, cn)| {
                let dispatch = format_ident!("__dispatch_{}_{}", ns_key, cn.to_lowercase());
                quote! {
                    if let Some(method) = rest.strip_prefix(#prefix) {
                        return self.#dispatch(method, heap, permit, args, ctx, call_id);
                    }
                }
            })
            .collect();

        let free_fn_arms: Vec<TokenStream> = node
            .free_fns
            .iter()
            .map(|f| {
                let fn_name_str = io_method_name(f);
                let glue_ident = format_ident!("__glue_{}", f.fn_name);
                quote! { #fn_name_str => Some(self.#glue_ident(heap, permit, args, ctx, call_id)) }
            })
            .collect();

        quote! {
            #(#class_routes)*
            match rest {
                #(#free_fn_arms,)*
                _ => None,
            }
        }
    };

    quote! {
        pub trait #trait_ident #supertrait_bound {
            #(#free_fn_clean)*

            #(#free_fn_glues)*

            fn #dispatch_fn_ident<'a>(
                &self,
                rest: &str,
                heap: &std::sync::Arc<BexHeap>,
                permit: PermitProof<'a>,
                args: Vec<BexValue<'a>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> Option<SysOpResult> {
                #dispatch_body
            }
        }
    }
}

/// Generate the `.into_result(...)` call for a given return type.
///
/// - Scalar types implementing `AsBexExternalValue` use `.into_result(op)`.
/// - `List(Named(...))` uses `.into_result_mapped(op, |v| ...)` because
///   `Vec<ClassName>` does not implement `AsBexExternalValue` (orphan rules
///   prevent it in the generated crate).
fn emit_into_result_call(
    return_type: &BamlType,
    variant_ident: &syn::Ident,
    call_expr: &TokenStream,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    if let BamlType::List(inner) = return_type {
        if let BamlType::Named(name) = inner.as_ref() {
            if let Some(ns) = class_ns_map.get(name.as_str()) {
                let owned = &paths.owned;
                let ns_ident = format_ident!("{}", ns);
                let name_ident = format_ident!("{}", name);
                return quote! {
                    #call_expr
                        .into_result_mapped(SysOp::#variant_ident, |v| {
                            BexExternalValue::Array {
                                // `#name` is a real class, but `BamlType` drops
                                // its generic args, so the complete element type
                                // is only recoverable from the runtime value.
                                element_type: baml_type::RuntimeTy::unknown(),
                                items: v.into_iter()
                                    .map(|item| <#owned::#ns_ident::#name_ident as AsBexExternalValue>::into_bex_external_value(item))
                                    .collect(),
                            }
                        })
                };
            }
        }
    }
    quote! {
        #call_expr.into_result(SysOp::#variant_ident)
    }
}

fn emit_free_fn_glue(
    builtin: &NativeBuiltin,
    ns_trait_ident: &syn::Ident,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let glue_ident = format_ident!("__glue_{}", builtin.fn_name);
    let variant_ident = format_ident!("{}", builtin.sys_op_variant_name());
    let clean_ident = format_ident!("{}", io_method_name(builtin));

    // Synthetic type-arg slots (appended after value args by the compiler).
    let type_arg_idents: Vec<syn::Ident> = (0..builtin.generics.len())
        .map(|i| format_ident!("__type_arg{}", i))
        .collect();
    let type_arg_lets: Vec<TokenStream> = type_arg_idents
        .iter()
        .map(|id| quote! { let #id = __args.next().unwrap(); })
        .collect();
    let type_arg_extractions: Vec<TokenStream> = type_arg_idents
        .iter()
        .enumerate()
        .map(|(i, raw_id)| {
            let extracted_ident = format_ident!("__type_arg_val{}", i);
            quote! { let #extracted_ident = #raw_id.as_baml_type_owned(heap.as_ref(), permit)?; }
        })
        .collect();
    let type_arg_val_idents: Vec<syn::Ident> = (0..builtin.generics.len())
        .map(|i| format_ident!("__type_arg_val{}", i))
        .collect();
    // Named params as passed to clean method: type_arg_0, type_arg_1, ...
    let clean_type_arg_params: Vec<syn::Ident> = (0..builtin.generics.len())
        .map(|i| format_ident!("type_arg_{}", i))
        .collect();
    let type_arg_bind_stmts: Vec<TokenStream> = type_arg_val_idents
        .iter()
        .zip(clean_type_arg_params.iter())
        .map(|(val_id, param_id)| quote! { let #param_id = #val_id; })
        .collect();

    if builtin.params.is_empty() && builtin.generics.is_empty() {
        let call_expr = quote! {
            #ns_trait_ident::#clean_ident(self, heap, call_id, ctx)
        };
        let into_result = emit_into_result_call(
            &builtin.return_type,
            &variant_ident,
            &call_expr,
            class_ns_map,
            paths,
        );
        return quote! {
            fn #glue_ident<'a>(
                &self,
                heap: &std::sync::Arc<BexHeap>,
                _permit: PermitProof<'a>,
                args: Vec<BexValue<'a>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> SysOpResult {
                #into_result
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

    // Build ok_pattern and extraction_return accounting for type args.
    let all_extracted_idents: Vec<syn::Ident> = param_idents
        .iter()
        .chain(type_arg_val_idents.iter())
        .cloned()
        .collect();

    let ok_pattern = if all_extracted_idents.len() == 1 {
        let id = &all_extracted_idents[0];
        quote! { #id }
    } else {
        quote! { (#(#all_extracted_idents),*) }
    };

    let extraction_return = if all_extracted_idents.len() == 1 {
        let id = &all_extracted_idents[0];
        quote! { Ok::<_, AccessError>(#id) }
    } else {
        quote! { Ok::<_, AccessError>((#(#all_extracted_idents),*)) }
    };

    let call_expr = quote! {
        #ns_trait_ident::#clean_ident(self, heap, call_id, #(#param_idents,)* #(#clean_type_arg_params,)* ctx)
    };
    let into_result = emit_into_result_call(
        &builtin.return_type,
        &variant_ident,
        &call_expr,
        class_ns_map,
        paths,
    );

    quote! {
        fn #glue_ident<'a>(
            &self,
            heap: &std::sync::Arc<BexHeap>,
            permit: PermitProof<'a>,
            args: Vec<BexValue<'a>>,
            ctx: &SysOpContext,
            call_id: CallId,
        ) -> SysOpResult {
            let mut __args = args.into_iter();
            #(#arg_lets)*
            // Synthetic type-arg slots (appended by lower_call for generic IO functions).
            #(#type_arg_lets)*

            let __extraction = (|| {
                #(#param_extractions)*
                #(#type_arg_extractions)*
                #extraction_return
            })();

            match __extraction {
                Ok(#ok_pattern) => {
                    #(#type_arg_bind_stmts)*
                    #into_result
                }
                Err(e) => SysOpResult::Ready(Err(OpError::new(
                    SysOp::#variant_ident,
                    bex_vm_types::errors::VmBamlError::AccessError { message: e.to_string() },
                ))),
            }
        }
    }
}

// ============================================================================
// Root trait
// ============================================================================

/// Emit the root trait composing every namespace trait, with a
/// package-then-namespace router.
///
/// The trait keeps the name `IoPackageBaml` (it is implemented by every host
/// sys-op provider) even though it now also carries sys-ops declared in other
/// stdlib packages — `ai.internal.*` today.
fn emit_root_trait(tree: &BTreeMap<String, IoNamespaceNode>) -> TokenStream {
    let ns_trait_idents: Vec<syn::Ident> = tree.keys().map(|ns| ns_trait_ident(ns)).collect();

    // Group namespaces by package so the router matches `{package}.{ns}.{rest}`.
    let mut by_package: BTreeMap<&str, Vec<(&str, syn::Ident)>> = BTreeMap::new();
    for (ns_key, node) in tree {
        by_package.entry(node.package.as_str()).or_default().push((
            node.namespace.as_str(),
            format_ident!("__dispatch_{}", ns_key),
        ));
    }

    let package_arms: Vec<TokenStream> = by_package
        .iter()
        .map(|(package, namespaces)| {
            let ns_arms: Vec<TokenStream> = namespaces
                .iter()
                .filter(|(ns_str, _)| !ns_str.is_empty())
                .map(|(ns_str, dispatch_fn_ident)| {
                    quote! {
                        Some((#ns_str, rest)) => self.#dispatch_fn_ident(rest, heap, permit, args, ctx, call_id)
                    }
                })
                .collect();
            // A package's top-level classes (empty namespace, e.g.
            // `ai.OutputFormat._render`) have no namespace segment to
            // consume: route the full `{Class}.{method}` rest to the
            // package-root dispatcher instead of `None`.
            let fallback = namespaces
                .iter()
                .find(|(ns_str, _)| ns_str.is_empty())
                .map(|(_, dispatch_fn_ident)| {
                    quote! { _ => self.#dispatch_fn_ident(rest, heap, permit, args, ctx, call_id) }
                })
                .unwrap_or_else(|| quote! { _ => None });
            quote! {
                Some((#package, rest)) => {
                    match rest.split_once('.') {
                        #(#ns_arms,)*
                        #fallback,
                    }
                }
            }
        })
        .collect();

    quote! {
        pub trait IoPackageBaml: #(#ns_trait_idents)+* {
            fn get_sys_op_fn<'a>(
                &self,
                path: &str,
                heap: &std::sync::Arc<BexHeap>,
                permit: PermitProof<'a>,
                args: Vec<BexValue<'a>>,
                ctx: &SysOpContext,
                call_id: CallId,
            ) -> Option<SysOpResult> {
                match path.split_once('.') {
                    #(#package_arms,)*
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
                    std::sync::Arc::new(move |heap, permit, args, ctx, call_id| {
                        t.get_sys_op_fn(#path_str, heap, permit, args, ctx, call_id)
                            .unwrap_or_else(|| SysOpResult::Ready(Err(OpError::new(
                                SysOp::#variant_ident,
                                bex_vm_types::errors::VmBamlError::Unsupported {
                                    message: "Operation not supported on this platform".to_string(),
                                },
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
                std::sync::Arc::new(move |_, _, _, _, _| {
                    SysOpResult::Ready(Err(OpError::new(
                        operation,
                        bex_vm_types::errors::VmBamlError::Unsupported {
                            message: "Operation not supported on this platform".to_string(),
                        },
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

// ============================================================================
// RuntimeIo trait generation (for sys_types)
// ============================================================================

/// Derive the `RuntimeIo` trait method name from a builtin path.
///
/// - Free functions: `{ns}_{method}` (e.g. `"baml.http.send"` -> `"http_send"`)
/// - Class methods: `{ns}_{class}_{method}` lowercase (e.g. `"baml.http.Response.text"` -> `"http_response_text"`)
///
/// Only the `baml` package prefix is stripped, so a sys-op from another stdlib
/// package keeps it (`"ai.internal._gcp_access_token"` ->
/// `"ai_internal__gcp_access_token"`) and can never collide with a `baml` one.
fn runtime_io_method_name(builtin: &NativeBuiltin) -> String {
    // `$` appears in impl-block method paths (`{iface}$for${Class}.method`)
    // and is not a valid identifier character.
    let after_baml = builtin.path.strip_prefix("baml.").unwrap_or(&builtin.path);
    after_baml.replace(['.', '$'], "_").to_lowercase()
}

/// Derive the handle type name for a class (e.g. `"Response"` in namespace `"http"` -> `"HttpResponseHandle"`).
fn handle_type_name(ns: &str, class: &str) -> syn::Ident {
    format_ident!("{}{}Handle", capitalize_first(ns), class)
}

/// Generate the `RuntimeIo` trait, handle types, `RuntimeIoError`, and `NoopRuntimeIo`.
///
/// This is included in `sys_types` so that `sys_ops` and `sys_auth` can use it.
pub fn generate_runtime_io(
    io_builtins: &[NativeBuiltin],
    class_defs: &[NativeClassDef],
    structs_path: &str,
) -> String {
    let tree = build_io_namespace_tree(io_builtins);
    let io_class_defs = filter_io_class_defs(io_builtins, class_defs);
    let class_ns_map = build_class_ns_map(&io_class_defs);
    let class_defs_by_ns = group_class_defs_by_ns(&io_class_defs);

    let paths = CodegenPaths::external(structs_path);

    let error_type = emit_runtime_io_error();
    let handles = emit_runtime_io_handles(
        &tree,
        &io_class_defs,
        &class_ns_map,
        &class_defs_by_ns,
        &paths,
    );
    let trait_def = emit_runtime_io_trait(io_builtins, &tree, &class_ns_map, &paths);
    let noop = emit_noop_runtime_io(io_builtins, &tree, &class_ns_map, &paths);

    let tokens = quote! {
        use std::pin::Pin;
        use std::future::Future;
        use std::sync::Arc;
        use std::panic::{UnwindSafe, RefUnwindSafe};

        #error_type
        #handles
        #trait_def
        #noop
    };

    crate::format_tokens(&tokens)
}

fn emit_runtime_io_error() -> TokenStream {
    quote! {
        #[derive(Debug, Clone)]
        pub enum RuntimeIoError {
            Unsupported,
            Other(String),
        }

        impl std::fmt::Display for RuntimeIoError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    RuntimeIoError::Unsupported => write!(f, "unsupported operation"),
                    RuntimeIoError::Other(msg) => write!(f, "{msg}"),
                }
            }
        }

        impl std::error::Error for RuntimeIoError {}
    }
}

/// Emit handle structs for classes that have `$rust_io_function` methods.
fn emit_runtime_io_handles(
    tree: &BTreeMap<String, IoNamespaceNode>,
    _io_class_defs: &[&NativeClassDef],
    class_ns_map: &BTreeMap<String, String>,
    class_defs_by_ns: &BTreeMap<String, Vec<&NativeClassDef>>,
    paths: &CodegenPaths,
) -> TokenStream {
    let mut handles = Vec::new();

    for (ns, node) in tree {
        for (class_name, methods) in &node.classes {
            // Fieldless marker classes (e.g. `random.SystemRandom`) have no
            // generated `owned::` struct, so there is nothing to wrap in a
            // handle and `from_external` would not resolve. They are never used
            // as a receiver handle in the `RuntimeIo` trait either (see
            // `emit_runtime_io_trait`), so skip them entirely.
            let instance_backed = methods
                .first()
                .and_then(|m| m.receiver.as_ref())
                .is_none_or(|r| r.instance_backed);
            if !instance_backed {
                continue;
            }
            let handle_ident = handle_type_name(ns, class_name);

            // Find the class def to get non-opaque fields.
            let class_def = class_defs_by_ns
                .get(ns.as_str())
                .and_then(|defs| defs.iter().find(|cd| cd.name == *class_name));

            // Build public fields for non-$rust_type fields.
            let mut pub_fields = Vec::new();
            let mut from_raw_fields = Vec::new();
            if let Some(cd) = class_def {
                for field in &cd.fields {
                    if field.field_type == BamlType::RustType {
                        continue;
                    }
                    let field_ident = rust_field_ident(&field.name);
                    let field_ty = owned_rust_type(&field.field_type, class_ns_map, paths);
                    pub_fields.push(quote! { pub #field_ident: #field_ty });

                    let val_expr = quote! { __owned.#field_ident };
                    from_raw_fields.push(quote! { #field_ident: #val_expr });
                }
            }

            let owned = &paths.owned;
            let ns_ident = format_ident!("{}", ns);
            let class_ident = format_ident!("{}", class_name);
            let owned_ty = quote! { #owned::#ns_ident::#class_ident };

            handles.push(quote! {
                pub struct #handle_ident {
                    pub raw: BexExternalValue,
                    #(#pub_fields,)*
                }

                impl #handle_ident {
                    pub fn from_raw(raw: BexExternalValue) -> Result<Self, RuntimeIoError> {
                        let __owned = #owned_ty::from_external(raw.clone())
                            .map_err(|e| RuntimeIoError::Other(format!("{e:?}")))?;
                        Ok(Self {
                            raw,
                            #(#from_raw_fields,)*
                        })
                    }
                }
            });
        }
    }

    quote! { #(#handles)* }
}

/// For a given builtin's return type, produce the `RuntimeIo` trait return type.
/// If the return type is a class that has a handle type, return the handle type instead.
fn runtime_io_return_type(
    builtin: &NativeBuiltin,
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    if let BamlType::Named(name) = &builtin.return_type {
        if let Some(ns) = class_ns_map.get(name.as_str()) {
            if let Some(node) = tree.get(ns) {
                if node.classes.contains_key(name.as_str()) {
                    let handle = handle_type_name(ns, name);
                    return quote! { #handle };
                }
            }
        }
    }
    clean_rust_type(&builtin.return_type, class_ns_map, paths)
}

fn emit_runtime_io_trait(
    io_builtins: &[NativeBuiltin],
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let mut methods = Vec::new();

    for builtin in io_builtins {
        let method_name = runtime_io_method_name(builtin);
        let method_ident = format_ident!("{}", method_name);
        let ret_ty = runtime_io_return_type(builtin, tree, class_ns_map, paths);

        let mut params: Vec<TokenStream> = Vec::new();

        // For class methods, the first param is a handle reference. Fieldless
        // marker receivers (e.g. `random.SystemRandom`) have no handle type, so
        // they take no receiver param — the adapter synthesizes their `self`.
        if let Some(ref receiver) = builtin.receiver {
            if receiver.instance_backed {
                // Key by package+namespace so top-level classes of non-`baml`
                // packages (`ai.Context` → `AiContextHandle`) match the
                // handle-definition loop, which iterates tree keys.
                let ns = io_ns_key(io_package_name(builtin), io_namespace_name(builtin));
                let handle = handle_type_name(&ns, &receiver.class_name);
                let param_ident = format_ident!("{}", receiver.class_name.to_lowercase());
                params.push(quote! { #param_ident: &#handle });
            }
        }

        for p in &builtin.params {
            let p_ident = format_ident!("{}", p.name);
            // The host-facing `RuntimeIo` path uses `clean_rust_type` (not
            // `clean_param_type`): a `function` param stays `BexExternalValue`
            // here on purpose. A host can't synthesize a VM closure `Handle`, so
            // this path is never used for callbacks — don't "unify" it with the
            // VM-facing trait's `Handle` mapping.
            let p_ty = clean_rust_type(&p.ty, class_ns_map, paths);
            params.push(quote! { #p_ident: #p_ty });
        }

        methods.push(quote! {
            fn #method_ident(&self, #(#params),*)
                -> Pin<Box<dyn Future<Output = Result<#ret_ty, RuntimeIoError>> + Send + '_>>
            {
                Box::pin(std::future::ready(Err(RuntimeIoError::Unsupported)))
            }
        });
    }

    quote! {
        pub trait RuntimeIo: Send + Sync + UnwindSafe + RefUnwindSafe {
            #(#methods)*
        }
    }
}

fn emit_noop_runtime_io(
    _io_builtins: &[NativeBuiltin],
    _tree: &BTreeMap<String, IoNamespaceNode>,
    _class_ns_map: &BTreeMap<String, String>,
    _paths: &CodegenPaths,
) -> TokenStream {
    quote! {
        pub struct NoopRuntimeIo;
        impl RuntimeIo for NoopRuntimeIo {}
    }
}

// ============================================================================
// RuntimeIoAdapter generation (for sys_ops)
// ============================================================================

/// Generate the `RuntimeIoAdapter` struct, `RuntimeIo` impl, and `build_runtime_io()`.
///
/// This is included in `sys_ops` and bridges the `SysOpFn` pointers to the `RuntimeIo` trait.
pub fn generate_io_adapter(
    io_builtins: &[NativeBuiltin],
    class_defs: &[NativeClassDef],
    structs_path: &str,
) -> String {
    let tree = build_io_namespace_tree(io_builtins);
    let io_class_defs = filter_io_class_defs(io_builtins, class_defs);
    let class_ns_map = build_class_ns_map(&io_class_defs);

    let paths = CodegenPaths::external(structs_path);

    let adapter_struct = emit_adapter_struct(io_builtins);
    let adapter_impl = emit_adapter_impl(io_builtins, &tree, &class_ns_map, &paths);
    let build_fn = emit_build_runtime_io(io_builtins);
    let resolve_fn = emit_resolve_helper();

    let tokens = quote! {
        // Bring `HeapPermit::proof()` into scope for the adapter impl below.
        use ::bex_heap::HeapPermit as _;

        #resolve_fn
        #adapter_struct
        #adapter_impl
        #build_fn
    };

    crate::format_tokens(&tokens)
}

fn emit_resolve_helper() -> TokenStream {
    quote! {
        async fn __resolve_sys_op_result(
            result: SysOpResult,
        ) -> Result<BexExternalValue, RuntimeIoError> {
            match result {
                SysOpResult::Ready(Ok(val)) => Ok(val),
                SysOpResult::Ready(Err(e)) => Err(RuntimeIoError::Other(format!("{e:?}"))),
                SysOpResult::Async(fut) => {
                    fut.await.map_err(|e| RuntimeIoError::Other(format!("{e:?}")))
                }
            }
        }
    }
}

fn emit_adapter_struct(io_builtins: &[NativeBuiltin]) -> TokenStream {
    let fields: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let field_ident = format_ident!("{}", runtime_io_method_name(b));
            quote! { #field_ident: SysOpFn }
        })
        .collect();

    quote! {
        pub struct RuntimeIoAdapter {
            heap: Arc<BexHeap>,
            permit_manager: Arc<HeapPermitManager>,
            ctx: SysOpContext,
            #(#fields,)*
        }

        /// SAFETY: We never catch panics across the `SysOpFn` boundaries.
        /// The bounds are required by the `RuntimeIo` trait (for AWS SDK compatibility).
        impl std::panic::UnwindSafe for RuntimeIoAdapter {}
        impl std::panic::RefUnwindSafe for RuntimeIoAdapter {}
    }
}

fn emit_adapter_impl(
    io_builtins: &[NativeBuiltin],
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    let mut methods = Vec::new();

    for builtin in io_builtins {
        let method_name = runtime_io_method_name(builtin);
        let method_ident = format_ident!("{}", method_name);
        let ret_ty = runtime_io_return_type(builtin, tree, class_ns_map, paths);

        let mut params: Vec<TokenStream> = Vec::new();

        // Build the marshaling: convert typed args to BexExternalValue, then to BexValue refs.
        let mut ext_bindings = Vec::new();
        let mut arg_exprs = Vec::new();

        if let Some(ref receiver) = builtin.receiver {
            let ns = io_ns_key(io_package_name(builtin), io_namespace_name(builtin));
            if receiver.instance_backed {
                let handle = handle_type_name(&ns, &receiver.class_name);
                let param_ident = format_ident!("{}", receiver.class_name.to_lowercase());
                params.push(quote! { #param_ident: &#handle });
                ext_bindings.push(quote! { let __recv_raw = #param_ident.raw.clone(); });
                arg_exprs.push(quote! { BexValue::ExternalValue(&__recv_raw) });
            } else if !receiver.receiver_type.is_static() {
                // Fieldless marker receiver: the SysOpFn glue still consumes a
                // leading `self` slot, so synthesize an empty instance for it.
                // The `RuntimeIo` method itself takes no receiver param.
                let class_fqn = format!(
                    "{}.{}.{}",
                    io_package_name(builtin),
                    ns,
                    receiver.class_name
                );
                ext_bindings.push(quote! {
                    let __recv_raw =
                        BexExternalValue::instance(#class_fqn, indexmap::IndexMap::new());
                });
                arg_exprs.push(quote! { BexValue::ExternalValue(&__recv_raw) });
            }
        }

        for (i, p) in builtin.params.iter().enumerate() {
            let p_ident = format_ident!("{}", p.name);
            let p_ty = clean_rust_type(&p.ty, class_ns_map, paths);
            params.push(quote! { #p_ident: #p_ty });

            let ext_ident = format_ident!("__ext_{}", i);
            let ext_expr = owned_to_external_expr(&quote! { #p_ident }, &p.ty, class_ns_map);
            ext_bindings.push(quote! { let #ext_ident: BexExternalValue = #ext_expr; });
            arg_exprs.push(quote! { BexValue::ExternalValue(&#ext_ident) });
        }

        let result_conversion = emit_result_conversion(builtin, tree, class_ns_map, paths);

        let body = quote! {
            let fn_ptr = self.#method_ident.clone();
            let heap = self.heap.clone();
            let permit_manager = self.permit_manager.clone();
            let ctx = self.ctx.clone();
            #(#ext_bindings)*
            Box::pin(async move {
                // Acquire a `()`-backed permit so the SysOpFn has a valid GC-exclusion
                // proof for arg extraction. RuntimeIoAdapter callers run outside the VM
                // event loop, so no other permit is in scope here.
                let permit = permit_manager.new_permit(()).await.acquire().await;
                let result = fn_ptr(
                    &heap,
                    permit.proof(),
                    vec![#(#arg_exprs),*],
                    &ctx,
                    CallId::next(),
                );
                drop(permit);
                let __val = __resolve_sys_op_result(result).await?;
                #result_conversion
            })
        };

        methods.push(quote! {
            fn #method_ident(&self, #(#params),*)
                -> Pin<Box<dyn Future<Output = Result<#ret_ty, RuntimeIoError>> + Send + '_>>
            {
                #body
            }
        });
    }

    quote! {
        impl RuntimeIo for RuntimeIoAdapter {
            #(#methods)*
        }
    }
}

/// Generate the expression that converts `__val: BexExternalValue` to the method's return type.
///
/// Top-level entry: looks up handle classes in `tree` and supports `Null`
/// (returning unit). Recursive calls into containers pass `tree = None` and
/// override the error suffix so messages reflect the nesting context.
fn emit_result_conversion(
    builtin: &NativeBuiltin,
    tree: &BTreeMap<String, IoNamespaceNode>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
) -> TokenStream {
    emit_result_conversion_for_ty(&builtin.return_type, Some(tree), class_ns_map, paths, "")
}

fn emit_result_conversion_for_ty(
    ty: &BamlType,
    tree: Option<&BTreeMap<String, IoNamespaceNode>>,
    class_ns_map: &BTreeMap<String, String>,
    paths: &CodegenPaths,
    ctx: &str,
) -> TokenStream {
    // Handle-class shortcut: only at the top level (lists/maps of handles
    // aren't supported).
    if let (Some(tree), BamlType::Named(name)) = (tree, ty) {
        if let Some(ns) = class_ns_map.get(name.as_str()) {
            if let Some(node) = tree.get(ns) {
                if node.classes.contains_key(name.as_str()) {
                    let handle = handle_type_name(ns, name);
                    return quote! { #handle::from_raw(__val) };
                }
            }
        }
    }

    match ty {
        BamlType::String => {
            let msg = format!("expected string{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::String(s) => Ok(s.to_string()),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Int => {
            let msg = format!("expected int{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Int(v) => Ok(v),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Bigint => {
            let msg = format!("expected bigint{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Bigint(v) => Ok(std::sync::Arc::new(v)),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Float => {
            let msg = format!("expected float{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Float(v) => Ok(v),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Bool => {
            let msg = format!("expected bool{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Bool(v) => Ok(v),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Uint8Array => {
            let msg = format!("expected uint8array{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Uint8Array(v) => Ok(v),
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        // `Null` as a return type means unit; only meaningful at the top level.
        BamlType::Null if tree.is_some() => quote! { Ok(()) },
        BamlType::Optional(inner) => {
            let inner_conv = emit_result_conversion_for_ty(inner, None, class_ns_map, paths, ctx);
            quote! {
                match __val {
                    BexExternalValue::Null => Ok(None),
                    other => {
                        let __val = other;
                        Ok(Some({ #inner_conv }?))
                    }
                }
            }
        }
        BamlType::List(inner) => {
            let inner_conv =
                emit_result_conversion_for_ty(inner, None, class_ns_map, paths, " in list");
            let msg = format!("expected array{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Array { items, .. } => {
                        items.into_iter()
                            .map(|__val| { #inner_conv })
                            .collect::<Result<Vec<_>, _>>()
                    }
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Map(key, value) if matches!(key.as_ref(), BamlType::String) => {
            let value_conv =
                emit_result_conversion_for_ty(value, None, class_ns_map, paths, " in map");
            let msg = format!("expected map{ctx}, got {{}}");
            quote! {
                match __val {
                    BexExternalValue::Map { entries, .. } => {
                        entries.into_iter()
                            .map(|(__key, __val)| Ok((__key, { #value_conv }?)))
                            .collect::<Result<indexmap::IndexMap<_, _>, _>>()
                    }
                    other => Err(RuntimeIoError::Other(
                        format!(#msg, other.type_name()),
                    )),
                }
            }
        }
        BamlType::Named(name) => match name.as_str() {
            "type" => {
                let msg = format!("expected type{ctx}, got {{}}");
                quote! {
                    match __val {
                        BexExternalValue::Adt(
                            bex_external_types::BexExternalAdt::Type(ty),
                        ) => Ok(ty),
                        other => Err(RuntimeIoError::Other(
                            format!(#msg, other.type_name()),
                        )),
                    }
                }
            }
            _ => {
                if let Some(ns) = class_ns_map.get(name.as_str()) {
                    let owned = &paths.owned;
                    let ns_ident = format_ident!("{}", ns);
                    let name_ident = format_ident!("{}", name);
                    quote! {
                        #owned::#ns_ident::#name_ident::from_external(__val)
                            .map_err(|e| RuntimeIoError::Other(format!("{e:?}")))
                    }
                } else {
                    quote! { Ok(__val) }
                }
            }
        },
        _ => quote! { Ok(__val) },
    }
}

fn emit_build_runtime_io(io_builtins: &[NativeBuiltin]) -> TokenStream {
    let field_inits: Vec<TokenStream> = io_builtins
        .iter()
        .map(|b| {
            let field_ident = format_ident!("{}", runtime_io_method_name(b));
            let sys_ops_field = format_ident!("{}", b.fn_name);
            quote! { #field_ident: sys_ops.#sys_ops_field.clone() }
        })
        .collect();

    quote! {
        pub fn build_runtime_io(
            sys_ops: &SysOps,
            heap: &Arc<BexHeap>,
            permit_manager: &Arc<HeapPermitManager>,
            ctx: &SysOpContext,
        ) -> Arc<dyn RuntimeIo> {
            Arc::new(RuntimeIoAdapter {
                heap: heap.clone(),
                permit_manager: permit_manager.clone(),
                ctx: ctx.clone(),
                #(#field_inits,)*
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use super::{CodegenPaths, emit_owned_struct, emit_view_struct};
    use crate::{
        rust_ident::rust_field_ident,
        types::{BamlType, NativeClassDef, NativeClassField},
    };

    #[test]
    fn generated_keyword_fields_keep_original_vm_keys() {
        const BAML_FIELD_NAMES: &[&str] = &[
            "type",
            "match",
            "move",
            "self",
            "self_",
            "Self",
            "super",
            "crate",
            "_",
            "dash-name",
            "$data",
            "__baml_field_73656c66",
        ];
        let class = NativeClassDef {
            name: "KeywordFields".to_string(),
            namespace_prefix: "baml.test".to_string(),
            generic_params: Vec::new(),
            fields: BAML_FIELD_NAMES
                .iter()
                .copied()
                .enumerate()
                .map(|(index, name)| NativeClassField {
                    name: name.to_string(),
                    field_type: BamlType::Bool,
                    index,
                })
                .collect(),
            source_file: "<test>/keywords.baml".to_string(),
        };
        let class_ns_map = BTreeMap::new();
        let paths = CodegenPaths::inline();

        let owned = crate::format_tokens(&emit_owned_struct(
            &class,
            &class_ns_map,
            "test",
            &paths,
            &HashSet::new(),
        ));
        let view = crate::format_tokens(&emit_view_struct(&class, &class_ns_map, "test", &paths));
        let compact_owned: String = owned.chars().filter(|c| !c.is_whitespace()).collect();
        let compact_view: String = view.chars().filter(|c| !c.is_whitespace()).collect();

        for baml_name in BAML_FIELD_NAMES {
            let rust_name = rust_field_ident(baml_name);
            assert!(
                compact_owned.contains(&format!("pub{rust_name}:bool")),
                "missing escaped owned field `{rust_name}`:\n{owned}"
            );
            assert!(
                compact_view.contains(&format!("pubfn{rust_name}(")),
                "missing escaped view accessor `{rust_name}`:\n{view}"
            );
        }

        for baml_name in BAML_FIELD_NAMES {
            assert!(
                compact_owned.contains(&format!("\"{baml_name}\".to_string()")),
                "external conversion changed the BAML key `{baml_name}`:\n{owned}"
            );
            assert!(
                compact_owned.contains(&format!("fields.swap_remove(\"{baml_name}\")")),
                "external lookup changed the BAML key `{baml_name}`:\n{owned}"
            );
            assert!(
                compact_view.contains(&format!("self.cls.field(\"{baml_name}\")")),
                "VM lookup changed the BAML key `{baml_name}`:\n{view}"
            );
        }
    }
}
