//! DSL parser for the `with_builtins!` macro input.
//!
//! Parses Rust-like module/struct/fn declarations into an AST used by the
//! collection and codegen passes.

use syn::{
    Attribute, Generics, Ident, Result, ReturnType, Token, Type, braced, parenthesized,
    parse::{Parse, ParseStream},
};

/// The root input to the macro: a list of modules.
pub(crate) struct BuiltinsInput {
    pub(crate) modules: Vec<ModuleItem>,
}

impl Parse for BuiltinsInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut modules = Vec::new();
        while !input.is_empty() {
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
pub(crate) struct ModuleItem {
    pub(crate) name: Ident,
    pub(crate) items: Vec<ModuleContent>,
    /// Whether this module is marked with #[hide] (hidden from type checker).
    pub(crate) is_hidden: bool,
}

/// Content inside a module.
pub(crate) enum ModuleContent {
    Struct(StructItem),
    Enum(EnumItem),
    Function(Box<FunctionItem>),
    Module(ModuleItem),
}

/// An enum with variants (e.g., `#[builtin] enum ClientType { Primitive, Fallback, RoundRobin }`).
pub(crate) struct EnumItem {
    pub(crate) name: Ident,
    pub(crate) variants: Vec<Ident>,
    /// Whether this enum is marked with #[builtin].
    pub(crate) is_builtin: bool,
}

/// Content inside a struct.
pub(crate) enum StructMember {
    Field(Box<StructField>),
    Method(Box<FunctionItem>),
}

/// A field declaration in a struct.
pub(crate) struct StructField {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
    pub(crate) is_private: bool,
}

/// A struct with fields and methods.
pub(crate) struct StructItem {
    pub(crate) name: Ident,
    pub(crate) generics: Generics,
    pub(crate) members: Vec<StructMember>,
    /// Whether this struct is marked with #[builtin] (builtin type).
    pub(crate) is_builtin: bool,
    /// Whether this struct is marked with #[opaque] (dedicated heap variant, not Instance).
    pub(crate) is_opaque: bool,
}

/// A function or method declaration.
pub(crate) struct FunctionItem {
    pub(crate) name: Ident,
    pub(crate) generics: Generics,
    /// First parameter if it's `self: Type` or `self: mut Type`.
    /// The bool indicates whether it's mutable.
    pub(crate) receiver: Option<(Type, bool)>,
    /// Other parameters.
    pub(crate) params: Vec<(Ident, Type)>,
    /// Return type.
    pub(crate) return_type: Type,
    /// Whether this function uses the VM (marked with #[uses(vm)]).
    pub(crate) uses_vm: bool,
    /// Whether this function is a `sys_op` (marked with #[`sys_op`]).
    pub(crate) is_sys_op: bool,
    /// Whether this `sys_op` needs engine context (marked with #[`uses(engine_ctx)`]).
    pub(crate) uses_engine_ctx: bool,
}

impl ModuleItem {
    pub(crate) fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let is_hidden = attrs.iter().any(|attr| attr.path().is_ident("hide"));

        input.parse::<Token![mod]>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut items = Vec::new();
        while !content.is_empty() {
            let lookahead = content.lookahead1();
            if lookahead.peek(Token![mod]) {
                items.push(ModuleContent::Module(ModuleItem::parse_with_attrs(
                    &content,
                    &[],
                )?));
            } else if lookahead.peek(Token![struct]) {
                items.push(ModuleContent::Struct(content.parse()?));
            } else if lookahead.peek(Token![enum]) {
                items.push(ModuleContent::Enum(EnumItem::parse_with_attrs(
                    &content,
                    &[],
                )?));
            } else if lookahead.peek(Token![#]) {
                let attrs = content.call(Attribute::parse_outer)?;
                let lookahead2 = content.lookahead1();
                if lookahead2.peek(Token![struct]) {
                    items.push(ModuleContent::Struct(StructItem::parse_with_attrs(
                        &content, &attrs,
                    )?));
                } else if lookahead2.peek(Token![enum]) {
                    items.push(ModuleContent::Enum(EnumItem::parse_with_attrs(
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
    pub(crate) fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let is_builtin = attrs.iter().any(|attr| attr.path().is_ident("builtin"));
        let is_opaque = attrs.iter().any(|attr| attr.path().is_ident("opaque"));

        input.parse::<Token![struct]>()?;
        let name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let content;
        braced!(content in input);

        let mut members = Vec::new();
        while !content.is_empty() {
            let lookahead = content.lookahead1();
            if lookahead.peek(Token![#]) {
                let attrs = content.call(Attribute::parse_outer)?;
                members.push(StructMember::Method(Box::new(
                    FunctionItem::parse_with_attrs(&content, &attrs)?,
                )));
            } else if lookahead.peek(Token![fn]) {
                members.push(StructMember::Method(Box::new(content.parse()?)));
            } else {
                let fork = content.fork();
                let is_private = if let Ok(ident) = fork.parse::<Ident>() {
                    if ident == "private" {
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
                // Trailing comma required after every field (including the last).
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
            is_opaque,
        })
    }
}

impl Parse for StructItem {
    fn parse(input: ParseStream) -> Result<Self> {
        Self::parse_with_attrs(input, &[])
    }
}

impl FunctionItem {
    pub(crate) fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let mut uses_vm = false;
        let mut uses_engine_ctx = false;
        for attr in attrs.iter().filter(|a| a.path().is_ident("uses")) {
            let nested: Ident = attr.parse_args()?;
            match nested.to_string().as_str() {
                "vm" => uses_vm = true,
                "engine_ctx" => uses_engine_ctx = true,
                other => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        format!("unknown uses(...) argument: {other}"),
                    ));
                }
            }
        }
        let is_sys_op = attrs.iter().any(|attr| attr.path().is_ident("sys_op"));

        input.parse::<Token![fn]>()?;
        let name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let params_content;
        parenthesized!(params_content in input);

        let mut receiver = None;
        let mut params = Vec::new();

        let mut first = true;
        while !params_content.is_empty() {
            if !first {
                params_content.parse::<Token![,]>()?;
                if params_content.is_empty() {
                    break;
                }
            }
            first = false;

            if params_content.peek(Token![self]) {
                if !params.is_empty() || receiver.is_some() {
                    return Err(params_content.error("`self` must be the first parameter"));
                }
                params_content.parse::<Token![self]>()?;
                params_content.parse::<Token![:]>()?;
                let is_mut = params_content.peek(Token![mut]);
                if is_mut {
                    params_content.parse::<Token![mut]>()?;
                }
                let ty: Type = params_content.parse()?;
                receiver = Some((ty, is_mut));
            } else {
                let param_name: Ident = params_content.parse()?;
                params_content.parse::<Token![:]>()?;
                let param_type: Type = params_content.parse()?;
                params.push((param_name, param_type));
            }
        }

        let return_type = match input.parse::<ReturnType>()? {
            ReturnType::Default => syn::parse_quote!(()),
            ReturnType::Type(_, ty) => *ty,
        };

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
        let attrs = input.call(Attribute::parse_outer)?;
        Self::parse_with_attrs(input, &attrs)
    }
}

impl EnumItem {
    pub(crate) fn parse_with_attrs(input: ParseStream, attrs: &[Attribute]) -> Result<Self> {
        let is_builtin = attrs.iter().any(|attr| attr.path().is_ident("builtin"));

        input.parse::<Token![enum]>()?;
        let name: Ident = input.parse()?;

        let content;
        braced!(content in input);

        let mut variants = Vec::new();
        while !content.is_empty() {
            let variant_name: Ident = content.parse()?;
            variants.push(variant_name);
            // Optional trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(EnumItem {
            name,
            variants,
            is_builtin,
        })
    }
}
