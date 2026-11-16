//! `#[derive(TokenTable)]` on the token enum.
//!
//! The enum's variants carry Logos's own `#[token("…")]`/`#[regex("…")]`
//! attributes (which `#[derive(Logos)]` consumes) plus two of ours:
//!
//! - `#[group(trivia | keyword | contextual | punct | literal | invalid)]`,
//!   required — which classification predicate the kind belongs to
//!   (`invalid`: what could not be lexed as intended; exists only to be
//!   reported);
//! - `#[describe = "…"]` — the user-facing description, required for a kind
//!   without a fixed spelling (trivia, literals) and forbidden where there is
//!   one (a keyword, contextual keyword or punctuation variant has exactly one
//!   `#[token("…")]` and is described by it);
//! - `#[spelling = "…"]` — the fixed spelling of a keyword/punctuation kind
//!   that has no Logos pattern because the lexer's driver produces it (`${`
//!   exists only inside template text). Exclusive with `#[token]`/`#[regex]`.
//!
//! From one definition it emits `ALL`, the six `is_*` predicates,
//! `describe()`, a marker type per kind in `trivia::`/`kw::`/`ctx::`/
//! `punct::`/`lit::`/`invalid::` (for `Token<K>`), and the `Token!`/`TokenKind!` macros
//! that resolve a spelling to its type or kind.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit, Meta, Token, punctuated::Punctuated,
    spanned::Spanned as _,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Trivia,
    /// Comment-shaped tokens that are syntax: `///` doc lines and `//baml:`
    /// directives, which a declaration owns.
    Doc,
    Keyword,
    Contextual,
    Punct,
    Literal,
    Invalid,
}

impl Group {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        Ok(match ident.to_string().as_str() {
            "trivia" => Self::Trivia,
            "doc" => Self::Doc,
            "keyword" => Self::Keyword,
            "contextual" => Self::Contextual,
            "punct" => Self::Punct,
            "literal" => Self::Literal,
            "invalid" => Self::Invalid,
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!(
                        "unknown token group `{other}`; expected trivia, doc, keyword, contextual, punct, literal or invalid"
                    ),
                ));
            }
        })
    }

    /// Kinds in these groups are spelled one fixed way.
    fn is_fixed(self) -> bool {
        matches!(self, Self::Keyword | Self::Contextual | Self::Punct)
    }

    fn module(self) -> Ident {
        format_ident!(
            "{}",
            match self {
                Self::Trivia => "trivia",
                Self::Doc => "doc",
                Self::Keyword => "kw",
                Self::Contextual => "ctx",
                Self::Punct => "punct",
                Self::Literal => "lit",
                Self::Invalid => "invalid",
            }
        )
    }
}

struct Variant {
    ident: Ident,
    group: Group,
    describe: String,
    /// The fixed spelling, for `Token!["…"]`.
    spelling: Option<String>,
}

/// The string literal a `#[token("…", …)]` attribute starts with.
fn token_spelling(attr: &Attribute) -> syn::Result<String> {
    let Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new(attr.span(), "expected `#[token(\"…\")]`"));
    };
    let args: Punctuated<Expr, Token![,]> = list.parse_args_with(Punctuated::parse_terminated)?;
    match args.first() {
        Some(Expr::Lit(lit)) if matches!(lit.lit, Lit::Str(_)) => {
            let Lit::Str(s) = &lit.lit else {
                unreachable!("matched above")
            };
            Ok(s.value())
        }
        _ => Err(syn::Error::new(
            attr.span(),
            "`#[token(…)]` must start with a string literal",
        )),
    }
}

/// The string of a `#[name = "…"]` attribute.
fn name_value_str(attr: &Attribute) -> syn::Result<String> {
    let Meta::NameValue(nv) = &attr.meta else {
        return Err(syn::Error::new(attr.span(), "expected `#[… = \"…\"]`"));
    };
    let Expr::Lit(lit) = &nv.value else {
        return Err(syn::Error::new(attr.span(), "expected a string literal"));
    };
    let Lit::Str(s) = &lit.lit else {
        return Err(syn::Error::new(attr.span(), "expected a string literal"));
    };
    Ok(s.value())
}

fn parse_variant(variant: &syn::Variant) -> syn::Result<Variant> {
    if !matches!(variant.fields, Fields::Unit) {
        return Err(syn::Error::new(
            variant.span(),
            "token kinds carry no data: the text is in the source",
        ));
    }
    let mut group = None;
    let mut describe = None;
    let mut spelling = None;
    let mut tokens = Vec::new();
    let mut has_regex = false;
    for attr in &variant.attrs {
        if attr.path().is_ident("group") {
            let ident: Ident = attr.parse_args()?;
            group = Some(Group::parse(&ident)?);
        } else if attr.path().is_ident("describe") {
            describe = Some(name_value_str(attr)?);
        } else if attr.path().is_ident("spelling") {
            spelling = Some(name_value_str(attr)?);
        } else if attr.path().is_ident("token") {
            tokens.push(token_spelling(attr)?);
        } else if attr.path().is_ident("regex") {
            has_regex = true;
        }
    }
    let group = group
        .ok_or_else(|| syn::Error::new(variant.span(), "every token kind needs `#[group(…)]`"))?;
    let (describe, spelling) = if group.is_fixed() {
        if describe.is_some() {
            return Err(syn::Error::new(
                variant.span(),
                "a fixed-spelling kind is described by its spelling; drop `#[describe]`",
            ));
        }
        let spelling = match (spelling, tokens.as_slice(), has_regex) {
            (None, [token], false) => token.clone(),
            (Some(spelling), [], false) => spelling,
            _ => {
                return Err(syn::Error::new(
                    variant.span(),
                    "a keyword, contextual keyword or punctuation kind has exactly one `#[token(\"…\")]`, \
                     or `#[spelling = \"…\"]` if the lexer's driver produces it, and no `#[regex]`",
                ));
            }
        };
        (spelling.clone(), Some(spelling))
    } else {
        if spelling.is_some() {
            return Err(syn::Error::new(
                variant.span(),
                "`#[spelling]` is for keyword and punctuation kinds; a trivia, literal or invalid kind has none",
            ));
        }
        let describe = describe.ok_or_else(|| {
            syn::Error::new(
                variant.span(),
                "a trivia, literal or invalid kind needs `#[describe = \"…\"]`",
            )
        })?;
        (describe, None)
    };
    Ok(Variant {
        ident: variant.ident.clone(),
        group,
        describe,
        spelling,
    })
}

