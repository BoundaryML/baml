//! Code generation for `ty_family!`.
//!
//! Every member enum and satellite struct is produced from the master
//! definition by **token-level ident replacement**: the master enum's ident
//! (the self-reference placeholder) and any satellite idents are rewritten to
//! the target member's equivalents, descending through `Box`/`Box<[..]>`/`Vec`/
//! `Option`/tuples and method bodies alike. Because the placeholder is the master ident
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
use syn::{Attribute, Fields, GenericParam, Generics, Index, parse_quote};

use crate::parse::{Child, Family, MVariant, Member, Satellite};

pub(crate) fn emit(family: &Family) -> TokenStream {
    let mut out = TokenStream::new();
    for member in &family.members {
        match &member.child {
            Child::Member(child_idx) => {
                out.extend(gen_member_enum(family, member, *child_idx));
                out.extend(gen_accessors(family, member));
                out.extend(gen_head_visitors(family, member));
                out.extend(gen_head_mappers(family, member));
            }
            // An interned member is the hash-cons pool's kind: children are
            // handles, so its emission is type-shape-aware rather than
            // ident-for-ident, and the plain walkers/mappers (which descend
            // through child *trees*) do not apply.
            Child::Interned(handle) => {
                out.extend(crate::interned_member::gen_interned_member(
                    family, member, handle,
                ));
            }
        }
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

fn gen_member_enum(family: &Family, member: &Member, child_idx: usize) -> TokenStream {
    let child = &family.members[child_idx];
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

pub(crate) fn attr_arm(name: &Ident, v: &MVariant) -> TokenStream {
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

pub(crate) fn with_attr_arm(name: &Ident, v: &MVariant) -> TokenStream {
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

// ── Head visitors ────────────────────────────────────────────────────────────
//
// A *head* is an occurrence of the family's type parameter — the nominal
// reference in `Class`, `Interface`, `Enum`, `EnumVariant`, `TypeAlias`, and
// `Interface::name`. These walkers exist so a consumer that must reach every
// head (a garbage collector tracing and forwarding heap-anchored heads) can do
// so exhaustively *by construction*: the arms are generated from the same
// variant list as the enum, so a new head-bearing variant cannot be added
// without its traversal appearing with it. Hand-maintained tracing is precisely
// where a missed edge becomes a dangling pointer.

/// The family's sole type parameter. Head traversal is defined only for a
/// single-parameter family — with two, "the head" would be ambiguous — so a
/// wider family is rejected at expansion rather than given a walker that
/// silently covers one parameter.
fn head_param(family: &Family) -> Result<Option<&Ident>, TokenStream> {
    let mut params = family.generics.params.iter().filter_map(|p| match p {
        GenericParam::Type(t) => Some(&t.ident),
        GenericParam::Lifetime(_) | GenericParam::Const(_) => None,
    });
    let Some(first) = params.next() else {
        return Ok(None);
    };
    if params.next().is_some() {
        return Err(quote! {
            ::core::compile_error!(
                "ty_family: head visitors support a single type parameter; \
                 with more than one, which parameter is the nominal head is ambiguous"
            );
        });
    }
    Ok(Some(first))
}

fn gen_head_visitors(family: &Family, member: &Member) -> TokenStream {
    let param = match head_param(family) {
        Ok(Some(p)) => p,
        Ok(None) => return TokenStream::new(),
        Err(e) => return e,
    };
    let name = &member.name;
    let (impl_g, ty_g, where_c) = family.generics.split_for_impl();
    let shared = member_variants(family, member)
        .map(|v| head_arm(family, param, name, v, Mutability::Shared));
    let unique = member_variants(family, member)
        .map(|v| head_arm(family, param, name, v, Mutability::Unique));
    let (shared_doc, unique_doc) = visitor_docs(param);
    quote! {
        impl #impl_g #name #ty_g #where_c {
            #[doc = #shared_doc]
            pub fn visit_heads<F: ::core::ops::FnMut(&#param)>(&self, f: &mut F) {
                match self {
                    #(#shared)*
                }
            }

            #[doc = #unique_doc]
            pub fn visit_heads_mut<F: ::core::ops::FnMut(&mut #param)>(&mut self, f: &mut F) {
                match self {
                    #(#unique)*
                }
            }
        }
    }
}

/// The satellite analogue: a plain struct, so a field walk rather than a match.
fn gen_sat_head_visitors(family: &Family, sat_name: &Ident, sat: &Satellite) -> TokenStream {
    let param = match head_param(family) {
        Ok(Some(p)) => p,
        Ok(None) => return TokenStream::new(),
        Err(e) => return e,
    };
    let (impl_g, ty_g, where_c) = sat.generics.split_for_impl();
    let walk = |m: Mutability| -> Vec<TokenStream> {
        sat.fields
            .iter()
            .filter_map(|field| {
                let fid = field.ident.as_ref()?;
                let place = m.field_place(quote!(self), fid);
                visit_expr(family, param, &field.ty, place, m)
            })
            .collect()
    };
    let shared = walk(Mutability::Shared);
    let unique = walk(Mutability::Unique);
    let (shared_doc, unique_doc) = visitor_docs(param);
    quote! {
        impl #impl_g #sat_name #ty_g #where_c {
            #[doc = #shared_doc]
            pub fn visit_heads<F: ::core::ops::FnMut(&#param)>(&self, f: &mut F) {
                #(#shared)*
            }

            #[doc = #unique_doc]
            pub fn visit_heads_mut<F: ::core::ops::FnMut(&mut #param)>(&mut self, f: &mut F) {
                #(#unique)*
            }
        }
    }
}

fn visitor_docs(param: &Ident) -> (String, String) {
    (
        format!(
            " Call `f` on every `{param}` head reachable from this type, in\n\
              declaration order, descending through nested positions."
        ),
        format!(
            " Call `f` on every `{param}` head reachable from this type, by unique\n\
              reference, so each can be rewritten in place.\n\n\
              A relocating collector uses this to forward heads after a move."
        ),
    )
}

/// Whether a traversal borrows shared or uniquely. The two visitors are the
/// same walk over the same positions, differing only in how each place is
/// reached, so both are generated from one description.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mutability {
    Shared,
    Unique,
}

impl Mutability {
    fn method(self) -> Ident {
        match self {
            Mutability::Shared => format_ident!("visit_heads"),
            Mutability::Unique => format_ident!("visit_heads_mut"),
        }
    }

    /// `iter` / `iter_mut`, and `as_ref` / `as_mut`.
    fn iter(self) -> Ident {
        match self {
            Mutability::Shared => format_ident!("iter"),
            Mutability::Unique => format_ident!("iter_mut"),
        }
    }

    fn opt(self) -> Ident {
        match self {
            Mutability::Shared => format_ident!("as_ref"),
            Mutability::Unique => format_ident!("as_mut"),
        }
    }

    // Every place is parenthesized: it flows into method chains
    // (`place.iter()`, `place.as_ref()`), and a bare `&self.generics.iter()`
    // would bind the `&` to the whole chain rather than the field.
    fn deref_box(self, place: &TokenStream) -> TokenStream {
        match self {
            Mutability::Shared => quote!((&**#place)),
            Mutability::Unique => quote!((&mut **#place)),
        }
    }

    fn field_place(self, base: TokenStream, field: &Ident) -> TokenStream {
        match self {
            Mutability::Shared => quote!((&#base.#field)),
            Mutability::Unique => quote!((&mut #base.#field)),
        }
    }

    fn tuple_elem(self, place: &TokenStream, idx: &Index) -> TokenStream {
        match self {
            Mutability::Shared => quote!((&#place.#idx)),
            Mutability::Unique => quote!((&mut #place.#idx)),
        }
    }
}

/// One match arm. Only head-bearing fields are bound; a variant with none gets
/// an explicit empty arm rather than falling into a catch-all, so the expansion
/// names every variant it considered.
fn head_arm(
    family: &Family,
    param: &Ident,
    name: &Ident,
    v: &MVariant,
    m: Mutability,
) -> TokenStream {
    let id = &v.ident;
    let skip = match &v.fields {
        Fields::Unit => quote!(),
        Fields::Unnamed(_) => quote!((..)),
        Fields::Named(_) => quote!({ .. }),
    };
    let empty = quote! { #name::#id #skip => {} };
    match &v.fields {
        Fields::Unit => empty,
        Fields::Named(named) => {
            let mut binds = Vec::new();
            let mut visits = Vec::new();
            for field in &named.named {
                let Some(fid) = field.ident.as_ref() else {
                    continue;
                };
                if let Some(visit) = visit_expr(family, param, &field.ty, quote!(#fid), m) {
                    binds.push(quote!(#fid));
                    visits.push(visit);
                }
            }
            if visits.is_empty() {
                return empty;
            }
            quote! { #name::#id { #(#binds,)* .. } => { #(#visits)* } }
        }
        Fields::Unnamed(unnamed) => {
            let mut pats = Vec::new();
            let mut visits = Vec::new();
            for (i, field) in unnamed.unnamed.iter().enumerate() {
                let b = format_ident!("f{}", i);
                match visit_expr(family, param, &field.ty, quote!(#b), m) {
                    Some(visit) => {
                        pats.push(quote!(#b));
                        visits.push(visit);
                    }
                    None => pats.push(quote!(_)),
                }
            }
            if visits.is_empty() {
                return empty;
            }
            quote! { #name::#id ( #(#pats),* ) => { #(#visits)* } }
        }
    }
}

/// A statement visiting every head behind `place` (already a reference of the
/// requested mutability), or `None` when the type holds no head at all.
fn visit_expr(
    family: &Family,
    param: &Ident,
    ty: &syn::Type,
    place: TokenStream,
    m: Mutability,
) -> Option<TokenStream> {
    if !mentions(ty, param) {
        return None;
    }
    // A bare head — the leaf this whole walk exists to reach.
    if crate::convert::path_head(ty).is_some_and(|id| id == param) {
        return Some(quote! { f(#place); });
    }
    // A family type (member enum or satellite) carries its own walker; its
    // generic arguments need no inspection, since the method is defined at
    // whatever arguments the field holds.
    if let Some(id) = crate::convert::path_head(ty)
        && (*id == family.master_ident || family.satellites.iter().any(|s| s.name == *id))
    {
        // `place` is already a reference of the right mutability, and method
        // resolution auto-reborrows, so no adjustment is needed here.
        let method = m.method();
        return Some(quote! { #place.#method(f); });
    }
    if let Some(inner) = crate::convert::boxed_slice_arg(ty) {
        let iter = m.iter();
        let body = visit_expr(family, param, inner, quote!(__head_item), m)?;
        return Some(quote! { for __head_item in #place.#iter() { #body } });
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Box") {
        let inner_place = m.deref_box(&place);
        return visit_expr(family, param, inner, inner_place, m);
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Vec") {
        let iter = m.iter();
        let body = visit_expr(family, param, inner, quote!(__head_item), m)?;
        return Some(quote! { for __head_item in #place.#iter() { #body } });
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Option") {
        let opt = m.opt();
        let body = visit_expr(family, param, inner, quote!(__head_item), m)?;
        return Some(quote! {
            if let ::core::option::Option::Some(__head_item) = #place.#opt() { #body }
        });
    }
    if let syn::Type::Tuple(t) = ty {
        let parts: Vec<TokenStream> = t
            .elems
            .iter()
            .enumerate()
            .filter_map(|(i, et)| {
                let idx = Index::from(i);
                visit_expr(family, param, et, m.tuple_elem(&place, &idx), m)
            })
            .collect();
        return (!parts.is_empty()).then(|| quote! { #(#parts)* });
    }
    Some(crate::convert::unsupported(ty))
}

// ── Head mappers ─────────────────────────────────────────────────────────────
//
// The visitors above reach every head at a *fixed* representation. Re-anchoring
// the runtime onto heap-backed heads needs the other direction: rebuild the same
// tree at a *different* head type. That cannot be a `visit_heads_mut` — an
// in-place rewrite cannot change a value's type — so it is a structural map,
// generated for the same reason the visitors are. A head-bearing variant added
// later gets its mapping with it, instead of silently keeping the old head type
// on a hand-maintained conversion.
//
// The fallible form is the primitive: resolving a name to a heap pointer can
// fail, and a failed lookup must surface rather than fill a stand-in head.
// `map_heads` is the infallible wrapper, for total directions like recovering a
// name from an anchored head.

/// A method-level parameter named `preferred`, or `preferred2` if the family's
/// head parameter already claims that name.
fn fresh_param(head: &Ident, preferred: &str) -> Ident {
    if head == preferred {
        format_ident!("{preferred}2")
    } else {
        format_ident!("{preferred}")
    }
}

/// The bounds declared on the head parameter, to be reapplied to the mapped-to
/// parameter — the target of a map must satisfy whatever the family requires of
/// a head, and duplicating a literal `Clone` here would drift from the DSL.
fn head_bounds(generics: &Generics) -> Option<TokenStream> {
    generics.params.iter().find_map(|p| match p {
        GenericParam::Type(t) if !t.bounds.is_empty() => {
            let bounds = &t.bounds;
            Some(quote!(#bounds))
        }
        GenericParam::Type(_) | GenericParam::Lifetime(_) | GenericParam::Const(_) => None,
    })
}

/// A mapper names the mapped-to type (`Ty<M>`), so unlike the visitors it cannot
/// tolerate a lifetime or const parameter riding alongside the head.
fn mapper_generics_ok(generics: &Generics) -> Result<(), TokenStream> {
    if generics.params.len() == 1 {
        return Ok(());
    }
    Err(quote! {
        ::core::compile_error!(
            "ty_family: head mappers require the head to be the family's only \
             generic parameter; naming the mapped-to type is impossible otherwise"
        );
    })
}

fn mapper_docs(param: &Ident, m: &Ident) -> (String, String) {
    (
        format!(
            " Rebuild this type with every `{param}` head replaced by the `{m}` that\n\
              `f` resolves it to, failing on the first head `f` rejects.\n\n\
              This is the only way to change a type's head representation: heads sit\n\
              in the type, so re-anchoring them produces a different type rather\n\
              than mutating this one."
        ),
        format!(
            " Rebuild this type with every `{param}` head replaced by `f`'s result.\n\n\
              The total counterpart of `try_map_heads`, for directions that cannot\n\
              fail — recovering a name from an anchored head, say."
        ),
    )
}

/// The mapper pair for a member enum: a match rebuilding each variant.
fn gen_head_mappers(family: &Family, member: &Member) -> TokenStream {
    let param = match head_param(family) {
        Ok(Some(p)) => p,
        Ok(None) => return TokenStream::new(),
        Err(e) => return e,
    };
    if let Err(e) = mapper_generics_ok(&family.generics) {
        return e;
    }
    let name = &member.name;
    let arms = member_variants(family, member).map(|v| map_arm(family, param, name, v));
    mapper_impl(
        family,
        param,
        name,
        &family.generics,
        quote! { match self { #(#arms)* } },
    )
}

/// The satellite analogue: a plain struct, so a field-wise rebuild.
fn gen_sat_head_mappers(family: &Family, sat_name: &Ident, sat: &Satellite) -> TokenStream {
    let param = match head_param(family) {
        Ok(Some(p)) => p,
        Ok(None) => return TokenStream::new(),
        Err(e) => return e,
    };
    if let Err(e) = mapper_generics_ok(&sat.generics) {
        return e;
    }
    let inits = sat.fields.iter().filter_map(|field| {
        let fid = field.ident.as_ref()?;
        let place = quote!((&self.#fid));
        let expr = map_expr(family, param, &field.ty, place);
        Some(quote!(#fid: #expr))
    });
    mapper_impl(
        family,
        param,
        sat_name,
        &sat.generics,
        quote! { #sat_name { #(#inits),* } },
    )
}

/// The shared `try_map_heads` / `map_heads` impl block; `body` is the expression
/// producing the rebuilt value (a match for an enum, a literal for a struct).
fn mapper_impl(
    family: &Family,
    param: &Ident,
    name: &Ident,
    generics: &Generics,
    body: TokenStream,
) -> TokenStream {
    let (impl_g, ty_g, where_c) = generics.split_for_impl();
    let m = fresh_param(param, "M");
    let e = fresh_param(param, "E");
    let f = fresh_param(param, "F");
    let m_bound = head_bounds(&family.generics).map(|b| quote!(#m: #b,));
    let (try_doc, total_doc) = mapper_docs(param, &m);
    quote! {
        impl #impl_g #name #ty_g #where_c {
            #[doc = #try_doc]
            pub fn try_map_heads<#m, #e, #f>(
                &self,
                f: &mut #f,
            ) -> ::core::result::Result<#name<#m>, #e>
            where
                #m_bound
                #f: ::core::ops::FnMut(&#param) -> ::core::result::Result<#m, #e>,
            {
                ::core::result::Result::Ok(#body)
            }

            #[doc = #total_doc]
            pub fn map_heads<#m, #f>(&self, f: &mut #f) -> #name<#m>
            where
                #m_bound
                #f: ::core::ops::FnMut(&#param) -> #m,
            {
                match self.try_map_heads::<#m, ::core::convert::Infallible, _>(
                    &mut |__head: &#param| ::core::result::Result::Ok(f(__head)),
                ) {
                    ::core::result::Result::Ok(__mapped) => __mapped,
                    // `Infallible` is uninhabited, so this arm is unreachable by
                    // type, not by argument.
                    ::core::result::Result::Err(__never) => match __never {},
                }
            }
        }
    }
}

/// One rebuild arm. Unlike [`head_arm`], *every* field is bound and reproduced —
/// a map returns a whole value, so the head-free fields must be carried across
/// rather than skipped.
fn map_arm(family: &Family, param: &Ident, name: &Ident, v: &MVariant) -> TokenStream {
    let id = &v.ident;
    match &v.fields {
        Fields::Unit => quote! { #name::#id => #name::#id, },
        Fields::Named(named) => {
            let mut binds = Vec::new();
            let mut inits = Vec::new();
            for field in &named.named {
                let Some(fid) = field.ident.as_ref() else {
                    continue;
                };
                let expr = map_expr(family, param, &field.ty, quote!(#fid));
                binds.push(quote!(#fid));
                inits.push(quote!(#fid: #expr));
            }
            quote! { #name::#id { #(#binds),* } => #name::#id { #(#inits),* }, }
        }
        Fields::Unnamed(unnamed) => {
            let mut pats = Vec::new();
            let mut inits = Vec::new();
            for (i, field) in unnamed.unnamed.iter().enumerate() {
                let b = format_ident!("f{}", i);
                inits.push(map_expr(family, param, &field.ty, quote!(#b)));
                pats.push(quote!(#b));
            }
            quote! { #name::#id ( #(#pats),* ) => #name::#id ( #(#inits),* ), }
        }
    }
}

/// An expression rebuilding what `place` (a shared reference) holds, with heads
/// mapped through `f`. `?` inside propagates out of the generated method, so
/// every wrapper is expanded as a block or `match` rather than a closure.
fn map_expr(family: &Family, param: &Ident, ty: &syn::Type, place: TokenStream) -> TokenStream {
    // Head-free payloads (a `TyAttr`, a field `Name`, a discriminant) are carried
    // across unchanged. Spelled as a call rather than `place.clone()` so a
    // non-`Clone` payload is a type error here instead of silently cloning the
    // reference.
    if !mentions(ty, param) {
        return quote! { ::core::clone::Clone::clone(#place) };
    }
    if crate::convert::path_head(ty).is_some_and(|id| id == param) {
        return quote! { f(#place)? };
    }
    if let Some(id) = crate::convert::path_head(ty)
        && (*id == family.master_ident || family.satellites.iter().any(|s| s.name == *id))
    {
        return quote! { #place.try_map_heads(f)? };
    }
    if let Some(inner) = crate::convert::boxed_slice_arg(ty) {
        let inner_expr = map_expr(family, param, inner, quote!(__head_item));
        return quote! {{
            let mut __head_out = ::std::vec::Vec::with_capacity(#place.len());
            for __head_item in #place.iter() {
                __head_out.push(#inner_expr);
            }
            __head_out.into_boxed_slice()
        }};
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Box") {
        let inner_expr = map_expr(family, param, inner, quote!((&**#place)));
        return quote! { ::std::boxed::Box::new(#inner_expr) };
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Vec") {
        let inner_expr = map_expr(family, param, inner, quote!(__head_item));
        return quote! {{
            let mut __head_out = ::std::vec::Vec::with_capacity(#place.len());
            for __head_item in #place.iter() {
                __head_out.push(#inner_expr);
            }
            __head_out
        }};
    }
    if let Some(inner) = crate::convert::wrapper_arg(ty, "Option") {
        let inner_expr = map_expr(family, param, inner, quote!(__head_item));
        return quote! {
            match #place.as_ref() {
                ::core::option::Option::Some(__head_item) => {
                    ::core::option::Option::Some(#inner_expr)
                }
                ::core::option::Option::None => ::core::option::Option::None,
            }
        };
    }
    if let syn::Type::Tuple(t) = ty {
        // Trailing comma so a one-element tuple stays a tuple rather than
        // collapsing to a parenthesized expression.
        let parts = t.elems.iter().enumerate().map(|(i, et)| {
            let idx = Index::from(i);
            map_expr(family, param, et, quote!((&#place.#idx)))
        });
        return quote! { ( #(#parts,)* ) };
    }
    crate::convert::unsupported(ty)
}

/// Whether `ty` mentions `param` anywhere, including inside generic arguments.
fn mentions(ty: &syn::Type, param: &Ident) -> bool {
    fn walk(ts: TokenStream, param: &Ident) -> bool {
        ts.into_iter().any(|tt| match tt {
            TokenTree::Ident(id) => id == *param,
            TokenTree::Group(g) => walk(g.stream(), param),
            _ => false,
        })
    }
    walk(quote!(#ty), param)
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

    let visitors = gen_sat_head_visitors(family, &sat_name, sat);
    let mappers = gen_sat_head_mappers(family, &sat_name, sat);
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
        #visitors
        #mappers
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

/// The rewrite plan when generating for `target`: the master ident becomes
/// `target`, each satellite name becomes `target`'s satellite, and idents in
/// *member-path position* are exempt (see [`Replacements::guards`]).
struct Replacements {
    map: HashMap<String, Ident>,
    /// Idents that open a member path (`Ty::`, `Self::`, any member name).
    /// An ident directly after `<guard>::` names a *variant or associated
    /// item* of that member, not a top-level type — `Ty::Interface` is the
    /// existential variant, which merely shares its name with the `Interface`
    /// satellite — so it is never rewritten, even when it collides with a
    /// satellite name.
    guards: std::collections::HashSet<String>,
}

fn replacements(family: &Family, target: &Ident) -> Replacements {
    let mut map = HashMap::new();
    map.insert(family.master_ident.to_string(), target.clone());
    for sat in &family.satellites {
        map.insert(sat.name.to_string(), satellite_name_for(target, &sat.name));
    }
    let mut guards: std::collections::HashSet<String> =
        family.members.iter().map(|m| m.name.to_string()).collect();
    guards.insert(family.master_ident.to_string());
    guards.insert("Self".to_string());
    Replacements { map, guards }
}

/// Walk `tokens`, replacing any [`Ident`] found in the map — except in
/// member-path position (directly after `<guard>::`) — recursing into groups.
/// A path cannot straddle a group boundary, so per-group tracking suffices.
fn replace_idents(tokens: TokenStream, rep: &Replacements) -> TokenStream {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = Vec::with_capacity(tokens.len());
    for (i, tt) in tokens.iter().enumerate() {
        let mapped = match tt {
            TokenTree::Group(g) => {
                let inner = replace_idents(g.stream(), rep);
                let mut replaced = Group::new(g.delimiter(), inner);
                replaced.set_span(g.span());
                TokenTree::Group(replaced)
            }
            TokenTree::Ident(id) => {
                let guarded = i >= 3
                    && matches!(&tokens[i - 1], TokenTree::Punct(p) if p.as_char() == ':')
                    && matches!(&tokens[i - 2], TokenTree::Punct(p) if p.as_char() == ':')
                    && matches!(&tokens[i - 3], TokenTree::Ident(prev)
                        if rep.guards.contains(&prev.to_string()));
                match (guarded, rep.map.get(&id.to_string())) {
                    (false, Some(target)) => {
                        let mut target = target.clone();
                        target.set_span(id.span());
                        TokenTree::Ident(target)
                    }
                    (true, _) | (false, None) => TokenTree::Ident(id.clone()),
                }
            }
            other => other.clone(),
        };
        out.push(mapped);
    }
    out.into_iter().collect()
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

pub(crate) fn is_doc(attr: &Attribute) -> bool {
    attr.path().is_ident("doc")
}
