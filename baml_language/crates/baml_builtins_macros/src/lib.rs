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

/// A collected builtin type definition (struct marked with #[builtin]).
struct BuiltinTypeDef {
    /// Full path like "baml.http.Response"
    path: String,
    /// Field definitions
    fields: Vec<BuiltinFieldDef>,
}

/// A field in a builtin type.
struct BuiltinFieldDef {
    /// Field name (e.g., "_handle", "`status_code`")
    name: String,
    /// Type pattern. All fields have a type (including private ones).
    /// Privacy is handled separately by the `is_private` field.
    ty: Option<TokenStream2>,
    /// Whether this field is private (not visible to BAML code).
    /// Private fields still have types but are excluded from type-checking maps.
    is_private: bool,
    /// Field index in the struct
    index: usize,
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
    /// Whether this `sys_op` needs engine context (marked with #[`uses(engine_ctx)`])
    uses_engine_ctx: bool,
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

/// Content inside a struct.
enum StructMember {
    Field(Box<StructField>),
    Method(Box<FunctionItem>),
}

/// A field declaration in a struct.
struct StructField {
    name: Ident,
    ty: Type,
    is_private: bool,
}

/// A struct with fields and methods.
struct StructItem {
    name: Ident,
    generics: Generics,
    members: Vec<StructMember>,
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
    /// Whether this `sys_op` needs engine context (marked with #[`uses(engine_ctx)`])
    uses_engine_ctx: bool,
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

        // Parse: struct Name<Generics> { ... }
        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let content;
        braced!(content in input);

