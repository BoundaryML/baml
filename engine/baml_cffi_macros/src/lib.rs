use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, ImplItem, ItemImpl, ReturnType, Token, Type};

#[proc_macro_attribute]
pub fn export_baml_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    // Extract methods marked with #[export_baml_fn]
    let mut exported_methods = Vec::new();
    let mut regular_methods = Vec::new();

    for item in &input.items {
        if let ImplItem::Fn(method) = item {
            let has_export_attr = method
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("export_baml_fn"));

            if has_export_attr {
                exported_methods.push(method);
            } else {
                regular_methods.push(item);
            }
        } else {
            regular_methods.push(item);
        }
    }

    // Generate the CallMethod implementation
    let _type_name = &input.self_ty;
    let match_arms = exported_methods.iter().map(|method| {
        let method_name = &method.sig.ident;
        let method_name_str = method_name.to_string();

        // Extract method parameters (excluding &self)
        let params: Vec<_> = method.sig.inputs.iter().skip(1).collect();

        let method_call = if params.is_empty() {
            quote! { self.#method_name() }
        } else {
            // Collect parameter information for validation, excluding runtime parameters
            let param_info: Vec<_> = params.iter().filter_map(|param| {
                if let syn::FnArg::Typed(pat_type) = param {
                    if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                        let param_name = &pat_ident.ident;
                        let param_name_str = param_name.to_string();

                        // Skip runtime parameters as they're injected automatically
                        if param_name_str == "runtime" && is_runtime_type(&pat_type.ty) {
                            return None;
                        }

                        return Some((param_name, param_name_str, &pat_type.ty));
                    }
                }
                None
            }).collect();

            let required_params: Vec<_> = param_info.iter()
                .filter(|(_, _, ty)| !is_optional_type(ty))
                .map(|(_, name_str, _)| name_str.clone())
                .collect();

            let all_param_names: Vec<_> = param_info.iter()
                .map(|(_, name_str, _)| name_str.clone())
                .collect();

            // Generate parameter validation
            let param_validation = if !param_info.is_empty() {
                let required_params_str = format!("[{}]", required_params.join(", "));
                let all_params_str = format!("[{}]", all_param_names.join(", "));

                quote! {
                    // Validate required parameters are present
                    let required_params = vec![#(#required_params),*];
                    for required_param in &required_params {
                        if !_kwargs.contains_key(*required_param) {
                            return Err(format!("Missing required parameter: '{}'. Required parameters: {}", 
                                required_param, #required_params_str));
                        }
                    }

                    // Validate no extra parameters
                    let valid_params = vec![#(#all_param_names),*];
                    for provided_param in _kwargs.keys() {
                        if !valid_params.contains(&provided_param.as_str()) {
                            return Err(format!("Unknown parameter: '{}'. Valid parameters: {}", 
                                provided_param, #all_params_str));
                        }
                    }
                }
            } else {
                quote! {
                    // Validate no parameters provided when none expected
                    if !_kwargs.is_empty() {
                        let provided_params: Vec<_> = _kwargs.keys().collect();
                        return Err(format!("Method '{}' accepts no parameters, but got: {:?}", 
                            method_name, provided_params));
                    }
                }
            };
            // Generate parameter extraction code
            let param_extractions: Vec<_> = param_info.iter().map(|(param_name, param_name_str, ty)| {
                generate_param_extraction(param_name, param_name_str, ty)
            }).collect();
            // Collect all parameter names, including runtime parameters
            let all_param_names: Vec<_> = params.iter().filter_map(|param| {
                if let syn::FnArg::Typed(pat_type) = param {
                    if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                        Some(&pat_ident.ident)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }).collect();

            // Generate method call arguments, replacing runtime parameters with the runtime variable
            let method_call_args: Vec<_> = all_param_names.iter().map(|param_name| {
                let param_name_str = param_name.to_string();
                if param_name_str == "runtime" {
                    quote! { runtime }
                } else {
                    quote! { #param_name }
                }
            }).collect();

            quote! {
                {
                    #param_validation
                    #(#param_extractions)*
                    self.#method_name(#(#method_call_args),*)
                }
            }
        };

        // Analyze return type to determine how to wrap the result
        let wrapper_call = match &method.sig.output {
            ReturnType::Type(_, ty) => {
                if is_result_type(ty) {
                    // Handle Result<T, E> types - check what T is
                    if is_result_rawptrtype(ty) {
                        quote! {
                            match #method_call {
                                Ok(result) => Ok(BamlObjectResponseSuccess::new_object(result)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_vec_rawptrtype(ty) {
                        quote! {
                            match #method_call {
                                Ok(result) => Ok(BamlObjectResponseSuccess::new_objects(result)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_vec_convertible_to_rawptr(ty) {
                        quote! {
                            match #method_call {
                                Ok(result) => Ok(BamlObjectResponseSuccess::new_objects(result.into_iter().map(|item| item.into()).collect())),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_option_rawptrtype(ty) {
                        quote! {
                            match #method_call {
                                Ok(Some(result)) => Ok(BamlObjectResponseSuccess::new_object(result)),
                                Ok(None) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_option_bamlvalue(ty) {
                        quote! {
                            match #method_call {
                                Ok(Some(result)) => Ok(BamlObjectResponseSuccess::new_value(result)),
                                Ok(None) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_option_vec_rawptrtype(ty) {
                        quote! {
                            match #method_call {
                                Ok(Some(result)) => Ok(BamlObjectResponseSuccess::new_objects(result)),
                                Ok(None) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_option_string(ty) {
                        quote! {
                            match #method_call {
                                Ok(Some(value)) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(value))),
                                Ok(None) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_bool(ty) {
                        quote! {
                            match #method_call {
                                Ok(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Bool(value))),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_bamlvalue(ty) {
                        // Result<BamlValue, E>
                        quote! {
                            match #method_call {
                                Ok(result) => Ok(BamlObjectResponseSuccess::new_value(result)),
                                Err(e) => Err(e),
                            }
                        }
                    } else if is_result_unit(ty) {
                        // Result<(), E>
                        quote! {
                            match #method_call {
                                Ok(()) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                Err(e) => Err(e),
                            }
                        }
                    } else {
                        // Result<T, E> where T converts to RawPtrType via .into()
                        quote! {
                            match #method_call {
                                Ok(result) => Ok(BamlObjectResponseSuccess::new_object(result.into())),
                                Err(e) => Err(e),
                            }
                        }
                    }
                } else if is_rawptrtype(ty) {
                    // Direct RawPtrType return - wrap in Ok and new_object
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_object(#method_call))
                    }
                } else if is_vec_rawptrtype(ty) {
                    // Direct Vec<RawPtrType> return - wrap in Ok and new_objects
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_objects(#method_call))
                    }
                } else if is_option_rawptrtype(ty) {
                    // Direct Option<RawPtrType> return - wrap in Ok and handle None
                    quote! {
                        match #method_call {
                            Some(result) => Ok(BamlObjectResponseSuccess::new_object(result)),
                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                        }
                    }
                } else if is_vec_convertible_to_rawptr(ty) {
                    // Direct Vec<T> where T converts to RawPtrType via .into()
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_objects((#method_call).into_iter().map(|item| item.into()).collect()))
                    }
                } else if is_vec_either_convertible_to_rawptr(ty) {
                    // Direct Vec<Either<L, R>> where both L and R convert to RawPtrType
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_objects((#method_call).into_iter().map(|either| {
                            match either {
                                either::Either::Left(left) => left.into(),
                                either::Either::Right(right) => right.into(),
                            }
                        }).collect()))
                    }
                } else if is_option_vec_convertible_to_rawptr(ty) {
                    // Direct Option<Vec<T>> where T converts to RawPtrType via .into()
                    quote! {
                        match #method_call {
                            Some(vec) => Ok(BamlObjectResponseSuccess::new_objects(vec.into_iter().map(|item| item.into()).collect())),
                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                        }
                    }
                } else if is_option_either_convertible_to_rawptr(ty) {
                    // Direct Option<Either<L, R>> where both L and R convert to RawPtrType
                    quote! {
                        match #method_call {
                            Some(either) => Ok(BamlObjectResponseSuccess::new_object(match either {
                                either::Either::Left(left) => left.into(),
                                either::Either::Right(right) => right.into(),
                            })),
                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                        }
                    }
                } else if is_either_convertible_to_rawptr(ty) {
                    // Direct Either<L, R> where both L and R convert to RawPtrType
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_object(match #method_call {
                            either::Either::Left(left) => left.into(),
                            either::Either::Right(right) => right.into(),
                        }))
                    }
                } else if is_option_convertible_to_rawptr(ty) {
                    // Direct Option<T> where T converts to RawPtrType via .into()
                    quote! {
                        match #method_call {
                            Some(result) => Ok(BamlObjectResponseSuccess::new_object(result.into())),
                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                        }
                    }
                } else if is_vec_basic_baml_type(ty) {
                    // Direct Vec<basic_type> where basic_type converts to BamlValue
                    if let Type::Path(type_path) = &**ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            if segment.ident == "Vec" {
                                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                                        if let Type::Path(inner_type_path) = inner_type {
                                            if let Some(inner_segment) = inner_type_path.path.segments.last() {
                                                let type_name = inner_segment.ident.to_string();
                                                match type_name.as_str() {
                                                    "i64" | "i32" => quote! {
                                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::Int(item)).collect())))
                                                    },
                                                    "f64" | "f32" => quote! {
                                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::Float(item as f64)).collect())))
                                                    },
                                                    "bool" => quote! {
                                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::Bool(item)).collect())))
                                                    },
                                                    "String" => quote! {
                                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::String(item)).collect())))
                                                    },
                                                    _ => quote! {
                                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::String(item.to_string())).collect())))
                                                    }
                                                }
                                            } else {
                                                quote! {
                                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                                                }
                                            }
                                        } else if let Type::Reference(_inner_type_ref) = inner_type {
                                            // Handle Vec<&str>
                                            quote! {
                                                Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| BamlValue::String(item.to_string())).collect())))
                                            }
                                        } else {
                                            quote! {
                                                Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                                            }
                                        }
                                    } else {
                                        quote! {
                                            Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                                        }
                                    }
                                } else {
                                    quote! {
                                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                                    }
                                }
                            } else {
                                quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                                }
                            }
                        } else {
                            quote! {
                                Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                            }
                        }
                    } else {
                        quote! {
                            Ok(BamlObjectResponseSuccess::new_value(BamlValue::List((#method_call).into_iter().map(|item| item.into()).collect())))
                        }
                    }
                } else if is_option_basic_baml_type(ty) {
                    // Direct Option<basic_type> where basic_type converts to BamlValue
                    if let Type::Path(type_path) = &**ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            if segment.ident == "Option" {
                                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                                        if let Type::Path(inner_type_path) = inner_type {
                                            if let Some(inner_segment) = inner_type_path.path.segments.last() {
                                                let type_name = inner_segment.ident.to_string();
                                                match type_name.as_str() {
                                                    "i64" | "i32" => quote! {
                                                        match #method_call {
                                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(value))),
                                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                        }
                                                    },
                                                    "f64" | "f32" => quote! {
                                                        match #method_call {
                                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Float(value as f64))),
                                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                        }
                                                    },
                                                    "bool" => quote! {
                                                        match #method_call {
                                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Bool(value))),
                                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                        }
                                                    },
                                                    "String" => quote! {
                                                        match #method_call {
                                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(value))),
                                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                        }
                                                    },
                                                    _ => quote! {
                                                        match #method_call {
                                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(value.to_string()))),
                                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                        }
                                                    }
                                                }
                                            } else {
                                                quote! {
                                                    match #method_call {
                                                        Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                                        None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                    }
                                                }
                                            }
                                        } else if let Type::Reference(_inner_type_ref) = inner_type {
                                            // Handle Option<&str>
                                            quote! {
                                                match #method_call {
                                                    Some(value) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(value.to_string()))),
                                                    None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                }
                                            }
                                        } else {
                                            quote! {
                                                match #method_call {
                                                    Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                                    None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                                }
                                            }
                                        }
                                    } else {
                                        quote! {
                                            match #method_call {
                                                Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                                None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                            }
                                        }
                                    }
                                } else {
                                    quote! {
                                        match #method_call {
                                            Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                            None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                        }
                                    }
                                }
                            } else {
                                quote! {
                                    match #method_call {
                                        Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                        None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                    }
                                }
                            }
                        } else {
                            quote! {
                                match #method_call {
                                    Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                    None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                                }
                            }
                        }
                    } else {
                        quote! {
                            match #method_call {
                                Some(value) => Ok(BamlObjectResponseSuccess::new_value(value.into())),
                                None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                            }
                        }
                    }
                } else if is_basic_baml_type(ty) {
                    // Direct basic type that converts to BamlValue
                    if let Type::Path(type_path) = &**ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            let type_name = segment.ident.to_string();
                            match type_name.as_str() {
                                "i64" | "i32" => quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(#method_call)))
                                },
                                "f64" | "f32" => quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Float(#method_call as f64)))
                                },
                                "bool" => quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Bool(#method_call)))
                                },
                                "String" => quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(#method_call)))
                                },
                                _ => quote! {
                                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::String((#method_call).to_string())))
                                }
                            }
                        } else {
                            quote! {
                                Ok(BamlObjectResponseSuccess::new_value((#method_call).into()))
                            }
                        }
                    } else if let Type::Reference(_type_ref) = &**ty {
                        // Handle &str
                        quote! {
                            Ok(BamlObjectResponseSuccess::new_value(BamlValue::String((#method_call).to_string())))
                        }
                    } else {
                        quote! {
                            Ok(BamlObjectResponseSuccess::new_value((#method_call).into()))
                        }
                    }
                } else if is_convertible_to_rawptr(ty) {
                    // Direct type that can convert to RawPtrType via .into()
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_object((#method_call).into()))
                    }
                } else {
                    // Any other direct type - try to use as BamlValue
                    quote! {
                        Ok(BamlObjectResponseSuccess::new_value((#method_call).into()))
                    }
                }
            }
            _ => {
                quote! {
                    #method_call;
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
        };

        quote! {
            #method_name_str => #wrapper_call,
        }
    });

    // Generate the clean impl block without export_baml_fn attributes
    let clean_methods: Vec<_> = exported_methods
        .iter()
        .map(|method| {
            let mut clean_method = (*method).clone();
            clean_method
                .attrs
                .retain(|attr| !attr.path().is_ident("export_baml_fn"));
            ImplItem::Fn(clean_method)
        })
        .chain(regular_methods.into_iter().cloned())
        .collect();

    let type_name = input.self_ty.clone();
    let clean_impl = ItemImpl {
        items: clean_methods,
        ..input
    };

    // Collect all method names for the error message
    let method_names: Vec<String> = exported_methods
        .iter()
        .map(|method| method.sig.ident.to_string())
        .collect();
    let available_methods = format!("[{}]", method_names.join(", "));

    let expanded = quote! {
        #clean_impl

        impl CallMethod for #type_name {
            fn call_method(
                &self,
                runtime: &baml_runtime::BamlRuntime,
                method_name: &str,
                _kwargs: &baml_types::BamlMap<String, crate::ffi::Value>,
            ) -> BamlObjectResponse {
                match method_name {
                    "~destructor" => {
                        self.clone().destroy();
                        Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                    }
                    #(#match_arms)*
                    _ => Err(format!(
                        "Failed to call function: \"{}\" on object type: {}. Available methods: {}",
                        method_name, stringify!(#type_name), #available_methods
                    )),
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

fn is_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();

            // Check for Arc<T> where T is convertible to RawPtrType
            if type_name == "Arc" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_convertible_to_rawptr(inner_type);
                    }
                }
                return false;
            }

            // Also check for fully qualified Arc type
            if type_path.path.segments.len() >= 2 {
                let segments: Vec<_> = type_path.path.segments.iter().collect();
                if segments.len() >= 2 {
                    let last_two = &segments[segments.len() - 2..];
                    if last_two[0].ident == "sync" && last_two[1].ident == "Arc" {
                        if let syn::PathArguments::AngleBracketed(args) = &last_two[1].arguments {
                            if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first()
                            {
                                return is_convertible_to_rawptr(inner_type);
                            }
                        }
                        return false;
                    }
                }
            }

            // Only types defined in define_raw_ptr_types! can convert to RawPtrType
            return matches!(
                type_name.as_str(),
                "Collector"
                    | "Usage"
                    | "FunctionLog"
                    | "Timing"
                    | "StreamTiming"
                    | "LLMCall"
                    | "LLMStreamCall"
                    | "HTTPRequest"
                    | "HTTPResponse"
                    | "HTTPBody"
                    | "SSEEvent"
                    | "BamlMedia"
                    | "TypeBuilder"
                    | "EnumBuilder"
                    | "EnumValueBuilder"
                    | "ClassBuilder"
                    | "ClassPropertyBuilder"
                    | "TypeIR"
            );
        }
    }
    false
}

fn is_vec_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_convertible_to_rawptr(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_option_vec_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_vec_convertible_to_rawptr(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_either_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Either" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if args.args.len() == 2 {
                        if let (
                            Some(syn::GenericArgument::Type(left_type)),
                            Some(syn::GenericArgument::Type(right_type)),
                        ) = (args.args.first(), args.args.get(1))
                        {
                            return is_convertible_to_rawptr(left_type)
                                && is_convertible_to_rawptr(right_type);
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_vec_either_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_either_convertible_to_rawptr(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_option_either_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_either_convertible_to_rawptr(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_option_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_convertible_to_rawptr(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_basic_baml_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            return matches!(
                type_name.as_str(),
                "i64" | "i32" | "f64" | "f32" | "bool" | "String"
            );
        }
    }

    // Also check for &str
    if let Type::Reference(type_ref) = ty {
        if let Type::Path(path) = &*type_ref.elem {
            if path
                .path
                .segments
                .last()
                .map(|seg| seg.ident == "str")
                .unwrap_or(false)
            {
                return true;
            }
        }
    }

    false
}

fn is_option_basic_baml_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_basic_baml_type(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_vec_basic_baml_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return is_basic_baml_type(inner_type);
                    }
                }
            }
        }
    }
    false
}

fn is_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "RawPtrType";
        }
    }
    false
}

fn is_vec_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(inner_type_path))) =
                        args.args.first()
                    {
                        if let Some(inner_segment) = inner_type_path.path.segments.last() {
                            return inner_segment.ident == "RawPtrType";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_option_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(inner_type_path))) =
                        args.args.first()
                    {
                        if let Some(inner_segment) = inner_type_path.path.segments.last() {
                            return inner_segment.ident == "RawPtrType";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_bamlvalue(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            return result_segment.ident == "BamlValue";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_unit(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Tuple(tuple_type))) =
                        args.args.first()
                    {
                        return tuple_type.elems.is_empty(); // () is an empty tuple
                    }
                }
            }
        }
    }
    false
}

