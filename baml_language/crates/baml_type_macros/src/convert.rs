//! Conversion-matrix code generation for `ty_family!`.
//!
//! For every pair of members `Sub ≤ Super` in the family's product order, this
//! emits an infallible widening (`From<Sub>`/`From<&Sub>` for `Super`) and a
//! fallible narrowing (`TryFrom<Super>`/`TryFrom<&Super>` for `Sub`), plus one
//! `Not<Member>` error type per narrowing target.
//!
//! `Sub ≤ Super` iff `includes(Sub) ⊆ includes(Super)` **and**
//! `includes(child(Sub)) ⊆ includes(child(Super))` — a product order over the
//! top-level variant set and the (deep) child type.
//!
//! Both an owned and a by-reference form are generated for each direction. The
//! **owned** form is a genuine move: when the two sides share a child type
//! (`Sub` and `Super` differ only at the top level) every field is moved
//! wholesale — zero allocation — and otherwise only the recursive spine is
//! rebuilt while leaf payloads are moved. The **by-reference** form clones.

use std::collections::HashSet;

use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{Fields, GenericArgument, Index, PathArguments, Type};

use crate::{
    emit::{member_variants, satellite_name_for},
    parse::{Family, MVariant},
};

pub(crate) fn emit_conversions(family: &Family) -> TokenStream {
    let mut out = TokenStream::new();

    // One error type per narrowing target (every member that is some pair's
    // sub), plus the `From<NotChild>` bridges shallow members need so that `?`
    // composes when their narrowing recurses through a different child type.
    for (i, member) in family.members.iter().enumerate() {
        if (0..family.members.len()).any(|j| j != i && is_le(family, i, j)) {
            out.extend(gen_error(&member.name));
        }
        if member.child != i {
            out.extend(gen_error_bridge(family, member.child, i));
        }
    }

    for (sub, sup) in comparable_pairs(family) {
        out.extend(gen_widen(family, sub, sup));
        out.extend(gen_narrow(family, sub, sup));
    }

    // Satellite conversions, for the comparable pairs among deep members (the
    // only members that own satellites).
    for (sub, sup) in comparable_pairs(family) {
        if family.members[sub].deep && family.members[sup].deep {
            for sat in &family.satellites {
                let sf = SatFields {
                    name: sat.name.clone(),
                    fields: sat.fields.iter().cloned().collect(),
                };
                out.extend(gen_sat(family, sub, sup, &sf));
            }
        }
    }

    out
}

// ── Product order ────────────────────────────────────────────────────────────

fn subset(a: &[usize], b: &[usize]) -> bool {
    a.iter().all(|x| b.contains(x))
}

/// `a ≤ b` (a is a sub-member of b): top-level include-sets and child
/// include-sets are both subsets.
fn is_le(family: &Family, a: usize, b: usize) -> bool {
    let (ma, mb) = (&family.members[a], &family.members[b]);
    subset(&ma.includes, &mb.includes)
        && subset(
            &family.members[ma.child].includes,
            &family.members[mb.child].includes,
        )
}

/// All ordered pairs `(sub, super)` with `sub < super`.
fn comparable_pairs(family: &Family) -> Vec<(usize, usize)> {
    let n = family.members.len();
    let mut pairs = Vec::new();
    for sub in 0..n {
        for sup in 0..n {
            if sub != sup && is_le(family, sub, sup) {
                pairs.push((sub, sup));
            }
        }
    }
    pairs
}

// ── Conversion context ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Own {
    Ref,
    Owned,
}

/// Per-impl context: the child member feeding nested positions on each side,
/// and whether this is the owned (move) or by-reference (clone) form.
struct Cx<'a> {
    family: &'a Family,
    sub_child: usize,
    sup_child: usize,
    own: Own,
}

impl Cx<'_> {
    /// `Sub` and `Super` share a child type, so every field has the same type
    /// on both sides and can be moved/cloned wholesale.
    fn shallow(&self) -> bool {
        self.sub_child == self.sup_child
    }
}

// ── Enum conversions ─────────────────────────────────────────────────────────

