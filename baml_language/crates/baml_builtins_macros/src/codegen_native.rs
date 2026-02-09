//! Code generation for `generate_native_trait` — the VM-native function trait.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

use crate::collect::{CollectedBuiltins, NativeFnDef};

pub(crate) fn generate(collected: &CollectedBuiltins) -> TokenStream2 {
    let non_sys_ops: Vec<_> = collected
        .native_defs
        .iter()
        .filter(|d| !d.is_sys_op)
        .collect();

    // Generate required trait methods (clean signatures).
    let required_methods: Vec<_> = non_sys_ops
        .iter()
        .map(|d| {
            let fn_name = &d.fn_name;
            let params = generate_clean_params(d);
            let return_type = generate_clean_return_type(d);
            let has_mut_receiver = d.receiver.as_ref().is_some_and(|r| r.is_mut);

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

    // Generate default glue methods.
    let glue_methods: Vec<_> = non_sys_ops
        .iter()
        .map(|d| {
            let fn_name = &d.fn_name;
            let glue_fn_name = format_ident!("__{}", fn_name);
            let extract_args = generate_arg_extraction(d);
            let call_args = generate_call_args(d);
            let convert_result = generate_result_conversion(d);
            let has_mut_receiver = d.receiver.as_ref().is_some_and(|r| r.is_mut);
            let is_fallible = d.returns.is_fallible;

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

    // Generate get_native_fn match arms.
    let match_arms: Vec<_> = non_sys_ops
        .iter()
        .map(|d| {
            let path = &d.path;
            let glue_fn_name = format_ident!("__{}", d.fn_name);
            quote! {
                #path => Some(Self::#glue_fn_name),
            }
        })
        .collect();

    // Generate public wrapper functions.
    let public_wrappers: Vec<_> = non_sys_ops
        .iter()
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

    quote! {
        /// Trait for implementing native BAML functions.
        ///
        /// Implement the `baml_*` methods — they have clean Rust types.
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
        #(#public_wrappers)*
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Generate the clean parameter list for a trait method.
fn generate_clean_params(d: &NativeFnDef) -> TokenStream2 {
    let mut params = Vec::new();

    if let Some(r) = &d.receiver {
        let param_name = format_ident!("{}", r.name);
        let param_type = rust_type_for_input(&r.type_name, r.is_generic, r.is_mut);
        params.push(quote!(#param_name: #param_type));
    }

    for p in &d.params {
        let param_name = format_ident!("{}", p.name);
        let param_type = rust_type_for_input(&p.type_name, p.is_generic, false);
        params.push(quote!(#param_name: #param_type));
    }

    quote!(#(#params),*)
}

/// Generate the clean return type for a trait method.
fn generate_clean_return_type(d: &NativeFnDef) -> TokenStream2 {
    let inner_type = rust_type_for_output(&d.returns.type_name, d.returns.is_generic);
    if d.returns.is_fallible {
        quote!(Result<#inner_type, VmError>)
    } else {
        inner_type
    }
}

/// Map BAML type names to Rust input types.
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
            let inner = &t[7..t.len() - 1];
            let inner_type = rust_type_for_output(inner, false);
            quote!(Option<#inner_type>)
        }
        _ => quote!(Value),
    }
}

/// Generate code to extract arguments from `&[Value]`.
fn generate_arg_extraction(d: &NativeFnDef) -> TokenStream2 {
    let mut extractions = Vec::new();
    let has_mut_receiver = d.receiver.as_ref().is_some_and(|r| r.is_mut);

    if has_mut_receiver {
        // For mutable receivers: extract params first (they clone), then receiver last.
        for (idx, p) in d.params.iter().enumerate() {
            let var_name = format_ident!("{}", p.name);
            let arg_idx = idx + 1;
            let extraction =
                generate_single_extraction(&var_name, arg_idx, &p.type_name, p.is_generic, false);
            extractions.push(extraction);
        }

        if let Some(r) = &d.receiver {
            let var_name = format_ident!("{}", r.name);
            let extraction =
                generate_single_extraction(&var_name, 0, &r.type_name, r.is_generic, r.is_mut);
            extractions.push(extraction);
        }
    } else {
        let mut arg_idx = 0;

        if let Some(r) = &d.receiver {
            let var_name = format_ident!("{}", r.name);
            let extraction = generate_single_extraction(
                &var_name,
                arg_idx,
                &r.type_name,
                r.is_generic,
                r.is_mut,
            );
            extractions.push(extraction);
            arg_idx += 1;
        }

        for p in &d.params {
            let var_name = format_ident!("{}", p.name);
            let extraction =
                generate_single_extraction(&var_name, arg_idx, &p.type_name, p.is_generic, false);
            extractions.push(extraction);
            arg_idx += 1;
        }
    }

    quote!(#(#extractions)*)
}

/// Generate extraction code for a single argument.
fn generate_single_extraction(
    var_name: &syn::Ident,
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
fn generate_call_args(d: &NativeFnDef) -> TokenStream2 {
    let mut args = Vec::new();

    if let Some(r) = &d.receiver {
        let var_name = format_ident!("{}", r.name);
        if r.is_mut {
            args.push(quote!(#var_name));
        } else {
            let needs_ref = needs_reference(&r.type_name, r.is_generic);
            if needs_ref {
                args.push(quote!(&#var_name));
            } else {
                args.push(quote!(#var_name));
            }
        }
    }

    for p in &d.params {
        let var_name = format_ident!("{}", p.name);
        let needs_ref = needs_reference(&p.type_name, p.is_generic);
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
        return false;
    }

    matches!(
        type_name,
        "String" | "Media" | "PromptAst" | "PrimitiveClient"
    ) || type_name.starts_with("Array")
        || type_name.starts_with("Map")
}

/// Generate code to convert the result back to `Value`.
fn generate_result_conversion(d: &NativeFnDef) -> TokenStream2 {
    let type_name = &d.returns.type_name;
    let is_generic = d.returns.is_generic;

    if is_generic {
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
