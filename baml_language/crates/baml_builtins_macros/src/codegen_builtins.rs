//! Code generation for `define_builtins` — the main signature registration macro.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

use crate::{collect::CollectedBuiltins, util::to_pascal_case};

pub(crate) fn generate(collected: &CollectedBuiltins) -> TokenStream2 {
    // Generate path constants.
    let path_consts: Vec<_> = collected
        .defs
        .iter()
        .map(|d| {
            let name = &d.const_name;
            let path = &d.path;
            quote!(pub const #name: &str = #path;)
        })
        .collect();

    let all_paths: Vec<_> = collected.defs.iter().map(|d| &d.path).collect();
    let const_names: Vec<_> = collected.defs.iter().map(|d| &d.const_name).collect();

    let native_const_names: Vec<_> = collected
        .defs
        .iter()
        .filter(|d| !d.is_sys_op)
        .map(|d| &d.const_name)
        .collect();

    // Generate builtin signatures.
    let signatures: Vec<_> = collected
        .defs
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
            let throw_strs: Vec<_> = d.throws.iter().map(String::as_str).collect();
            let panic_strs: Vec<_> = d.panics.iter().map(String::as_str).collect();

            quote! {
                BuiltinSignature {
                    path: paths::#const_name,
                    receiver: #receiver,
                    params: vec![#(#params),*],
                    returns: #returns,
                    is_sys_op: #is_sys_op,
                    throws: &[#(#throw_strs),*],
                    panics: &[#(#panic_strs),*],
                }
            }
        })
        .collect();

    // Generate native function info entries.
    let native_fn_entries: Vec<_> = collected
        .native_defs
        .iter()
        .map(|d| {
            let const_name = &d.const_name;
            let path = &d.path;
            let fn_name = &d.fn_name;
            let uses_vm = d.uses_vm;

            let receiver_tokens = match &d.receiver {
                Some(r) => {
                    let name = &r.name;
                    let ty = &r.type_name;
                    let is_generic = r.is_generic;
                    let is_mut = r.is_mut;
                    quote!( some((#name, #ty, #is_generic, #is_mut)) )
                }
                None => quote!( none ),
            };

            let params_tokens: Vec<_> = d
                .params
                .iter()
                .map(|p| {
                    let name = &p.name;
                    let ty = &p.type_name;
                    let is_generic = p.is_generic;
                    quote!( (#name, #ty, #is_generic) )
                })
                .collect();

            let ret_ty = &d.returns.type_name;
            let ret_is_generic = d.returns.is_generic;
            let ret_is_fallible = d.returns.is_fallible;

            quote! {
                (#const_name, #path, #fn_name, #receiver_tokens, [#(#params_tokens),*], (#ret_ty, #ret_is_generic, #ret_is_fallible), #uses_vm)
            }
        })
        .collect();

    // Generate builtin type definitions.
    let type_definitions: Vec<_> = collected
        .type_defs
        .iter()
        .map(|td| {
            let path = &td.path;
            let field_defs: Vec<_> = td
                .fields
                .iter()
                .map(|f| {
                    let name = &f.name;
                    let ty = &f.ty;
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

            let runtime_kind = if td.has_dedicated_heap_variant {
                quote!(RuntimeKind::PromptAst)
            } else {
                quote!(RuntimeKind::Instance)
            };

            quote! {
                BuiltinTypeDefinition {
                    path: #path,
                    fields: vec![#(#field_defs),*],
                    runtime_kind: #runtime_kind,
                }
            }
        })
        .collect();

    // Generate builtin enum definitions.
    let enum_definitions: Vec<_> = collected
        .enum_defs
        .iter()
        .map(|ed| {
            let path = &ed.path;
            let variants: Vec<_> = ed.variants.iter().map(|v| quote!(#v)).collect();

            quote! {
                BuiltinEnumDefinition {
                    path: #path,
                    variants: vec![#(#variants),*],
                }
            }
        })
        .collect();

    // Generate sys_op entries.
    let sys_op_entries: Vec<_> = collected
        .native_defs
        .iter()
        .filter(|d| d.is_sys_op)
        .map(|d| {
            let fn_name_str = d.fn_name.to_string();
            let variant_name = format_ident!("{}", to_pascal_case(&fn_name_str));
            let path = &d.path;
            let fn_name = &d.fn_name;
            let uses_engine_ctx = d.uses_engine_ctx;
            let throw_cats: Vec<_> = d
                .throws
                .iter()
                .map(|s| format_ident!("{}", s))
                .collect();
            let panic_cats: Vec<_> = d
                .panics
                .iter()
                .map(|s| format_ident!("{}", s))
                .collect();

            quote! {
                { #variant_name, #path, #fn_name, #uses_engine_ctx, [#(#throw_cats),*], [#(#panic_cats),*] }
            }
        })
        .collect();

    quote! {
        /// Path constants for all builtins.
        pub mod paths {
            #(#path_consts)*

            /// All builtin paths as a slice.
            pub const ALL: &[&str] = &[#(#all_paths),*];
        }

        /// Invoke a macro with all builtin constant names.
        #[macro_export]
        macro_rules! for_all_builtins {
            ($callback:ident) => {
                $callback!(#(#const_names),*)
            };
        }

        /// Invoke a macro with only native (non-sys_op) builtin constant names.
        #[macro_export]
        macro_rules! for_native_builtins {
            ($callback:ident) => {
                $callback!(#(#native_const_names),*)
            };
        }

        /// Invoke a macro with all native function info.
        #[macro_export]
        macro_rules! for_native_functions {
            ($callback:ident) => {
                $callback!(
                    #(#native_fn_entries),*
                );
            };
        }

        /// Invoke a macro with all sys_op definitions.
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

        /// All built-in enum definitions.
        static BUILTIN_ENUMS: std::sync::LazyLock<Vec<BuiltinEnumDefinition>> = std::sync::LazyLock::new(|| {
            vec![
                #(#enum_definitions),*
            ]
        });
    }
}
