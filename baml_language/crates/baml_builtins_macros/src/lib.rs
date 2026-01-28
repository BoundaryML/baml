//! Proc macro for defining BAML built-in functions with ergonomic Rust-like syntax.
//!
//! This macro transforms Rust-like module/struct/fn declarations into the
//! `BuiltinSignature` definitions used by `baml_builtins`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, GenericArgument, Generics, Ident, PathArguments, Result, ReturnType, Token, Type,
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

/// A collected builtin function definition.
struct BuiltinDef {
    /// Full path like "baml.Array.length"
    path: String,
    /// Constant name like `BAML_ARRAY_LENGTH`
    const_name: Ident,
    /// Receiver type pattern (None for free functions)
    receiver: Option<TokenStream2>,
    /// Parameters as (name, `type_pattern`) pairs
    params: Vec<(String, TokenStream2)>,
    /// Return type pattern
    returns: TokenStream2,
    /// Whether this is a `sys_op` function (runs async outside VM)
    is_sys_op: bool,
}

/// Info for generating native function implementations.
struct NativeFnDef {
    /// Constant name like `BAML_ARRAY_LENGTH`
    const_name: Ident,
    /// Full path like "baml.Array.length"
    path: String,
    /// Function name like `baml_array_length`
    fn_name: Ident,
    /// Receiver info: (`param_name`, `type_name`, `is_generic`, `is_mut`)
    /// None for free functions
    receiver: Option<(String, String, bool, bool)>,
    /// Parameters: (name, `type_name`, `is_generic`)
    params: Vec<(String, String, bool)>,
    /// Return type: (`type_name`, `is_generic`, `is_fallible`)
    /// `is_fallible` is true when declared as `Result<T>`
    returns: (String, bool, bool),
    /// Whether this function needs the VM (marked with #[uses(vm)])
    uses_vm: bool,
    /// Whether this is a `sys_op` function (runs async outside VM)
    is_sys_op: bool,
}

/// The root input to the macro: a list of modules.
struct BuiltinsInput {
    modules: Vec<ModuleItem>,
}

impl Parse for BuiltinsInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut modules = Vec::new();
        while !input.is_empty() {
            // Check for attributes before mod
            if input.peek(Token![#]) {
                let attrs = input.call(Attribute::parse_outer)?;
                modules.push(ModuleItem::parse_with_attrs(input, &attrs)?);
            } else {
                modules.push(input.parse()?);
            }
        }
        Ok(BuiltinsInput { modules })
    }
}

/// A module item containing structs, functions, or nested modules.
struct ModuleItem {
    name: Ident,
    items: Vec<ModuleContent>,
    /// Whether this module is marked with #[hide] (hidden from type checker).
    is_hidden: bool,
}

/// Content inside a module.
enum ModuleContent {
    Struct(StructItem),
    Function(Box<FunctionItem>),
    Module(ModuleItem),
}

/// A struct with methods.
struct StructItem {
    name: Ident,
    generics: Generics,
    methods: Vec<FunctionItem>,
    /// Whether this struct is marked with #[builtin] (builtin type).
    is_builtin: bool,
}

/// A function or method declaration.
struct FunctionItem {
    name: Ident,
    generics: Generics,
    /// First parameter if it's `self: Type` or `self: mut Type`
    /// The bool indicates whether it's mutable
    receiver: Option<(Type, bool)>,
    /// Other parameters
    params: Vec<(Ident, Type)>,
    /// Return type
    return_type: Type,
    /// Whether this function uses the VM (marked with #[uses(vm)])
    uses_vm: bool,
    /// Whether this function is a `sys_op` (marked with #[`sys_op`])
    /// `Sys_op` functions run asynchronously outside the VM.
    is_sys_op: bool,
}

impl ModuleItem {
    fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let is_hidden = attrs.iter().any(|attr| attr.path().is_ident("hide"));

        // Parse: mod name { ... }
        input.parse::<Token![mod]>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut items = Vec::new();
        while !content.is_empty() {
            // Peek to determine what kind of item this is
            // Handle attributes first (for #[opaque] struct, #[uses(vm)]/#[sys_op] fn, or #[hide] mod)
            let lookahead = content.lookahead1();
            if lookahead.peek(Token![mod]) {
                items.push(ModuleContent::Module(ModuleItem::parse_with_attrs(
                    &content,
                    &[],
                )?));
            } else if lookahead.peek(Token![struct]) {
                items.push(ModuleContent::Struct(content.parse()?));
            } else if lookahead.peek(Token![#]) {
                // Could be #[opaque] struct, #[uses(vm)]/#[sys_op] fn, or #[hide] mod
                // Parse attributes first, then peek again
                let attrs = content.call(Attribute::parse_outer)?;
                let lookahead2 = content.lookahead1();
                if lookahead2.peek(Token![struct]) {
                    items.push(ModuleContent::Struct(StructItem::parse_with_attrs(
                        &content, &attrs,
                    )?));
                } else if lookahead2.peek(Token![fn]) {
                    items.push(ModuleContent::Function(Box::new(
                        FunctionItem::parse_with_attrs(&content, &attrs)?,
                    )));
                } else if lookahead2.peek(Token![mod]) {
                    items.push(ModuleContent::Module(ModuleItem::parse_with_attrs(
                        &content, &attrs,
                    )?));
                } else {
                    return Err(lookahead2.error());
                }
            } else if lookahead.peek(Token![fn]) {
                items.push(ModuleContent::Function(Box::new(content.parse()?)));
            } else {
                return Err(lookahead.error());
            }
        }

        Ok(ModuleItem {
            name,
            items,
            is_hidden,
        })
    }
}

