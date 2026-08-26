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
//! **Deep, equal-size pairs** additionally get zero-cost borrow conversions and
//! convert by reinterpreting the bytes in place: widening is
//! `Sub::as_<super>(&self) -> &Super` and a pure-move `From`; narrowing adds
//! `TryFrom<&Super> for &Sub` (validate once, reinterpret the borrow) alongside
//! the owned/clone `TryFrom`s. See
//! [`gen_widen_reinterpret`], [`gen_as_upcast`], and [`gen_narrow_reinterpret`].
//!
//! **Shallow-involved pairs** (different-sized members) keep the structural
//! walk. Its owned form is a genuine move — when the two sides share a child
//! type every field is moved wholesale (zero allocation), otherwise only the
//! recursive spine is rebuilt while leaf payloads are moved; the by-reference
//! form clones.
//!
//! ## Reinterpreting under a generic parameter
//!
//! The family is generic (`Ty<N>`, `RuntimeTy<N>`, …), which rules out
//! `mem::transmute` for *owned* values: it demands both sizes be known before
//! monomorphization, and `Sub<N>` "can vary because of N" (E0512). The owned
//! conversions therefore reinterpret through a `ManuallyDrop` + `ptr::read`
//! pair, which defers the size question to the co-located `const` assert that
//! fires per instantiation. References are unaffected — `&Sub<N>` is a thin
//! pointer whichever `N` is chosen — so the borrow conversions still spell it
//! `mem::transmute`.
//!
//! ## Layout reliance (the residual `unsafe` assumption)
//!
//! The reinterpreting conversions are sound only because a member and its sibling
//! are laid out identically wherever they differ. `#[repr(C, u8)]` (enums) and
//! `#[repr(C)]` (satellites) pin the tag and field order, and the per-site
//! `const` assert pins total size + align. Beyond that, reinterpreting a nested
//! position — `Box<[Ty]> -> Box<[RuntimeTy]>`, `(Name, Ty) -> (Name, RuntimeTy)`
//! — rests on a *de-facto*, not language-guaranteed, property: `Box<[T]>` is a
//! `(ptr, len)` fat pointer (and `Vec<T>` a `(ptr, cap, len)` triple) whose
//! layout is independent of `T`, and a `#[repr(Rust)]` tuple's layout is a
//! deterministic function of its fields' sizes + aligns (equal here, since `Ty`
//! and its members are equal size + align). This holds on current `rustc` but
//! is not a stability guarantee; the unit tests plus a
//! `cargo +nightly miri test -p baml_type` run in CI are what guard it against
//! a future layout change.

use std::collections::HashSet;

use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{Fields, GenericArgument, Generics, Index, PathArguments, Type, parse_quote};

use crate::{
    emit::{member_variants, satellite_name_for, with_clone_bounds},
    parse::{Family, MVariant, Satellite},
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
        // A pair of *deep* members that this family builds equal-size (every
        // deep member includes the largest-payload axis) reinterprets in both
        // directions: widening is a pure move (every narrower value is a valid
        // wider one), and narrowing walks *once* to validate then reuses the
        // same bytes — no rebuild, no reallocation. Shallow-involved pairs
        // differ in size (a shallow member omits the wider variants), so they
        // keep the structural rebuild. The equal-size premise is enforced, not
        // assumed: every emitted reinterpretation carries a co-located `const`
        // size+align assert that fails compilation if a future member breaks it.
        if family.members[sub].deep && family.members[sup].deep {
            out.extend(gen_widen_reinterpret(family, sub, sup));
            out.extend(gen_as_upcast(family, sub, sup));
            out.extend(gen_narrow_reinterpret(family, sub, sup));
        } else {
            out.extend(gen_widen(family, sub, sup));
            // Narrowing to a shallow member walks and rebuilds (sizes differ).
            out.extend(gen_narrow(family, sub, sup));
        }
    }

    // Satellite conversions, for the comparable pairs among deep members (the
    // only members that own satellites). Deep satellites are equal-size too, so
    // their widenings reinterpret as well.
    for (sub, sup) in comparable_pairs(family) {
        if family.members[sub].deep && family.members[sup].deep {
            for sat in &family.satellites {
                out.extend(gen_sat(family, sub, sup, sat));
            }
        }
    }

    out
}

