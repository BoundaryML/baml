//! Code generation for `ty_family!`.
//!
//! Every member enum and satellite struct is produced from the master
//! definition by **token-level ident replacement**: the master enum's ident
//! (the self-reference placeholder) and any satellite idents are rewritten to
//! the target member's equivalents, descending through `Box`/`Vec`/`Option`/
//! tuples and method bodies alike. Because the placeholder is the master ident
//! itself, the master `enum` in the DSL is the ordinary `Ty` definition plus
//! `#[axis(..)]` tags — no separate rewriting syntax is needed.
//!
//! Replacement is deliberately *ident-for-ident*, never ident-for-type: the DSL
//! spells nested positions out in full (`Box<Ty<N>>`), so a generic argument
//! list rides along as ordinary tokens and needs no context-sensitive splicing.
//! That is what keeps method bodies — where a bare `Ty<N>` in expression
//! position would need a turbofish — rewriting correctly.

use std::collections::HashMap;

use proc_macro2::{Group, Ident, Literal, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Fields, GenericParam, Generics, parse_quote};

use crate::parse::{Family, MVariant, Member, Satellite};

pub(crate) fn emit(family: &Family) -> TokenStream {
    let mut out = TokenStream::new();
    for member in &family.members {
        out.extend(gen_member_enum(family, member));
        out.extend(gen_accessors(family, member));
    }
    // Satellites are generated only for deep members; shallow members reuse
    // their child's satellite (e.g. `ConcreteTy::Function` holds
    // `RuntimeFunctionParamTy`, never a `ConcreteFunctionParamTy`).
    for member in &family.members {
        if member.deep {
            for sat in &family.satellites {
                out.extend(gen_satellite(family, member, sat));
            }
        }
    }
    out
}

fn gen_member_enum(family: &Family, member: &Member) -> TokenStream {
    let child = &family.members[member.child];
    let map = replacements(family, &child.name);

    // Each variant carries its explicit stable discriminant from the master
    // declaration — not its position within this member. Members
    // that omit variants therefore leave gaps in their discriminant sequence
    // rather than renumbering the tail, so a given logical variant has the same
    // `#[repr(C, u8)]` tag in every member that includes it. That cross-member
    // tag identity is exactly what lets an upcast reinterpret one member's bytes
    // as another's without a walk (see `convert::gen_widen_transmute`).
    let variants = family
        .variants
        .iter()
        .filter(|v| member.includes.contains(&v.axis))
        .map(|v| {
            let attrs = &v.attrs;
            let ident = &v.ident;
            let fields = replace_idents(v.fields.to_token_stream(), &map);
            let disc = Literal::u8_unsuffixed(v.discriminant);
            quote! { #(#attrs)* #ident #fields = #disc }
        });

    let docs = member_docs(family, member);
    let derives = nondoc_attrs(&family.master_attrs);
    let name = &member.name;
    // Declared with the master's generics *including* defaults, so a bare `Ty`
    // in type position keeps meaning `Ty<TypeName>` at every existing use site.
    let generics = &family.generics;
    quote! {
        #(#docs)*
        #(#derives)*
        // `repr(C, u8)`: a layout-stable `{ u8 tag, C-union payload }` shared
        // identically across the family, so members with matching size/align are
        // mutually transmutable.
        #[repr(C, u8)]
        pub enum #name #generics {
            #(#variants),*
        }
    }
}

/// The variants present in `member` (those whose axis is in its include-set).
pub(crate) fn member_variants<'a>(
    family: &'a Family,
    member: &'a Member,
) -> impl Iterator<Item = &'a MVariant> {
    family
        .variants
        .iter()
        .filter(move |v| member.includes.contains(&v.axis))
}

/// Generate the mechanical `attr` / `with_attr` accessors for a member. An
/// attr-carrying variant exposes its `TyAttr` (a named `attr` field or the last
/// tuple positional); an attr-less template leaf (`TypeArgRef`)
/// borrows the shared [`TyAttr::EMPTY`] and ignores `with_attr` (it has nowhere
/// to store one).
fn gen_accessors(family: &Family, member: &Member) -> TokenStream {
    let name = &member.name;
    let attr_arms = member_variants(family, member).map(|v| attr_arm(name, v));
    let with_arms = member_variants(family, member).map(|v| with_attr_arm(name, v));
    let (impl_g, ty_g, where_c) = family.generics.split_for_impl();
    quote! {
        impl #impl_g #name #ty_g #where_c {
            #[doc = " Borrow this type's streaming/SAP attributes ([`TyAttr`])."]
            pub fn attr(&self) -> &TyAttr {
                match self {
                    #(#attr_arms),*
                }
            }

            #[doc = " Return this type with its [`TyAttr`] replaced by `attr`."]
            pub fn with_attr(self, attr: TyAttr) -> Self {
                match self {
                    #(#with_arms),*
                }
            }
        }
    }
}

