//! Emission of *interned* family members (`child: interned(Handle)`).
//!
//! An interned member is the hash-cons pool's **kind**: the one-level-deep
//! structural layer a pool handle dereferences to. Where a plain member's
//! nested positions hold child *trees* (`Box<Ty<N>>`, `Box<[Ty<N>]>`), an
//! interned member's hold the handle type named in the DSL — pool lookups
//! hash and compare shallowly, and substructure is shared automatically.
//!
//! Because the handle replaces the child type rather than renaming it, this
//! emission is **type-shape-aware** where the plain one is ident-for-ident:
//!
//! - `Box<Ty<N>>` and bare `Ty<N>` both become the handle — the handle *is*
//!   the indirection, so the `Box` collapses.
//! - `Box<[Ty<N>]>` becomes `Box<[Handle]>` (the sequence shape is shared;
//!   only the element type changes), and tuples map elementwise, so
//!   `Box<[(Name, Ty<N>)]>` becomes `Box<[(Name, Handle)]>`.
//! - `Box<Sat<N>>` and bare `Sat<N>` become the member's *twin* satellite,
//!   inline: with handle children the twin is small, so the plain side's
//!   size-bounding `Box` has nothing left to bound.
//! - The head parameter is fixed at its declared default — the pool is
//!   monomorphic, so the generated types take no generics.
//!
//! Emitted: the kind enum, its twin satellites (named by the shared prefix
//! rule: `InferTy` + `FunctionParamTy` → `InferFunctionParamTy`), and the
//! `attr`/`with_attr` accessors. Deliberately **not** emitted, staying
//! hand-written where the pool lives: the handle/pool/flags themselves, child
//! walkers, the boundary conversions (interning and finalization are semantic,
//! not structural), and satellite `methods` blocks (their bodies assume plain
//! shapes). The member is likewise excluded from the conversion matrix and the
//! head visitors/mappers — its layout is alien to the `#[repr(C, u8)]`
//! transmute family, and a generic walker cannot descend through a handle.
//!
//! Derives are the master's minus borsh: a handle is a pool pointer, never
//! serialized. No `#[repr(C, u8)]` and no discriminants either (the enum is
//! never transmuted or persisted); declaration order alone preserves `Ord`
//! parity with the plain members, whose explicit discriminants are monotone in
//! declaration order.

use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{Attribute, Fields, GenericParam, Token, punctuated::Punctuated};

use crate::{
    convert::{
        boxed_slice_arg, contains_recursion, path_head, reachable_satellites, unsupported,
        wrapper_arg,
    },
    emit::{attr_arm, is_doc, member_variants, satellite_name_for, with_attr_arm},
    parse::{Family, Member, Satellite},
};

/// Everything the type transform needs at each position.
struct Cx<'a> {
    family: &'a Family,
    member: &'a Member,
    /// The handle type named by `child: interned(..)`.
    handle: &'a syn::Type,
    /// The family's head parameter and its declared default, when the family
    /// is generic. The generated types are monomorphic: every occurrence of
    /// the parameter is replaced by the default.
    head: Option<(&'a Ident, &'a syn::Type)>,
}

pub(crate) fn gen_interned_member(
    family: &Family,
    member: &Member,
    handle: &syn::Type,
) -> TokenStream {
    let head = match head_and_default(family) {
        Ok(head) => head,
        Err(e) => return e,
    };
    let cx = Cx {
        family,
        member,
        handle,
        head,
    };
    let mut out = gen_enum(&cx);
    out.extend(gen_accessors(&cx));
    // Twin satellites, pruned to those reachable through the member's own
    // variants (mirroring the conversion validators' pruning): a satellite
    // reached only through an excluded variant would be a dead type.
    let reachable = reachable_satellites(family, member);
    for sat in &family.satellites {
        if reachable.contains(&sat.name.to_string()) {
            out.extend(gen_twin_satellite(&cx, sat));
        }
    }
    out
}

