//! The classes and enums compiled code touches, each emitted once as a
//! generated struct or enum.
//!
//! [`ClassTable`] is the one place a BAML runtime type becomes a
//! [`NativeTy`]: mapping a type registers every class and enum it mentions,
//! checks a class is data-only and non-generic, maps its fields (which may
//! register further declarations), and remembers the result so a declaration
//! reachable from several functions is checked and emitted once. A class
//! whose fields leave the subset rejects every function that touches it,
//! with the field named.

use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout,
};
use baml_compiler2_mir::{RuntimeTy, class_link_name, enum_link_name};
use baml_type::DeclName;
use proc_macro2::{Ident, Span};
use rustc_hash::FxHashMap;

use crate::{
    Rejection, rust_name,
    types::{NativeTy, Resolver, TypeDecl, Unsupported, from_runtime_ty},
};

/// One field of a generated struct.
#[derive(Debug, Clone)]
pub(crate) struct FieldInfo<'db> {
    /// The BAML field name.
    pub name: String,
    /// The Rust field identifier: the BAML name sanitized, with a trailing
    /// `_` when the name is a Rust keyword.
    pub ident: Ident,
    pub ty: NativeTy<'db>,
}

impl FieldInfo<'_> {
    /// Whether the Rust identifier differs from the BAML name, so the struct
    /// needs a `#[serde(rename)]` to keep the wire name.
    pub(crate) fn renamed(&self) -> bool {
        self.ident != self.name.as_str()
    }
}

/// A class admitted to the subset.
#[derive(Debug, Clone)]
pub(crate) struct ClassInfo<'db> {
    /// The BAML link name, e.g. `user.Cell`.
    pub link_name: String,
    /// The unqualified class name, as `to_string` renders it.
    pub display_name: String,
    /// The name the VM uses in JSON decode errors: the user-facing display
    /// name (`State`, `ns.Item`, or package-qualified for a dependency).
    pub json_name: String,
    /// The generated struct's identifier.
    pub ident: Ident,
    /// Fields in declaration (= slot) order.
    pub fields: Vec<FieldInfo<'db>>,
}

/// One variant of a generated enum.
#[derive(Debug, Clone)]
pub(crate) struct VariantInfo {
    /// The BAML variant name, which `to_string` and JSON use.
    pub name: String,
    /// The Rust variant identifier: the BAML name sanitized, with a
    /// trailing `_` when the name is a Rust keyword.
    pub ident: Ident,
}

impl VariantInfo {
    /// Whether the Rust identifier differs from the BAML name, so the enum
    /// needs a `#[serde(rename)]` to keep the wire name.
    pub(crate) fn renamed(&self) -> bool {
        self.ident != self.name.as_str()
    }
}

/// An enum admitted to the subset.
#[derive(Debug, Clone)]
pub(crate) struct EnumInfo<'db> {
    /// The BAML link name, e.g. `user.Color`.
    pub link_name: String,
    /// The generated enum's identifier.
    pub ident: Ident,
    /// Variants in declaration (= discriminant) order.
    pub variants: Vec<VariantInfo>,
    _marker: std::marker::PhantomData<EnumRef<'db>>,
}

enum State<'db> {
    /// The class's fields are being mapped; a reference back to it from a
    /// field is fine (it is a `Shared` handle).
    Checking,
    Done(Result<ClassInfo<'db>, String>),
}

/// Every class and enum reached while mapping types, each in first-seen
/// order.
pub(crate) struct ClassTable<'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    states: FxHashMap<ClassRef<'db>, State<'db>>,
    order: Vec<ClassRef<'db>>,
    enums: FxHashMap<EnumRef<'db>, Result<EnumInfo<'db>, String>>,
    enum_order: Vec<EnumRef<'db>>,
}

impl<'db> Resolver<'db> for ClassTable<'db> {
    fn class(&mut self, head: &DeclName, args: &[RuntimeTy]) -> Result<NativeTy<'db>, Unsupported> {
        if !args.is_empty() {
            return Err(Unsupported("generic class".into()));
        }
        let Some(class) = layout::class_ref_of(self.db, head) else {
            return Err(Unsupported("class without a declaration".into()));
        };
        self.check(class)?;
        Ok(NativeTy::Class(class))
    }

    fn enum_(&mut self, head: &DeclName) -> Result<NativeTy<'db>, Unsupported> {
        let Some(enum_ref) = layout::enum_ref_of(self.db, head) else {
            return Err(Unsupported("enum without a declaration".into()));
        };
        self.check_enum(enum_ref)?;
        Ok(NativeTy::Enum(enum_ref))
    }
}

impl<'db> ClassTable<'db> {
    pub(crate) fn new(db: &'db dyn baml_compiler2_mir::Db) -> Self {
        Self {
            db,
            states: FxHashMap::default(),
            order: Vec::new(),
            enums: FxHashMap::default(),
            enum_order: Vec::new(),
        }
    }

    /// The BAML spelling of a runtime type, for messages.
    pub(crate) fn spell(&self, ty: &RuntimeTy) -> String {
        let spelling = baml_compiler2_hir::package::spelling(self.db);
        ty.map_heads(&mut |decl| spelling.wire(decl)).to_string()
    }

