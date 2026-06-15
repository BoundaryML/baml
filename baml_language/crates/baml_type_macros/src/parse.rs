//! Parsing and resolution of the `ty_family!` DSL.
//!
//! The DSL has four sections, in order:
//! 1. `axes { a, b, c, ... }` — the membership categories.
//! 2. one or more `type Name { includes: [..axes..], child: Self | Other }`.
//! 3. zero or more `satellite Name { fields } methods { ... }`.
//! 4. the master `enum`, each variant tagged with exactly one `#[axis(..)]`.
//!
//! [`FamilyInput`] is the raw parse; [`Family`] is the resolved form with axis
//! and child names turned into indices and validated.

use proc_macro2::TokenStream;
use syn::{
    Attribute, Field, Fields, Ident, ItemEnum, Token, braced, bracketed,
    ext::IdentExt,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

mod kw {
    syn::custom_keyword!(axes);
    syn::custom_keyword!(satellite);
    syn::custom_keyword!(includes);
    syn::custom_keyword!(child);
    syn::custom_keyword!(methods);
}

// ── Raw parse ────────────────────────────────────────────────────────────────

pub(crate) struct FamilyInput {
    axes: Vec<Ident>,
    members: Vec<MemberInput>,
    satellites: Vec<SatelliteInput>,
    master: ItemEnum,
}

struct MemberInput {
    name: Ident,
    includes: Vec<Ident>,
    child: ChildRef,
}

enum ChildRef {
    /// `child: Self` — a deep, self-recursive member.
    SelfRef,
    /// `child: Other` — a shallow member whose nested positions hold `Other`.
    Named(Ident),
}

struct SatelliteInput {
    name: Ident,
    fields: Punctuated<Field, Token![,]>,
    methods: Option<TokenStream>,
}

impl Parse for FamilyInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::axes>()?;
        let axes_content;
        braced!(axes_content in input);
        // Axis names are a DSL vocabulary, so accept keywords (e.g. `abstract`).
        let axes =
            Punctuated::<Ident, Token![,]>::parse_terminated_with(&axes_content, Ident::parse_any)?
                .into_iter()
                .collect();

        let mut members = Vec::new();
        let mut satellites = Vec::new();
        loop {
            if input.peek(Token![type]) {
                members.push(parse_member(input)?);
            } else if input.peek(kw::satellite) {
                satellites.push(parse_satellite(input)?);
            } else {
                break;
            }
        }

        let master: ItemEnum = input.parse()?;
        Ok(FamilyInput {
            axes,
            members,
            satellites,
            master,
        })
    }
}

fn parse_member(input: ParseStream) -> syn::Result<MemberInput> {
    input.parse::<Token![type]>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);

    content.parse::<kw::includes>()?;
    content.parse::<Token![:]>()?;
    let inc_content;
    bracketed!(inc_content in content);
    let includes =
        Punctuated::<Ident, Token![,]>::parse_terminated_with(&inc_content, Ident::parse_any)?
            .into_iter()
            .collect();
    content.parse::<Token![,]>()?;

    content.parse::<kw::child>()?;
    content.parse::<Token![:]>()?;
    let child = if content.peek(Token![Self]) {
        content.parse::<Token![Self]>()?;
        ChildRef::SelfRef
    } else {
        ChildRef::Named(content.parse::<Ident>()?)
    };
    let _trailing: Option<Token![,]> = content.parse()?;

    Ok(MemberInput {
        name,
        includes,
        child,
    })
}

fn parse_satellite(input: ParseStream) -> syn::Result<SatelliteInput> {
    input.parse::<kw::satellite>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    let fields =
        Punctuated::<Field, Token![,]>::parse_terminated_with(&content, Field::parse_named)?;

    let methods = if input.peek(kw::methods) {
        input.parse::<kw::methods>()?;
        let m_content;
        braced!(m_content in input);
        Some(m_content.parse::<TokenStream>()?)
    } else {
        None
    };

    Ok(SatelliteInput {
        name,
        fields,
        methods,
    })
}

// ── Resolved form ────────────────────────────────────────────────────────────