pub(crate) fn token_table(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`TokenTable` applies to the token kind enum",
        ));
    };
    let name = &input.ident;
    let variants: Vec<Variant> = data
        .variants
        .iter()
        .map(parse_variant)
        .collect::<syn::Result<_>>()?;

    let all = variants.iter().map(|v| &v.ident);
    let predicate = |group: Group, fn_name: &str| {
        let fn_name = format_ident!("{fn_name}");
        let members: Vec<&Ident> = variants
            .iter()
            .filter(|v| v.group == group)
            .map(|v| &v.ident)
            .collect();
        if members.is_empty() {
            quote! { pub const fn #fn_name(self) -> bool { false } }
        } else {
            quote! { pub const fn #fn_name(self) -> bool { matches!(self, #(Self::#members)|*) } }
        }
    };
    let is_trivia = predicate(Group::Trivia, "is_trivia");
    let is_doc = predicate(Group::Doc, "is_doc");
    let is_keyword = predicate(Group::Keyword, "is_keyword");
    let is_contextual = predicate(Group::Contextual, "is_contextual_keyword");
    let is_punct = predicate(Group::Punct, "is_punct");
    let is_literal = predicate(Group::Literal, "is_literal");
    let is_invalid = predicate(Group::Invalid, "is_invalid");
    let describe_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let text = &v.describe;
        quote! { Self::#ident => #text, }
    });

    let modules = [
        Group::Trivia,
        Group::Doc,
        Group::Keyword,
        Group::Contextual,
        Group::Punct,
        Group::Literal,
        Group::Invalid,
    ]
    .map(|group| {
        let module = group.module();
        let markers = variants.iter().filter(|v| v.group == group).map(|v| {
            let ident = &v.ident;
            quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                pub struct #ident;
                impl super::TokenType for #ident {
                    const KIND: super::#name = super::#name::#ident;
                }
            }
        });
        quote! {
            /// Marker types for `Token<K>`, one per kind in this group.
            pub mod #module { #(#markers)* }
        }
    });

    let fixed: Vec<&Variant> = variants.iter().filter(|v| v.spelling.is_some()).collect();
    let token_arms = fixed.iter().map(|v| {
        let spelling = v
            .spelling
            .as_deref()
            .unwrap_or_else(|| unreachable!("filtered on `spelling`"));
        let module = v.group.module();
        let ident = &v.ident;
        quote! { (#spelling) => { $crate::token::Token<$crate::token::#module::#ident> }; }
    });
    let kind_arms = fixed.iter().map(|v| {
        let spelling = v
            .spelling
            .as_deref()
            .unwrap_or_else(|| unreachable!("filtered on `spelling`"));
        let ident = &v.ident;
        quote! { (#spelling) => { $crate::token::#name::#ident }; }
    });

    Ok(quote! {
        impl #name {
            /// Every kind, in declaration order.
            pub const ALL: &'static [#name] = &[#(Self::#all,)*];

            /// Whitespace, newlines and comments: never part of a node's shape.
            #is_trivia
            #is_doc
            /// A reserved word.
            #is_keyword
            /// A word with meaning only in some positions; a name elsewhere.
            #is_contextual
            /// Punctuation and operators.
            #is_punct
            /// A value with variable text: a name, a number, a string or
            /// template piece.
            #is_literal
            /// Text that could not be lexed as intended: a stray character or
            /// an unclosed literal. Never part of a valid tree; the parser
            /// reports it.
            #is_invalid

            /// The spelling of a fixed token, or a description of a variable one.
            pub const fn describe(self) -> &'static str {
                match self { #(#describe_arms)* }
            }
        }

        #(#modules)*

        /// The type of a token field, by spelling or by name, like `syn::Token!`:
        ///
        /// - `Token!["{"]`, `Token!["function"]`, `Token!["map"]` — a fixed
        ///   token (punctuation, keyword or contextual keyword), by its spelling;
        /// - `Token![Word]`, `Token![TemplateChunk]` — a token whose text varies,
        ///   by kind name;
        /// - `Token![LetIntroducer]`, `Token![Ident]` — one of a set of kinds.
        #[macro_export]
        macro_rules! Token {
            #(#token_arms)*
            ($name_:ident) => { $crate::token::Token<$crate::token::by_name::$name_> };
        }

        /// The kind of a fixed token, by its spelling: `TokenKind!["?"]`.
        #[macro_export]
        macro_rules! TokenKind {
            #(#kind_arms)*
        }
    })
}
