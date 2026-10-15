//! The generated code names a stdlib declaration by identity: a
//! `baml_base::LangItem` written with `lang_item!`, never a dotted string the
//! runtime would have to resolve through some package's name.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// The `lang_item!(Package: ns.Name)` invocation for the class `name` under
/// the dotted `namespace_prefix` (`baml.errors`, `reflect.class`, `ai`): the
/// prefix's first segment is the language package's manifest name, the rest
/// its namespace path. Shared by the token-stream and string emitters so the
/// two cannot spell an item differently.
fn parts(namespace_prefix: &str, name: &str) -> (String, Vec<String>) {
    let mut segments = namespace_prefix.split('.');
    let manifest_name = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| panic!("class `{name}` has no package in its namespace prefix"));
    let package = baml_base::LangPackage::from_manifest_name(manifest_name).unwrap_or_else(|| {
        panic!("class `{name}` is declared in `{manifest_name}`, which is no language package")
    });
    let mut path: Vec<String> = segments.map(str::to_owned).collect();
    path.push(name.to_owned());
    // `LangPackage`'s variants are unit variants, so `Debug` prints the
    // variant's identifier — the token `lang_item!` takes.
    (format!("{package:?}"), path)
}

/// [`parts`] as tokens: `::baml_base::lang_item!(Baml: errors.Timeout)`.
pub(crate) fn lang_item_tokens(namespace_prefix: &str, name: &str) -> TokenStream {
    let (package, path) = parts(namespace_prefix, name);
    let package = format_ident!("{package}");
    let path = path.iter().map(|segment| format_ident!("{segment}"));
    quote! { ::baml_base::lang_item!(#package: #(#path).*) }
}

/// [`parts`] as source text, for the string-built emitters.
pub(crate) fn lang_item_expr(namespace_prefix: &str, name: &str) -> String {
    let (package, path) = parts(namespace_prefix, name);
    format!("::baml_base::lang_item!({package}: {})", path.join("."))
}

#[cfg(test)]
mod tests {
    use super::{lang_item_expr, lang_item_tokens};

    #[test]
    fn a_class_is_spelled_as_its_package_and_path() {
        assert_eq!(
            lang_item_expr("baml.errors", "Timeout"),
            "::baml_base::lang_item!(Baml: errors.Timeout)"
        );
        assert_eq!(
            lang_item_expr("reflect", "Arg"),
            "::baml_base::lang_item!(Reflect: Arg)"
        );
        assert_eq!(
            lang_item_expr("claude_code", "Client"),
            "::baml_base::lang_item!(ClaudeCode: Client)"
        );
        assert_eq!(
            lang_item_tokens("reflect.enum", "Type").to_string(),
            ":: baml_base :: lang_item ! (Reflect : enum . Type)"
        );
    }
}
