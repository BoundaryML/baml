//! `baml_codegen_types::Ty` → Rust type expression.
//!
//! Emitted types are fully qualified (`::std::string::String`, never bare
//! `String`) so user-declared BAML symbols can never shadow them under
//! `PreserveCase`.
//!
//! Types the Rust SDK cannot represent yet return [`Unsupported`]; the
//! caller skips the enclosing symbol and reports it — never erasing to a
//! catch-all type.

use baml_codegen_types::Ty;
use proc_macro2::TokenStream;
use quote::quote;

/// A type the generator cannot translate yet. The reason names the
/// missing capability for the skip warning.
pub(crate) struct Unsupported {
    pub(crate) reason: String,
}

fn unsupported(what: &str) -> Unsupported {
    Unsupported {
        reason: format!("unsupported type: {what}"),
    }
}

/// Translate a resolved BAML type to its Rust type expression.
pub(crate) fn translate(ty: &Ty) -> Result<TokenStream, Unsupported> {
    match ty {
        Ty::Int => Ok(quote! { ::core::primitive::i64 }),
        Ty::Bigint => Ok(quote! { ::baml_rs::BigInt }),
        Ty::Float => Ok(quote! { ::core::primitive::f64 }),
        Ty::String => Ok(quote! { ::std::string::String }),
        Ty::Bool => Ok(quote! { ::core::primitive::bool }),
        // BAML `null` (as a type) and `void` both surface as unit: null
        // rides the wire as an absent value, a void function returns null.
        Ty::Null | Ty::Unit => Ok(quote! { () }),
        // Rust cannot refine value-level literals in types; a literal type
        // widens to its base primitive (the same widening TS applies going
        // from `Literal[42]`-style types to `number`).
        Ty::Literal(lit) => Ok(match lit {
            baml_base::Literal::Int(_) => quote! { ::core::primitive::i64 },
            baml_base::Literal::Bigint(_) => quote! { ::baml_rs::BigInt },
            baml_base::Literal::Float(_) => quote! { ::core::primitive::f64 },
            baml_base::Literal::String(_) => quote! { ::std::string::String },
            baml_base::Literal::Bool(_) => quote! { ::core::primitive::bool },
        }),
        Ty::Uint8Array => Ok(quote! { ::std::vec::Vec<::core::primitive::u8> }),
        Ty::List(inner) => {
            let inner = translate(inner)?;
            Ok(quote! { ::std::vec::Vec<#inner> })
        }
        Ty::Union(items) => {
            // A two-arm union with a `null` member is BAML optionality
            // (`T?`); anything else needs real union codegen.
            if let [a, b] = items.as_slice() {
                let inner = match (a, b) {
                    (Ty::Null, other) | (other, Ty::Null) => Some(other),
                    _ => None,
                };
                if let Some(inner) = inner {
                    let inner = translate(inner)?;
                    return Ok(quote! { ::std::option::Option<#inner> });
                }
            }
            Err(unsupported("union"))
        }
        Ty::Media(kind) => Err(unsupported(&format!("media ({kind})"))),
        Ty::Class(_, _) => Err(unsupported("class")),
        Ty::Enum(_) => Err(unsupported("enum")),
        Ty::TypeAlias(_) => Err(unsupported("type alias")),
        Ty::TypeVar(_) => Err(unsupported("type variable (generics)")),
        Ty::Map { .. } => Err(unsupported("map")),
        Ty::BuiltinUnknown => Err(unsupported("unknown")),
        Ty::Callable { .. } => Err(unsupported("callable")),
        Ty::BamlOptions => Err(unsupported("baml.Options")),
        Ty::RustType => Err(unsupported("$rust_type handle")),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn rendered(ty: &Ty) -> String {
        translate(ty)
            .map(|t| t.to_string())
            .unwrap_or_else(|u| panic!("expected {ty} to translate, got: {}", u.reason))
    }

    #[test]
    fn primitives() {
        assert_eq!(rendered(&Ty::Int), ":: core :: primitive :: i64");
        assert_eq!(rendered(&Ty::String), ":: std :: string :: String");
        assert_eq!(rendered(&Ty::Unit), "()");
        assert_eq!(rendered(&Ty::Null), "()");
        assert_eq!(rendered(&Ty::Bigint), ":: baml_rs :: BigInt");
        assert_eq!(
            rendered(&Ty::Uint8Array),
            ":: std :: vec :: Vec < :: core :: primitive :: u8 >"
        );
    }

    #[test]
    fn literals_widen_to_their_base_primitive() {
        assert_eq!(
            rendered(&Ty::Literal(baml_base::Literal::String(
                "hello world".into()
            ))),
            ":: std :: string :: String"
        );
        assert_eq!(
            rendered(&Ty::Literal(baml_base::Literal::Int(42))),
            ":: core :: primitive :: i64"
        );
    }

    #[test]
    fn null_union_is_option_in_either_arm_order() {
        let expected = ":: std :: option :: Option < :: core :: primitive :: i64 >";
        assert_eq!(rendered(&Ty::Union(vec![Ty::Int, Ty::Null])), expected);
        assert_eq!(rendered(&Ty::Union(vec![Ty::Null, Ty::Int])), expected);
    }

    #[test]
    fn lists_nest() {
        assert_eq!(
            rendered(&Ty::List(Box::new(Ty::Union(vec![Ty::Int, Ty::Null])))),
            ":: std :: vec :: Vec < :: std :: option :: Option < :: core :: primitive :: i64 > >"
        );
    }

    #[test]
    fn non_null_unions_are_unsupported() {
        assert!(translate(&Ty::Union(vec![Ty::Int, Ty::String])).is_err());
        assert!(translate(&Ty::Union(vec![Ty::Int, Ty::String, Ty::Null])).is_err());
    }
}