impl Parse for ModuleItem {
    fn parse(input: ParseStream) -> Result<Self> {
        Self::parse_with_attrs(input, &[])
    }
}

impl StructItem {
    fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let is_builtin = attrs.iter().any(|attr| attr.path().is_ident("builtin"));

        // Parse: struct Name<Generics> { fn... }
        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let content;
        braced!(content in input);

        let mut methods = Vec::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
        }

        Ok(StructItem {
            name,
            generics,
            methods,
            is_builtin,
        })
    }
}

impl Parse for StructItem {
    fn parse(input: ParseStream) -> Result<Self> {
        Self::parse_with_attrs(input, &[])
    }
}

impl FunctionItem {
    fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let uses_vm = attrs.iter().any(|attr| {
            if attr.path().is_ident("uses") {
                // Check if it's #[uses(vm)]
                if let Ok(nested) = attr.parse_args::<Ident>() {
                    return nested == "vm";
                }
            }
            false
        });
        let is_sys_op = attrs.iter().any(|attr| attr.path().is_ident("sys_op"));

        // Parse: fn name<Generics>(params...) -> RetType;
        input.parse::<Token![fn]>()?;
        let name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let params_content;
        parenthesized!(params_content in input);

        let mut receiver = None;
        let mut params = Vec::new();

        // Custom parsing to handle `self: mut Type` syntax
        // We can't use parse_terminated because we need to handle `mut` specially
        let mut first = true;
        while !params_content.is_empty() {
            if !first {
                params_content.parse::<Token![,]>()?;
                if params_content.is_empty() {
                    break; // trailing comma
                }
            }
            first = false;

            // Check if this is a `self` parameter
            if params_content.peek(Token![self]) {
                params_content.parse::<Token![self]>()?;
                params_content.parse::<Token![:]>()?;

                // Check for `mut` keyword
                let is_mut = params_content.peek(Token![mut]);
                if is_mut {
                    params_content.parse::<Token![mut]>()?;
                }

                let ty: Type = params_content.parse()?;
                receiver = Some((ty, is_mut));
            } else {
                // Regular parameter: name: Type
                let param_name: Ident = params_content.parse()?;
                params_content.parse::<Token![:]>()?;
                let param_type: Type = params_content.parse()?;
                params.push((param_name, param_type));
            }
        }

        // Parse return type
        let return_type = match input.parse::<ReturnType>()? {
            ReturnType::Default => {
                // Unit type () means Null
                syn::parse_quote!(())
            }
            ReturnType::Type(_, ty) => *ty,
        };

        // Expect semicolon
        input.parse::<Token![;]>()?;

        Ok(FunctionItem {
            name,
            generics,
            receiver,
            params,
            return_type,
            uses_vm,
            is_sys_op,
        })
    }
}

impl Parse for FunctionItem {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse attributes like #[uses(vm)] or #[sys_op]
        let attrs = input.call(Attribute::parse_outer)?;
        Self::parse_with_attrs(input, &attrs)
    }
}

use std::collections::HashMap;

/// Context for collecting builtin definitions.
///
/// Groups common parameters to reduce argument count in collection functions.
struct CollectContext<'a> {
    path_prefix: String,
    const_prefix: String,
    fn_name_prefix: String,
    defs: &'a mut Vec<BuiltinDef>,
    native_defs: &'a mut Vec<NativeFnDef>,
    builtin_types: &'a HashMap<String, String>,
    is_hidden: bool,
}

/// Collect all builtin struct paths from modules (first pass).
/// Returns a map from struct name to full path (e.g., "File" -> "baml.fs.File").
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