/// The named type of family member `idx` at the family's own parameters —
/// `RuntimeTy<N>`. Every conversion holds `N` fixed, so both sides of a pair are
/// spelled at the same arguments.
fn member_ty(family: &Family, idx: usize) -> TokenStream {
    let name = &family.members[idx].name;
    let (_, ty_g, _) = family.generics.split_for_impl();
    quote!(#name #ty_g)
}

/// The same, for member `idx`'s copy of `sat` — `RuntimeFunctionParamTy<N>`.
fn sat_ty(family: &Family, idx: usize, sat: &Satellite) -> TokenStream {
    let name = satellite_name_for(&family.members[idx].name, &sat.name);
    let (_, ty_g, _) = sat.generics.split_for_impl();
    quote!(#name #ty_g)
}

/// `generics` with lifetime `'a` prepended, for the borrow-to-borrow narrowing
/// (`impl<'a, N> TryFrom<&'a Sup<N>> for &'a Sub<N>`). Lifetimes must lead the
/// parameter list, hence the insert at the front.
fn with_lifetime(generics: &Generics) -> Generics {
    let mut out = generics.clone();
    out.params.insert(0, parse_quote!('a));
    out
}

/// Move `value` (typed `from`) into an equivalently-laid-out `to`.
///
/// Stands in for `mem::transmute`, which cannot see that `Sub<N>` and `Sup<N>`
/// have equal size while `N` is still generic (E0512). `ManuallyDrop` suppresses
/// the source's destructor — its heap allocations are handed to the result
/// unchanged — and the read reinterprets those bytes in place.
fn reinterpret_owned(from: &TokenStream, to: &TokenStream, value: TokenStream) -> TokenStream {
    quote! {
        {
            let __src = ::core::mem::ManuallyDrop::<#from>::new(#value);
            // SAFETY: guarded by the co-located `const` size + align assert, and
            // `ManuallyDrop<T>` is `repr(transparent)` over `T`, so `__src` is a
            // correctly-aligned, initialized `#to` for the read below.
            unsafe { ::core::ptr::read((&raw const __src).cast::<#to>()) }
        }
    }
}

/// A `const`-block guard, co-located with every unsafe reinterpretation
/// (widening, narrowing, and satellite), that fails compilation unless `a` and
/// `b` share size *and* alignment. Nothing else checks it: the owned
/// conversions read through a pointer (see [`reinterpret_owned`]) and a
/// *reference* transmute is pointer-sized regardless, so without this a size
/// mismatch would be silent UB; alignment is never checked by `transmute` at
/// all. Being an inline `const`, it inherits the enclosing generics and so fires
/// per instantiation of `N`. It guards *total* size and align only — per-field
/// layout identity additionally rests on the de-facto reliance documented at the
/// module level.
fn layout_assert(a: &TokenStream, b: &TokenStream) -> TokenStream {
    quote! {
        const {
            ::core::assert!(
                ::core::mem::size_of::<#a>() == ::core::mem::size_of::<#b>()
                    && ::core::mem::align_of::<#a>() == ::core::mem::align_of::<#b>(),
                "ty_family: family members must share size and alignment for a \
                 zero-cost transmute conversion",
            );
        };
    }
}

/// `RuntimeTy` → `runtime_ty`.
fn snake_ident(id: &Ident) -> String {
    let s = id.to_string();
    let mut snake = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                snake.push('_');
            }
            snake.push(ch.to_ascii_lowercase());
        } else {
            snake.push(ch);
        }
    }
    snake
}

/// `RuntimeTy` → `as_runtime_ty`: the borrowed-*upcast* accessor for widening
/// `&sub -> &sup`. (Narrowing is the fallible `TryFrom<&sup> for &sub`, not an
/// inherent method.)
fn as_method_name(target: &Ident) -> Ident {
    format_ident!("as_{}", snake_ident(target))
}

/// Widen `sub` → `sup` (both deep, equal-size) by reinterpretation. The owned
/// form is a genuine zero-cost move of the whole value; the by-reference form
/// widens the borrow and clones through it (so it composes with the recursion
/// but is not itself zero-cost — for a zero-cost borrow, the generated
/// `as_<sup>()` accessor is emitted separately by `gen_as_upcast`).
fn gen_widen_reinterpret(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sub_ty = member_ty(family, sub);
    let sup_ty = member_ty(family, sup);
    let assert = layout_assert(&sub_ty, &sup_ty);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let cloneable = with_clone_bounds(&family.generics);
    let (clone_impl_g, _, clone_where_c) = cloneable.split_for_impl();
    // SAFETY (for the `reinterpret_owned` below): `sub` ≤ `sup` in the family
    // order, so every variant of `sub` exists in `sup` at the same
    // `#[repr(C, u8)]` discriminant. Both are equal size + align (the `const`
    // assert), and the recursion travels through `Box`/`Vec`/tuple positions
    // whose in-memory layout depends only on the element's (here equal) size +
    // align, so nested `sub` children are likewise valid `sup` when read back —
    // a de-facto layout guarantee, not a language-level one (see the module
    // docs). Ownership of the heap allocations transfers unchanged; dealloc uses
    // the identical layout.
    let widened = reinterpret_owned(&sub_ty, &sup_ty, quote!(value));
    quote! {
        impl #impl_g ::core::convert::From<#sub_ty> for #sup_ty #where_c {
            fn from(value: #sub_ty) -> Self {
                #assert
                #widened
            }
        }
        impl #clone_impl_g ::core::convert::From<&#sub_ty> for #sup_ty #clone_where_c {
            fn from(value: &#sub_ty) -> Self {
                <#sup_ty as ::core::convert::From<#sub_ty>>::from(value.clone())
            }
        }
    }
}

/// The zero-cost borrowed upcast `sub::as_<sup>(&self) -> &sup`, for a deep,
/// equal-size pair. Used on hot paths that only need to *view* a narrower type
/// through the wider type's algorithms (rendering, subtyping) without cloning.
fn gen_as_upcast(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let sub_ty = member_ty(family, sub);
    let sup_ty = member_ty(family, sup);
    let method = as_method_name(sup_name);
    let assert = layout_assert(&sub_ty, &sup_ty);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let doc = format!(
        " Reinterpret this `{sub_name}` as a `{sup_name}` without copying.\n\n\
          Every `{sub_name}` is a valid `{sup_name}` and the two share an\n\
          identical layout, so this widening is a pure borrow — no allocation,\n\
          no walk. Prefer it over `{sup_name}::from(&self)` when a shared\n\
          reference suffices."
    );
    quote! {
        impl #impl_g #sub_ty #where_c {
            #[doc = #doc]
            #[must_use]
            pub fn #method(&self) -> &#sup_ty {
                #assert
                // SAFETY: as in the owned `From` above, but at the reference
                // level — a shared borrow reinterpreted in place. Sound only
                // because the two types are equal size + align (asserted).
                unsafe { ::core::mem::transmute::<&#sub_ty, &#sup_ty>(self) }
            }
        }
    }
}