fn is_result_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            return result_segment.ident == "RawPtrType";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_vec_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Vec" {
                                if let syn::PathArguments::AngleBracketed(vec_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(Type::Path(
                                        vec_inner_type,
                                    ))) = vec_args.args.first()
                                    {
                                        if let Some(vec_inner_segment) =
                                            vec_inner_type.path.segments.last()
                                        {
                                            return vec_inner_segment.ident == "RawPtrType";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_vec_convertible_to_rawptr(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Vec" {
                                if let syn::PathArguments::AngleBracketed(vec_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(vec_inner_type)) =
                                        vec_args.args.first()
                                    {
                                        return is_convertible_to_rawptr(vec_inner_type);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_option_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Option" {
                                if let syn::PathArguments::AngleBracketed(option_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(Type::Path(
                                        option_inner_type,
                                    ))) = option_args.args.first()
                                    {
                                        if let Some(option_inner_segment) =
                                            option_inner_type.path.segments.last()
                                        {
                                            return option_inner_segment.ident == "RawPtrType";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_option_bamlvalue(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Option" {
                                if let syn::PathArguments::AngleBracketed(option_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(Type::Path(
                                        option_inner_type,
                                    ))) = option_args.args.first()
                                    {
                                        if let Some(option_inner_segment) =
                                            option_inner_type.path.segments.last()
                                        {
                                            return option_inner_segment.ident == "BamlValue";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_option_vec_rawptrtype(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Option" {
                                if let syn::PathArguments::AngleBracketed(option_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(Type::Path(
                                        option_inner_type,
                                    ))) = option_args.args.first()
                                    {
                                        if let Some(option_inner_segment) =
                                            option_inner_type.path.segments.last()
                                        {
                                            if option_inner_segment.ident == "Vec" {
                                                if let syn::PathArguments::AngleBracketed(
                                                    vec_args,
                                                ) = &option_inner_segment.arguments
                                                {
                                                    if let Some(syn::GenericArgument::Type(
                                                        Type::Path(vec_inner_type),
                                                    )) = vec_args.args.first()
                                                    {
                                                        if let Some(vec_inner_segment) =
                                                            vec_inner_type.path.segments.last()
                                                        {
                                                            return vec_inner_segment.ident
                                                                == "RawPtrType";
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_option_string(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            if result_segment.ident == "Option" {
                                if let syn::PathArguments::AngleBracketed(option_args) =
                                    &result_segment.arguments
                                {
                                    if let Some(syn::GenericArgument::Type(Type::Path(
                                        option_inner_type,
                                    ))) = option_args.args.first()
                                    {
                                        if let Some(option_inner_segment) =
                                            option_inner_type.path.segments.last()
                                        {
                                            return option_inner_segment.ident == "String";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_result_bool(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(result_type))) =
                        args.args.first()
                    {
                        if let Some(result_segment) = result_type.path.segments.last() {
                            return result_segment.ident == "bool";
                        }
                    }
                }
            }
        }
    }
    false
}

fn is_optional_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn is_runtime_type(ty: &Type) -> bool {
    if let Type::Reference(type_ref) = ty {
        if let Type::Path(type_path) = &*type_ref.elem {
            if let Some(segment) = type_path.path.segments.last() {
                return segment.ident == "BamlRuntime";
            }
        }
    }
    false
}

fn generate_param_extraction(
    param_name: &syn::Ident,
    param_name_str: &str,
    ty: &Type,
) -> proc_macro2::TokenStream {
    // Check if it's an Option type
    if is_optional_type(ty) {
        // Extract the inner type from Option<T>
        if let Type::Path(type_path) = ty {
            if let Some(segment) = type_path.path.segments.last() {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        return generate_optional_param_extraction(
                            param_name,
                            param_name_str,
                            inner_type,
                        );
                    }
                }
            }
        }
        // Fallback for Option types
        return quote! {
            let #param_name: Option<&str> = _kwargs
                .get(#param_name_str)
                .and_then(|v| match v {
                    crate::ffi::Value::String(s, _) => Some(s.as_str()),
                    _ => None,
                });
        };
    }

    // Required parameter extraction with type validation
    if let Type::Reference(type_ref) = ty {
        if let Type::Path(path) = &*type_ref.elem {
            if path
                .path
                .segments
                .last()
                .map(|seg| seg.ident == "str")
                .unwrap_or(false)
            {
                return quote! {
                    let #param_name = _kwargs
                        .get(#param_name_str)
                        .and_then(|v| match v {
                            crate::ffi::Value::String(s, _) => Some(s.as_str()),
                            _ => None,
                        })
                        .ok_or_else(|| format!("Parameter '{}' is required but missing", #param_name_str))?;
                };
            }

            // Handle &TypeWrapper parameters by extracting from RawPtr variant
            let type_name = path
                .path
                .segments
                .last()
                .map(|seg| seg.ident.to_string())
                .unwrap_or_default();
            if type_name.ends_with("Wrapper") {
                return quote! {
                    let #param_name = _kwargs
                        .get(#param_name_str)
                        .and_then(|v| match v {
                            crate::ffi::Value::RawPtr(raw_ptr_type, _) => {
                                match raw_ptr_type {
                                    crate::raw_ptr_wrapper::RawPtrType::TypeDef(wrapper) => Some(wrapper),
                                    _ => None,
                                }
                            }
                            _ => None,
                        })
                        .ok_or_else(|| format!("Parameter '{}' must be a {} object", #param_name_str, #type_name))?;
                };
            }

            // Handle &Vec<TypeWrapper> parameters by extracting from List variant
            if let Type::Path(vec_path) = &*type_ref.elem {
                if let Some(vec_segment) = vec_path.path.segments.last() {
                    if vec_segment.ident == "Vec" {
                        if let syn::PathArguments::AngleBracketed(args) = &vec_segment.arguments {
                            if let Some(syn::GenericArgument::Type(Type::Path(inner_path))) =
                                args.args.first()
                            {
                                if let Some(inner_segment) = inner_path.path.segments.last() {
                                    let inner_type_name = inner_segment.ident.to_string();
                                    if inner_type_name.ends_with("Wrapper") {
                                        return quote! {
                                            let #param_name = &{
                                                _kwargs
                                                    .get(#param_name_str)
                                                    .and_then(|v| match v {
                                                        crate::ffi::Value::List(list, _) => {
                                                            let extracted_wrappers: Result<Vec<_>, String> = list.iter()
                                                                .map(|item| match item {
                                                                    crate::ffi::Value::RawPtr(raw_ptr_type, _) => {
                                                                        match raw_ptr_type {
                                                                            crate::raw_ptr_wrapper::RawPtrType::TypeDef(wrapper) => Ok(wrapper.clone()),
                                                                            _ => Err(format!("List item is not a {} object", #inner_type_name)),
                                                                        }
                                                                    }
                                                                    _ => Err(format!("List item is not a {} object", #inner_type_name)),
                                                                })
                                                                .collect();
                                                            extracted_wrappers.ok()
                                                        }
                                                        _ => None,
                                                    })
                                                    .ok_or_else(|| format!("Parameter '{}' must be a list of {} objects", #param_name_str, #inner_type_name))?
                                            };
                                        };
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Handle other types (i64, bool, etc.)
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            match segment.ident.to_string().as_str() {
                "i64" => {
                    return quote! {
                        let #param_name = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::Int(i, _) => Some(*i),
                                _ => None,
                            })
                            .ok_or_else(|| format!("Parameter '{}' must be an integer", #param_name_str))?;
                    };
                }
                "bool" => {
                    return quote! {
                        let #param_name = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::Bool(b, _) => Some(*b),
                                _ => None,
                            })
                            .ok_or_else(|| format!("Parameter '{}' must be a boolean", #param_name_str))?;
                    };
                }
                "String" => {
                    return quote! {
                        let #param_name = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::String(s, _) => Some(s.clone()),
                                _ => None,
                            })
                            .ok_or_else(|| format!("Parameter '{}' must be a string", #param_name_str))?;
                    };
                }
                _ => {}
            }
        }
    }

    // Default fallback
    quote! {
        let #param_name = _kwargs
            .get(#param_name_str)
            .ok_or_else(|| format!("Parameter '{}' is required but missing", #param_name_str))?;
    }
}

fn generate_optional_param_extraction(
    param_name: &syn::Ident,
    param_name_str: &str,
    inner_type: &Type,
) -> proc_macro2::TokenStream {
    if let Type::Reference(type_ref) = inner_type {
        if let Type::Path(path) = &*type_ref.elem {
            if path
                .path
                .segments
                .last()
                .map(|seg| seg.ident == "str")
                .unwrap_or(false)
            {
                return quote! {
                    let #param_name: Option<&str> = _kwargs
                        .get(#param_name_str)
                        .and_then(|v| match v {
                            crate::ffi::Value::String(s, _) => Some(s.as_str()),
                            _ => None,
                        });
                };
            }
        }
    }

    if let Type::Path(type_path) = inner_type {
        if let Some(segment) = type_path.path.segments.last() {
            match segment.ident.to_string().as_str() {
                "i64" => {
                    return quote! {
                        let #param_name: Option<i64> = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::Int(i, _) => Some(*i),
                                _ => None,
                            });
                    };
                }
                "bool" => {
                    return quote! {
                        let #param_name: Option<bool> = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::Bool(b, _) => Some(*b),
                                _ => None,
                            });
                    };
                }
                "String" => {
                    return quote! {
                        let #param_name: Option<String> = _kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::String(s, _) => Some(s.clone()),
                                _ => None,
                            });
                    };
                }
                _ => {}
            }
        }
    }

    // Default fallback for optional
    quote! {
        let #param_name = _kwargs.get(#param_name_str);
    }
}

#[proc_macro]
pub fn define_raw_ptr_types(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RawPtrTypeDefinitions);

    let variants: Vec<_> = input
        .types
        .iter()
        .map(|type_def| {
            let variant_name = &type_def.variant_name;
            let wrapper_type = &type_def.wrapper_type;
            quote! {
                #variant_name(#wrapper_type)
            }
        })
        .collect();

    let from_impls: Vec<_> = input
        .types
        .iter()
        .map(|type_def| {
            let inner_type = &type_def.inner_type;
            let variant_name = &type_def.variant_name;
            let wrapper_type = &type_def.wrapper_type;

            quote! {
                impl From<#inner_type> for RawPtrType {
                    fn from(value: #inner_type) -> Self {
                        RawPtrType::#variant_name(#wrapper_type::from_object(value))
                    }
                }

                impl From<Arc<#inner_type>> for RawPtrType {
                    fn from(value: Arc<#inner_type>) -> Self {
                        RawPtrType::#variant_name(#wrapper_type::from_arc(value))
                    }
                }

                impl From<#wrapper_type> for RawPtrType {
                    fn from(value: #wrapper_type) -> Self {
                        RawPtrType::#variant_name(value)
                    }
                }
            }
        })
        .collect();

    let name_arms: Vec<_> = input
        .types
        .iter()
        .map(|type_def| {
            let variant_name = &type_def.variant_name;
            let display_name = &type_def.display_name;

            // Special handling for Media type
            if variant_name == "Media" {
                quote! {
                    RawPtrType::#variant_name(m) => match m.media_type {
                        baml_types::BamlMediaType::Image => "Image",
                        baml_types::BamlMediaType::Audio => "Audio",
                        baml_types::BamlMediaType::Pdf => "PDF",
                        baml_types::BamlMediaType::Video => "Video",
                    }
                }
            } else {
                quote! {
                    RawPtrType::#variant_name(_) => #display_name
                }
            }
        })
        .collect();

    let call_method_arms: Vec<_> = input.types.iter().map(|type_def| {
        let variant_name = &type_def.variant_name;
        let field_name = variant_name.to_string().to_lowercase();
        let field_ident = syn::Ident::new(&field_name, variant_name.span());
        quote! {
            RawPtrType::#variant_name(#field_ident) => #field_ident.call_method(runtime, method_name, kwargs)
        }
    }).collect();

    // Generate type aliases
    let type_aliases: Vec<_> = input
        .types
        .iter()
        .map(|type_def| {
            let inner_type = &type_def.inner_type;
            let wrapper_type = &type_def.wrapper_type;
            quote! {
                pub type #wrapper_type = RawPtrWrapper<#inner_type>;
            }
        })
        .collect();

    let expanded = quote! {
        #(#type_aliases)*

        #[derive(Debug, Clone)]
        pub enum RawPtrType {
            #(#variants,)*
        }

        #(#from_impls)*

        impl RawPtrType {
            pub fn name(&self) -> &str {
                match self {
                    #(#name_arms,)*
                }
            }
        }

        impl CallMethod for RawPtrType {
            fn call_method(
                &self,
                runtime: &baml_runtime::BamlRuntime,
                method_name: &str,
                kwargs: &baml_types::BamlMap<String, crate::ffi::Value>,
            ) -> BamlObjectResponse {
                match self {
                    #(#call_method_arms,)*
                }
            }
        }

    };

    TokenStream::from(expanded)
}

#[proc_macro]
pub fn generate_encode_decode_impls(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RawPtrTypeDefinitions);

    // Generate decode match arms
    let decode_arms: Vec<_> = input
        .types
        .iter()
        .flat_map(|type_def| {
            let variant_name = &type_def.variant_name;
            let wrapper_type = &type_def.wrapper_type;

            type_def.object_variants.iter().map(move |obj_variant| {
                quote! {
                    Some(Object::#obj_variant(pointer)) => {
                        Ok(RawPtrType::#variant_name(#wrapper_type::decode(pointer)?))
                    }
                }
            })
        })
        .collect();

    // Generate encode match arms
    let encode_arms: Vec<_> = input
        .types
        .iter()
        .map(|type_def| {
            let variant_name = &type_def.variant_name;
            let field_name = variant_name.to_string().to_lowercase();
            let field_ident = syn::Ident::new(&field_name, variant_name.span());

            quote! {
                RawPtrType::#variant_name(#field_ident) => #field_ident.encode()
            }
        })
        .collect();

    // Generate wrapper implementations directly (skip Media type as it has special handling)
    let wrapper_impls: Vec<_> = input
        .types
        .iter()
        .filter_map(|type_def| {
            // Skip Media type as it has special handling
            if type_def.variant_name == "Media" {
                return None;
            }

            if let Some(obj_variant) = type_def.object_variants.first() {
                let wrapper_type = &type_def.wrapper_type;
                Some(quote! {
                    impl Decode for #wrapper_type {
                        type From = BamlPointerType;
                        fn decode(from: Self::From) -> Result<Self, anyhow::Error>
                        where
                            Self: Sized,
                        {
                            Ok(#wrapper_type::from_raw(
                                from.pointer as *const libc::c_void,
                                true,
                            ))
                        }
                    }
                    impl ObjectType for #wrapper_type {
                        fn object_type(&self) -> Object {
                            Object::#obj_variant(self.pointer())
                        }
                    }
                })
            } else {
                None
            }
        })
        .collect();

    let expanded = quote! {
        impl Decode for RawPtrType {
            type From = BamlObjectHandle;

            fn decode(from: Self::From) -> Result<Self, anyhow::Error>
            where
                Self: Sized,
            {
                match from.object {
                    #(#decode_arms)*
                    None => Err(anyhow::anyhow!("Invalid object type")),
                }
            }
        }

        impl Encode<BamlObjectHandle> for RawPtrType {
            fn encode(self) -> BamlObjectHandle {
                match self {
                    #(#encode_arms,)*
                }
            }
        }

        #(#wrapper_impls)*
    };

    TokenStream::from(expanded)
}

struct RawPtrTypeDefinition {
    inner_type: Ident,
    variant_name: Ident,
    wrapper_type: Ident,
    display_name: String,
    object_variants: Vec<Ident>,
}

struct RawPtrTypeDefinitions {
    types: Vec<RawPtrTypeDefinition>,
}

impl syn::parse::Parse for RawPtrTypeDefinitions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut types = Vec::new();

        while !input.is_empty() {
            let inner_type: Ident = input.parse()?;

            // Check if this is the simple form (just type name) or complex form (with =>)
            if input.peek(Token![=>]) {
                // Complex form: full syntax for special cases
                input.parse::<Token![=>]>()?;
                let variant_name: Ident = input.parse()?;
                input.parse::<Token![as]>()?;
                let wrapper_type: Ident = input.parse()?;
                input.parse::<Token![:]>()?;
                let display_name: syn::LitStr = input.parse()?;

                // Parse optional object variants in parentheses
                let mut object_variants = Vec::new();
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);

                    while !content.is_empty() {
                        // Parse Object::Variant format
                        content.parse::<syn::Ident>()?; // "Object"
                        content.parse::<Token![::]>()?;
                        let variant: Ident = content.parse()?;
                        object_variants.push(variant);

                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }

                types.push(RawPtrTypeDefinition {
                    inner_type,
                    variant_name,
                    wrapper_type,
                    display_name: display_name.value(),
                    object_variants,
                });
            } else {
                // Simple form: just the type name
                let type_name = inner_type.to_string();

                // Generate conventional names
                let variant_name = inner_type.clone();
                let wrapper_type = Ident::new(&format!("{type_name}Wrapper"), inner_type.span());
                let display_name = type_name.clone();

                // Generate conventional object variant
                let object_variant = Ident::new(&type_name, inner_type.span());
                let object_variants = vec![object_variant];

                types.push(RawPtrTypeDefinition {
                    inner_type,
                    variant_name,
                    wrapper_type,
                    display_name,
                    object_variants,
                });
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(RawPtrTypeDefinitions { types })
    }
}

#[proc_macro_attribute]
pub fn export_baml_new_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    // Extract methods marked with #[export_baml_new_fn(CffiObjectType)]
    let mut constructor_methods = Vec::new();
    let mut regular_methods = Vec::new();

    for item in &input.items {
        if let ImplItem::Fn(method) = item {
            let mut cffi_object_type = None;
            let mut has_export_attr = false;

            for attr in &method.attrs {
                if attr.path().is_ident("export_baml_new_fn") {
                    has_export_attr = true;
                    // Parse the CffiObjectType from the attribute
                    if let Ok(tokens) = attr.parse_args::<syn::Ident>() {
                        cffi_object_type = Some(tokens);
                    }
                }
            }

            if has_export_attr {
                constructor_methods.push((method, cffi_object_type));
            } else {
                regular_methods.push(item);
            }
        } else {
            regular_methods.push(item);
        }
    }

    // Generate the new_from method
    let match_arms: Vec<_> = constructor_methods.iter().map(|(method, cffi_type)| {
        let method_name = &method.sig.ident;
        let cffi_variant = cffi_type.as_ref().expect("CffiObjectType must be provided");

        // Extract method parameters (excluding &self if present)
        let params: Vec<_> = method.sig.inputs.iter().skip(
            if matches!(method.sig.inputs.first(), Some(syn::FnArg::Receiver(_))) { 1 } else { 0 }
        ).collect();

        // Generate parameter extraction code
        let param_extractions: Vec<_> = params.iter().filter_map(|param| {
            if let syn::FnArg::Typed(pat_type) = param {
                if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                    let param_name = &pat_ident.ident;
                    let param_name_str = param_name.to_string();
                    return Some(generate_constructor_param_extraction(param_name, &param_name_str, &pat_type.ty));
                }
            }
            None
        }).collect();

        let param_names: Vec<_> = params.iter().filter_map(|param| {
            if let syn::FnArg::Typed(pat_type) = param {
                if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                    Some(&pat_ident.ident)
                } else {
                    None
                }
            } else {
                None
            }
        }).collect();
        let method_call = if param_names.is_empty() {
            quote! { Self::#method_name() }
        } else {
            quote! { Self::#method_name(#(#param_names),*) }
        };

        quote! {
            cffi::BamlObjectType::#cffi_variant => {
                #(#param_extractions)*
                match #method_call {
                    Ok(wrapper) => Ok(BamlObjectResponseSuccess::new_object(RawPtrType::from(wrapper))),
                    Err(e) => Err(e),
                }
            }
        }
    }).collect();

    // Generate the clean impl block without export_baml_new_fn attributes
    let clean_methods: Vec<_> = constructor_methods
        .iter()
        .map(|(method, _)| {
            let mut clean_method = (*method).clone();
            clean_method
                .attrs
                .retain(|attr| !attr.path().is_ident("export_baml_new_fn"));
            ImplItem::Fn(clean_method)
        })
        .chain(regular_methods.into_iter().cloned())
        .collect();

    let type_name = input.self_ty.clone();
    let clean_impl = ItemImpl {
        items: clean_methods,
        ..input
    };

    let expanded = quote! {
        #clean_impl

        impl #type_name {
            pub fn new_from(
                object: cffi::BamlObjectType,
                kwargs: &baml_types::BamlMap<String, crate::ffi::Value>,
            ) -> BamlObjectResponse {
                match object {
                    #(#match_arms)*
                    _ => Err(format!(
                        "Cannot create object of type {}",
                        object.as_str_name()
                    )),
                }
            }
        }
    };

    TokenStream::from(expanded)
}

fn generate_constructor_param_extraction(
    param_name: &syn::Ident,
    param_name_str: &str,
    ty: &Type,
) -> proc_macro2::TokenStream {
    // Check if it's an Option type
    if is_optional_type(ty) {
        // For optional parameters, just try to extract without requiring presence
        quote! {
            let #param_name = kwargs
                .get(#param_name_str)
                .and_then(|v| match v {
                    crate::ffi::Value::String(s, _) => Some(s.as_str()),
                    _ => None,
                });
        }
    } else {
        // For required parameters, require presence and type validation
        if let Type::Reference(type_ref) = ty {
            if let Type::Path(path) = &*type_ref.elem {
                if path
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident == "str")
                    .unwrap_or(false)
                {
                    return quote! {
                        let #param_name = kwargs
                            .get(#param_name_str)
                            .and_then(|v| match v {
                                crate::ffi::Value::String(s, _) => Some(s.as_str()),
                                _ => None,
                            })
                            .ok_or_else(|| format!("Parameter '{}' is required", #param_name_str))?;
                    };
                }
            }
        }

        // Default fallback for required parameters
        quote! {
            let #param_name = kwargs
                .get(#param_name_str)
                .ok_or_else(|| format!("Parameter '{}' is required", #param_name_str))?;
        }
    }
}
