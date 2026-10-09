//! The classes and enums compiled code touches, each emitted once as a
//! generated struct or enum.
//!
//! [`ClassTable`] is the one place a BAML runtime type becomes a
//! [`NativeTy`]: mapping a type registers every class instance and enum it
//! mentions, checks a class is data-only, maps its fields at the instance's
//! type arguments (which may register further declarations), and remembers
//! the result so an instance reachable from several functions is checked
//! and emitted once: a generic class is one struct per type-argument tuple
//! (`user_Box__int`), as a generic function is one instance per tuple (D4).
//! A class whose fields leave the subset rejects every function that
//! touches it, with the field named. Every closed union a mapped type
//! mentions is registered too ([`UnionInfo`]), so its generated enum is
//! emitted once.

use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    layout,
    package_interface::reduce_ground_projections,
};
use baml_compiler2_mir::{RealizedTy, RuntimeTy, class_link_name, enum_link_name};
use baml_type::{DeclName, Ty, unify::substitute_ty};
use proc_macro2::{Ident, Span};
use rustc_hash::FxHashMap;

use crate::{
    Rejection, rust_name,
    types::{ClassInst, NativeTy, Resolver, TypeDecl, Unsupported, from_runtime_ty},
};

/// How many associated type projections may reduce through one another
/// while a field's type is realized at the class's arguments.
const PROJECTION_FUEL: u32 = 32;

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

/// A class instance admitted to the subset.
#[derive(Debug, Clone)]
pub(crate) struct ClassInfo<'db> {
    /// The BAML link name, e.g. `user.Cell`, or `user.Box<int>` for an
    /// instance of a generic class.
    pub link_name: String,
    /// The class's own link name, `user.Box`: what a class test, a thrown
    /// instance and the readable rendering name.
    pub class_fqn: String,
    /// The unqualified class name, as `to_string` renders it.
    pub display_name: String,
    /// The name the VM uses in JSON decode errors: the user-facing display
    /// name (`State`, `ns.Item`, or package-qualified for a dependency).
    pub json_name: String,
    /// The generated struct's identifier: the link name sanitized, then
    /// `__` and each type argument mangled (`user_Box__int`).
    pub ident: Ident,
    /// Fields in declaration (= slot) order, typed at the instance's
    /// arguments.
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

/// Where [`ClassTable::literal_type_in`] found a literal type.
enum LiteralSite {
    /// In the type itself.
    Here,
    /// In a class field, described.
    Field(String),
}

/// A closed union admitted to the subset: a generated enum with one
/// variant per member.
#[derive(Debug, Clone)]
pub(crate) struct UnionInfo<'db> {
    /// The members, in the type system's canonical order.
    pub members: Vec<NativeTy<'db>>,
    /// The generated enum's identifier, `Union_int_or_float`.
    pub ident: Ident,
    /// The variant identifiers, parallel to `members`: each member mangled
    /// (`int`, `string_array`, `user_A`).
    pub variants: Vec<Ident>,
    /// The BAML spelling, `int | float`, with classes by link name.
    pub described: String,
}

impl<'db> UnionInfo<'db> {
    /// The variant holding `member`.
    pub(crate) fn variant_of(&self, member: &NativeTy<'db>) -> Option<&Ident> {
        self.members
            .iter()
            .position(|candidate| candidate == member)
            .map(|index| &self.variants[index])
    }
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
    states: FxHashMap<ClassInst<'db>, State<'db>>,
    order: Vec<ClassInst<'db>>,
    enums: FxHashMap<EnumRef<'db>, Result<EnumInfo<'db>, String>>,
    enum_order: Vec<EnumRef<'db>>,
    /// Every union a mapped type mentions, in first-seen order.
    unions: Vec<UnionInfo<'db>>,
}