    /// Map `ty`, registering the classes it mentions. The error reads
    /// `` type `map<string, int>`: map ``.
    pub(crate) fn native_ty(&mut self, ty: &RuntimeTy) -> Result<NativeTy<'db>, Rejection> {
        self.map(ty).map_err(|Unsupported(reason)| {
            Rejection::unsupported(format!("type `{}`: {reason}", self.spell(ty)))
        })
    }

    fn map(&mut self, ty: &RuntimeTy) -> Result<NativeTy<'db>, Unsupported> {
        from_runtime_ty(ty, self)
    }

    /// Admit `enum_ref`, naming its variants once.
    pub(crate) fn check_enum(&mut self, enum_ref: EnumRef<'db>) -> Result<(), Unsupported> {
        if let Some(state) = self.enums.get(&enum_ref) {
            return state
                .as_ref()
                .map(drop)
                .map_err(|reason| Unsupported(reason.clone()));
        }
        let result = self.describe_enum(enum_ref);
        let outcome = result
            .as_ref()
            .map(drop)
            .map_err(|reason| Unsupported(reason.clone()));
        self.enums.insert(enum_ref, result);
        self.enum_order.push(enum_ref);
        outcome
    }

    fn describe_enum(&self, enum_ref: EnumRef<'db>) -> Result<EnumInfo<'db>, String> {
        let link_name = enum_link_name(self.db, enum_ref);
        let variants: Vec<VariantInfo> = layout::enum_variants(self.db, enum_ref)
            .iter()
            .map(|name| VariantInfo {
                name: name.to_string(),
                ident: field_ident(name.as_str()),
            })
            .collect();
        let mut seen = FxHashMap::default();
        for variant in &variants {
            if let Some(other) = seen.insert(variant.ident.to_string(), &variant.name) {
                return Err(format!(
                    "enum `{link_name}` variants `{}` and `{other}` both need the Rust name `{}`",
                    variant.name, variant.ident
                ));
            }
        }
        Ok(EnumInfo {
            ident: Ident::new(&rust_name(&link_name), Span::call_site()),
            link_name,
            variants,
            _marker: std::marker::PhantomData,
        })
    }

    /// The admitted enum `enum_ref`. Every enum a mapped type mentions is
    /// admitted, so a miss is a bug in the caller.
    pub(crate) fn enum_info(&self, enum_ref: EnumRef<'db>) -> Result<&EnumInfo<'db>, Rejection> {
        match self.enums.get(&enum_ref) {
            Some(Ok(info)) => Ok(info),
            Some(Err(reason)) => Err(Rejection::unsupported(reason.clone())),
            None => Err(Rejection::invalid(format!(
                "enum `{}` was used before it was admitted",
                enum_link_name(self.db, enum_ref)
            ))),
        }
    }

    /// Admit `class`, mapping its fields once.
    pub(crate) fn check(&mut self, class: ClassRef<'db>) -> Result<(), Unsupported> {
        match self.states.get(&class) {
            Some(State::Checking | State::Done(Ok(_))) => return Ok(()),
            Some(State::Done(Err(reason))) => return Err(Unsupported(reason.clone())),
            None => {}
        }
        self.states.insert(class, State::Checking);
        self.order.push(class);
        let result = self.describe(class);
        let outcome = result
            .as_ref()
            .map(drop)
            .map_err(|reason| Unsupported(reason.clone()));
        self.states.insert(class, State::Done(result));
        outcome
    }

    fn describe(&mut self, class: ClassRef<'db>) -> Result<ClassInfo<'db>, String> {
        let link_name = class_link_name(self.db, class);
        if !layout::class_generic_params(self.db, class).is_empty() {
            return Err(format!("generic class `{link_name}`"));
        }
        let head = layout::class_head(self.db, class);
        let display_name = head.name().to_string();
        let json_name = baml_compiler2_hir::package::spelling(self.db)
            .wire(&head)
            .display_name()
            .to_string();
        let mut fields = Vec::new();
        for (name, ty) in layout::class_fields(self.db, class) {
            let runtime_ty = RuntimeTy::try_from(&ty).map_err(|_| {
                format!("class `{link_name}` field `{name}` has a compile-time-only type")
            })?;
            let native = self.map(&runtime_ty).map_err(|Unsupported(reason)| {
                format!(
                    "class `{link_name}` field `{name}` of type `{}`: {reason}",
                    self.spell(&runtime_ty)
                )
            })?;
            let ident = field_ident(name.as_str());
            fields.push(FieldInfo {
                name: name.to_string(),
                ident,
                ty: native,
            });
        }
        let mut seen = FxHashMap::default();
        for field in &fields {
            if let Some(other) = seen.insert(field.ident.to_string(), &field.name) {
                return Err(format!(
                    "class `{link_name}` fields `{}` and `{other}` both need the Rust name `{}`",
                    field.name, field.ident
                ));
            }
        }
        Ok(ClassInfo {
            ident: Ident::new(&rust_name(&link_name), Span::call_site()),
            link_name,
            display_name,
            json_name,
            fields,
        })
    }

    /// The admitted class `class`. Every class a mapped type mentions is
    /// admitted, so a miss is a bug in the caller.
    pub(crate) fn info(&self, class: ClassRef<'db>) -> Result<&ClassInfo<'db>, Rejection> {
        match self.states.get(&class) {
            Some(State::Done(Ok(info))) => Ok(info),
            Some(State::Done(Err(reason))) => Err(Rejection::unsupported(reason.clone())),
            Some(State::Checking) | None => Err(Rejection::invalid(format!(
                "class `{}` was used before it was admitted",
                class_link_name(self.db, class)
            ))),
        }
    }

    /// The generated item's identifier, for [`NativeTy::to_tokens`].
    pub(crate) fn ident(&self, decl: TypeDecl<'db>) -> Ident {
        match decl {
            TypeDecl::Class(class) => match self.info(class) {
                Ok(info) => info.ident.clone(),
                Err(_) => Ident::new(
                    &rust_name(&class_link_name(self.db, class)),
                    Span::call_site(),
                ),
            },
            TypeDecl::Enum(enum_ref) => match self.enum_info(enum_ref) {
                Ok(info) => info.ident.clone(),
                Err(_) => Ident::new(
                    &rust_name(&enum_link_name(self.db, enum_ref)),
                    Span::call_site(),
                ),
            },
        }
    }

    /// Whether `decl` has an explicit `implements` block for `interface`
    /// (its own `to_string`, say), which would override the structural
    /// default the generated code uses.
    pub(crate) fn has_explicit_impl(&self, decl: TypeDecl<'db>, interface: &DeclName) -> bool {
        use baml_compiler2_hir_ty::impls::{ResolvedImplementation, resolve_implementation};
        use baml_type::interned::{InferInterface, Ty as InternedTy};
        let concrete = match decl {
            TypeDecl::Class(class) => {
                baml_type::Ty::Class(layout::class_head(self.db, class), Box::new([]))
            }
            TypeDecl::Enum(enum_ref) => baml_type::Ty::Enum(layout::enum_head(self.db, enum_ref)),
        };
        let interface = InferInterface::new(interface.clone(), Box::new([]), Box::new([]));
        matches!(
            resolve_implementation(self.db, &InternedTy::from_plain(&concrete), &interface),
            Some(ResolvedImplementation::Explicit(_))
        )
    }

    /// The BAML link name, for [`NativeTy::describe`].
    pub(crate) fn link_name(&self, decl: TypeDecl<'db>) -> String {
        match decl {
            TypeDecl::Class(class) => class_link_name(self.db, class),
            TypeDecl::Enum(enum_ref) => enum_link_name(self.db, enum_ref),
        }
    }

    /// Every admitted class and enum, each in first-seen order, once no two
    /// of them need the same item name.
    #[allow(clippy::type_complexity)]
    pub(crate) fn finish(&self) -> Result<(Vec<&ClassInfo<'db>>, Vec<&EnumInfo<'db>>), Rejection> {
        let mut by_ident: FxHashMap<String, &str> = FxHashMap::default();
        let mut classes = Vec::with_capacity(self.order.len());
        for class in &self.order {
            let info = self.info(*class)?;
            if let Some(other) = by_ident.insert(info.ident.to_string(), &info.link_name) {
                return Err(Rejection::unsupported(format!(
                    "classes `{}` and `{other}` both need the Rust name `{}`",
                    info.link_name, info.ident
                )));
            }
            classes.push(info);
        }
        let mut enums = Vec::with_capacity(self.enum_order.len());
        for enum_ref in &self.enum_order {
            let info = self.enum_info(*enum_ref)?;
            if let Some(other) = by_ident.insert(info.ident.to_string(), &info.link_name) {
                return Err(Rejection::unsupported(format!(
                    "`{}` and `{other}` both need the Rust name `{}`",
                    info.link_name, info.ident
                )));
            }
            enums.push(info);
        }
        Ok((classes, enums))
    }
}

/// Keywords `syn` still parses as identifiers but the generated crate's
/// edition (2024) reserves.
const EDITION_2024_KEYWORDS: &[&str] = &["gen"];

/// The Rust identifier for a BAML field or variant name: sanitized like a
/// link name, with `_` appended when the result is a keyword (`type` ->
/// `type_`, `Self` -> `Self_`).
fn field_ident(name: &str) -> Ident {
    let mut candidate = rust_name(name);
    if syn::parse_str::<Ident>(&candidate).is_err() || EDITION_2024_KEYWORDS.contains(&&*candidate)
    {
        candidate.push('_');
    }
    Ident::new(&candidate, Span::call_site())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_identifiers_avoid_keywords() {
        assert_eq!(field_ident("value").to_string(), "value");
        assert_eq!(field_ident("type").to_string(), "type_");
        assert_eq!(field_ident("self").to_string(), "self_");
        assert_eq!(field_ident("gen").to_string(), "gen_");
        assert_eq!(field_ident("Self").to_string(), "Self_");
        assert_eq!(field_ident("1st").to_string(), "_1st");
    }
}