/// The family's single head parameter and its default. A parameterless family
/// has no head to fix (`Ok(None)`); more than one parameter or a missing
/// default is a spanned expansion error, since the transform could neither
/// pick "the" head nor a type to fix it at.
fn head_and_default(family: &Family) -> Result<Option<(&Ident, &syn::Type)>, TokenStream> {
    let mut params = family.generics.params.iter().filter_map(|p| match p {
        GenericParam::Type(t) => Some(t),
        GenericParam::Lifetime(_) | GenericParam::Const(_) => None,
    });
    let Some(first) = params.next() else {
        return Ok(None);
    };
    if params.next().is_some() {
        return Err(quote! {
            ::core::compile_error!(
                "ty_family: an interned member supports a single head parameter; \
                 with more than one, which to fix at its default is ambiguous"
            );
        });
    }
    let Some(default) = &first.default else {
        return Err(quote! {
            ::core::compile_error!(
                "ty_family: an interned member requires the head parameter to \
                 declare a default (`<N: Clone = TypeName>`): the pool is \
                 monomorphic, so the generated kind fixes the head at that default"
            );
        });
    };
    Ok(Some((&first.ident, default)))
}

fn gen_enum(cx: &Cx) -> TokenStream {
    let name = &cx.member.name;
    let master = &cx.family.master_ident;
    let handle = cx.handle;
    let variants = member_variants(cx.family, cx.member).map(|v| {
        let attrs = &v.attrs;
        let ident = &v.ident;
        let fields = interned_fields(cx, &v.fields);
        quote! { #(#attrs)* #ident #fields }
    });
    let derives = interned_attrs(&cx.family.master_attrs);
    let doc = format!(
        " The interned member of the [`{master}`] type family generated by\n\
          `ty_family!`: the hash-cons pool's kind, with nested positions holding\n\
          [`{handle_text}`] handles instead of child trees.",
        handle_text = quote!(#handle),
    );
    quote! {
        #[doc = #doc]
        #(#derives)*
        pub enum #name {
            #(#variants),*
        }
    }
}

/// A variant's fields with every type run through the interned transform.
/// Field-level attributes (docs) ride along unchanged.
fn interned_fields(cx: &Cx, fields: &Fields) -> TokenStream {
    match fields {
        Fields::Unit => quote!(),
        Fields::Named(n) => {
            let fields = n.named.iter().map(|f| {
                let attrs = &f.attrs;
                let ident = &f.ident;
                let ty = interned_ty(cx, &f.ty);
                quote! { #(#attrs)* #ident: #ty }
            });
            quote! { { #(#fields),* } }
        }
        Fields::Unnamed(u) => {
            let fields = u.unnamed.iter().map(|f| {
                let attrs = &f.attrs;
                let ty = interned_ty(cx, &f.ty);
                quote! { #(#attrs)* #ty }
            });
            quote! { ( #(#fields),* ) }
        }
    }
}

/// The interned image of a master field type. See the module docs for the
/// shape rules; anything head-free and family-free passes through unchanged.
fn interned_ty(cx: &Cx, ty: &syn::Type) -> TokenStream {
    if !mentions_family_or_head(cx, ty) {
        return quote!(#ty);
    }
    // The head parameter, fixed at its declared default.
    if let Some((param, default)) = cx.head
        && path_head(ty).is_some_and(|id| id == param)
    {
        return quote!(#default);
    }
    if let Some(id) = path_head(ty) {
        if *id == cx.family.master_ident {
            let handle = cx.handle;
            return quote!(#handle);
        }
        if let Some(sat) = cx.family.satellites.iter().find(|s| s.name == *id) {
            let twin = satellite_name_for(&cx.member.name, &sat.name);
            return quote!(#twin);
        }
    }
    if let Some(inner) = boxed_slice_arg(ty) {
        let elem = interned_ty(cx, inner);
        return quote!(::std::boxed::Box<[#elem]>);
    }
    if let Some(inner) = wrapper_arg(ty, "Box") {
        // `Box<Ty<N>>` → handle, `Box<Sat<N>>` → inline twin: the box exists
        // on the plain side to bound the variant's size, a job the one-word
        // handle (or the twin, whose children are handles) already does.
        if path_head(inner).is_some_and(|id| *id == cx.family.master_ident) {
            let handle = cx.handle;
            return quote!(#handle);
        }
        if let Some(sat) =
            path_head(inner).and_then(|id| cx.family.satellites.iter().find(|s| s.name == *id))
        {
            let twin = satellite_name_for(&cx.member.name, &sat.name);
            return quote!(#twin);
        }
        let elem = interned_ty(cx, inner);
        return quote!(::std::boxed::Box<#elem>);
    }
    if let Some(inner) = wrapper_arg(ty, "Vec") {
        let elem = interned_ty(cx, inner);
        return quote!(::std::vec::Vec<#elem>);
    }
    if let Some(inner) = wrapper_arg(ty, "Option") {
        let elem = interned_ty(cx, inner);
        return quote!(::core::option::Option<#elem>);
    }
    if let syn::Type::Tuple(t) = ty {
        // Trailing comma so a one-element tuple stays a tuple type.
        let elems = t.elems.iter().map(|et| interned_ty(cx, et));
        return quote!(( #(#elems,)* ));
    }
    unsupported(ty)
}

/// Whether `ty` mentions the master ident, a satellite name, or the head
/// parameter — the positions the transform must rewrite.
fn mentions_family_or_head(cx: &Cx, ty: &syn::Type) -> bool {
    if contains_recursion(cx.family, ty) {
        return true;
    }
    let Some((param, _)) = cx.head else {
        return false;
    };
    fn walk(ts: TokenStream, param: &Ident) -> bool {
        ts.into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Ident(id) => id == *param,
            proc_macro2::TokenTree::Group(g) => walk(g.stream(), param),
            _ => false,
        })
    }
    walk(quote!(#ty), param)
}

/// The `attr`/`with_attr` accessors. Same arms as the plain members', but the
/// impl takes no generics — the interned member is monomorphic.
fn gen_accessors(cx: &Cx) -> TokenStream {
    let name = &cx.member.name;
    let attr_arms = member_variants(cx.family, cx.member).map(|v| attr_arm(name, v));
    let with_arms = member_variants(cx.family, cx.member).map(|v| with_attr_arm(name, v));
    quote! {
        impl #name {
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

/// The interned twin of a satellite: same fields through the same transform,
/// monomorphic, no `#[repr(C)]` (never transmuted), and no `methods` block —
/// hand-written impls (constructors preserving invariants like sorted
/// bindings) attach where the pool lives.
fn gen_twin_satellite(cx: &Cx, sat: &Satellite) -> TokenStream {
    let twin = satellite_name_for(&cx.member.name, &sat.name);
    let fields = sat.fields.iter().map(|f| {
        let attrs = &f.attrs;
        let vis = &f.vis;
        let ident = &f.ident;
        let ty = interned_ty(cx, &f.ty);
        quote! { #(#attrs)* #vis #ident: #ty }
    });
    let derives = interned_attrs(&cx.family.master_attrs);
    let doc = format!(
        " Interned companion of [`{}`] for the `{}` member of the family:\n\
          nested positions hold pool handles.",
        sat.name, cx.member.name
    );
    quote! {
        #[doc = #doc]
        #(#derives)*
        pub struct #twin {
            #(#fields),*
        }
    }
}

/// The master's attributes minus docs and minus everything borsh: the
/// `#[borsh(..)]` helper and any `Borsh*` derive. A handle is a pool pointer —
/// interned types are never serialized, and the derives would not compile
/// against it.
fn interned_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|a| !is_doc(a) && !a.path().is_ident("borsh"))
        .filter_map(|a| {
            if !a.path().is_ident("derive") {
                return Some((*a).clone());
            }
            // Rebuild the derive list without the borsh derives. A derive list
            // that fails to parse as paths is left in place for rustc to
            // report at its own site.
            let Ok(paths) = a.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            else {
                return Some((*a).clone());
            };
            let kept: Vec<syn::Path> = paths
                .into_iter()
                .filter(|p| {
                    !p.segments
                        .last()
                        .is_some_and(|s| s.ident.to_string().starts_with("Borsh"))
                })
                .collect();
            if kept.is_empty() {
                return None;
            }
            Some(syn::parse_quote!(#[derive(#(#kept),*)]))
        })
        .collect()
}