fn gen_widen(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let by_ref = widen_body(family, sub, sup, Own::Ref);
    let owned = widen_body(family, sub, sup, Own::Owned);
    quote! {
        impl ::core::convert::From<&#sub_name> for #sup_name {
            fn from(value: &#sub_name) -> Self { #by_ref }
        }
        impl ::core::convert::From<#sub_name> for #sup_name {
            fn from(value: #sub_name) -> Self { #owned }
        }
    }
}

fn widen_body(family: &Family, sub: usize, sup: usize, own: Own) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let cx = Cx {
        family,
        sub_child: family.members[sub].child,
        sup_child: family.members[sup].child,
        own,
    };
    let arms = member_variants(family, &family.members[sub])
        .map(|v| widen_arm(&cx, sub_name, sup_name, v));
    quote! { match value { #(#arms),* } }
}

fn gen_narrow(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let err = format_ident!("Not{}", sub_name);
    let by_ref = narrow_body(family, sub, sup, Own::Ref);
    let owned = narrow_body(family, sub, sup, Own::Owned);
    quote! {
        impl ::core::convert::TryFrom<&#sup_name> for #sub_name {
            type Error = #err;
            fn try_from(value: &#sup_name) -> ::core::result::Result<Self, Self::Error> { #by_ref }
        }
        impl ::core::convert::TryFrom<#sup_name> for #sub_name {
            type Error = #err;
            fn try_from(value: #sup_name) -> ::core::result::Result<Self, Self::Error> { #owned }
        }
    }
}

fn narrow_body(family: &Family, sub: usize, sup: usize, own: Own) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let err = format_ident!("Not{}", sub_name);
    let sub_includes = &family.members[sub].includes;
    let cx = Cx {
        family,
        sub_child: family.members[sub].child,
        sup_child: family.members[sup].child,
        own,
    };
    let arms = member_variants(family, &family.members[sup]).map(|v| {
        narrow_arm(
            &cx,
            sub_name,
            sup_name,
            &err,
            v,
            sub_includes.contains(&v.axis),
        )
    });
    quote! { match value { #(#arms),* } }
}

fn widen_arm(cx: &Cx, sub: &Ident, sup: &Ident, v: &MVariant) -> TokenStream {
    let id = &v.ident;
    let (pat, body) = arm_fields(v, |fty, bind| field_widen(cx, fty, bind));
    quote! { #sub::#id #pat => #sup::#id #body }
}

fn narrow_arm(
    cx: &Cx,
    sub: &Ident,
    sup: &Ident,
    err: &Ident,
    v: &MVariant,
    in_sub: bool,
) -> TokenStream {
    let id = &v.ident;
    if !in_sub {
        let name = id.to_string();
        let skip = match &v.fields {
            Fields::Unit => quote!(),
            Fields::Unnamed(_) => quote!((..)),
            Fields::Named(_) => quote!({ .. }),
        };
        return quote! {
            #sup::#id #skip => ::core::result::Result::Err(#err { variant: #name })
        };
    }
    let (pat, body) = arm_fields(v, |fty, bind| field_narrow(cx, fty, bind));
    quote! { #sup::#id #pat => ::core::result::Result::Ok(#sub::#id #body) }
}

/// Build a `(pattern, constructor)` token pair for a variant, applying `conv`
/// to each field's `(type, binding-ident)`.
fn arm_fields(
    v: &MVariant,
    mut conv: impl FnMut(&Type, TokenStream) -> TokenStream,
) -> (TokenStream, TokenStream) {
    match &v.fields {
        Fields::Unit => (quote!(), quote!()),
        Fields::Unnamed(u) => {
            let binds: Vec<Ident> = (0..u.unnamed.len())
                .map(|i| format_ident!("f{}", i))
                .collect();
            let convs: Vec<TokenStream> = u
                .unnamed
                .iter()
                .zip(&binds)
                .map(|(f, b)| conv(&f.ty, quote!(#b)))
                .collect();
            (quote!( ( #(#binds),* ) ), quote!( ( #(#convs),* ) ))
        }
        Fields::Named(n) => {
            let names: Vec<&Ident> = n.named.iter().filter_map(|f| f.ident.as_ref()).collect();
            let convs: Vec<TokenStream> = n
                .named
                .iter()
                .map(|f| {
                    let fid = f.ident.as_ref().unwrap();
                    let e = conv(&f.ty, quote!(#fid));
                    quote!( #fid: #e )
                })
                .collect();
            (quote!( { #(#names),* } ), quote!( { #(#convs),* } ))
        }
    }
}

// ── Per-field conversion ─────────────────────────────────────────────────────

fn field_widen(cx: &Cx, ty: &Type, binding: TokenStream) -> TokenStream {
    if cx.shallow() || !contains_recursion(cx.family, ty) {
        keep(cx.own, binding)
    } else {
        widen_expr(cx, ty, binding)
    }
}

fn field_narrow(cx: &Cx, ty: &Type, binding: TokenStream) -> TokenStream {
    if cx.shallow() || !contains_recursion(cx.family, ty) {
        keep(cx.own, binding)
    } else {
        let e = narrow_expr(cx, ty, binding);
        quote! { (#e)? }
    }
}

/// Move (owned) or clone (by-ref) a value of identical type.
fn keep(own: Own, binding: TokenStream) -> TokenStream {
    match own {
        Own::Ref => quote! { (#binding).clone() },
        Own::Owned => binding,
    }
}

/// Convert a recursion-bearing field to the widened (owned) value. `binding`
/// borrows the source field (by-ref) or holds it (owned).
fn widen_expr(cx: &Cx, ty: &Type, binding: TokenStream) -> TokenStream {
    if !contains_recursion(cx.family, ty) {
        return keep(cx.own, binding);
    }
    if let Some(target) = terminal_target(cx, ty, cx.sup_child) {
        return quote! { #target::from(#binding) };
    }
    if let Some(inner) = wrapper_arg(ty, "Box") {
        let e = widen_expr(cx, inner, deref(cx.own, &binding));
        return quote! { ::std::boxed::Box::new(#e) };
    }
    if let Some(inner) = wrapper_arg(ty, "Vec") {
        let iter = iter(cx.own);
        let e = widen_expr(cx, inner, quote!(__v));
        return quote! { #binding.#iter().map(|__v| #e).collect() };
    }
    if let Some(inner) = wrapper_arg(ty, "Option") {
        let opt = opt_map(cx.own);
        let e = widen_expr(cx, inner, quote!(__v));
        return quote! { #binding #opt.map(|__v| #e) };
    }
    if let Type::Tuple(t) = ty {
        let parts = t.elems.iter().enumerate().map(|(i, et)| {
            let idx = Index::from(i);
            widen_expr(cx, et, tuple_elem(cx.own, &binding, &idx))
        });
        return quote! { ( #(#parts),* ) };
    }
    unsupported(ty)
}

/// Convert a recursion-bearing field to a `Result<owned, _>`.
fn narrow_expr(cx: &Cx, ty: &Type, binding: TokenStream) -> TokenStream {
    if !contains_recursion(cx.family, ty) {
        let kept = keep(cx.own, binding);
        return quote! { ::core::result::Result::Ok(#kept) };
    }
    if let Some(target) = terminal_target(cx, ty, cx.sub_child) {
        return quote! { #target::try_from(#binding) };
    }
    if let Some(inner) = wrapper_arg(ty, "Box") {
        let e = narrow_expr(cx, inner, deref(cx.own, &binding));
        return quote! { (#e).map(::std::boxed::Box::new) };
    }
    if let Some(inner) = wrapper_arg(ty, "Vec") {
        let iter = iter(cx.own);
        let e = narrow_expr(cx, inner, quote!(__v));
        return quote! {
            #binding.#iter().map(|__v| #e).collect::<::core::result::Result<::std::vec::Vec<_>, _>>()
        };
    }
    if let Some(inner) = wrapper_arg(ty, "Option") {
        let opt = opt_map(cx.own);
        let e = narrow_expr(cx, inner, quote!(__v));
        return quote! { #binding #opt.map(|__v| #e).transpose() };
    }
    if let Type::Tuple(t) = ty {
        return narrow_tuple(cx, t, &binding);
    }
    unsupported(ty)
}

/// Narrow a tuple field carrying exactly one recursion-bearing element
/// (e.g. `(Name, Ty)`): convert that element, keep the rest.
fn narrow_tuple(cx: &Cx, t: &syn::TypeTuple, binding: &TokenStream) -> TokenStream {
    let rec: Vec<usize> = t
        .elems
        .iter()
        .enumerate()
        .filter(|(_, et)| contains_recursion(cx.family, et))
        .map(|(i, _)| i)
        .collect();
    let [r] = rec[..] else {
        return quote! {
            ::core::compile_error!("ty_family: tuple field must have exactly one convertible element")
        };
    };
    let rec_idx = Index::from(r);
    let rec_expr = narrow_expr(cx, &t.elems[r], tuple_elem(cx.own, binding, &rec_idx));
    let built = t.elems.iter().enumerate().map(|(i, _)| {
        if i == r {
            quote!(__t)
        } else {
            let idx = Index::from(i);
            keep(cx.own, tuple_elem(cx.own, binding, &idx))
        }
    });
    quote! { (#rec_expr).map(|__t| ( #(#built),* )) }
}

// ── Ownership-dependent accessors ────────────────────────────────────────────

fn deref(own: Own, binding: &TokenStream) -> TokenStream {
    match own {
        Own::Ref => quote!(&**#binding),
        Own::Owned => quote!(*#binding),
    }
}

fn iter(own: Own) -> TokenStream {
    match own {
        Own::Ref => quote!(iter),
        Own::Owned => quote!(into_iter),
    }
}

fn opt_map(own: Own) -> TokenStream {
    match own {
        Own::Ref => quote!(.as_ref()),
        Own::Owned => quote!(),
    }
}

fn tuple_elem(own: Own, binding: &TokenStream, idx: &Index) -> TokenStream {
    match own {
        Own::Ref => quote!(&#binding.#idx),
        Own::Owned => quote!(#binding.#idx),
    }
}

/// If `ty` is exactly the master ident or a satellite name, the terminal
/// conversion target on `child` (an enum member or its satellite).
fn terminal_target(cx: &Cx, ty: &Type, child: usize) -> Option<Ident> {
    let id = single_ident(ty)?;
    let child_name = &cx.family.members[child].name;
    if *id == cx.family.master_ident {
        Some(child_name.clone())
    } else {
        let sat = cx.family.satellites.iter().find(|s| s.name == *id)?;
        Some(satellite_name_for(child_name, &sat.name))
    }
}

// ── Satellite conversions ────────────────────────────────────────────────────

/// A satellite reduced to the data the converter needs.
struct SatFields {
    name: Ident,
    fields: Vec<syn::Field>,
}

fn gen_sat(family: &Family, sub: usize, sup: usize, sat: &SatFields) -> TokenStream {
    let sub_sat = satellite_name_for(&family.members[sub].name, &sat.name);
    let sup_sat = satellite_name_for(&family.members[sup].name, &sat.name);
    let err = format_ident!("Not{}", &family.members[sub].name);
    let widen_ref = sat_widen_body(family, sub, sup, sat, Own::Ref);
    let widen_owned = sat_widen_body(family, sub, sup, sat, Own::Owned);
    let narrow_ref = sat_narrow_body(family, sub, sup, sat, Own::Ref);
    let narrow_owned = sat_narrow_body(family, sub, sup, sat, Own::Owned);
    quote! {
        impl ::core::convert::From<&#sub_sat> for #sup_sat {
            fn from(value: &#sub_sat) -> Self { #widen_ref }
        }
        impl ::core::convert::From<#sub_sat> for #sup_sat {
            fn from(value: #sub_sat) -> Self { #widen_owned }
        }
        impl ::core::convert::TryFrom<&#sup_sat> for #sub_sat {
            type Error = #err;
            fn try_from(value: &#sup_sat) -> ::core::result::Result<Self, Self::Error> { #narrow_ref }
        }
        impl ::core::convert::TryFrom<#sup_sat> for #sub_sat {
            type Error = #err;
            fn try_from(value: #sup_sat) -> ::core::result::Result<Self, Self::Error> { #narrow_owned }
        }
    }
}

// Satellites live on deep members, so each side's child is the member itself.
fn sat_widen_body(
    family: &Family,
    sub: usize,
    sup: usize,
    sat: &SatFields,
    own: Own,
) -> TokenStream {
    let sup_sat = satellite_name_for(&family.members[sup].name, &sat.name);
    let cx = Cx {
        family,
        sub_child: sub,
        sup_child: sup,
        own,
    };
    let fields = sat.fields.iter().map(|f| {
        let fid = f.ident.as_ref().unwrap();
        let e = field_widen(&cx, &f.ty, value_field(own, fid));
        quote!( #fid: #e )
    });
    quote! { #sup_sat { #(#fields),* } }
}

fn sat_narrow_body(
    family: &Family,
    sub: usize,
    sup: usize,
    sat: &SatFields,
    own: Own,
) -> TokenStream {
    let sub_sat = satellite_name_for(&family.members[sub].name, &sat.name);
    let cx = Cx {
        family,
        sub_child: sub,
        sup_child: sup,
        own,
    };
    let fields = sat.fields.iter().map(|f| {
        let fid = f.ident.as_ref().unwrap();
        let e = field_narrow(&cx, &f.ty, value_field(own, fid));
        quote!( #fid: #e )
    });
    quote! { ::core::result::Result::Ok(#sub_sat { #(#fields),* }) }
}

fn value_field(own: Own, fid: &Ident) -> TokenStream {
    match own {
        Own::Ref => quote!(&value.#fid),
        Own::Owned => quote!(value.#fid),
    }
}

// ── Error types ──────────────────────────────────────────────────────────────

fn gen_error(member: &Ident) -> TokenStream {
    let err = format_ident!("Not{}", member);
    let msg = format!("`{{}}` is not a valid `{member}`");
    let doc = format!(
        " Error returned when a value cannot narrow to [`{member}`]: it records,\
         \n by name, the variant that has no representation in `{member}`."
    );
    quote! {
        #[doc = #doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct #err {
            pub variant: &'static str,
        }

        impl ::core::fmt::Display for #err {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::write!(f, #msg, self.variant)
            }
        }

        impl ::std::error::Error for #err {}
    }
}

/// A shallow member's narrowing recurses through its (different) child type, so
/// `?` must lift the child's error into the member's. Both carry only a
/// `variant` name, so the bridge just forwards it.
fn gen_error_bridge(family: &Family, child: usize, member: usize) -> TokenStream {
    let from = format_ident!("Not{}", &family.members[child].name);
    let to = format_ident!("Not{}", &family.members[member].name);
    quote! {
        impl ::core::convert::From<#from> for #to {
            fn from(e: #from) -> Self {
                #to { variant: e.variant }
            }
        }
    }
}

// ── syn helpers ──────────────────────────────────────────────────────────────

/// `ty` as a bare single-segment ident with no generic args.
fn single_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(p) = ty else { return None };
    if p.qself.is_some() || p.path.segments.len() != 1 {
        return None;
    }
    let seg = &p.path.segments[0];
    matches!(seg.arguments, PathArguments::None).then_some(&seg.ident)
}

/// If `ty` is `Wrapper<Inner>` (one type arg), the `Inner` type.
fn wrapper_arg<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// Whether `ty` mentions the master ident or any satellite name.
fn contains_recursion(family: &Family, ty: &Type) -> bool {
    fn walk(ts: TokenStream, names: &HashSet<String>) -> bool {
        ts.into_iter().any(|tt| match tt {
            TokenTree::Ident(id) => names.contains(&id.to_string()),
            TokenTree::Group(g) => walk(g.stream(), names),
            _ => false,
        })
    }
    let mut names = HashSet::new();
    names.insert(family.master_ident.to_string());
    for s in &family.satellites {
        names.insert(s.name.to_string());
    }
    walk(quote!(#ty), &names)
}

fn unsupported(ty: &Type) -> TokenStream {
    let shown = quote!(#ty).to_string();
    let msg = format!("ty_family: unsupported recursive field shape `{shown}`");
    quote! { ::core::compile_error!(#msg) }
}
