//! Collection pass: walks the parsed AST and gathers data structures used by
//! every codegen backend.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use quote::format_ident;
use syn::Ident;

use crate::{
    parse::{FunctionItem, ModuleContent, ModuleItem, StructItem, StructMember},
    util::{
        is_generic_type, path_to_rust_ident, to_screaming_snake_case, to_snake_case,
        type_to_pattern, type_to_simple_name, unwrap_result_type,
    },
};

// ============================================================================
// Data types
// ============================================================================

/// A collected builtin function definition (for signature registration).
pub(crate) struct BuiltinDef {
    /// Full path like `"baml.Array.length"`.
    pub path: String,
    /// Constant name like `BAML_ARRAY_LENGTH`.
    pub const_name: Ident,
    /// Receiver type pattern (`None` for free functions).
    pub receiver: Option<TokenStream2>,
    /// Parameters as `(name, type_pattern)` pairs.
    pub params: Vec<(String, TokenStream2)>,
    /// Return type pattern.
    pub returns: TokenStream2,
    /// Whether this is a `sys_op` function (runs async outside VM).
    pub is_sys_op: bool,
}

/// A collected builtin type definition (struct marked with `#[builtin]`).
pub(crate) struct BuiltinTypeDef {
    /// Full path like `"baml.http.Response"`.
    pub path: String,
    /// Field definitions.
    pub fields: Vec<BuiltinFieldDef>,
    /// Whether this type has a dedicated VM heap variant (vs `Object::Instance`).
    pub has_dedicated_heap_variant: bool,
}

/// A field in a builtin type.
pub(crate) struct BuiltinFieldDef {
    /// Field name (e.g., `"_handle"`, `"status_code"`).
    pub name: String,
    /// Type pattern. All fields have a type (including private ones).
    pub ty: TokenStream2,
    /// Whether this field is private (not visible to BAML code).
    pub is_private: bool,
    /// Field index in the struct.
    pub index: usize,
}

/// Receiver info for a native function.
pub(crate) struct ReceiverInfo {
    /// Parameter name (e.g., `snake_case` of the struct name).
    pub name: String,
    /// Simple type name (e.g., `"String"`, `"Array<T>"`).
    pub type_name: String,
    /// Whether the type is a generic type parameter.
    pub is_generic: bool,
    /// Whether the receiver is mutable (`self: mut Type`).
    pub is_mut: bool,
}

/// Parameter info for a native function.
pub(crate) struct ParamInfo {
    /// Parameter name.
    pub name: String,
    /// Simple type name.
    pub type_name: String,
    /// Whether the type is a generic type parameter.
    pub is_generic: bool,
}

/// Return type info for a native function.
pub(crate) struct ReturnInfo {
    /// Simple type name.
    pub type_name: String,
    /// Whether the type is a generic type parameter.
    pub is_generic: bool,
    /// Whether declared as `Result<T>` (fallible).
    pub is_fallible: bool,
}

/// Info for generating native function implementations.
pub(crate) struct NativeFnDef {
    /// Constant name like `BAML_ARRAY_LENGTH`.
    pub const_name: Ident,
    /// Full path like `"baml.Array.length"`.
    pub path: String,
    /// Function name like `baml_array_length`.
    pub fn_name: Ident,
    /// Receiver info (`None` for free functions).
    pub receiver: Option<ReceiverInfo>,
    /// Parameters.
    pub params: Vec<ParamInfo>,
    /// Return type.
    pub returns: ReturnInfo,
    /// Whether this function needs the VM (marked with `#[uses(vm)]`).
    pub uses_vm: bool,
    /// Whether this is a `sys_op` function (runs async outside VM).
    pub is_sys_op: bool,
    /// Whether this `sys_op` needs engine context (marked with `#[uses(engine_ctx)]`).
    pub uses_engine_ctx: bool,
}

/// Classify a DSL field type for accessor code generation.
pub(crate) enum FieldTypeKind {
    String,
    Int,
    Float,
    Bool,
    ResourceHandle,
    MapStringString,
    /// `Map<String, Unknown>` — values are opaque `BexExternalValue`.
    MapStringUnknown,
    ArrayString,
}

/// Data collected from one `#[builtin]` struct for accessor codegen.
pub(crate) struct AccessorTypeDef {
    /// Full DSL path, e.g., `"baml.http.Response"`.
    pub path: String,
    /// Rust struct name, e.g., `HttpResponse`.
    pub rust_name: Ident,
    /// Fields (in declaration order).
    pub fields: Vec<AccessorFieldDef>,
}

pub(crate) struct AccessorFieldDef {
    pub name: Ident,
    pub name_str: String,
    #[allow(dead_code)]
    pub is_private: bool,
    pub kind: FieldTypeKind,
}