pub(crate) struct Family {
    pub(crate) master_ident: Ident,
    /// Attributes on the master `enum` (derives + docs), re-emitted per member.
    pub(crate) master_attrs: Vec<Attribute>,
    pub(crate) members: Vec<Member>,
    pub(crate) satellites: Vec<Satellite>,
    pub(crate) variants: Vec<MVariant>,
}

pub(crate) struct Member {
    pub(crate) name: Ident,
    /// Axis indices this member includes (a variant is present iff its axis is here).
    pub(crate) includes: Vec<usize>,
    /// Index into [`Family::members`] for nested positions; equal to this
    /// member's own index iff the member is deep (`child: Self`).
    pub(crate) child: usize,
    /// `true` for the master member (its name equals [`Family::master_ident`]).
    pub(crate) is_master: bool,
    /// `true` iff `child` points at this member itself.
    pub(crate) deep: bool,
}

pub(crate) struct Satellite {
    pub(crate) name: Ident,
    pub(crate) fields: Punctuated<Field, Token![,]>,
    pub(crate) methods: Option<TokenStream>,
}

pub(crate) struct MVariant {
    /// Variant attributes with `#[axis(..)]` removed (docs etc. retained).
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) ident: Ident,
    pub(crate) fields: Fields,
    pub(crate) axis: usize,
}

impl Family {
    pub(crate) fn from_input(input: FamilyInput) -> syn::Result<Self> {
        let FamilyInput {
            axes,
            members,
            satellites,
            master,
        } = input;

        let axis_index = |name: &Ident| -> syn::Result<usize> {
            axes.iter()
                .position(|a| a == name)
                .ok_or_else(|| syn::Error::new(name.span(), format!("unknown axis `{name}`")))
        };
        let member_index = |name: &Ident| -> syn::Result<usize> {
            members.iter().position(|m| &m.name == name).ok_or_else(|| {
                syn::Error::new(name.span(), format!("unknown family member `{name}`"))
            })
        };

        let master_ident = master.ident.clone();

        let resolved_members = members
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let includes = m
                    .includes
                    .iter()
                    .map(&axis_index)
                    .collect::<syn::Result<Vec<_>>>()?;
                let child = match &m.child {
                    ChildRef::SelfRef => i,
                    ChildRef::Named(name) => member_index(name)?,
                };
                Ok(Member {
                    name: m.name.clone(),
                    includes,
                    child,
                    is_master: m.name == master_ident,
                    deep: child == i,
                })
            })
            .collect::<syn::Result<Vec<_>>>()?;

        let resolved_satellites = satellites
            .into_iter()
            .map(|s| Satellite {
                name: s.name,
                fields: s.fields,
                methods: s.methods,
            })
            .collect();

        let variants = master
            .variants
            .into_iter()
            .map(|v| resolve_variant(v, &axis_index))
            .collect::<syn::Result<Vec<_>>>()?;

        Ok(Family {
            master_ident,
            master_attrs: master.attrs,
            members: resolved_members,
            satellites: resolved_satellites,
            variants,
        })
    }
}

fn resolve_variant(
    variant: syn::Variant,
    axis_index: &impl Fn(&Ident) -> syn::Result<usize>,
) -> syn::Result<MVariant> {
    let span = variant.ident.span();
    let mut axis_ident: Option<Ident> = None;
    let mut attrs = Vec::new();
    for attr in variant.attrs {
        if attr.path().is_ident("axis") {
            if axis_ident.is_some() {
                return Err(syn::Error::new(
                    span,
                    format!(
                        "variant `{}` has more than one `#[axis(..)]`",
                        variant.ident
                    ),
                ));
            }
            axis_ident = Some(attr.parse_args_with(Ident::parse_any)?);
        } else {
            attrs.push(attr);
        }
    }
    let axis_ident = axis_ident.ok_or_else(|| {
        syn::Error::new(
            span,
            format!(
                "variant `{}` must declare exactly one `#[axis(..)]`",
                variant.ident
            ),
        )
    })?;
    let axis = axis_index(&axis_ident)?;
    Ok(MVariant {
        attrs,
        ident: variant.ident,
        fields: variant.fields,
        axis,
    })
}