// ── Narrowing by validate-then-transmute ─────────────────────────────────────
//
// For a deep, equal-size pair, narrowing `sup -> sub` is a single read-only walk
// that proves every node (at every depth) is representable in `sub`, followed by
// a `transmute` that reuses the already-allocated tree in place. This replaces
// the structural rebuild's per-node `Box::new`/`collect` allocations with zero.
// The walk itself is unavoidable — a single illegal variant nested anywhere
// would make the `transmute` UB — so completeness is the correctness obligation:
// [`gen_validators`] descends through every `Box`/`Vec`/`Option`/tuple/satellite
// position.

/// Emit the three `TryFrom` narrowings for a deep, equal-size pair, all backed
/// by a single validation walk: owned (`Sup -> Sub`, validate then move), by-ref
/// to owned (`&Sup -> Sub`, validate then clone), and the zero-copy
/// borrow-to-borrow (`&Sup -> &Sub`, validate then reinterpret the reference in
/// place). The last recovers the `Not<Sub>` reason on failure — the reason a
/// plain `Option`-returning accessor cannot.
fn gen_narrow_reinterpret(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sub_name = &family.members[sub].name;
    let sup_name = &family.members[sup].name;
    let sub_ty = member_ty(family, sub);
    let sup_ty = member_ty(family, sup);
    let err = format_ident!("Not{}", sub_name);
    let vfn = enum_validator_name(family, sub, sup);
    let validators = gen_validators(family, sub, sup);
    let assert = layout_assert(&sub_ty, &sup_ty);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let cloneable = with_clone_bounds(&family.generics);
    let (clone_impl_g, _, clone_where_c) = cloneable.split_for_impl();
    let borrowed = with_lifetime(&family.generics);
    let (borrow_impl_g, _, borrow_where_c) = borrowed.split_for_impl();
    // SAFETY (both `reinterpret_owned` sites below): the walk proved every node
    // is a variant `sub` also has, and the two types are equal size + align
    // (asserted), so the owned tree is bit-valid as `sub`; its heap allocations
    // are reused unchanged.
    let narrowed = reinterpret_owned(&sup_ty, &sub_ty, quote!(value));
    let narrowed_clone = reinterpret_owned(&sup_ty, &sub_ty, quote!(value.clone()));
    let ref_doc = format!(
        " Narrow `&{sup_name}` to `&{sub_name}` without copying, if every nested\n\
          variant is representable in `{sub_name}`.\n\n\
          Walks once to validate, then reinterprets the borrow in place — no\n\
          allocation. The borrow-preserving analogue of the owned\n\
          `{sub_name}::try_from`; `Err` carries the offending variant."
    );
    quote! {
        #validators

        impl #impl_g ::core::convert::TryFrom<#sup_ty> for #sub_ty #where_c {
            type Error = #err;
            fn try_from(value: #sup_ty) -> ::core::result::Result<Self, Self::Error> {
                #assert
                #vfn(&value)?;
                ::core::result::Result::Ok(#narrowed)
            }
        }

        impl #clone_impl_g ::core::convert::TryFrom<&#sup_ty> for #sub_ty #clone_where_c {
            type Error = #err;
            fn try_from(value: &#sup_ty) -> ::core::result::Result<Self, Self::Error> {
                #assert
                #vfn(value)?;
                // A borrow can't be moved out, so clone the validated tree and
                // reinterpret the owned copy.
                ::core::result::Result::Ok(#narrowed_clone)
            }
        }

        #[doc = #ref_doc]
        impl #borrow_impl_g ::core::convert::TryFrom<&'a #sup_ty> for &'a #sub_ty #borrow_where_c {
            type Error = #err;
            fn try_from(value: &'a #sup_ty) -> ::core::result::Result<Self, Self::Error> {
                #assert
                #vfn(value)?;
                // SAFETY: the walk proved every node is a valid `#sub_name`, and
                // the two types are equal size + align (asserted). A shared borrow
                // reinterpreted in place, lifetime preserved.
                ::core::result::Result::Ok(unsafe {
                    ::core::mem::transmute::<&#sup_ty, &#sub_ty>(value)
                })
            }
        }
    }
}