// ============================================================================
// CollectedBuiltins — shared output of the collection pass
// ============================================================================

/// All data collected from parsing and walking the `with_builtins!` DSL.
///
/// Each proc macro creates one of these via [`CollectedBuiltins::from_modules`]
/// and then uses the subset of fields it needs.
pub(crate) struct CollectedBuiltins {
    /// Map from struct name to full path (e.g., `"File"` → `"baml.fs.File"`).
    pub builtin_types: HashMap<String, String>,
    /// Builtin function definitions (for signature registration).
    pub defs: Vec<BuiltinDef>,
    /// Native function definitions (for trait + glue generation).
    pub native_defs: Vec<NativeFnDef>,
    /// Builtin type definitions (structs with field info).
    pub type_defs: Vec<BuiltinTypeDef>,
    /// Accessor type data for `#[builtin]` structs.
    pub accessor_types: Vec<AccessorTypeDef>,
}

impl CollectedBuiltins {
    /// Run both collection passes and return all collected data.
    pub(crate) fn from_modules(modules: &[ModuleItem]) -> Self {
        // First pass: collect all builtin struct paths.
        let builtin_types = collect_builtin_types(modules);

        // Second pass: collect definitions.
        let mut defs = Vec::new();
        let mut native_defs = Vec::new();
        let mut type_defs = Vec::new();
        let mut accessor_types = Vec::new();
        for module in modules {
            let mut ctx = CollectContext {
                path_prefix: String::new(),
                const_prefix: String::new(),
                fn_name_prefix: String::new(),
                defs: &mut defs,
                native_defs: &mut native_defs,
                type_defs: &mut type_defs,
                accessor_types: &mut accessor_types,
                builtin_types: &builtin_types,
                is_hidden: false,
            };
            collect_builtins(module, &mut ctx);
        }

        CollectedBuiltins {
            builtin_types,
            defs,
            native_defs,
            type_defs,
            accessor_types,
        }
    }
}

// ============================================================================
// Internal collection logic
// ============================================================================

/// Context for collecting builtin definitions.
struct CollectContext<'a> {
    path_prefix: String,
    const_prefix: String,
    fn_name_prefix: String,
    defs: &'a mut Vec<BuiltinDef>,
    native_defs: &'a mut Vec<NativeFnDef>,
    type_defs: &'a mut Vec<BuiltinTypeDef>,
    accessor_types: &'a mut Vec<AccessorTypeDef>,
    builtin_types: &'a HashMap<String, String>,
    is_hidden: bool,
}

/// Collect all builtin struct paths from modules (first pass).
fn collect_builtin_types(modules: &[ModuleItem]) -> HashMap<String, String> {
    let mut builtin_types = HashMap::new();
    for module in modules {
        collect_builtin_types_from_module(module, "", &mut builtin_types);
    }
    builtin_types
}

fn collect_builtin_types_from_module(
    module: &ModuleItem,
    path_prefix: &str,
    builtin_types: &mut HashMap<String, String>,
) {
    let module_name = module.name.to_string();
    let new_path_prefix = if path_prefix.is_empty() {
        module_name
    } else {
        format!("{path_prefix}.{module_name}")
    };

    for item in &module.items {
        match item {
            ModuleContent::Struct(s) if s.is_builtin => {
                let struct_name = s.name.to_string();
                let full_path = format!("{new_path_prefix}.{struct_name}");
                builtin_types.insert(struct_name, full_path);
            }
            ModuleContent::Module(m) => {
                collect_builtin_types_from_module(m, &new_path_prefix, builtin_types);
            }
            _ => {}
        }
    }
}

/// Collect all builtin definitions from a module.
fn collect_builtins(module: &ModuleItem, ctx: &mut CollectContext) {
    let module_name = module.name.to_string();
    let new_path_prefix = if ctx.path_prefix.is_empty() {
        module_name.clone()
    } else {
        format!("{}.{module_name}", ctx.path_prefix)
    };

    let new_const_prefix = if ctx.const_prefix.is_empty() {
        to_screaming_snake_case(&module_name)
    } else {
        format!(
            "{}_{}",
            ctx.const_prefix,
            to_screaming_snake_case(&module_name)
        )
    };

    let new_fn_name_prefix = if ctx.fn_name_prefix.is_empty() {
        to_snake_case(&module_name)
    } else {
        format!("{}_{}", ctx.fn_name_prefix, to_snake_case(&module_name))
    };

    let hidden = ctx.is_hidden || module.is_hidden;

    let mut child_ctx = CollectContext {
        path_prefix: new_path_prefix,
        const_prefix: new_const_prefix,
        fn_name_prefix: new_fn_name_prefix,
        defs: ctx.defs,
        native_defs: ctx.native_defs,
        type_defs: ctx.type_defs,
        accessor_types: ctx.accessor_types,
        builtin_types: ctx.builtin_types,
        is_hidden: hidden,
    };

    for item in &module.items {
        match item {
            ModuleContent::Struct(s) => {
                collect_struct_builtins(s, &mut child_ctx);
            }
            ModuleContent::Function(f) => {
                collect_function_builtins(f, &mut child_ctx);
            }
            ModuleContent::Module(m) => {
                collect_builtins(m, &mut child_ctx);
            }
        }
    }
}