fn attr_arm(name: &Ident, v: &MVariant) -> TokenStream {
    let vident = &v.ident;
    // Attr-less leaves borrow the shared empty attribute set.
    if !v.has_attr {
        return match &v.fields {
            Fields::Unit => quote! { #name::#vident => &TyAttr::EMPTY },
            _ => quote! { #name::#vident { .. } => &TyAttr::EMPTY },
        };
    }
    match &v.fields {
        Fields::Named(_) => quote! { #name::#vident { attr, .. } => attr },
        Fields::Unnamed(_) => quote! { #name::#vident(.., attr) => attr },
        // `has_attr` is false for a unit variant, handled above.
        Fields::Unit => unreachable!("attr-carrying unit variant is impossible"),
    }
}

fn with_attr_arm(name: &Ident, v: &MVariant) -> TokenStream {
    let vident = &v.ident;
    // Attr-less leaves have nowhere to store an attribute, so `with_attr` is
    // the identity — the incoming `attr` is dropped. `_ = &attr` silences the
    // unused-binding lint without moving it (every other arm consumes `attr`).
    if !v.has_attr {
        return match &v.fields {
            Fields::Unit => quote! { this @ #name::#vident => { let _ = &attr; this } },
            _ => quote! { this @ #name::#vident { .. } => { let _ = &attr; this } },
        };
    }
    match &v.fields {
        Fields::Named(named) => {
            let rest: Vec<&Ident> = named
                .named
                .iter()
                .filter_map(|f| f.ident.as_ref())
                .filter(|id| *id != "attr")
                .collect();
            quote! {
                #name::#vident { #(#rest,)* .. } => #name::#vident { #(#rest,)* attr }
            }
        }
        Fields::Unnamed(unnamed) => {
            let lead = unnamed.unnamed.len().saturating_sub(1);
            let binds: Vec<Ident> = (0..lead).map(|i| format_ident!("f{}", i)).collect();
            quote! {
                #name::#vident(#(#binds,)* _) => #name::#vident(#(#binds,)* attr)
            }
        }
        // `has_attr` is false for a unit variant, handled above.
        Fields::Unit => unreachable!("attr-carrying unit variant is impossible"),
    }
}

fn gen_satellite(family: &Family, member: &Member, sat: &Satellite) -> TokenStream {
    let sat_name = satellite_name_for(&member.name, &sat.name);
    let map = replacements(family, &member.name);

    let fields = replace_idents(sat.fields.to_token_stream(), &map);
    let derives = satellite_attrs(&family.master_attrs);
    let generics = &sat.generics;
    let (impl_g, ty_g, where_c) = sat.generics.split_for_impl();
    let methods = sat.methods.as_ref().map(|body| {
        let body = replace_idents(body.clone(), &map);
        quote! { impl #impl_g #sat_name #ty_g #where_c { #body } }
    });

    let doc = format!(
        " Companion of [`{}`] for the `{}` member of the family.",
        sat.name, member.name
    );
    quote! {
        #[doc = #doc]
        #(#derives)*
        // `repr(C)`: a fixed field order shared across the family so a satellite
        // is layout-identical to its siblings (they differ only in the recursive
        // field's member type), letting the enum-level conversion reinterpret a
        // `Vec<FunctionParamTy>` field as `Vec<RuntimeFunctionParamTy>` soundly.
        #[repr(C)]
        pub struct #sat_name #generics {
            #fields
        }
        #methods
    }
}

/// `generics` with every type parameter additionally bounded by `Clone`.
///
/// The by-reference halves of the conversion matrix (`From<&Sub>`,
/// `TryFrom<&Sup>`) clone the tree they convert, which the derived
/// `Clone` supplies only under `N: Clone`. Emitted only on those impls, so the
/// owned and borrow-to-borrow conversions stay bound-free.
pub(crate) fn with_clone_bounds(generics: &Generics) -> Generics {
    let mut bounded = generics.clone();
    for param in &mut bounded.params {
        if let GenericParam::Type(t) = param {
            t.bounds.push(parse_quote!(::core::clone::Clone));
        }
    }
    bounded
}

/// The idents to rewrite when generating for `target`: the master ident becomes
/// `target`, and each satellite name becomes `target`'s satellite.
fn replacements(family: &Family, target: &Ident) -> HashMap<String, Ident> {
    let mut map = HashMap::new();
    map.insert(family.master_ident.to_string(), target.clone());
    for sat in &family.satellites {
        map.insert(sat.name.to_string(), satellite_name_for(target, &sat.name));
    }
    map
}

/// Walk `tokens`, replacing any [`Ident`] found in `map`; recurse into groups.
fn replace_idents(tokens: TokenStream, map: &HashMap<String, Ident>) -> TokenStream {
    tokens
        .into_iter()
        .map(|tt| match tt {
            TokenTree::Group(g) => {
                let inner = replace_idents(g.stream(), map);
                let mut replaced = Group::new(g.delimiter(), inner);
                replaced.set_span(g.span());
                TokenTree::Group(replaced)
            }
            TokenTree::Ident(id) => match map.get(&id.to_string()) {
                Some(rep) => {
                    let mut rep = rep.clone();
                    rep.set_span(id.span());
                    TokenTree::Ident(rep)
                }
                None => TokenTree::Ident(id),
            },
            other => other,
        })
        .collect()
}

/// `RuntimeTy` + `FunctionParamTy` → `RuntimeFunctionParamTy`; the master `Ty`
/// (prefix empty) leaves the satellite name unchanged.
pub(crate) fn satellite_name_for(member: &Ident, sat: &Ident) -> Ident {
    let s = member.to_string();
    let prefix = s.strip_suffix("Ty").unwrap_or(&s);
    Ident::new(&format!("{prefix}{sat}"), sat.span())
}

fn member_docs(family: &Family, member: &Member) -> Vec<TokenStream> {
    if member.is_master {
        family
            .master_attrs
            .iter()
            .filter(|a| is_doc(a))
            .map(|a| quote! { #a })
            .collect()
    } else {
        let text = format!(
            " A member of the [`{}`] type family generated by `ty_family!`.",
            family.master_ident
        );
        vec![quote! { #[doc = #text] }]
    }
}

fn nondoc_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs.iter().filter(|a| !is_doc(a)).collect()
}

fn satellite_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs
        .iter()
        .filter(|attr| !is_doc(attr) && !attr.path().is_ident("borsh"))
        .collect()
}

fn is_doc(attr: &Attribute) -> bool {
    attr.path().is_ident("doc")
}