impl<'db> Resolver<'db> for ClassTable<'db> {
    fn class(&mut self, head: &DeclName, args: &[RuntimeTy]) -> Result<NativeTy<'db>, Unsupported> {
        let Some(class) = layout::class_ref_of(self.db, head) else {
            return Err(Unsupported("class without a declaration".into()));
        };
        let params = layout::class_generic_params(self.db, class);
        if params.len() != args.len() {
            // A method of a generic class reads `self` as the bare class;
            // the frame fills its arguments before the type reaches here.
            return Err(Unsupported(format!(
                "generic class `{}` with {} of {} type arguments",
                class_link_name(self.db, class),
                args.len(),
                params.len()
            )));
        }
        let mut realized = Vec::with_capacity(args.len());
        for arg in args {
            realized.push(RealizedTy::try_from(arg).map_err(|error| {
                Unsupported(match error.variant {
                    "TypeVar" => "type variable (a class type argument)".to_string(),
                    _ => "associated type projection (a class type argument)".to_string(),
                })
            })?);
        }
        let instance = ClassInst::new(class, realized);
        self.check(&instance)?;
        Ok(NativeTy::Class(instance))
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
            unions: Vec::new(),
        }
    }

    /// A literal or variant type `ty` mentions, directly or through the
    /// fields of the classes it names: the native type erases it to its
    /// primitive (or its enum), which a typed JSON decode must not do, since
    /// the VM rejects a value outside the literal. The result names the
    /// type, or the class field, that holds it.
    pub(crate) fn literal_type_in(&self, ty: &RuntimeTy) -> Option<String> {
        let mut visited = Vec::new();
        match self.literal_type_walk(ty, &mut visited)? {
            LiteralSite::Here => Some(format!("`{}`", self.spell(ty))),
            LiteralSite::Field(described) => Some(described),
        }
    }

    fn literal_type_walk(
        &self,
        ty: &RuntimeTy,
        visited: &mut Vec<ClassRef<'db>>,
    ) -> Option<LiteralSite> {
        match ty {
            RuntimeTy::Literal(..) | RuntimeTy::EnumVariant(..) => Some(LiteralSite::Here),
            RuntimeTy::List(inner) => self.literal_type_walk(inner, visited),
            RuntimeTy::Map { key, value } => self
                .literal_type_walk(key, visited)
                .or_else(|| self.literal_type_walk(value, visited)),
            RuntimeTy::Union(members) => members
                .iter()
                .find_map(|member| self.literal_type_walk(member, visited)),
            RuntimeTy::Class(head, args) => {
                if let Some(found) = args
                    .iter()
                    .find_map(|arg| self.literal_type_walk(arg, visited))
                {
                    return Some(found);
                }
                let class = layout::class_ref_of(self.db, head)?;
                if visited.contains(&class) {
                    return None;
                }
                visited.push(class);
                let params = layout::class_generic_params(self.db, class);
                let bindings: FxHashMap<_, _> = params
                    .iter()
                    .cloned()
                    .zip(args.iter().map(Ty::from))
                    .collect();
                layout::class_fields(self.db, class)
                    .iter()
                    .find_map(|(name, field_ty)| {
                        let runtime_ty =
                            RuntimeTy::try_from(&substitute_ty(field_ty, &bindings)).ok()?;
                        Some(match self.literal_type_walk(&runtime_ty, visited)? {
                            LiteralSite::Here => LiteralSite::Field(format!(
                                "`{}` in field `{name}` of class `{}`",
                                self.spell(&runtime_ty),
                                class_link_name(self.db, class)
                            )),
                            inner @ LiteralSite::Field(_) => inner,
                        })
                    })
            }
            _ => None,
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
        let native = from_runtime_ty(ty, self)?;
        self.register_unions(&native)?;
        Ok(native)
    }

    /// Register every union `ty` mentions, naming its variants once.
    fn register_unions(&mut self, ty: &NativeTy<'db>) -> Result<(), Unsupported> {
        let mut found = Vec::new();
        ty.unions(&mut found);
        for members in found {
            if self.unions.iter().any(|info| info.members == members) {
                continue;
            }
            let info = self.describe_union(members)?;
            self.unions.push(info);
        }
        Ok(())
    }

    fn describe_union(&self, members: Vec<NativeTy<'db>>) -> Result<UnionInfo<'db>, Unsupported> {
        let name = |decl| self.ident(decl);
        let ident = NativeTy::union_ident(&members, &name);
        let variants: Vec<Ident> = members
            .iter()
            .map(|member| field_ident(&member.mangle(&name)))
            .collect();
        let described = NativeTy::Union(members.clone()).describe(&|decl| self.link_name(decl));
        let mut seen = FxHashMap::default();
        for (variant, member) in variants.iter().zip(&members) {
            if let Some(other) = seen.insert(variant.to_string(), member) {
                return Err(Unsupported(format!(
                    "union `{described}`: members `{}` and `{}` both need the Rust name `{variant}`",
                    member.describe(&|decl| self.link_name(decl)),
                    other.describe(&|decl| self.link_name(decl))
                )));
            }
        }
        Ok(UnionInfo {
            members,
            ident,
            variants,
            described,
        })
    }

    /// The admitted union with exactly `members`. Every union a mapped type
    /// mentions is admitted, so a miss is a bug in the caller.
    pub(crate) fn union_info(
        &self,
        members: &[NativeTy<'db>],
    ) -> Result<&UnionInfo<'db>, Rejection> {
        self.unions
            .iter()
            .find(|info| info.members == members)
            .ok_or_else(|| {
                Rejection::invalid(format!(
                    "union `{}` was used before it was admitted",
                    NativeTy::Union(members.to_vec()).describe(&|decl| self.link_name(decl))
                ))
            })
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

    /// Admit the class instance `instance`, mapping its fields once.
    pub(crate) fn check(&mut self, instance: &ClassInst<'db>) -> Result<(), Unsupported> {
        match self.states.get(instance) {
            Some(State::Checking | State::Done(Ok(_))) => return Ok(()),
            Some(State::Done(Err(reason))) => return Err(Unsupported(reason.clone())),
            None => {}
        }
        self.states.insert(instance.clone(), State::Checking);
        self.order.push(instance.clone());
        let result = self.describe(instance);
        let outcome = result
            .as_ref()
            .map(drop)
            .map_err(|reason| Unsupported(reason.clone()));
        self.states.insert(instance.clone(), State::Done(result));
        outcome
    }

    /// The link name of a class instance: the class's, with its type
    /// arguments spelled (`user.Box<int>`).
    pub(crate) fn instance_link_name(&self, instance: &ClassInst<'db>) -> String {
        let link_name = class_link_name(self.db, instance.class);
        if instance.args.is_empty() {
            return link_name;
        }
        let args = instance
            .args
            .iter()
            .map(|arg| self.spell(&RuntimeTy::from(arg.clone())))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{link_name}<{args}>")
    }

    fn describe(&mut self, instance: &ClassInst<'db>) -> Result<ClassInfo<'db>, String> {
        let class = instance.class;
        let class_fqn = class_link_name(self.db, class);
        let link_name = self.instance_link_name(instance);
        let params = layout::class_generic_params(self.db, class);
        if params.len() != instance.args.len() {
            return Err(format!(
                "generic class `{class_fqn}` with {} of {} type arguments",
                instance.args.len(),
                params.len()
            ));
        }
        // The struct's name carries the arguments mangled, which maps them
        // (and registers what they mention) first.
        let mut ident = rust_name(&class_fqn);
        for arg in &instance.args {
            let runtime_ty = RuntimeTy::from(arg.clone());
            let native = self.map(&runtime_ty).map_err(|Unsupported(reason)| {
                format!(
                    "class `{link_name}` type argument `{}`: {reason}",
                    self.spell(&runtime_ty)
                )
            })?;
            ident.push_str("__");
            ident.push_str(&native.mangle(&|decl| self.ident(decl)));
        }
        let head = layout::class_head(self.db, class);
        let display_name = head.name().to_string();
        let json_name = baml_compiler2_hir::package::spelling(self.db)
            .wire(&head)
            .display_name()
            .to_string();
        let bindings: FxHashMap<_, _> = params
            .into_iter()
            .zip(
                instance
                    .args
                    .iter()
                    .map(|arg| Ty::from(arg.as_runtime_ty())),
            )
            .collect();
        let mut fields = Vec::new();
        for (name, ty) in layout::class_fields(self.db, class) {
            // The field's type at the instance's arguments, with any
            // projection over them reduced.
            let realized =
                reduce_ground_projections(self.db, &substitute_ty(&ty, &bindings), PROJECTION_FUEL);
            let runtime_ty = RuntimeTy::try_from(&realized).map_err(|_| {
                format!("class `{link_name}` field `{name}` has a compile-time-only type")
            })?;
            let native = self.map(&runtime_ty).map_err(|Unsupported(reason)| {
                format!(
                    "class `{link_name}` field `{name}` of type `{}`: {reason}",
                    self.spell(&runtime_ty)
                )
            })?;
            // The struct renders, serializes and compares its fields; a
            // function value does none of that.
            if native.mentions_fn() {
                return Err(format!(
                    "class `{link_name}` field `{name}` of type `{}`: function type (a class field)",
                    self.spell(&runtime_ty)
                ));
            }
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
            ident: Ident::new(&ident, Span::call_site()),
            link_name,
            class_fqn,
            display_name,
            json_name,
            fields,
        })
    }

    /// The admitted class instance `instance`. Every instance a mapped type
    /// mentions is admitted, so a miss is a bug in the caller.
    pub(crate) fn info(&self, instance: &ClassInst<'db>) -> Result<&ClassInfo<'db>, Rejection> {
        match self.states.get(instance) {
            Some(State::Done(Ok(info))) => Ok(info),
            Some(State::Done(Err(reason))) => Err(Rejection::unsupported(reason.clone())),
            Some(State::Checking) | None => Err(Rejection::invalid(format!(
                "class `{}` was used before it was admitted",
                self.instance_link_name(instance)
            ))),
        }
    }

    /// The link name of a class declaration, `user.Box`: what a class test
    /// and a thrown instance are decided by, whatever the arguments.
    pub(crate) fn class_fqn(&self, class: ClassRef<'db>) -> String {
        class_link_name(self.db, class)
    }

    /// The generated item's identifier, for [`NativeTy::to_tokens`].
    pub(crate) fn ident(&self, decl: TypeDecl<'db>) -> Ident {
        match decl {
            TypeDecl::Class(instance) => match self.info(&instance) {
                Ok(info) => info.ident.clone(),
                Err(_) => Ident::new(
                    &rust_name(&self.instance_link_name(&instance)),
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
            TypeDecl::Class(instance) => baml_type::Ty::Class(
                layout::class_head(self.db, instance.class),
                instance
                    .args
                    .iter()
                    .map(|arg| Ty::from(arg.as_runtime_ty()))
                    .collect(),
            ),
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
            TypeDecl::Class(instance) => self.instance_link_name(&instance),
            TypeDecl::Enum(enum_ref) => enum_link_name(self.db, enum_ref),
        }
    }

    /// Every admitted class, enum and union, each in first-seen order, once
    /// no two of them need the same item name.
    #[allow(clippy::type_complexity)]
    pub(crate) fn finish(
        &self,
    ) -> Result<
        (
            Vec<&ClassInfo<'db>>,
            Vec<&EnumInfo<'db>>,
            Vec<&UnionInfo<'db>>,
        ),
        Rejection,
    > {
        let mut by_ident: FxHashMap<String, &str> = FxHashMap::default();
        let mut classes = Vec::with_capacity(self.order.len());
        for class in &self.order {
            let info = self.info(class)?;
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
        let mut unions = Vec::with_capacity(self.unions.len());
        for info in &self.unions {
            if let Some(other) = by_ident.insert(info.ident.to_string(), &info.described) {
                return Err(Rejection::unsupported(format!(
                    "union `{}` and `{other}` both need the Rust name `{}`",
                    info.described, info.ident
                )));
            }
            unions.push(info);
        }
        Ok((classes, enums, unions))
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