/// Collect builtin definitions from a struct.
fn collect_struct_builtins(s: &StructItem, ctx: &mut CollectContext) {
    let struct_name = s.name.to_string();
    let struct_path = format!("{}.{struct_name}", ctx.path_prefix);
    let struct_const_prefix = format!(
        "{}_{}",
        ctx.const_prefix,
        to_screaming_snake_case(&struct_name)
    );
    let struct_fn_name_prefix = format!("{}_{}", ctx.fn_name_prefix, to_snake_case(&struct_name));

    let struct_generics: Vec<String> = s
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();

    // If this is a builtin type, collect field information for type checking.
    if s.is_builtin && !ctx.is_hidden {
        let mut fields = Vec::new();
        let mut field_index = 0;

        for member in &s.members {
            if let StructMember::Field(field) = member {
                let ty = type_to_pattern(&field.ty, &struct_generics, ctx.builtin_types);

                fields.push(BuiltinFieldDef {
                    name: field.name.to_string(),
                    ty,
                    is_private: field.is_private,
                    index: field_index,
                });
                field_index += 1;
            }
        }

        // Always register the type, even if it has no fields (e.g., PromptAst).
        // This ensures the type is in class_names for type resolution.
        // Types marked #[opaque] have a dedicated Object variant in the VM heap
        // (wraps an opaque Rust ADT). All other builtin structs use Object::Instance.
        let has_dedicated_heap_variant = s.is_opaque;
        ctx.type_defs.push(BuiltinTypeDef {
            path: struct_path.clone(),
            fields,
            has_dedicated_heap_variant,
        });
    }

    // Also collect accessor type data for all builtin structs.
    if s.is_builtin {
        let accessor_fields: Vec<AccessorFieldDef> = s
            .members
            .iter()
            .filter_map(|m| {
                if let StructMember::Field(f) = m {
                    let name_str = f.name.to_string();
                    let kind = classify_field_type(&f.ty);
                    Some(AccessorFieldDef {
                        name: format_ident!("{}", name_str),
                        name_str,
                        is_private: f.is_private,
                        kind,
                    })
                } else {
                    None
                }
            })
            .collect();

        if !accessor_fields.is_empty() {
            ctx.accessor_types.push(AccessorTypeDef {
                path: struct_path.clone(),
                rust_name: path_to_rust_ident(&struct_path),
                fields: accessor_fields,
            });
        }
    }

    // Collect methods.
    for member in &s.members {
        let method = match member {
            StructMember::Method(m) => m,
            StructMember::Field(_) => continue,
        };

        let mut all_generics = struct_generics.clone();
        all_generics.extend(method.generics.type_params().map(|p| p.ident.to_string()));

        let method_name = method.name.to_string();
        let path = format!("{struct_path}.{method_name}");
        let const_name = format_ident!(
            "{}_{}",
            struct_const_prefix,
            to_screaming_snake_case(&method_name)
        );
        let fn_name = format_ident!("{}_{}", struct_fn_name_prefix, to_snake_case(&method_name));

        let receiver = method
            .receiver
            .as_ref()
            .map(|(ty, _is_mut)| type_to_pattern(ty, &all_generics, ctx.builtin_types));

        let params: Vec<(String, TokenStream2)> = method
            .params
            .iter()
            .map(|(name, ty)| {
                (
                    name.to_string(),
                    type_to_pattern(ty, &all_generics, ctx.builtin_types),
                )
            })
            .collect();

        let (inner_return_ty, _) = unwrap_result_type(&method.return_type);
        let returns = type_to_pattern(inner_return_ty, &all_generics, ctx.builtin_types);

        if !ctx.is_hidden {
            ctx.defs.push(BuiltinDef {
                path: path.clone(),
                const_name: const_name.clone(),
                receiver,
                params,
                returns,
                is_sys_op: method.is_sys_op,
            });
        }

        // Build native fn def with named structs.
        let native_receiver = method.receiver.as_ref().map(|(ty, is_mut)| ReceiverInfo {
            name: to_snake_case(&struct_name),
            type_name: type_to_simple_name(ty),
            is_generic: is_generic_type(ty, &all_generics),
            is_mut: *is_mut,
        });

        let native_params: Vec<ParamInfo> = method
            .params
            .iter()
            .map(|(name, ty)| ParamInfo {
                name: name.to_string(),
                type_name: type_to_simple_name(ty),
                is_generic: is_generic_type(ty, &all_generics),
            })
            .collect();

        let native_returns = {
            let (inner_ty, is_fallible) = unwrap_result_type(&method.return_type);
            ReturnInfo {
                type_name: type_to_simple_name(inner_ty),
                is_generic: is_generic_type(inner_ty, &all_generics),
                is_fallible,
            }
        };

        ctx.native_defs.push(NativeFnDef {
            const_name,
            path,
            fn_name,
            receiver: native_receiver,
            params: native_params,
            returns: native_returns,
            uses_vm: method.uses_vm,
            is_sys_op: method.is_sys_op,
            uses_engine_ctx: method.uses_engine_ctx,
        });
    }
}