/// Convert a Rust type to a `TypePattern` token stream.
fn type_to_pattern(
    ty: &Type,
    generic_params: &[String],
    builtin_types: &HashMap<String, String>,
) -> TokenStream2 {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident = &segment.ident;
            let ident_str = ident.to_string();

            // Check if it's a generic type parameter
            if generic_params.contains(&ident_str) {
                let lit = syn::LitStr::new(&ident_str, ident.span());
                return quote!(TypePattern::Var(#lit));
            }

            match ident_str.as_str() {
                "String" => quote!(TypePattern::String),
                "i64" => quote!(TypePattern::Int),
                "f64" => quote!(TypePattern::Float),
                "bool" => quote!(TypePattern::Bool),
                "Media" => quote!(TypePattern::Media),
                "Option" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let inner_pattern =
                                type_to_pattern(inner, generic_params, builtin_types);
                            return quote!(TypePattern::Optional(Box::new(#inner_pattern)));
                        }
                    }
                    quote!(TypePattern::Optional(Box::new(TypePattern::Null)))
                }
                "Array" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            let inner_pattern =
                                type_to_pattern(inner, generic_params, builtin_types);
                            return quote!(TypePattern::Array(Box::new(#inner_pattern)));
                        }
                    }
                    quote!(TypePattern::Array(Box::new(TypePattern::Null)))
                }
                "Map" => {
                    if let PathArguments::AngleBracketed(args) = &segment.arguments {
                        let mut iter = args.args.iter();
                        let key = iter
                            .next()
                            .and_then(|a| {
                                if let GenericArgument::Type(t) = a {
                                    Some(t)
                                } else {
                                    None
                                }
                            })
                            .map(|t| type_to_pattern(t, generic_params, builtin_types))
                            .unwrap_or_else(|| quote!(TypePattern::String));
                        let value = iter
                            .next()
                            .and_then(|a| {
                                if let GenericArgument::Type(t) = a {
                                    Some(t)
                                } else {
                                    None
                                }
                            })
                            .map(|t| type_to_pattern(t, generic_params, builtin_types))
                            .unwrap_or_else(|| quote!(TypePattern::Null));
                        return quote!(TypePattern::Map {
                            key: Box::new(#key),
                            value: Box::new(#value),
                        });
                    }
                    quote!(TypePattern::Map {
                        key: Box::new(TypePattern::String),
                        value: Box::new(TypePattern::Null),
                    })
                }
                _ => {
                    // Check if it's a builtin type
                    if let Some(full_path) = builtin_types.get(&ident_str) {
                        return quote!(TypePattern::Builtin(#full_path));
                    }
                    // Single uppercase letter is likely a type variable
                    if ident_str.len() == 1 && ident_str.chars().next().unwrap().is_uppercase() {
                        let lit = syn::LitStr::new(&ident_str, ident.span());
                        quote!(TypePattern::Var(#lit))
                    } else {
                        // Unknown type - treat as a type variable
                        let lit = syn::LitStr::new(&ident_str, ident.span());
                        quote!(TypePattern::Var(#lit))
                    }
                }
            }
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => {
            // Unit type () means Null
            quote!(TypePattern::Null)
        }
        _ => {
            // Fallback
            quote!(TypePattern::Null)
        }
    }
}

/// Convert an identifier from camelCase to `SCREAMING_SNAKE_CASE`.
fn to_screaming_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_uppercase());
    }
    result
}

/// Convert an identifier from camelCase to `snake_case`.
fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

/// Get the simple type name from a Type (for native fn generation).
fn type_to_simple_name(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident_str = segment.ident.to_string();

            // Handle generic types
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                let inner_types: Vec<String> = args
                    .args
                    .iter()
                    .filter_map(|arg| {
                        if let GenericArgument::Type(t) = arg {
                            Some(type_to_simple_name(t))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !inner_types.is_empty() {
                    return format!("{}<{}>", ident_str, inner_types.join(", "));
                }
            }
            ident_str
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => "()".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Check if a type is a generic type parameter.
fn is_generic_type(ty: &Type, generic_params: &[String]) -> bool {
    match ty {
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last().unwrap();
            let ident_str = segment.ident.to_string();
            generic_params.contains(&ident_str)
        }
        _ => false,
    }
}

/// Check if a type is `Result<T>` and return the inner type if so.
/// Returns (`inner_type`, `is_result`) where `inner_type` is the T from Result<T> or the original type.
fn unwrap_result_type(ty: &Type) -> (&Type, bool) {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last().unwrap();
        if segment.ident == "Result" {
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return (inner, true);
                }
            }
        }
    }
    (ty, false)
}

/// Collect all builtin definitions from a module.
///
/// When `is_hidden` is true, items are not added to `defs` (signatures)
/// but are still added to `native_defs` (native function implementations).
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

    // If this module is hidden, propagate to children
    let hidden = ctx.is_hidden || module.is_hidden;

    // Create child context with updated prefixes
    let mut child_ctx = CollectContext {
        path_prefix: new_path_prefix,
        const_prefix: new_const_prefix,
        fn_name_prefix: new_fn_name_prefix,
        defs: ctx.defs,
        native_defs: ctx.native_defs,
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

    // Collect generic params from struct
    let struct_generics: Vec<String> = s
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();

    for method in &s.methods {
        // Combine struct and method generics
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

        // Build receiver pattern from the self type (ignoring mutability for type pattern)
        let receiver = method
            .receiver
            .as_ref()
            .map(|(ty, _is_mut)| type_to_pattern(ty, &all_generics, ctx.builtin_types));

        // Build params
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

        // Build return type (unwrap Result<T> to just T for TypePattern)
        let (inner_return_ty, _) = unwrap_result_type(&method.return_type);
        let returns = type_to_pattern(inner_return_ty, &all_generics, ctx.builtin_types);

        // Only add to defs (signatures) if not hidden
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

        // Build native fn def
        let native_receiver = method.receiver.as_ref().map(|(ty, is_mut)| {
            let type_name = type_to_simple_name(ty);
            let is_generic = is_generic_type(ty, &all_generics);
            // Use struct name in snake_case as the parameter name
            (to_snake_case(&struct_name), type_name, is_generic, *is_mut)
        });

        let native_params: Vec<(String, String, bool)> = method
            .params
            .iter()
            .map(|(name, ty)| {
                let type_name = type_to_simple_name(ty);
                let is_generic = is_generic_type(ty, &all_generics);
                (name.to_string(), type_name, is_generic)
            })
            .collect();

        let native_returns = {
            let (inner_ty, is_fallible) = unwrap_result_type(&method.return_type);
            let type_name = type_to_simple_name(inner_ty);
            let is_generic = is_generic_type(inner_ty, &all_generics);
            (type_name, is_generic, is_fallible)
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

    // Free functions shouldn't have receivers (ignoring mutability for type pattern)
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

    // Unwrap Result<T> to just T for TypePattern
    let (inner_return_ty, _) = unwrap_result_type(&f.return_type);
    let returns = type_to_pattern(inner_return_ty, &fn_generics, ctx.builtin_types);

    // Only add to defs (signatures) if not hidden
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

    // Build native fn def
    let native_receiver = f.receiver.as_ref().map(|(ty, is_mut)| {
        let type_name = type_to_simple_name(ty);
        let is_generic = is_generic_type(ty, &fn_generics);
        ("receiver".to_string(), type_name, is_generic, *is_mut)
    });

    let native_params: Vec<(String, String, bool)> = f
        .params
        .iter()
        .map(|(name, ty)| {
            let type_name = type_to_simple_name(ty);
            let is_generic = is_generic_type(ty, &fn_generics);
            (name.to_string(), type_name, is_generic)
        })
        .collect();

    let native_returns = {
        let (inner_ty, is_fallible) = unwrap_result_type(&f.return_type);
        let type_name = type_to_simple_name(inner_ty);
        let is_generic = is_generic_type(inner_ty, &fn_generics);
        (type_name, is_generic, is_fallible)
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
    });
}

/// The main proc macro entry point.
#[proc_macro]
pub fn define_builtins(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);

    // First pass: collect all opaque types
    let builtin_types = collect_builtin_types(&input.modules);

    // Second pass: collect all builtin definitions
    let mut defs = Vec::new();
    let mut native_defs = Vec::new();
    for module in &input.modules {
        let mut ctx = CollectContext {
            path_prefix: String::new(),
            const_prefix: String::new(),
            fn_name_prefix: String::new(),
            defs: &mut defs,
            native_defs: &mut native_defs,
            builtin_types: &builtin_types,
            is_hidden: false, // Not hidden at root level; modules handle their own is_hidden flag
        };
        collect_builtins(module, &mut ctx);
    }

    // Generate path constants
    let path_consts: Vec<_> = defs
        .iter()
        .map(|d| {
            let name = &d.const_name;
            let path = &d.path;
            quote!(pub const #name: &str = #path;)
        })
        .collect();

    let all_paths: Vec<_> = defs.iter().map(|d| &d.path).collect();

    // Generate const names for the for_all_builtins macro
    let const_names: Vec<_> = defs.iter().map(|d| &d.const_name).collect();

    // Generate const names for the for_native_builtins macro (exclude external functions)
    let native_const_names: Vec<_> = defs
        .iter()
        .filter(|d| !d.is_sys_op)
        .map(|d| &d.const_name)
        .collect();

    // Generate builtin signatures
    let signatures: Vec<_> = defs
        .iter()
        .map(|d| {
            let const_name = &d.const_name;
            let receiver = match &d.receiver {
                Some(r) => quote!(Some(#r)),
                None => quote!(None),
            };
            let params: Vec<_> = d
                .params
                .iter()
                .map(|(name, ty)| quote!((#name, #ty)))
                .collect();
            let returns = &d.returns;
            let is_sys_op = d.is_sys_op;

            quote! {
                BuiltinSignature {
                    path: paths::#const_name,
                    receiver: #receiver,
                    params: vec![#(#params),*],
                    returns: #returns,
                    is_sys_op: #is_sys_op,
                }
            }
        })
        .collect();

    // Generate native function info for for_native_functions! macro
    // Format: (const_name, path, fn_name, receiver_info, params_info, return_info, uses_vm)
    let native_fn_entries: Vec<_> = native_defs
        .iter()
        .map(|d| {
            let const_name = &d.const_name;
            let path = &d.path;
            let fn_name = &d.fn_name;
            let uses_vm = d.uses_vm;

            // Receiver: (name, type, is_generic, is_mut) or none
            let receiver_tokens = match &d.receiver {
                Some((name, ty, is_generic, is_mut)) => {
                    quote!( some((#name, #ty, #is_generic, #is_mut)) )
                }
                None => quote!( none ),
            };

            // Params: [(name, type, is_generic), ...]
            let params_tokens: Vec<_> = d.params.iter().map(|(name, ty, is_generic)| {
                quote!( (#name, #ty, #is_generic) )
            }).collect();

            // Return: (type, is_generic, is_fallible)
            let (ret_ty, ret_is_generic, ret_is_fallible) = &d.returns;

            quote! {
                (#const_name, #path, #fn_name, #receiver_tokens, [#(#params_tokens),*], (#ret_ty, #ret_is_generic, #ret_is_fallible), #uses_vm)
            }
        })
        .collect();

    let output = quote! {
        /// Path constants for all builtins.
        ///
        /// Use these constants instead of raw strings to avoid typos.
        pub mod paths {
            #(#path_consts)*

            /// All builtin paths as a slice.
            pub const ALL: &[&str] = &[#(#all_paths),*];
        }

        /// Invoke a macro with all builtin constant names.
        ///
        /// Usage:
        /// ```ignore
        /// baml_builtins::for_all_builtins!(my_macro);
        /// // Expands to: my_macro!(BAML_ARRAY_LENGTH, BAML_ARRAY_PUSH, ...);
        /// ```
        #[macro_export]
        macro_rules! for_all_builtins {
            ($callback:ident) => {
                $callback!(#(#const_names),*)
            };
        }

        /// Invoke a macro with only native (non-external) builtin constant names.
        ///
        /// External functions are handled by the embedder, not native Rust.
        /// Use this instead of `for_all_builtins!` when generating native function
        /// implementations or registrations.
        ///
        /// Usage:
        /// ```ignore
        /// baml_builtins::for_native_builtins!(my_macro);
        /// // Expands to: my_macro!(BAML_ARRAY_LENGTH, BAML_ARRAY_PUSH, ...);
        /// // (excluding external functions like BAML_FS_FILE_READ)
        /// ```
        #[macro_export]
        macro_rules! for_native_builtins {
            ($callback:ident) => {
                $callback!(#(#native_const_names),*)
            };
        }

        /// Invoke a macro with all native function info.
        ///
        /// Each entry has format:
        /// `(CONST_NAME, "path.string", fn_name, receiver_info, params_info, return_info)`
        ///
        /// - receiver_info: `some((name, type, is_generic))` or `none`
        /// - params_info: `[(name, type, is_generic), ...]`
        /// - return_info: `(type, is_generic)`
        ///
        /// Usage:
        /// ```ignore
        /// baml_builtins::for_native_functions!(my_macro);
        /// ```
        #[macro_export]
        macro_rules! for_native_functions {
            ($callback:ident) => {
                $callback!(
                    #(#native_fn_entries),*
                );
            };
        }

        /// All built-in function signatures.
        static BUILTINS: std::sync::LazyLock<Vec<BuiltinSignature>> = std::sync::LazyLock::new(|| {
            vec![
                #(#signatures),*
            ]
        });
    };

    output.into()
}

/// Generate a `NativeFunctions` trait from the same builtin definitions.
///
/// This macro generates:
/// - Required `baml_*` methods with clean Rust types
/// - Default `__baml_*` glue methods that handle Value conversion
/// - Default `get_native_fn` method for path lookup
#[proc_macro]
pub fn generate_native_trait(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);

    // First pass: collect all opaque types
    let builtin_types = collect_builtin_types(&input.modules);

    // Second pass: collect all builtin definitions
    let mut native_defs = Vec::new();
    let mut defs = Vec::new();
    for module in &input.modules {
        let mut ctx = CollectContext {
            path_prefix: String::new(),
            const_prefix: String::new(),
            fn_name_prefix: String::new(),
            defs: &mut defs,
            native_defs: &mut native_defs,
            builtin_types: &builtin_types,
            is_hidden: false, // Not hidden at root level; modules handle their own is_hidden flag
        };
        collect_builtins(module, &mut ctx);
    }

    // Generate required trait methods (clean signatures)
    // Note: For mutable receivers, we don't pass `vm` due to borrow checker constraints
    // Note: For non-fallible functions, return type is just T (not Result<T, VmError>)
    // Note: External functions are skipped - they're handled by the embedder, not native Rust
    let required_methods: Vec<_> = native_defs
        .iter()
        .filter(|d| !d.is_sys_op) // Skip external functions
        .map(|d| {
            let fn_name = &d.fn_name;
            let params = generate_clean_params(d);
            let return_type = generate_clean_return_type(d);
            let has_mut_receiver = d.receiver.as_ref().is_some_and(|(_, _, _, is_mut)| *is_mut);

            // Only include vm parameter if:
            // - Function has #[uses(vm)] attribute AND
            // - Receiver is not mutable (mutable receivers can't have vm due to borrow checker)
            if d.uses_vm && !has_mut_receiver {
                quote! {
                    fn #fn_name(vm: &mut BexVm, #params) -> #return_type;
                }
            } else {
                quote! {
                    fn #fn_name(#params) -> #return_type;
                }
            }
        })
        .collect();

    // Generate default glue methods
    // Note: For mutable receivers, we don't pass `vm` to the clean function
    // Note: External functions are skipped - they're handled by the embedder
    let glue_methods: Vec<_> = native_defs
        .iter()
        .filter(|d| !d.is_sys_op) // Skip external functions
        .map(|d| {
            let fn_name = &d.fn_name;
            let glue_fn_name = format_ident!("__{}", fn_name);
            let extract_args = generate_arg_extraction(d);
            let call_args = generate_call_args(d);
            let convert_result = generate_result_conversion(d);
            let has_mut_receiver = d.receiver.as_ref().is_some_and(|(_, _, _, is_mut)| *is_mut);
            let is_fallible = d.returns.2;

            // For fallible functions, use `?` to propagate errors
            // For non-fallible functions, just call directly (no `?`)
            // Only pass vm when uses_vm is true AND receiver is not mutable
            let needs_vm = d.uses_vm && !has_mut_receiver;
            let call_expr = match (needs_vm, is_fallible) {
                (true, true) => quote!(Self::#fn_name(vm, #call_args)?),
                (true, false) => quote!(Self::#fn_name(vm, #call_args)),
                (false, true) => quote!(Self::#fn_name(#call_args)?),
                (false, false) => quote!(Self::#fn_name(#call_args)),
            };

            quote! {
                fn #glue_fn_name(vm: &mut BexVm, args: &[Value]) -> NativeFunctionResult {
                    #extract_args
                    let result = #call_expr;
                    #convert_result
                }
            }
        })
        .collect();

    // Generate get_native_fn match arms
    // Note: External functions are skipped - they don't have native implementations
    let match_arms: Vec<_> = native_defs
        .iter()
        .filter(|d| !d.is_sys_op) // Skip external functions
        .map(|d| {
            let path = &d.path;
            let glue_fn_name = format_ident!("__{}", d.fn_name);
            quote! {
                #path => Some(Self::#glue_fn_name),
            }
        })
        .collect();

    // Generate public wrapper functions that delegate to VmNatives::__baml_*
    // These are needed by builtins.rs which looks up native::baml_* functions
    // Note: External functions are skipped - they don't have native implementations
    let public_wrappers: Vec<_> = native_defs
        .iter()
        .filter(|d| !d.is_sys_op) // Skip external functions
        .map(|d| {
            let fn_name = &d.fn_name;
            let glue_fn_name = format_ident!("__{}", d.fn_name);
            quote! {
                pub fn #fn_name(vm: &mut BexVm, args: &[Value]) -> NativeFunctionResult {
                    VmNatives::#glue_fn_name(vm, args)
                }
            }
        })
        .collect();

    let output = quote! {
        /// Trait for implementing native BAML functions.
        ///
        /// Implement the `baml_*` methods - they have clean Rust types.
        /// The `__baml_*` glue methods and `get_native_fn` are auto-generated.
        pub trait NativeFunctions {
            // ========== Required methods (implement these) ==========
            #(#required_methods)*

            // ========== Glue methods (default implementations) ==========
            #(#glue_methods)*

            // ========== Lookup method (default implementation) ==========
            fn get_native_fn(path: &str) -> Option<NativeFunction> {
                match path {
                    #(#match_arms)*
                    _ => None,
                }
            }
        }

        // ========== Public wrapper functions (for builtins.rs) ==========
        // These delegate to VmNatives::__baml_* glue methods
        #(#public_wrappers)*
    };

    output.into()
}

/// Generate the clean parameter list for a trait method.
fn generate_clean_params(d: &NativeFnDef) -> TokenStream2 {
    let mut params = Vec::new();

    // Add receiver as first param after vm
    if let Some((name, type_name, is_generic, is_mut)) = &d.receiver {
        let param_name = format_ident!("{}", name);
        let param_type = rust_type_for_input(type_name, *is_generic, *is_mut);
        params.push(quote!(#param_name: #param_type));
    }

    // Add other params
    for (name, type_name, is_generic) in &d.params {
        let param_name = format_ident!("{}", name);
        let param_type = rust_type_for_input(type_name, *is_generic, false);
        params.push(quote!(#param_name: #param_type));
    }

    quote!(#(#params),*)
}

/// Generate the clean return type for a trait method.
/// For fallible functions (declared with Result<T>), returns Result<T, `VmError`>
/// For non-fallible functions, returns just T
fn generate_clean_return_type(d: &NativeFnDef) -> TokenStream2 {
    let (type_name, is_generic, is_fallible) = &d.returns;
    let inner_type = rust_type_for_output(type_name, *is_generic);
    if *is_fallible {
        quote!(Result<#inner_type, VmError>)
    } else {
        inner_type
    }
}

/// Map BAML type names to Rust input types.
///
/// When `is_mut` is true, generates `&mut` types for mutable receivers.
fn rust_type_for_input(type_name: &str, is_generic: bool, is_mut: bool) -> TokenStream2 {
    if is_generic {
        return if is_mut {
            quote!(&mut Value)
        } else {
            quote!(&Value)
        };
    }

    match type_name {
        "String" => {
            if is_mut {
                quote!(&mut String)
            } else {
                quote!(&str)
            }
        }
        "i64" => quote!(i64),
        "f64" => quote!(f64),
        "bool" => quote!(bool),
        "()" => quote!(()),
        "Media" => {
            if is_mut {
                quote!(&mut MediaValue)
            } else {
                quote!(&MediaValue)
            }
        }
        "PromptAst" => {
            if is_mut {
                quote!(&mut PromptAst)
            } else {
                quote!(&PromptAst)
            }
        }
        "PrimitiveClient" => {
            if is_mut {
                quote!(&mut PrimitiveClient)
            } else {
                quote!(&PrimitiveClient)
            }
        }
        t if t.starts_with("Array") => {
            if is_mut {
                quote!(&mut Vec<Value>)
            } else {
                quote!(&[Value])
            }
        }
        t if t.starts_with("Map") => {
            if is_mut {
                quote!(&mut IndexMap<String, Value>)
            } else {
                quote!(&IndexMap<String, Value>)
            }
        }
        t if t.starts_with("Option<") => {
            // Extract inner type
            let inner = &t[7..t.len() - 1];
            let inner_type = rust_type_for_input(inner, false, is_mut);
            quote!(Option<#inner_type>)
        }
        _ => {
            if is_mut {
                quote!(&mut Value)
            } else {
                quote!(&Value)
            }
        }
    }
}

/// Map BAML type names to Rust output types.
fn rust_type_for_output(type_name: &str, is_generic: bool) -> TokenStream2 {
    if is_generic {
        return quote!(Value);
    }

    match type_name {
        "String" => quote!(String),
        "i64" => quote!(i64),
        "f64" => quote!(f64),
        "bool" => quote!(bool),
        "()" => quote!(()),
        "Media" => quote!(MediaValue),
        "PromptAst" => quote!(PromptAst),
        "PrimitiveClient" => quote!(PrimitiveClient),
        t if t.starts_with("Array") => quote!(Vec<Value>),
        t if t.starts_with("Map") => quote!(IndexMap<String, Value>),
        t if t.starts_with("Option<") => {
            // Extract inner type
            let inner = &t[7..t.len() - 1];
            let inner_type = rust_type_for_output(inner, false);
            quote!(Option<#inner_type>)
        }
        _ => quote!(Value), // Fallback
    }
}

/// Generate code to extract arguments from &[Value].
///
/// For mutable receivers, we extract params first (with cloning) to avoid borrow conflicts.
fn generate_arg_extraction(d: &NativeFnDef) -> TokenStream2 {
    let mut extractions = Vec::new();
    let has_mut_receiver = d.receiver.as_ref().is_some_and(|(_, _, _, is_mut)| *is_mut);

    if has_mut_receiver {
        // For mutable receivers: extract params first (they clone), then receiver last
        // This avoids borrow conflicts with vm.objects

        // Extract other params first (params are never mutable, idx starts at 1 since receiver is at 0)
        for (idx, (name, type_name, is_generic)) in d.params.iter().enumerate() {
            let var_name = format_ident!("{}", name);
            let arg_idx = idx + 1; // Receiver is at index 0
            let extraction =
                generate_single_extraction(&var_name, arg_idx, type_name, *is_generic, false);
            extractions.push(extraction);
        }

        // Extract mutable receiver last
        if let Some((name, type_name, is_generic, is_mut)) = &d.receiver {
            let var_name = format_ident!("{}", name);
            let extraction =
                generate_single_extraction(&var_name, 0, type_name, *is_generic, *is_mut);
            extractions.push(extraction);
        }
    } else {
        // For non-mutable receivers: original order is fine
        let mut arg_idx = 0;

        // Extract receiver
        if let Some((name, type_name, is_generic, is_mut)) = &d.receiver {
            let var_name = format_ident!("{}", name);
            let extraction =
                generate_single_extraction(&var_name, arg_idx, type_name, *is_generic, *is_mut);
            extractions.push(extraction);
            arg_idx += 1;
        }

        // Extract other params (params are never mutable)
        for (name, type_name, is_generic) in &d.params {
            let var_name = format_ident!("{}", name);
            let extraction =
                generate_single_extraction(&var_name, arg_idx, type_name, *is_generic, false);
            extractions.push(extraction);
            arg_idx += 1;
        }
    }

    quote!(#(#extractions)*)
}

/// Generate extraction code for a single argument.
///
/// For immutable args: clone complex types to avoid borrow checker issues.
/// For mutable args: use `_mut` methods to get mutable references directly.
fn generate_single_extraction(
    var_name: &Ident,
    idx: usize,
    type_name: &str,
    is_generic: bool,
    is_mut: bool,
) -> TokenStream2 {
    if is_generic {
        return if is_mut {
            quote! {
                let #var_name = vm.as_value_mut(&args[#idx])?;
            }
        } else {
            quote! {
                let #var_name = &args[#idx];
            }
        };
    }

    match type_name {
        "String" => {
            if is_mut {
                quote! {
                    let #var_name = vm.as_string_mut(&args[#idx])?;
                }
            } else {
                // Clone string to release borrow on vm
                quote! {
                    let #var_name = vm.as_string(&args[#idx])?.clone();
                }
            }
        }
        "i64" => quote! {
            let #var_name = match args[#idx] {
                Value::Int(i) => i,
                _ => return Err(InternalError::TypeError {
                    expected: Type::Int,
                    got: vm.type_of(&args[#idx]),
                }.into()),
            };
        },
        "f64" => quote! {
            let #var_name = match args[#idx] {
                Value::Float(f) => f,
                _ => return Err(InternalError::TypeError {
                    expected: Type::Float,
                    got: vm.type_of(&args[#idx]),
                }.into()),
            };
        },
        "bool" => quote! {
            let #var_name = match args[#idx] {
                Value::Bool(b) => b,
                _ => return Err(InternalError::TypeError {
                    expected: Type::Bool,
                    got: vm.type_of(&args[#idx]),
                }.into()),
            };
        },
        "Media" => {
            if is_mut {
                quote! {
                    let #var_name = vm.as_media_mut(&args[#idx], MediaKind::Generic)?;
                }
            } else {
                quote! {
                    let #var_name = vm.as_media(&args[#idx], MediaKind::Generic)?.clone();
                }
            }
        }
        "PromptAst" => {
            if is_mut {
                // TODO: Add as_prompt_ast_mut to vm when needed
                quote! {
                    compile_error!("Mutable PromptAst parameters not yet supported");
                }
            } else {
                quote! {
                    let #var_name = vm.as_prompt_ast(&args[#idx])?.clone();
                }
            }
        }
        "PrimitiveClient" => {
            if is_mut {
                quote! {
                    compile_error!("Mutable PrimitiveClient parameters not yet supported");
                }
            } else {
                quote! {
                    let #var_name = vm.as_primitive_client(&args[#idx])?.clone();
                }
            }
        }
        t if t.starts_with("Array") => {
            if is_mut {
                quote! {
                    let #var_name = vm.as_array_mut(&args[#idx])?;
                }
            } else {
                quote! {
                    let #var_name = vm.as_array(&args[#idx])?.to_vec();
                }
            }
        }
        t if t.starts_with("Map") => {
            if is_mut {
                quote! {
                    let #var_name = vm.as_map_mut(&args[#idx])?;
                }
            } else {
                quote! {
                    let #var_name = vm.as_map(&args[#idx])?.clone();
                }
            }
        }
        _ => {
            if is_mut {
                quote! {
                    let #var_name = vm.as_value_mut(&args[#idx])?;
                }
            } else {
                quote! {
                    let #var_name = &args[#idx];
                }
            }
        }
    }
}

/// Generate the arguments to pass to the clean function.
///
/// For immutable args: complex types were cloned, so we pass references.
/// For mutable args: we already have mutable references, so pass directly.
fn generate_call_args(d: &NativeFnDef) -> TokenStream2 {
    let mut args = Vec::new();

    if let Some((name, type_name, is_generic, is_mut)) = &d.receiver {
        let var_name = format_ident!("{}", name);
        if *is_mut {
            // Mutable receiver: already have &mut, pass directly
            args.push(quote!(#var_name));
        } else {
            let needs_ref = needs_reference(type_name, *is_generic);
            if needs_ref {
                args.push(quote!(&#var_name));
            } else {
                args.push(quote!(#var_name));
            }
        }
    }

    for (name, type_name, is_generic) in &d.params {
        let var_name = format_ident!("{}", name);
        let needs_ref = needs_reference(type_name, *is_generic);
        if needs_ref {
            args.push(quote!(&#var_name));
        } else {
            args.push(quote!(#var_name));
        }
    }

    quote!(#(#args),*)
}

/// Check if a type needs a reference when passing to the clean function.
fn needs_reference(type_name: &str, is_generic: bool) -> bool {
    if is_generic {
        return false; // Generic types are already passed as &Value
    }

    matches!(
        type_name,
        "String" | "Media" | "PromptAst" | "PrimitiveClient"
    ) || type_name.starts_with("Array")
        || type_name.starts_with("Map")
}

/// Generate code to convert the result back to Value.
fn generate_result_conversion(d: &NativeFnDef) -> TokenStream2 {
    let (type_name, is_generic, _is_fallible) = &d.returns;

    if *is_generic {
        return quote!(Ok(result));
    }

    match type_name.as_str() {
        "String" => quote!(Ok(vm.alloc_string(result))),
        "i64" => quote!(Ok(Value::Int(result))),
        "f64" => quote!(Ok(Value::Float(result))),
        "bool" => quote!(Ok(Value::Bool(result))),
        "()" => quote!(Ok(Value::Null)),
        t if t.starts_with("Option<String>") => quote! {
            Ok(match result {
                Some(s) => vm.alloc_string(s),
                None => Value::Null,
            })
        },
        t if t.starts_with("Option<") => quote! {
            Ok(match result {
                Some(v) => v.into_value(vm),
                None => Value::Null,
            })
        },
        t if t.starts_with("Array") => quote!(Ok(vm.alloc_array(result))),
        t if t.starts_with("Map") => quote!(Ok(vm.alloc_map(result))),
        "PromptAst" => quote!(Ok(vm.alloc_prompt_ast(result))),
        "PrimitiveClient" => quote!(Ok(vm.alloc_primitive_client(result))),
        _ => quote!(Ok(result)),
    }
}
