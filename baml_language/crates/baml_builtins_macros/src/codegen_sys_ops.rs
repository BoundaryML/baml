//! Code generation for `generate_sys_op_traits` — per-module traits for
//! external/async operations.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

use crate::{
    collect::{CollectedBuiltins, NativeFnDef},
    util::{path_to_rust_ident, to_pascal_case, to_snake_case},
};

pub(crate) fn generate(collected: &CollectedBuiltins) -> TokenStream2 {
    let sys_op_defs: Vec<&NativeFnDef> = collected
        .native_defs
        .iter()
        .filter(|d| d.is_sys_op)
        .collect();

    // Group by module (preserving insertion order).
    let mut module_order: Vec<String> = Vec::new();
    let mut module_ops: HashMap<String, Vec<&NativeFnDef>> = HashMap::new();
    for d in &sys_op_defs {
        let module = module_from_path(&d.path).to_string();
        if !module_ops.contains_key(&module) {
            module_order.push(module.clone());
        }
        module_ops.entry(module).or_default().push(d);
    }

    // Generate one trait per module.
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

                    let clean_params = sys_op_clean_params(d, &collected.builtin_types);
                    let clean_call_args = sys_op_clean_call_args(d);

                    let arg_count = d.receiver.iter().count() + d.params.len();
                    let arg_count_lit = proc_macro2::Literal::usize_unsuffixed(arg_count);

                    let extraction = sys_op_extraction(d, &collected.builtin_types);

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

                    let output_type = sys_op_output_type(d, &collected.builtin_types);

                    let clean_method = quote! {
                        #[allow(unused_variables)]
                        fn #fn_name(#clean_params #ctx_param) -> #output_type {
                            SysOpOutput::err(OpErrorKind::Unsupported)
                        }
                    };

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

    // Generate SysOps::from_impl<T>().
    let trait_names: Vec<_> = module_order
        .iter()
        .map(|m| format_ident!("SysOp{}", to_pascal_case(m)))
        .collect();

    let field_assignments: Vec<_> = sys_op_defs
        .iter()
        .map(|d| {
            let fn_name = &d.fn_name;
            let glue_fn_name = format_ident!("__{}", fn_name);
            quote! { #fn_name: ::std::sync::Arc::new(T::#glue_fn_name), }
        })
        .collect();

    let from_impl_method = quote! {
        impl SysOps {
            /// Build a `SysOps` table from a type that implements the per-module traits.
            pub fn from_impl<T: #(#trait_names)+* + 'static>() -> Self {
                Self {
                    #(#field_assignments)*
                }
            }
        }
    };

    quote! {
        #(#trait_defs)*
        #from_impl_method
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract the module name from a `sys_op` path.
///
/// For 2-segment paths like `"env.get"`, the module is the first segment (`"env"`).
/// For 3+ segment paths like `"baml.fs.open"`, the module is the second segment (`"fs"`).
fn module_from_path(path: &str) -> &str {
    let mut segments = path.split('.');
    let first = segments
        .next()
        .unwrap_or_else(|| panic!("sys_op path '{path}' should have at least 2 segments"));
    match segments.next() {
        None => panic!("sys_op path '{path}' should have at least 2 segments"),
        Some(second) => {
            if segments.next().is_none() {
                // 2-segment path: "env.get" → module is "env"
                first
            } else {
                // 3+ segment path: "baml.fs.open" → module is "fs"
                second
            }
        }
    }
}

/// Map a DSL type name to the Rust type used in clean `sys_op` trait signatures.
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
        t if t.starts_with("Option<") && t.ends_with('>') => {
            let inner = &t[7..t.len() - 1];
            let inner_type = sys_op_rust_type(inner.trim(), builtin_types)?;
            Ok(quote!(Option<#inner_type>))
        }
        _ if builtin_types.contains_key(type_name) => {
            let full_path = builtin_types
                .get(type_name)
                .expect("checked by contains_key");
            let ref_ident = path_to_rust_ident(full_path);
            Ok(quote!(bex_heap::builtin_types::owned::#ref_ident))
        }
        other => Err(other.to_string()),
    }
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
            let full_path = builtin_types
                .get(type_name)
                .expect("checked by contains_key");
            let ref_type = path_to_rust_ident(full_path);
            quote!(
                #arg_ident
                    .as_builtin_class::<bex_heap::builtin_types::#ref_type>(&__p)?
                    .into_owned(&__p)?
            )
        }
        "PromptAst" => quote!(#arg_ident.as_prompt_ast_owned(&__p)?),
        _ => quote!(#arg_ident.as_owned_but_very_slow(&__p)?),
    }
}

/// Generate the clean parameter list for a `sys_op` trait method.
fn sys_op_clean_params(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    let mut params = Vec::new();

    if let Some(r) = &d.receiver {
        let param_name = format_ident!("{}", to_snake_case(&r.type_name));
        let param_type = sys_op_rust_type(&r.type_name, builtin_types)
            .unwrap_or_else(|_| quote!(bex_external_types::BexExternalValue));
        params.push(quote!(#param_name: #param_type));
    }

    for p in &d.params {
        let param_name = format_ident!("{}", p.name);
        let param_type = sys_op_rust_type(&p.type_name, builtin_types)
            .unwrap_or_else(|_| quote!(bex_external_types::BexExternalValue));
        params.push(quote!(#param_name: #param_type));
    }

    quote!(#(#params),*)
}

/// Generate the argument list for calling the clean method from the glue.
fn sys_op_clean_call_args(d: &NativeFnDef) -> TokenStream2 {
    let mut args = Vec::new();

    if let Some(r) = &d.receiver {
        let param_name = format_ident!("{}", to_snake_case(&r.type_name));
        args.push(quote!(#param_name));
    }

    for p in &d.params {
        let param_name = format_ident!("{}", p.name);
        args.push(quote!(#param_name));
    }

    quote!(#(#args),*)
}

/// Generate the full extraction block for a `sys_op`'s glue method.
fn sys_op_extraction(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    struct ArgInfo {
        param_name: Ident,
        type_name: String,
        arg_var: Ident,
    }

    let fn_name_str = d.fn_name.to_string();
    let variant_name = format_ident!("{}", to_pascal_case(&fn_name_str));

    let mut all_args: Vec<ArgInfo> = Vec::new();

    if let Some(r) = &d.receiver {
        all_args.push(ArgInfo {
            param_name: format_ident!("{}", to_snake_case(&r.type_name)),
            type_name: r.type_name.clone(),
            arg_var: format_ident!("__arg{}", all_args.len()),
        });
    }

    for p in &d.params {
        all_args.push(ArgInfo {
            param_name: format_ident!("{}", p.name),
            type_name: p.type_name.clone(),
            arg_var: format_ident!("__arg{}", all_args.len()),
        });
    }

    let arg_destructuring: Vec<_> = all_args
        .iter()
        .map(|a| {
            let arg_var = &a.arg_var;
            quote! { let #arg_var = __args_iter.next().unwrap(); }
        })
        .collect();

    let extraction_exprs: Vec<_> = all_args
        .iter()
        .map(|a| {
            let extract = sys_op_extract_one(&a.type_name, &a.arg_var, builtin_types);
            let param_name = &a.param_name;
            quote! { let #param_name = #extract; }
        })
        .collect();

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
fn sys_op_output_type(d: &NativeFnDef, builtin_types: &HashMap<String, String>) -> TokenStream2 {
    let type_name = &d.returns.type_name;
    let is_generic = d.returns.is_generic;

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