/// The module-private validation functions for a pair: one for the enum and one
/// per satellite. Each returns `Ok(())` iff its argument is representable in
/// `sub` at every depth; they recurse into one another to cover the whole tree.
fn gen_validators(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sup_ty = member_ty(family, sup);
    let err = format_ident!("Not{}", &family.members[sub].name);
    let vfn = enum_validator_name(family, sub, sup);
    let body = validate_body(family, sub, sup);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let reachable = reachable_satellites(family, sub);
    let sats = family
        .satellites
        .iter()
        .filter(|sat| reachable.contains(&sat.name.to_string()))
        .map(|sat| gen_sat_validator(family, sub, sup, sat));
    quote! {
        fn #vfn #impl_g (value: &#sup_ty) -> ::core::result::Result<(), #err> #where_c {
            #body
        }
        #(#sats)*
    }
}

fn validate_body(family: &Family, sub: usize, sup: usize) -> TokenStream {
    let sup_name = &family.members[sup].name;
    let sub_includes = &family.members[sub].includes;
    let arms = member_variants(family, &family.members[sup]).map(|v| {
        validate_arm(
            family,
            sub,
            sup,
            sup_name,
            v,
            sub_includes.contains(&v.axis),
        )
    });
    quote! { match value { #(#arms),* } }
}

/// One match arm of an enum validator. A variant absent from `sub` fails by
/// name; a present one binds just its recursion-bearing fields and checks each.
fn validate_arm(
    family: &Family,
    sub: usize,
    sup: usize,
    sup_name: &Ident,
    v: &MVariant,
    in_sub: bool,
) -> TokenStream {
    let id = &v.ident;
    if !in_sub {
        let name = id.to_string();
        let err = format_ident!("Not{}", &family.members[sub].name);
        let skip = match &v.fields {
            Fields::Unit => quote!(),
            Fields::Unnamed(_) => quote!((..)),
            Fields::Named(_) => quote!({ .. }),
        };
        return quote! {
            #sup_name::#id #skip => ::core::result::Result::Err(#err { variant: #name })
        };
    }
    match &v.fields {
        Fields::Unit => quote! { #sup_name::#id => ::core::result::Result::Ok(()) },
        Fields::Named(n) => {
            let mut binds = Vec::new();
            let mut checks = Vec::new();
            for f in &n.named {
                if contains_recursion(family, &f.ty) {
                    let fid = f.ident.as_ref().unwrap();
                    binds.push(quote!(#fid));
                    checks.push(validate_expr(family, sub, sup, &f.ty, quote!(#fid)));
                }
            }
            quote! {
                #sup_name::#id { #(#binds,)* .. } => { #(#checks?;)* ::core::result::Result::Ok(()) }
            }
        }
        Fields::Unnamed(u) => {
            let mut pats = Vec::new();
            let mut checks = Vec::new();
            for (i, f) in u.unnamed.iter().enumerate() {
                if contains_recursion(family, &f.ty) {
                    let b = format_ident!("f{}", i);
                    checks.push(validate_expr(family, sub, sup, &f.ty, quote!(#b)));
                    pats.push(quote!(#b));
                } else {
                    pats.push(quote!(_));
                }
            }
            quote! {
                #sup_name::#id ( #(#pats),* ) => { #(#checks?;)* ::core::result::Result::Ok(()) }
            }
        }
    }
}

/// A `Result<(), NotSub>` expression checking that the value behind `binding` (a
/// reference to a value of master-type `ty`) is representable in `sub`. Leaves
/// are trivially `Ok`; recursion-bearing shapes descend, terminating in a call
/// to the relevant validator.
fn validate_expr(
    family: &Family,
    sub: usize,
    sup: usize,
    ty: &Type,
    binding: TokenStream,
) -> TokenStream {
    if !contains_recursion(family, ty) {
        return quote! { ::core::result::Result::Ok(()) };
    }
    if let Some(id) = path_head(ty) {
        if *id == family.master_ident {
            let f = enum_validator_name(family, sub, sup);
            return quote! { #f(#binding) };
        }
        if let Some(sat) = family.satellites.iter().find(|s| s.name == *id) {
            let f = sat_validator_name(family, sub, sup, sat);
            return quote! { #f(#binding) };
        }
    }
    if let Some(inner) = boxed_slice_arg(ty) {
        let e = validate_expr(family, sub, sup, inner, quote!(__v));
        return quote! { #binding.iter().try_for_each(|__v| #e) };
    }
    if let Some(inner) = wrapper_arg(ty, "Box") {
        return validate_expr(family, sub, sup, inner, quote!(&**#binding));
    }
    if let Some(inner) = wrapper_arg(ty, "Vec") {
        let e = validate_expr(family, sub, sup, inner, quote!(__v));
        return quote! { #binding.iter().try_for_each(|__v| #e) };
    }
    if let Some(inner) = wrapper_arg(ty, "Option") {
        let e = validate_expr(family, sub, sup, inner, quote!(__v));
        return quote! {
            match #binding {
                ::core::option::Option::Some(__v) => #e,
                ::core::option::Option::None => ::core::result::Result::Ok(()),
            }
        };
    }
    if let Type::Tuple(t) = ty {
        let checks = t
            .elems
            .iter()
            .enumerate()
            .filter(|(_, et)| contains_recursion(family, et))
            .map(|(i, et)| {
                let idx = Index::from(i);
                validate_expr(family, sub, sup, et, quote!(&#binding.#idx))
            });
        return quote! { { #(#checks?;)* ::core::result::Result::Ok(()) } };
    }
    unsupported(ty)
}

/// A satellite's validator: check every recursion-bearing field, then `Ok`. The
/// field binding is parenthesized so a `Vec`/`Box` field composes as
/// `(&value.f).iter()`, not `&(value.f.iter())`.
fn gen_sat_validator(family: &Family, sub: usize, sup: usize, sat: &Satellite) -> TokenStream {
    let sup_sat = sat_ty(family, sup, sat);
    let err = format_ident!("Not{}", &family.members[sub].name);
    let vfn = sat_validator_name(family, sub, sup, sat);
    let (impl_g, _, where_c) = sat.generics.split_for_impl();
    let checks = sat
        .fields
        .iter()
        .filter(|f| contains_recursion(family, &f.ty))
        .map(|f| {
            let fid = f.ident.as_ref().unwrap();
            validate_expr(family, sub, sup, &f.ty, quote!((&value.#fid)))
        });
    quote! {
        fn #vfn #impl_g (value: &#sup_sat) -> ::core::result::Result<(), #err> #where_c {
            #(#checks?;)*
            ::core::result::Result::Ok(())
        }
    }
}

/// Satellite names transitively reachable when validating a value as `sub`: only
/// variants present in `sub` are walked, so a satellite reached only through an
/// excluded variant (e.g. `Interface`, reachable solely via the `typevar`
/// `AssociatedTypeProjection`) needs no validator. Emitting one anyway would be
/// dead code — this prunes it.
fn reachable_satellites(family: &Family, sub: usize) -> HashSet<String> {
    let sat_names: HashSet<String> = family
        .satellites
        .iter()
        .map(|s| s.name.to_string())
        .collect();
    let collect = |ty: &dyn quote::ToTokens, out: &mut Vec<String>| {
        fn walk(ts: TokenStream, names: &HashSet<String>, out: &mut Vec<String>) {
            for tt in ts {
                match tt {
                    TokenTree::Ident(id) if names.contains(&id.to_string()) => {
                        out.push(id.to_string());
                    }
                    TokenTree::Group(g) => walk(g.stream(), names, out),
                    _ => {}
                }
            }
        }
        walk(quote!(#ty), &sat_names, out);
    };

    let mut work: Vec<String> = Vec::new();
    for v in &family.variants {
        if family.members[sub].includes.contains(&v.axis) {
            collect(&v.fields, &mut work);
        }
    }
    let mut reached = HashSet::new();
    while let Some(name) = work.pop() {
        if reached.insert(name.clone())
            && let Some(sat) = family.satellites.iter().find(|s| s.name == name)
        {
            for f in &sat.fields {
                collect(&f.ty, &mut work);
            }
        }
    }
    reached
}

/// `Ty` narrowing to `RuntimeTy` → `__valid_ty_as_runtime_ty`. Module-private and
/// unique per ordered pair.
fn enum_validator_name(family: &Family, sub: usize, sup: usize) -> Ident {
    format_ident!(
        "__valid_{}_as_{}",
        snake_ident(&family.members[sup].name),
        snake_ident(&family.members[sub].name),
    )
}

/// The satellite analogue, e.g. `__valid_function_param_ty_as_runtime_function_param_ty`.
fn sat_validator_name(family: &Family, sub: usize, sup: usize, sat: &Satellite) -> Ident {
    let sup_sat = satellite_name_for(&family.members[sup].name, &sat.name);
    let sub_sat = satellite_name_for(&family.members[sub].name, &sat.name);
    format_ident!(
        "__valid_{}_as_{}",
        snake_ident(&sup_sat),
        snake_ident(&sub_sat)
    )
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
    let sub_ty = member_ty(family, sub);
    let sup_ty = member_ty(family, sup);
    let by_ref = widen_body(family, sub, sup, Own::Ref);
    let owned = widen_body(family, sub, sup, Own::Owned);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let cloneable = with_clone_bounds(&family.generics);
    let (clone_impl_g, _, clone_where_c) = cloneable.split_for_impl();
    quote! {
        impl #clone_impl_g ::core::convert::From<&#sub_ty> for #sup_ty #clone_where_c {
            fn from(value: &#sub_ty) -> Self { #by_ref }
        }
        impl #impl_g ::core::convert::From<#sub_ty> for #sup_ty #where_c {
            fn from(value: #sub_ty) -> Self { #owned }
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
    let sub_ty = member_ty(family, sub);
    let sup_ty = member_ty(family, sup);
    let err = format_ident!("Not{}", &family.members[sub].name);
    let by_ref = narrow_body(family, sub, sup, Own::Ref);
    let owned = narrow_body(family, sub, sup, Own::Owned);
    let (impl_g, _, where_c) = family.generics.split_for_impl();
    let cloneable = with_clone_bounds(&family.generics);
    let (clone_impl_g, _, clone_where_c) = cloneable.split_for_impl();
    quote! {
        impl #clone_impl_g ::core::convert::TryFrom<&#sup_ty> for #sub_ty #clone_where_c {
            type Error = #err;
            fn try_from(value: &#sup_ty) -> ::core::result::Result<Self, Self::Error> { #by_ref }
        }
        impl #impl_g ::core::convert::TryFrom<#sup_ty> for #sub_ty #where_c {
            type Error = #err;
            fn try_from(value: #sup_ty) -> ::core::result::Result<Self, Self::Error> { #owned }
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
    if let Some(inner) = boxed_slice_arg(ty) {
        let iter = iter(cx.own);
        let e = widen_expr(cx, inner, quote!(__v));
        return quote! { #binding.#iter().map(|__v| #e).collect() };
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
    if let Some(inner) = boxed_slice_arg(ty) {
        let iter = iter(cx.own);
        let e = narrow_expr(cx, inner, quote!(__v));
        return quote! {
            #binding.#iter().map(|__v| #e).collect::<::core::result::Result<::std::boxed::Box<[_]>, _>>()
        };
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
    let id = path_head(ty)?;
    let child_name = &cx.family.members[child].name;
    if *id == cx.family.master_ident {
        Some(child_name.clone())
    } else {
        let sat = cx.family.satellites.iter().find(|s| s.name == *id)?;
        Some(satellite_name_for(child_name, &sat.name))
    }
}

// ── Satellite conversions ────────────────────────────────────────────────────

fn gen_sat(family: &Family, sub: usize, sup: usize, sat: &Satellite) -> TokenStream {
    let sub_sat = sat_ty(family, sub, sat);
    let sup_sat = sat_ty(family, sup, sat);
    let err = format_ident!("Not{}", &family.members[sub].name);
    let assert = layout_assert(&sub_sat, &sup_sat);
    let narrow_ref = sat_narrow_body(family, sub, sup, sat, Own::Ref);
    let narrow_owned = sat_narrow_body(family, sub, sup, sat, Own::Owned);
    let (impl_g, _, where_c) = sat.generics.split_for_impl();
    let cloneable = with_clone_bounds(&sat.generics);
    let (clone_impl_g, _, clone_where_c) = cloneable.split_for_impl();
    // SAFETY (for `reinterpret_owned`): a satellite differs across the family
    // only in its recursive field's member type (`ty: Ty<N>` vs
    // `ty: RuntimeTy<N>`, …), which are equal-layout; `#[repr(C)]` pins the
    // field order identically, and the pair is equal size + align (asserted).
    // So the bytes of the narrower satellite are a valid wider one.
    let widened = reinterpret_owned(&sub_sat, &sup_sat, quote!(value));
    quote! {
        impl #clone_impl_g ::core::convert::From<&#sub_sat> for #sup_sat #clone_where_c {
            fn from(value: &#sub_sat) -> Self {
                <#sup_sat as ::core::convert::From<#sub_sat>>::from(value.clone())
            }
        }
        impl #impl_g ::core::convert::From<#sub_sat> for #sup_sat #where_c {
            fn from(value: #sub_sat) -> Self {
                #assert
                #widened
            }
        }
        impl #clone_impl_g ::core::convert::TryFrom<&#sup_sat> for #sub_sat #clone_where_c {
            type Error = #err;
            fn try_from(value: &#sup_sat) -> ::core::result::Result<Self, Self::Error> { #narrow_ref }
        }
        impl #impl_g ::core::convert::TryFrom<#sup_sat> for #sub_sat #where_c {
            type Error = #err;
            fn try_from(value: #sup_sat) -> ::core::result::Result<Self, Self::Error> { #narrow_owned }
        }
    }
}

// Satellites live on deep members, so each side's child is the member itself.
fn sat_narrow_body(
    family: &Family,
    sub: usize,
    sup: usize,
    sat: &Satellite,
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
        // Parenthesized: the by-ref binding flows into method chains like
        // `#binding.iter()` (for a `Vec<Self>`/`Option<Self>` satellite field), and
        // a bare `&value.#fid.iter()` would bind the `&` to the whole chain
        // (`&(value.#fid.iter()…)`) rather than the field.
        Own::Ref => quote!((&value.#fid)),
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

/// The head ident of a single-segment path type, ignoring any generic arguments
/// (`Ty<N>` → `Ty`, `Box<Ty<N>>` → `Box`).
///
/// Callers use it only to ask "is this position a family head?", and the answer
/// turns on the ident alone: a family head's arguments are the family's own
/// parameters, which every conversion holds fixed across both sides. Should a
/// head ever be written at some *other* argument (`Ty<Other>`), the emitted
/// `Target::from(..)` simply infers `Target<Other>` — still the type the
/// rewritten field holds — so the position stays well-typed rather than
/// silently converting to the wrong one.
pub(crate) fn path_head(ty: &Type) -> Option<&Ident> {
    let Type::Path(p) = ty else { return None };
    if p.qself.is_some() || p.path.segments.len() != 1 {
        return None;
    }
    Some(&p.path.segments[0].ident)
}

/// If `ty` is `Box<[Inner]>` (a boxed slice — the family's frozen-sequence
/// shape), the `Inner` type. Checked before the plain `Box` wrapper wherever
/// both could match: `wrapper_arg(_, "Box")` would surface the bare slice
/// type, which no walker can traverse.
pub(crate) fn boxed_slice_arg(ty: &Type) -> Option<&Type> {
    match wrapper_arg(ty, "Box")? {
        Type::Slice(slice) => Some(&slice.elem),
        _ => None,
    }
}

/// If `ty` is `Wrapper<Inner>` (one type arg), the `Inner` type.
pub(crate) fn wrapper_arg<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
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

pub(crate) fn unsupported(ty: &Type) -> TokenStream {
    let shown = quote!(#ty).to_string();
    let msg = format!("ty_family: unsupported recursive field shape `{shown}`");
    quote! { ::core::compile_error!(#msg) }
}
