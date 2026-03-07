mod codegen_accessors;
mod codegen_builtins;
mod codegen_native;
mod codegen_sys_ops;
mod collect;
mod parse;
mod util;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse_str;

use crate::{collect::CollectedBuiltins, parse::BuiltinsInput};

pub fn generate_module(input: &str) -> syn::Result<String> {
    let input = parse_str::<BuiltinsInput>(input)?;
    let collected = CollectedBuiltins::from_modules(&input.modules);

    let builtins = codegen_builtins::generate(&collected);
    let native = wrap_macro(
        "generate_native_trait",
        codegen_native::generate(&collected),
    );
    let sys_ops = wrap_macro(
        "generate_sys_op_traits",
        codegen_sys_ops::generate(&collected),
    );
    let accessors = wrap_macro(
        "generate_builtin_accessors",
        codegen_accessors::generate(&collected),
    );

    Ok(quote! {
        #builtins
        #native
        #sys_ops
        #accessors
    }
    .to_string())
}

fn wrap_macro(name: &str, body: TokenStream2) -> TokenStream2 {
    let name = syn::Ident::new(name, proc_macro2::Span::call_site());
    quote! {
        #[macro_export]
        macro_rules! #name {
            () => {
                #body
            };
        }
    }
}

pub fn validate_compiler2_stdlib() -> Result<(), String> {
    for builtin in baml_builtins2::ALL {
        // The `env` package file is still a compatibility alias layer using syntax
        // the compiler2 parser does not fully accept yet. Keep validating the
        // compiler2-owned `baml` package sources here.
        if builtin.package != "baml" {
            continue;
        }

        let tokens = baml_compiler_lexer::lex_lossless(builtin.contents, baml_base::FileId::new(0));
        let (green, errors) = baml_compiler_parser::parse_file(&tokens);
        if !errors.is_empty() {
            return Err(format!(
                "failed to parse compiler2 builtin {}: {errors:#?}",
                builtin.virtual_path()
            ));
        }

        let root = baml_compiler_syntax::SyntaxNode::new_root(green);
        let (_items, diags) = baml_compiler2_ast::lower_file(&root);
        if !diags.is_empty() {
            return Err(format!(
                "failed to lower compiler2 builtin {}: {diags:#?}",
                builtin.virtual_path()
            ));
        }
    }

    Ok(())
}
