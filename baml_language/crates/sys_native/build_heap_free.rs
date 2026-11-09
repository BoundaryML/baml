//! Keep one provider body for interpreter glue and the typed native runtime.
use quote::{ToTokens, format_ident, quote};

pub(super) fn generate(source: &str) -> String {
    let mut file = syn::parse_file(source).expect("native provider source");
    // include! expands inside the provider module; inner module documentation
    // belongs to its source rather than the generated token stream.
    file.attrs.retain(|a| !a.path().is_ident("doc"));
    let mut inherent = Vec::new();
    for item in &mut file.items {
        let syn::Item::Impl(implementation) = item else {
            continue;
        };
        let Some((_, path, _)) = &implementation.trait_ else {
            continue;
        };
        let Some(trait_name) = path.segments.last().map(|s| &s.ident) else {
            continue;
        };
        if !trait_name.to_string().starts_with("Io") {
            continue;
        }
        let mut methods = Vec::new();
        for method in &mut implementation.items {
            let syn::ImplItem::Fn(method) = method else {
                continue;
            };
            for (i, input) in method.sig.inputs.iter_mut().enumerate() {
                if let syn::FnArg::Typed(arg) = input
                    && matches!(arg.pat.as_ref(), syn::Pat::Wild(_))
                {
                    let name = format_ident!("__io_ignored_{i}");
                    arg.pat = Box::new(syn::parse_quote!(#name));
                }
            }
            let mut typed = method.clone();
            typed.sig.ident = format_ident!(
                "rt_{}_{}",
                trait_name.to_string().to_lowercase(),
                method.sig.ident.to_string().trim_start_matches('_')
            );
            typed.vis = syn::parse_quote!(pub);
            let mut arguments = Vec::new();
            typed.sig.inputs = typed
                .sig
                .inputs
                .into_iter()
                .filter(|arg| {
                    let syn::FnArg::Typed(arg) = arg else {
                        return true;
                    };
                    let syn::Pat::Ident(name) = arg.pat.as_ref() else {
                        panic!("provider argument pattern")
                    };
                    if arg
                        .ty
                        .to_token_stream()
                        .to_string()
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|word| word == "BexHeap")
                    {
                        assert!(
                            !typed
                                .block
                                .to_token_stream()
                                .to_string()
                                .split(|c: char| !c.is_alphanumeric() && c != '_')
                                .any(|word| word == name.ident.to_string()),
                            "provider body reads the interpreter heap"
                        );
                        false
                    } else {
                        arguments.push(name.ident.clone());
                        true
                    }
                })
                .collect();
            let name = &typed.sig.ident;
            method.block = syn::parse_quote!({self.#name(#(#arguments),*)});
            methods.push(typed);
        }
        let attributes = &implementation.attrs;
        let receiver = &implementation.self_ty;
        inherent.push(quote! {#(#attributes)* impl #receiver {#(#methods)*}});
    }
    let output = quote! {#file #(#inherent)*};
    let output = syn::parse2(output).expect("heap-free provider wrappers");
    prettyplease::unparse(&output)
}