        let mut members = Vec::new();
        while !content.is_empty() {
            // Check if this is a method (fn) or a field
            // Handle attributes first (for #[uses(vm)]/#[external] fn)
            let lookahead = content.lookahead1();
            if lookahead.peek(Token![#]) {
                // Must be a method with attributes
                let attrs = content.call(Attribute::parse_outer)?;
                members.push(StructMember::Method(Box::new(
                    FunctionItem::parse_with_attrs(&content, &attrs)?,
                )));
            } else if lookahead.peek(Token![fn]) {
                // Method without attributes
                members.push(StructMember::Method(Box::new(content.parse()?)));
            } else {
                // Field (possibly with "private" modifier)
                // Try to parse "private" as an identifier
                let fork = content.fork();
                let is_private = if let Ok(ident) = fork.parse::<Ident>() {
                    if ident == "private" {
                        // Consume the "private" keyword
                        content.parse::<Ident>()?;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                let field_name: Ident = content.parse()?;
                content.parse::<Token![:]>()?;
                let field_type: Type = content.parse()?;
                content.parse::<Token![,]>()?;
                members.push(StructMember::Field(Box::new(StructField {
                    name: field_name,
                    ty: field_type,
                    is_private,
                })));
            }
        }

        Ok(StructItem {
            name,
            generics,
            members,
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
        let uses_engine_ctx = attrs.iter().any(|attr| {
            if attr.path().is_ident("uses") {
                if let Ok(nested) = attr.parse_args::<Ident>() {
                    return nested == "engine_ctx";
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
            uses_engine_ctx,
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
    type_defs: &'a mut Vec<BuiltinTypeDef>,
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
                "ResourceHandle" => quote!(TypePattern::Resource),
                "PromptAst" => quote!(TypePattern::PromptAst),
                "PrimitiveClient" => quote!(TypePattern::PrimitiveClient),
                "Unknown" => quote!(TypePattern::BuiltinUnknown),
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
        Type::BareFn(fn_ty) => {
            // Function pointer type: fn(args) -> RetType
            let params: Vec<TokenStream2> = fn_ty
                .inputs
                .iter()
                .map(|arg| type_to_pattern(&arg.ty, generic_params, builtin_types))
                .collect();
            let ret = match &fn_ty.output {
                ReturnType::Default => quote!(TypePattern::Null),
                ReturnType::Type(_, ty) => type_to_pattern(ty, generic_params, builtin_types),
            };
            quote!(TypePattern::Function {
                params: vec![#(#params),*],
                ret: Box::new(#ret),
            })
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

/// Convert a `snake_case` identifier to `PascalCase`.
///
/// Used to generate `SysOp` enum variant names from function names.
/// E.g., "`baml_fs_open`" -> "`BamlFsOpen`"
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.collect::<String>()
                }
            }
        })
        .collect()
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
        type_defs: ctx.type_defs,
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

    // If this is a builtin type, collect field information
    if s.is_builtin && !ctx.is_hidden {
        let mut fields = Vec::new();
        let mut field_index = 0;

        for member in &s.members {
            if let StructMember::Field(field) = member {
                let ty = Some(type_to_pattern(
                    &field.ty,
                    &struct_generics,
                    ctx.builtin_types,
                ));

                fields.push(BuiltinFieldDef {
                    name: field.name.to_string(),
                    ty,
                    is_private: field.is_private,
                    index: field_index,
                });
                field_index += 1;
            }
        }

        if !fields.is_empty() {
            ctx.type_defs.push(BuiltinTypeDef {
                path: struct_path.clone(),
                fields,
            });
        }
    }

    for member in &s.members {
        let method = match member {
            StructMember::Method(m) => m,
            StructMember::Field(_) => continue, // Skip fields for now (handled separately)
        };
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
        uses_engine_ctx: f.uses_engine_ctx,
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
    let mut type_defs = Vec::new();
    for module in &input.modules {
        let mut ctx = CollectContext {
            path_prefix: String::new(),
            const_prefix: String::new(),
            fn_name_prefix: String::new(),
            defs: &mut defs,
            native_defs: &mut native_defs,
            type_defs: &mut type_defs,
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

    // Generate builtin type definitions with inline field vectors
    let type_definitions: Vec<_> = type_defs
        .iter()
        .map(|td| {
            let path = &td.path;
            let field_defs: Vec<_> = td
                .fields
                .iter()
                .map(|f| {
                    let name = &f.name;
                    let ty = &f.ty.as_ref().expect("all fields have types");
                    let ty = quote!(#ty);
                    let is_private = f.is_private;
                    let index = f.index;

                    quote! {
                        BuiltinField {
                            name: #name,
                            ty: #ty,
                            is_private: #is_private,
                            index: #index,
                        }
                    }
                })
                .collect();

            quote! {
                BuiltinTypeDefinition {
                    path: #path,
                    fields: vec![#(#field_defs),*],
                }
            }
        })
        .collect();

    // Generate sys_op entries for for_all_sys_ops! macro
    let sys_op_entries: Vec<_> = native_defs
        .iter()
        .filter(|d| d.is_sys_op)
        .map(|d| {
            let fn_name_str = d.fn_name.to_string();
            let variant_name = format_ident!("{}", to_pascal_case(&fn_name_str));
            let path = &d.path;
            let fn_name = &d.fn_name;
            let uses_engine_ctx = d.uses_engine_ctx;

            quote! {
                { #variant_name, #path, #fn_name, #uses_engine_ctx }
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

        /// Invoke a macro with all sys_op definitions.
        ///
        /// Each entry has format:
        /// `{ VariantName, "path.string", snake_name, uses_engine_ctx }`
        ///
        /// - `VariantName`: PascalCase enum variant (e.g., `BamlFsOpen`)
        /// - `"path.string"`: DSL path (e.g., `"baml.fs.open"`)
        /// - `snake_name`: snake_case function name (e.g., `baml_fs_open`)
        /// - `uses_engine_ctx`: whether the op needs `SysOpContext`
        ///
        /// Usage:
        /// ```ignore
        /// baml_builtins::for_all_sys_ops!(my_macro);
        /// // Expands to: my_macro! { { BamlFsOpen, "baml.fs.open", baml_fs_open, false } ... }
        /// ```
        #[macro_export]
        macro_rules! for_all_sys_ops {
            ($callback:ident) => {
                $callback! {
                    #(#sys_op_entries)*
                }
            };
        }

        /// All built-in function signatures.
        static BUILTINS: std::sync::LazyLock<Vec<BuiltinSignature>> = std::sync::LazyLock::new(|| {
            vec![
                #(#signatures),*
            ]
        });

        /// All built-in type definitions.
        static BUILTIN_TYPES: std::sync::LazyLock<Vec<BuiltinTypeDefinition>> = std::sync::LazyLock::new(|| {
            vec![
                #(#type_definitions),*
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
    let mut type_defs = Vec::new();
    for module in &input.modules {
        let mut ctx = CollectContext {
            path_prefix: String::new(),
            const_prefix: String::new(),
            fn_name_prefix: String::new(),
            defs: &mut defs,
            native_defs: &mut native_defs,
            type_defs: &mut type_defs,
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
///
/// Note: `ResourceHandle` is only used as a private field type, never as a function
/// parameter or return type, so it falls through to the `Value` fallback in both
/// `rust_type_for_input` and `rust_type_for_output`. This is intentional.
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

// ============================================================================
// Per-module sys_op traits
// ============================================================================

/// Extract the module name (second path segment) from a `sys_op` path.
///
/// E.g., `"baml.fs.open"` → `"fs"`, `"baml.llm.PrimitiveClient.parse"` → `"llm"`.
fn module_from_path(path: &str) -> &str {
    path.split('.').nth(1).unwrap_or_else(|| {
        panic!("sys_op path '{path}' should have at least 2 segments (e.g., baml.fs.open)")
    })
}

/// Generate per-module traits for `sys_op` implementations.
///
/// This proc macro is invoked via `baml_builtins::with_builtins!(...)` in `sys_types`.
/// It generates:
///
/// - One trait per DSL module (e.g., `SysOpFs`, `SysOpHttp`, `SysOpLlm`)
/// - **Clean trait methods** with typed parameters (no raw `Vec<BexValue>`)
///   that return `SysOpOutput` (no need to specify the `SysOp` variant)
/// - **Glue methods** (`__baml_*`) that handle arg extraction and error wrapping
/// - `SysOps::from_impl<T>()` to wire glue methods into the fn-pointer table
///
/// # Example
///
/// ```ignore
/// // Generated trait:
/// pub trait SysOpFs {
///     fn baml_fs_open(path: String) -> SysOpOutput { ... }
///     fn __baml_fs_open(heap: &Arc<BexHeap>, args: Vec<BexValue<'_>>, ctx: &SysOpContext) -> SysOpResult { ... }
/// }
///
/// // In sys_native:
/// impl SysOpFs for NativeSysOps {
///     fn baml_fs_open(path: String) -> SysOpOutput {
///         SysOpOutput::async_op(async move {
///             let file = File::open(&path).await.map_err(|e| OpErrorKind::Other(...))?;
///             Ok(BexExternalValue::String("done".into()))
///         })
///     }
/// }
/// ```
#[proc_macro]
pub fn generate_sys_op_traits(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);

    // Collect all builtin definitions (we only need native_defs for sys_ops)
    let builtin_types = collect_builtin_types(&input.modules);
    let mut native_defs = Vec::new();
    let mut defs = Vec::new();
    let mut type_defs = Vec::new();
    for module in &input.modules {
        let mut ctx = CollectContext {
            path_prefix: String::new(),
            const_prefix: String::new(),
            fn_name_prefix: String::new(),
            defs: &mut defs,
            native_defs: &mut native_defs,
            type_defs: &mut type_defs,
            builtin_types: &builtin_types,
            is_hidden: false,
        };
        collect_builtins(module, &mut ctx);
    }

    // Collect sys_ops with full NativeFnDef info
    let sys_op_defs: Vec<&NativeFnDef> = native_defs.iter().filter(|d| d.is_sys_op).collect();

    // Group by module (preserving insertion order)
    let mut module_order: Vec<String> = Vec::new();
    let mut module_ops: std::collections::HashMap<String, Vec<&NativeFnDef>> =
        std::collections::HashMap::new();
    for d in &sys_op_defs {
        let module = module_from_path(&d.path).to_string();
        if !module_ops.contains_key(&module) {
            module_order.push(module.clone());
        }
        module_ops.entry(module).or_default().push(d);
    }

    // Generate one trait per module
    let trait_defs: Vec<_> = module_order
        .iter()
        .map(|module_name| {
            let ops = &module_ops[module_name];
            let trait_name = format_ident!("SysOp{}", to_pascal_case(module_name));

            let methods: Vec<_> = ops
                .iter()
                .flat_map(|d| {
                    let fn_name = &d.fn_name;
                    let fn_name_str = fn_name.to_string();
                    let variant_name = format_ident!("{}", to_pascal_case(&fn_name_str));
                    let glue_fn_name = format_ident!("__{}", fn_name);

                    // Build clean parameter list
                    let clean_params = sys_op_clean_params(d, &builtin_types);
                    let clean_call_args = sys_op_clean_call_args(d, &builtin_types);

                    // Generate arg count (receiver counts as 1)
                    let arg_count = d.receiver.iter().count() + d.params.len();
                    let arg_count_lit = proc_macro2::Literal::usize_unsuffixed(arg_count);

                    // Generate extraction code
                    let extraction = sys_op_extraction(d, &builtin_types);

                    // Does the clean method get ctx?
                    let uses_ctx = d.uses_engine_ctx;
                    let ctx_param = if uses_ctx {
                        quote!(, ctx: &SysOpContext)
                    } else {
                        quote!()
                    };
                    let ctx_arg = if uses_ctx {
                        quote!(, ctx)
                    } else {
                        quote!()
                    };

                    // Compute the typed return: SysOpOutput<T>
                    let output_type = sys_op_output_type(d, &builtin_types);

                    // Clean method (overridable, default = Unsupported)
                    let clean_method = quote! {
                        #[allow(unused_variables)]
                        fn #fn_name(#clean_params #ctx_param) -> #output_type {
                            SysOpOutput::err(OpErrorKind::Unsupported)
                        }
                    };

                    // Glue method (default, not meant to be overridden)
                    let glue_method = quote! {
                        #[doc(hidden)]
                        fn #glue_fn_name(
                            heap: &::std::sync::Arc<BexHeap>,
                            args: Vec<bex_heap::BexValue<'_>>,
                            ctx: &SysOpContext,
                        ) -> SysOpResult {
                            if args.len() != #arg_count_lit {
                                return SysOpResult::Ready(Err(OpError::new(
                                    SysOp::#variant_name,
                                    OpErrorKind::InvalidArgumentCount {
                                        expected: #arg_count_lit,
                                        actual: args.len(),
                                    },
                                )));
                            }
                            #extraction
                            Self::#fn_name(#clean_call_args #ctx_arg).into_result(SysOp::#variant_name)
                        }
                    };

                    vec![clean_method, glue_method]
                })
                .collect();

            let doc = format!(
                "Per-module sys_op trait for the `{module_name}` module.\n\n\
                 Override the clean methods (e.g., `baml_fs_open`) with your \
                 implementation. The `__baml_*` glue methods handle arg \
                 extraction and error wrapping automatically."
            );
            quote! {
                #[doc = #doc]
                pub trait #trait_name {
                    #(#methods)*
                }
            }
        })
        .collect();

    // Generate SysOps::from_impl<T>() — wires to the GLUE methods
    let trait_names: Vec<_> = module_order
        .iter()
        .map(|m| format_ident!("SysOp{}", to_pascal_case(m)))
        .collect();

    let field_assignments: Vec<_> = sys_op_defs
        .iter()
        .map(|d| {
            let fn_name = &d.fn_name;
            let glue_fn_name = format_ident!("__{}", fn_name);
            quote! { #fn_name: T::#glue_fn_name, }
        })
        .collect();

    let from_impl_method = quote! {
        impl SysOps {
            /// Build a `SysOps` table from a type that implements the per-module traits.
            ///
            /// Each module trait (`SysOpFs`, `SysOpHttp`, etc.) provides default
            /// `Unsupported` implementations, so you only need to override the ops
            /// your provider supports.
            pub fn from_impl<T: #(#trait_names)+*>() -> Self {
                Self {
                    #(#field_assignments)*
                }
            }
        }
    };

    let output = quote! {
        #(#trait_defs)*
        #from_impl_method
    };

    output.into()
}

// ============================================================================
// Helpers for generating clean sys_op trait signatures
// ============================================================================

/// Map a DSL type name to the Rust type used in clean `sys_op` trait signatures.
///
/// Returns `Ok(tokens)` for known types, `Err(type_name)` for unrecognised ones.
/// Callers decide whether to fall back to `BexExternalValue` or emit a compile error.
fn sys_op_rust_type(
    type_name: &str,
    builtin_types: &HashMap<String, String>,
) -> std::result::Result<TokenStream2, String> {
    match type_name {
        "String" => Ok(quote!(String)),
        "i64" => Ok(quote!(i64)),
        "f64" => Ok(quote!(f64)),
        "bool" => Ok(quote!(bool)),
        "()" => Ok(quote!(())),
        "Media" => Ok(quote!(bex_vm_types::MediaValue)),
        "PromptAst" => Ok(quote!(bex_vm_types::PromptAst)),
        _ if builtin_types.contains_key(type_name) => {
            let ref_ident = sys_op_ref_type_ident(type_name, builtin_types);
            Ok(quote!(bex_heap::builtin_types::owned::#ref_ident))
        }
        other => Err(other.to_string()),
    }
}

/// Derive the Rust type identifier for a builtin struct from its DSL path.
///
/// Given a DSL path like `"baml.fs.File"`, strips the `baml.` prefix, `PascalCases`
/// the module segment, and joins with the struct name: `"fs.File"` → `FsFile`.
///
/// This is deterministic and requires no hardcoded mapping — adding a new builtin
/// struct to the DSL automatically gives it the correct Rust name.
fn sys_op_ref_type_ident(type_name: &str, builtin_types: &HashMap<String, String>) -> Ident {
    let path = builtin_types
        .get(type_name)
        .unwrap_or_else(|| panic!("Unknown builtin struct for sys_op extraction: {type_name}"));
    // path is e.g. "baml.fs.File" → strip "baml." → "fs.File"
    let without_baml = path
        .strip_prefix("baml.")
        .unwrap_or_else(|| panic!("builtin path '{path}' should start with 'baml.'"));
    // Split into ["fs", "File"], PascalCase each segment, join
    let ident_str: String = without_baml
        .split('.')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
                None => String::new(),
            }
        })
        .collect();
    format_ident!("{}", ident_str)
}

/// Generate the extraction expression for a single arg inside `with_gc_protection`.
fn sys_op_extract_one(
    type_name: &str,
    arg_ident: &Ident,
    builtin_types: &HashMap<String, String>,
) -> TokenStream2 {
    match type_name {
        "String" => quote!(#arg_ident.as_string(&__p).cloned()?),
        _ if builtin_types.contains_key(type_name) && type_name != "PromptAst" => {
            let ref_type = sys_op_ref_type_ident(type_name, builtin_types);
            quote!(
                #arg_ident
                    .as_builtin_class::<bex_heap::builtin_types::#ref_type>(&__p)?
                    .into_owned(&__p)?
            )
        }
        "PromptAst" => quote!(#arg_ident.as_prompt_ast_owned(&__p)?),
        // Generic fallback for Map, Array, Any, Unknown
        _ => quote!(#arg_ident.as_owned_but_very_slow(&__p)?),
    }
}

/// Generate the clean parameter list for a `sys_op` trait method.
///
/// Receiver becomes the first param (renamed from "self" to a safe name).
/// `ctx` is NOT included here — it's appended separately for `#[uses(engine_ctx)]` ops.
fn sys_op_clean_params(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    let mut params = Vec::new();

    if let Some((_name, type_name, _is_generic, _is_mut)) = &d.receiver {
        let param_name = sys_op_receiver_name(type_name);
        let param_type = sys_op_rust_type(type_name, builtin_types)
            .unwrap_or_else(|_| quote!(bex_external_types::BexExternalValue));
        params.push(quote!(#param_name: #param_type));
    }

    for (name, type_name, _is_generic) in &d.params {
        let param_name = format_ident!("{}", name);
        let param_type = sys_op_rust_type(type_name, builtin_types)
            .unwrap_or_else(|_| quote!(bex_external_types::BexExternalValue));
        params.push(quote!(#param_name: #param_type));
    }

    quote!(#(#params),*)
}

/// Generate the argument list for calling the clean method from the glue.
fn sys_op_clean_call_args(
    d: &NativeFnDef,
    _builtin_types: &HashMap<String, String>,
) -> TokenStream2 {
    let mut args = Vec::new();

    if let Some((_name, type_name, _is_generic, _is_mut)) = &d.receiver {
        let param_name = sys_op_receiver_name(type_name);
        args.push(quote!(#param_name));
    }

    for (name, _type_name, _is_generic) in &d.params {
        let param_name = format_ident!("{}", name);
        args.push(quote!(#param_name));
    }

    quote!(#(#args),*)
}

/// Generate the full extraction block for a `sys_op`'s glue method.
///
/// This creates:
/// 1. `args.into_iter()` and `next().unwrap()` for each arg
/// 2. `heap.with_gc_protection(|p| { ... })` to extract all args at once
/// 3. Destructuring of the extracted tuple into named variables
fn sys_op_extraction(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    // Collect all args: receiver (if any) + params
    struct ArgInfo {
        param_name: Ident,
        type_name: String,
        arg_var: Ident,
    }

    let fn_name_str = d.fn_name.to_string();
    let variant_name = format_ident!("{}", to_pascal_case(&fn_name_str));

    let mut all_args: Vec<ArgInfo> = Vec::new();

    if let Some((_name, type_name, _is_generic, _is_mut)) = &d.receiver {
        all_args.push(ArgInfo {
            param_name: sys_op_receiver_name(type_name),
            type_name: type_name.clone(),
            arg_var: format_ident!("__arg{}", all_args.len()),
        });
    }

    for (name, type_name, _is_generic) in &d.params {
        all_args.push(ArgInfo {
            param_name: format_ident!("{}", name),
            type_name: type_name.clone(),
            arg_var: format_ident!("__arg{}", all_args.len()),
        });
    }

    // Step 1: destructure args vec
    let arg_destructuring: Vec<_> = all_args
        .iter()
        .map(|a| {
            let arg_var = &a.arg_var;
            quote! { let #arg_var = __args_iter.next().unwrap(); }
        })
        .collect();

    // Step 2: extraction expressions inside with_gc_protection
    let extraction_exprs: Vec<_> = all_args
        .iter()
        .map(|a| {
            let extract = sys_op_extract_one(&a.type_name, &a.arg_var, builtin_types);
            let param_name = &a.param_name;
            quote! { let #param_name = #extract; }
        })
        .collect();

    // Step 3: result tuple (the names extracted inside GC scope)
    let result_names: Vec<_> = all_args.iter().map(|a| &a.param_name).collect();

    quote! {
        let mut __args_iter = args.into_iter();
        #(#arg_destructuring)*
        let (#(#result_names,)*) = match heap.with_gc_protection(move |__p| {
            #(#extraction_exprs)*
            Ok::<_, bex_heap::AccessError>((#(#result_names,)*))
        }) {
            Ok(v) => v,
            Err(e) => return SysOpResult::Ready(Err(OpError::new(
                SysOp::#variant_name,
                OpErrorKind::AccessError(e),
            ))),
        };
    }
}

/// Generate the typed `SysOpOutput<T>` return type for a `sys_op` trait method.
///
/// Uses the return type info from `NativeFnDef.returns` to pick a concrete `T`.
/// Falls back to `SysOpOutput` (= `SysOpOutput<BexExternalValue>`) for explicitly
/// generic/unknown return types. Panics at macro-expansion time for unrecognised
/// concrete types — add them to `sys_op_rust_type` instead.
fn sys_op_output_type(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    let (ref type_name, is_generic, _is_fallible) = d.returns;

    // Generic or unknown types → use the default (BexExternalValue)
    if is_generic || type_name == "Any" || type_name == "Unknown" || type_name == "unknown" {
        return quote!(SysOpOutput);
    }

    match sys_op_rust_type(type_name, builtin_types) {
        Ok(inner) => quote!(SysOpOutput<#inner>),
        Err(unknown) => panic!(
            "sys_op_output_type: unsupported return type `{unknown}`. \
             Add it to `sys_op_rust_type` in baml_builtins_macros."
        ),
    }
}

/// Generate a safe parameter name for a receiver (since "self" is a keyword).
fn sys_op_receiver_name(type_name: &str) -> Ident {
    let snake = type_name
        .chars()
        .enumerate()
        .fold(String::new(), |mut acc, (i, c)| {
            if c.is_uppercase() && i > 0 {
                acc.push('_');
            }
            acc.push(c.to_ascii_lowercase());
            acc
        });
    format_ident!("{}", snake)
}