/// Collect builtins from a single function.
fn collect_function_builtins(f: &FunctionItem, ctx: &mut CollectContext) {
    let fn_generics: Vec<String> = f
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();

    let original_fn_name = f.name.to_string();
    let path = format!("{}.{original_fn_name}", ctx.path_prefix);
    let const_name = format_ident!(
        "{}_{}",
        ctx.const_prefix,
        to_screaming_snake_case(&original_fn_name)
    );
    let fn_name = format_ident!(
        "{}_{}",
        ctx.fn_name_prefix,
        to_snake_case(&original_fn_name)
    );

    let receiver = f
        .receiver
        .as_ref()
        .map(|(ty, _is_mut)| type_to_pattern(ty, &fn_generics, ctx.builtin_types));

    let params: Vec<(String, TokenStream2)> = f
        .params
        .iter()
        .map(|(name, ty)| {
            (
                name.to_string(),
                type_to_pattern(ty, &fn_generics, ctx.builtin_types),
            )
        })
        .collect();

    let (inner_return_ty, _) = unwrap_result_type(&f.return_type);
    let returns = type_to_pattern(inner_return_ty, &fn_generics, ctx.builtin_types);

    if !ctx.is_hidden {
        ctx.defs.push(BuiltinDef {
            path: path.clone(),
            const_name: const_name.clone(),
            receiver,
            params,
            returns,
            is_sys_op: f.is_sys_op,
        });
    }

    let native_receiver = f.receiver.as_ref().map(|(ty, is_mut)| ReceiverInfo {
        name: "receiver".to_string(),
        type_name: type_to_simple_name(ty),
        is_generic: is_generic_type(ty, &fn_generics),
        is_mut: *is_mut,
    });

    let native_params: Vec<ParamInfo> = f
        .params
        .iter()
        .map(|(name, ty)| ParamInfo {
            name: name.to_string(),
            type_name: type_to_simple_name(ty),
            is_generic: is_generic_type(ty, &fn_generics),
        })
        .collect();

    let native_returns = {
        let (inner_ty, is_fallible) = unwrap_result_type(&f.return_type);
        ReturnInfo {
            type_name: type_to_simple_name(inner_ty),
            is_generic: is_generic_type(inner_ty, &fn_generics),
            is_fallible,
        }
    };

    ctx.native_defs.push(NativeFnDef {
        const_name,
        path,
        fn_name,
        receiver: native_receiver,
        params: native_params,
        returns: native_returns,
        uses_vm: f.uses_vm,
        is_sys_op: f.is_sys_op,
        uses_engine_ctx: f.uses_engine_ctx,
    });
}

/// Classify a DSL field type for accessor code generation.
fn classify_field_type(ty: &syn::Type) -> FieldTypeKind {
    let simple = type_to_simple_name(ty);
    match simple.as_str() {
        "String" => FieldTypeKind::String,
        "i64" => FieldTypeKind::Int,
        "f64" => FieldTypeKind::Float,
        "bool" => FieldTypeKind::Bool,
        "ResourceHandle" => FieldTypeKind::ResourceHandle,
        s if s.starts_with("Map<String, Unknown>") => FieldTypeKind::MapStringUnknown,
        s if s.starts_with("Map<String, String>") => FieldTypeKind::MapStringString,
        s if s.starts_with("Vec<String>") || s.starts_with("Array<String>") => {
            FieldTypeKind::ArrayString
        }
        other => panic!("Unsupported field type for builtin accessor codegen: {other}"),
    }
}
