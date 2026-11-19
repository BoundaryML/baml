//! What the runtime is told about types: the section of the type table
//! laid out in the runtime's `json_layout.rs`. `baml.json` writes and
//! reads values by it, and every class is described in it once, for
//! `baml.json` and for what writes an instance that is thrown and not
//! caught.
//!
//! A description is made from the type as MIR has it, so that a literal
//! type, an alias for `json` and a field's attributes are still there to
//! describe. What the runtime cannot yet write or read (an enum with its
//! own `to_json` or `from_json`; a `bigint`; a tuple) is found here, when
//! the program is checked, and reported as a blocker.
//!
//! A class with a `to_json` or a `from_json` of its own is described with
//! where the runtime calls each: an entry the backend compiles for the
//! purpose (`codegen/hooks.rs`).

use std::collections::HashMap;

use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    impls::{impl_facts, package_impl_locs, structural_interface},
    json_attrs::{is_optional, json_overrides, override_of},
    layout::{class_fields, class_head, class_ref_of, enum_head, enum_ref_of, interface_ref_of},
    render::Viewpoint,
};
use baml_compiler2_mir::{RuntimeTy, resolved_aliases_for_package};
use baml_type::{HeadDisplay, Literal, StructuralInterface};

use crate::{
    Blocker,
    json_layout::{
        DESCRIPTION, DESCRIPTION_A, DESCRIPTION_FLAGS, FIELD_MAY_BE_ABSENT, FIELD_OMIT_NULL,
        FLAG_HOOKS, FLAG_JSON, FLAG_SHOWN_ONLY, HOOK_FROM_JSON, HOOK_TO_JSON, JSON_BODY,
        JSON_VERSION, KIND_ARRAY, KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT,
        KIND_INT, KIND_INT_IS, KIND_MAP, KIND_NULL, KIND_OPAQUE, KIND_OPTIONAL, KIND_STRING,
        KIND_STRING_IS, KIND_UNION, KIND_UNKNOWN, KIND_VARIANT, MEMBER_SKIP, REPR_BOOL, REPR_BOXED,
        REPR_FLOAT, REPR_INT, REPR_NULL, REPR_PAIR, REPR_REF,
    },
    layout,
    types::{NativeTy, TypeArg, class_layout, field_slots, native_ty, type_argument},
};

/// A class or an enum whose type id a description is waiting for.
#[derive(Clone, Copy)]
pub(crate) enum Described<'db> {
    Class(ClassRef<'db>),
    Enum(EnumRef<'db>),
}

/// A description whose type id is not yet written.
pub(crate) struct Pending<'db> {
    pub(crate) at: u32,
    pub(crate) what: Described<'db>,
    /// The type arguments of a class, and its layout at them.
    pub(crate) arguments: Vec<TypeArg<'db>>,
    pub(crate) layout: Vec<NativeTy<'db>>,
    /// Whether `baml.json` finds the description by the type id.
    pub(crate) listed: bool,
}

/// A class's own `to_json` and `from_json`, in that order.
pub(crate) type Hooks<T> = [Option<T>; 2];

/// The functions `class` provides for `baml.ToJson` and `baml.FromJson`,
/// when it implements either with a body of its own.
pub(crate) fn hook_functions<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    class: ClassRef<'db>,
) -> Hooks<FunctionLoc<'db>> {
    let head = class_head(db, class);
    let mut found = [None; 2];
    // The class's own package holds an implementation of an interface of
    // the standard library.
    for block in package_impl_locs(db, head.root()).iter().copied() {
        let resolution = impl_facts(db, block);
        let Some(facts) = resolution.resolved() else {
            continue;
        };
        let (which, method) = match structural_interface(db, &facts.interface.name) {
            Some(StructuralInterface::ToJson) => (0, "to_json"),
            Some(StructuralInterface::FromJson) => (1, "from_json"),
            _ => continue,
        };
        if !matches!(facts.for_ty_pattern.to_plain(), baml_type::Ty::Class(of, _) if of == head) {
            continue;
        }
        if let Some(function) = facts.methods.iter().copied().find(|function| {
            baml_compiler2_hir::item_data::function_data(db, *function)
                .name
                .as_str()
                == method
        }) {
            found[which] = Some(function);
        }
    }
    found
}

/// The descriptions made so far.
pub(crate) struct JsonTypes<'db> {
    /// The section from [`JSON_BODY`] on.
    body: Vec<u8>,
    /// The classes and enums `baml.json` writes and reads.
    classes: HashMap<ClassRef<'db>, u32>,
    enums: HashMap<EnumRef<'db>, u32>,
    /// The enums it does not, described for a thrown instance to be
    /// written.
    shown_enums: HashMap<EnumRef<'db>, u32>,
    /// The classes it does not, each at its type arguments as a type id
    /// is keyed by them: described for a thrown instance to be written,
    /// and for a native to build one.
    shown_classes: HashMap<(ClassRef<'db>, Vec<TypeArg<'db>>), u32>,
    /// The descriptions whose type id is not yet written.
    pending: Vec<Pending<'db>>,
    /// The type id of each class and enum described, with its description.
    directory: Vec<(u32, u32)>,
    /// The entries the backend made for each class with a `to_json` or a
    /// `from_json` of its own, as the ids it knows them by. `None` while a
    /// program is only being checked: every such class will have them.
    entries: Option<HashMap<ClassRef<'db>, Hooks<u32>>>,
    /// Where the address of an entry goes: the offset in the section, and
    /// the entry.
    addresses: Vec<(u32, u32)>,
}

/// Whether `ty` is a class, or an optional, an array or a map of one: a
/// type whose description names the class.
fn holds_one_class(ty: &NativeTy<'_>) -> bool {
    match ty {
        NativeTy::Class(..) => true,
        NativeTy::Optional(inner) | NativeTy::Array(inner) => holds_one_class(inner),
        NativeTy::Map(_, value) => holds_one_class(value),
        _ => false,
    }
}

fn blocked(what: &str, ty: &RuntimeTy) -> Blocker {
    Blocker::new(format!("json of a {what}"), format!("{ty:?}"))
}

/// The name of a class, an enum or an alias as an error message of the
/// interpreter writes it: without its package when the program declares
/// it, and in full when the standard library does.
fn head_name(db: &dyn baml_compiler2_mir::Db, head: &baml_type::DeclName) -> baml_type::TypeName {
    use baml_compiler2_hir_ty::render::Viewpoint;
    let lang = baml_compiler2_hir::package::lang_roots(db);
    if lang.is(baml_base::LangPackage::Baml, head.root()) {
        Viewpoint::canonical(db).wire_head(head)
    } else {
        Viewpoint::user_facing(db, head.root()).wire_head(head)
    }
}

/// The name an uncaught throw of an instance of `class` is written with.
fn full_class_name<'db>(db: &'db dyn baml_compiler2_mir::Db, class: ClassRef<'db>) -> String {
    let head = class_head(db, class);
    let mut name = Viewpoint::canonical(db)
        .display_label(head.root())
        .to_string();
    for part in head.namespace() {
        name.push('.');
        name.push_str(part.as_str());
    }
    name.push('.');
    name.push_str(head.name().as_str());
    name
}

/// The name of a type as an error message of the interpreter writes it.
fn type_name(db: &dyn baml_compiler2_mir::Db, ty: &RuntimeTy) -> String {
    ty.map_heads(&mut |head| head_name(db, head)).to_string()
}

fn word(value: usize) -> u32 {
    u32::try_from(value).expect("the type table is smaller than 4 GiB")
}

/// How a slot holds a value of a type.
fn repr(ty: &NativeTy<'_>) -> u32 {
    match ty {
        NativeTy::Int | NativeTy::Enum(_) => REPR_INT,
        NativeTy::Float => REPR_FLOAT,
        NativeTy::Bool => REPR_BOOL,
        NativeTy::Null => REPR_NULL,
        NativeTy::Union(_) | NativeTy::Unknown => REPR_BOXED,
        NativeTy::Optional(inner) if inner.is_ref() => repr(inner),
        NativeTy::Optional(_) => REPR_PAIR,
        _ => REPR_REF,
    }
}

/// Whether a missing key reads as `null` for a field of this type.
fn admits_null(db: &dyn baml_compiler2_mir::Db, ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Null | RuntimeTy::Unknown => true,
        RuntimeTy::Union(members) => members.iter().any(|member| admits_null(db, member)),
        RuntimeTy::TypeAlias(head) => is_json_alias(db, head),
        _ => false,
    }
}

/// Whether `head` is the standard library's `baml.json.json`.
pub(crate) fn is_json_alias(db: &dyn baml_compiler2_mir::Db, head: &baml_type::DeclName) -> bool {
    baml_compiler2_hir::package::lang_roots(db).is(baml_base::LangPackage::Baml, head.root())
        && head
            .namespace()
            .iter()
            .map(baml_type::Name::as_str)
            .eq(["json"])
        && head.name().as_str() == "json"
}

impl<'db> JsonTypes<'db> {
    pub(crate) fn new() -> Self {
        Self {
            body: Vec::new(),
            classes: HashMap::new(),
            enums: HashMap::new(),
            shown_enums: HashMap::new(),
            shown_classes: HashMap::new(),
            pending: Vec::new(),
            directory: Vec::new(),
            entries: None,
            addresses: Vec::new(),
        }
    }

    /// The descriptions of a program being compiled, whose classes with a
    /// `to_json` or a `from_json` of their own have these entries.
    pub(crate) fn with_entries(entries: HashMap<ClassRef<'db>, Hooks<u32>>) -> Self {
        Self {
            entries: Some(entries),
            ..Self::new()
        }
    }

    fn here(&self) -> u32 {
        word(JSON_BODY + self.body.len())
    }

    fn push(&mut self, words: &[u32]) {
        for word in words {
            self.body.extend_from_slice(&word.to_ne_bytes());
        }
    }

    fn patch(&mut self, at: u32, value: u32) {
        let at = at as usize - JSON_BODY;
        self.body[at..at + 4].copy_from_slice(&value.to_ne_bytes());
    }

    /// Some text, as its offset and its length.
    fn text(&mut self, text: &str) -> [u32; 2] {
        let at = self.here();
        self.body.extend_from_slice(text.as_bytes());
        self.body.resize(self.body.len().next_multiple_of(4), 0);
        [at, word(text.len())]
    }

    /// A description, its name written first.
    fn description(&mut self, kind: u32, repr: u32, abc: [u32; 3], name: &str, flags: u32) -> u32 {
        let name = self.text(name);
        self.description_named(kind, repr, abc, name, flags)
    }

    /// A description whose name is text already written.
    fn description_named(
        &mut self,
        kind: u32,
        repr: u32,
        abc: [u32; 3],
        [name, length]: [u32; 2],
        flags: u32,
    ) -> u32 {
        let at = self.here();
        self.push(&[kind, repr, abc[0], abc[1], abc[2], name, length, flags]);
        debug_assert_eq!(self.here() - at, word(DESCRIPTION));
        at
    }

    /// The three names of a member: its own, and the ones it is written
    /// under with declared and with aliased names. The same text is
    /// written once.
    fn member_names(&mut self, own: &str, declared: &str, aliased: &str) -> [u32; 6] {
        let own_at = self.text(own);
        let declared_at = if declared == own {
            own_at
        } else {
            self.text(declared)
        };
        let aliased_at = if aliased == own {
            own_at
        } else if aliased == declared {
            declared_at
        } else {
            self.text(aliased)
        };
        [
            own_at[0],
            own_at[1],
            declared_at[0],
            declared_at[1],
            aliased_at[0],
            aliased_at[1],
        ]
    }

    /// The description of `ty`, or what keeps the runtime from writing or
    /// reading a value of it.
    pub(crate) fn describe(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &RuntimeTy,
    ) -> Result<u32, Blocker> {
        let native =
            native_ty(db, ty, &[]).ok_or_else(|| blocked("type with no native form", ty))?;
        let repr = repr(&native);
        let name = type_name(db, ty);
        let plain = |this: &mut Self, kind| this.description(kind, repr, [0; 3], &name, 0);
        Ok(match ty {
            RuntimeTy::Int => plain(self, KIND_INT),
            RuntimeTy::Float => plain(self, KIND_FLOAT),
            RuntimeTy::Bool => plain(self, KIND_BOOL),
            RuntimeTy::Null => plain(self, KIND_NULL),
            RuntimeTy::String => plain(self, KIND_STRING),
            RuntimeTy::Unknown => plain(self, KIND_UNKNOWN),
            RuntimeTy::TypeAlias(head) if is_json_alias(db, head) => {
                self.description(KIND_UNKNOWN, repr, [0; 3], &name, FLAG_JSON)
            }
            RuntimeTy::Literal(Literal::Int(value), _) => {
                let bits = value.cast_unsigned();
                let [low, high] = [bits & 0xFFFF_FFFF, bits >> 32]
                    .map(|half| u32::try_from(half).unwrap_or_default());
                let halves = [low, high, 0];
                self.description(KIND_INT_IS, repr, halves, &name, 0)
            }
            RuntimeTy::Literal(Literal::Bool(value), _) => {
                self.description(KIND_BOOL_IS, repr, [u32::from(*value), 0, 0], &name, 0)
            }
            RuntimeTy::Literal(Literal::String(value), _) => {
                let [at, length] = self.text(value);
                self.description(KIND_STRING_IS, repr, [at, length, 0], &name, 0)
            }
            RuntimeTy::Enum(head) => {
                let enum_ref = enum_ref_of(db, head).ok_or_else(|| blocked("enum", ty))?;
                self.enumeration(db, enum_ref)?
            }
            RuntimeTy::EnumVariant(head, variant) => {
                let enum_ref = enum_ref_of(db, head).ok_or_else(|| blocked("enum", ty))?;
                let index = baml_compiler2_hir_ty::layout::enum_variant_index(
                    db,
                    enum_ref,
                    variant,
                )
                .ok_or_else(|| blocked("enum variant", ty))?;
                let of = self.enumeration(db, enum_ref)?;
                self.description(KIND_VARIANT, repr, [of, index, 0], &name, 0)
            }
            RuntimeTy::Class(head, arguments) => {
                if interface_ref_of(db, head).is_some() {
                    return Err(blocked("value of an interface type", ty));
                }
                if !arguments.is_empty() {
                    return Err(blocked("generic class", ty));
                }
                let class = class_ref_of(db, head).ok_or_else(|| blocked("class", ty))?;
                self.class(db, class)?
            }
            RuntimeTy::List(element) => {
                let element = self.describe(db, element)?;
                self.description(KIND_ARRAY, repr, [element, 0, 0], &name, 0)
            }
            RuntimeTy::Map { key, value, .. } => {
                let key = self.describe(db, key)?;
                let value = self.describe(db, value)?;
                self.description(KIND_MAP, repr, [key, value, 0], &name, 0)
            }
            // `json`, written out one level as MIR has it.
            RuntimeTy::Union(members)
                if members.iter().any(|member| {
                    matches!(member, RuntimeTy::List(element)
                        if matches!(&**element, RuntimeTy::TypeAlias(head) if is_json_alias(db, head)))
                }) && members.iter().any(|member| matches!(member, RuntimeTy::Bigint)) =>
            {
                self.description(KIND_UNKNOWN, repr, [0; 3], "baml.json.json", FLAG_JSON)
            }
            RuntimeTy::Union(members) => {
                if let [RuntimeTy::Null, other] | [other, RuntimeTy::Null] = &members[..]
                    && !matches!(other, RuntimeTy::Null)
                {
                    let other = self.describe(db, other)?;
                    return Ok(self.description(KIND_OPTIONAL, repr, [other, 0, 0], &name, 0));
                }
                let members = members
                    .iter()
                    .map(|member| self.describe(db, member))
                    .collect::<Result<Vec<_>, _>>()?;
                self.union(&members, repr, &name)
            }
            RuntimeTy::Bigint | RuntimeTy::Literal(Literal::Bigint(_), _) => {
                return Err(blocked("bigint", ty));
            }
            RuntimeTy::Uint8Array => return Err(blocked("uint8array", ty)),
            RuntimeTy::Tuple(_) => return Err(blocked("tuple", ty)),
            RuntimeTy::Interface(..) => return Err(blocked("value of an interface type", ty)),
            RuntimeTy::TypeAlias(_) => return Err(blocked("recursive alias", ty)),
            RuntimeTy::Function { .. } => return Err(blocked("function", ty)),
            RuntimeTy::TypeVar(_) => return Err(blocked("type parameter", ty)),
            _ => return Err(blocked("type the native runtime has no JSON form for", ty)),
        })
    }

    fn union(&mut self, members: &[u32], repr: u32, name: &str) -> u32 {
        let list = self.here();
        self.push(members);
        self.description(KIND_UNION, repr, [list, word(members.len()), 0], name, 0)
    }

    /// The description of the static type of a value, as far as its
    /// representation says: zero for a value that only its header
    /// describes.
    pub(crate) fn describe_native(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> Result<u32, Blocker> {
        if *ty == NativeTy::Unknown {
            return Ok(0);
        }
        self.describe_held(db, ty)
    }

    /// [`Self::describe_native`], for a type something described holds:
    /// `unknown` has a description there.
    fn describe_held(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        ty: &NativeTy<'db>,
    ) -> Result<u32, Blocker> {
        let repr = repr(ty);
        let unsupported = |what: &str| Blocker::new(format!("json of a {what}"), "");
        Ok(match ty {
            NativeTy::Int => self.description(KIND_INT, repr, [0; 3], "int", 0),
            NativeTy::Float => self.description(KIND_FLOAT, repr, [0; 3], "float", 0),
            NativeTy::Bool => self.description(KIND_BOOL, repr, [0; 3], "bool", 0),
            NativeTy::Null => self.description(KIND_NULL, repr, [0; 3], "null", 0),
            NativeTy::String => self.description(KIND_STRING, repr, [0; 3], "string", 0),
            NativeTy::Unknown => self.description(KIND_UNKNOWN, repr, [0; 3], "unknown", 0),
            NativeTy::Enum(enum_ref) => self.enumeration(db, *enum_ref)?,
            NativeTy::Class(class, arguments) if arguments.is_empty() => self.class(db, *class)?,
            NativeTy::Class(..) => return Err(unsupported("generic class")),
            NativeTy::Array(element) => {
                let element = self.describe_held(db, element)?;
                self.description(KIND_ARRAY, repr, [element, 0, 0], "", 0)
            }
            NativeTy::Map(key, value) => {
                let key = self.describe_held(db, key)?;
                let value = self.describe_held(db, value)?;
                self.description(KIND_MAP, repr, [key, value, 0], "", 0)
            }
            NativeTy::Optional(inner) => {
                let inner = self.describe_held(db, inner)?;
                self.description(KIND_OPTIONAL, repr, [inner, 0, 0], "", 0)
            }
            NativeTy::Union(members) => {
                let members = members
                    .iter()
                    .map(|member| self.describe_held(db, member))
                    .collect::<Result<Vec<_>, _>>()?;
                self.union(&members, repr, "")
            }
            NativeTy::Tuple(_) => return Err(unsupported("tuple")),
            NativeTy::Bigint => return Err(unsupported("bigint")),
            NativeTy::Bytes => return Err(unsupported("uint8array")),
            NativeTy::Function(_) | NativeTy::Cell(_) => return Err(unsupported("function")),
            NativeTy::Future(_) => return Err(unsupported("future")),
        })
    }

    fn enumeration(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
    ) -> Result<u32, Blocker> {
        if let Some(known) = self.enums.get(&enum_ref) {
            return Ok(*known);
        }
        let head = enum_head(db, enum_ref);
        let name = head_name(db, &head).head_display_name();
        if override_of(json_overrides(db, head.root()), &head).is_some() {
            return Err(Blocker::new(
                "json of a type with its own to_json or from_json",
                name,
            ));
        }
        let DeclRef::Source(enum_loc) = enum_ref else {
            return Err(Blocker::new("json of an enum without source", name));
        };
        let variants = &baml_compiler2_hir::item_data::enum_data(db, enum_loc).variants;
        let mut records = Vec::new();
        for variant in variants {
            let own = variant.name.as_str();
            let rename = variant.attrs.json_rename.as_deref();
            let declared = rename.unwrap_or(own);
            let aliased = rename.unwrap_or(variant.attrs.schema.alias.as_deref().unwrap_or(own));
            records.extend(self.member_names(own, declared, aliased));
            records.push(if variant.attrs.skip { MEMBER_SKIP } else { 0 });
        }
        // The enum's name with its package follows its records.
        let full = baml_compiler2_hir_ty::render::Viewpoint::canonical(db)
            .wire_head(&head)
            .to_string();
        records.extend(self.text(&full));
        let table = self.here();
        self.push(&records);
        let at = self.description(
            KIND_ENUM,
            REPR_INT,
            [0, table, word(variants.len())],
            &name,
            0,
        );
        self.enums.insert(enum_ref, at);
        self.pending.push(Pending {
            at,
            what: Described::Enum(enum_ref),
            arguments: Vec::new(),
            layout: Vec::new(),
            listed: true,
        });
        Ok(at)
    }

    fn class(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
    ) -> Result<u32, Blocker> {
        if let Some(known) = self.classes.get(&class) {
            return Ok(*known);
        }
        let head = class_head(db, class);
        let name = head_name(db, &head).head_display_name();
        let blocked =
            |what: &str, detail: String| Err(Blocker::new(format!("json of a {what}"), detail));
        let entries = if override_of(json_overrides(db, head.root()), &head).is_some() {
            match &self.entries {
                None => Some([None; 2]),
                Some(entries) => match entries.get(&class) {
                    Some(entries) => Some(*entries),
                    None => {
                        return blocked(
                            "class whose own to_json or from_json the program does not hold",
                            name,
                        );
                    }
                },
            }
        } else {
            None
        };
        let DeclRef::Source(class_loc) = class else {
            return blocked("class without source", name);
        };
        if head
            .namespace()
            .first()
            .is_some_and(|first| first.as_str() == "media")
            && baml_compiler2_hir::package::lang_roots(db)
                .is(baml_base::LangPackage::Baml, head.root())
        {
            return blocked("media value", name);
        }
        let omit_nulls = baml_compiler2_hir::item_data::class_data(db, class_loc)
            .attrs
            .json_omit_nulls;
        let fields = baml_compiler2_hir_ty::lower::resolve_class_fields(db, class_loc);
        let layout = class_layout(db, class, &[])?;
        let slots = field_slots(&layout);
        // The description goes in first, for a field that holds the class
        // again to refer to.
        let mark = self.body.len();
        // The name with its package is written once, and the name without
        // is its end when it is that.
        let full = full_class_name(db, class);
        let full_at = self.text(&full);
        let short_at = if full.ends_with(name.as_str()) {
            [full_at[0] + word(full.len() - name.len()), word(name.len())]
        } else {
            self.text(&name)
        };
        let at = self.description_named(KIND_CLASS, REPR_REF, [0; 3], short_at, 0);
        self.classes.insert(class, at);
        let aliases = resolved_aliases_for_package(db, head.root());
        let mut records = Vec::new();
        for ((own, ty, attrs), slot) in fields.iter().zip(slots) {
            if attrs.json_flatten || attrs.json_fallback {
                self.forget(mark);
                return blocked(
                    "class with a @json.flatten or @json.fallback field",
                    format!("{name}.{own}"),
                );
            }
            let runtime = aliases.0.convert(ty);
            let field = match self.describe(db, &runtime) {
                Ok(field) => field,
                Err(blocker) => {
                    self.forget(mark);
                    return Err(blocker);
                }
            };
            let own = own.as_str();
            let rename = attrs.json_rename.as_deref();
            let declared = rename.unwrap_or(own);
            let aliased = rename.unwrap_or(attrs.schema.alias.as_deref().unwrap_or(own));
            let mut flags = if attrs.skip { MEMBER_SKIP } else { 0 };
            if omit_nulls && is_optional(db, ty) {
                flags |= FIELD_OMIT_NULL;
            }
            if admits_null(db, &runtime) {
                flags |= FIELD_MAY_BE_ABSENT;
            }
            records.extend(self.member_names(own, declared, aliased));
            records.extend([field, word(layout::field_offset(slot)), flags]);
        }
        records.extend(full_at);
        if let Some(entries) = entries {
            // Two addresses, each written by the linker.
            self.body.resize(self.body.len().next_multiple_of(8), 0);
            let hooks = self.here();
            self.push(&[0; 4]);
            for (entry, offset) in entries.into_iter().zip([HOOK_TO_JSON, HOOK_FROM_JSON]) {
                if let Some(entry) = entry {
                    self.addresses.push((hooks + word(offset), entry));
                }
            }
            records.push(hooks);
            self.patch(at + word(DESCRIPTION_FLAGS), FLAG_HOOKS);
        }
        let table = self.here();
        self.push(&records);
        self.patch(at + word(DESCRIPTION_A) + 4, table);
        self.patch(at + word(DESCRIPTION_A) + 8, word(fields.len()));
        self.pending.push(Pending {
            at,
            what: Described::Class(class),
            arguments: Vec::new(),
            layout,
            listed: true,
        });
        Ok(at)
    }

    /// The description of the class `class` at `arguments`, whose type is
    /// `type_id` and whose layout is `fields`: the one `baml.json` writes
    /// and reads its instances by when there is one, and otherwise one
    /// that says only how a thrown instance is written.
    pub(crate) fn class_type(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
        arguments: &[TypeArg<'db>],
        type_id: usize,
        fields: &[NativeTy<'db>],
    ) -> u32 {
        if arguments.is_empty()
            && let Ok(at) = self.class(db, class)
        {
            return at;
        }
        let at = self.shown_class(db, class, arguments, fields);
        self.patch(at + word(DESCRIPTION_A), word(type_id));
        at
    }

    /// The description of a class `baml.json` has no form for, at these
    /// type arguments and with this layout, made the first time it is
    /// asked for. Its type id is written by whoever has it.
    fn shown_class(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
        arguments: &[TypeArg<'db>],
        fields: &[NativeTy<'db>],
    ) -> u32 {
        let key = (
            class,
            arguments.iter().cloned().map(type_argument).collect(),
        );
        if let Some(known) = self.shown_classes.get(&key) {
            return *known;
        }
        let name = self.text(&full_class_name(db, class));
        let at = self.description_named(KIND_CLASS, REPR_REF, [0; 3], name, FLAG_SHOWN_ONLY);
        // Before its fields are described: one of them may hold the class.
        self.shown_classes.insert(key, at);
        let mut records = Vec::new();
        let declared = class_fields(db, class);
        for (((own, declared), ty), slot) in declared.iter().zip(fields).zip(field_slots(fields)) {
            let field = if matches!(declared, baml_type::Ty::RustType) {
                // Rust state: nothing writes it, and a native that builds
                // the class must give it.
                self.description(KIND_OPAQUE, REPR_REF, [0; 3], "", 0)
            } else {
                self.shown(db, ty)
            };
            let own = self.text(own.as_str());
            for _ in 0..3 {
                records.extend(own);
            }
            records.extend([field, word(layout::field_offset(slot)), 0]);
        }
        records.extend(name);
        let table = self.here();
        self.push(&records);
        self.patch(at + word(DESCRIPTION_A) + 4, table);
        self.patch(at + word(DESCRIPTION_A) + 8, word(fields.len()));
        at
    }

    /// The description of the type of a field of a class that is shown
    /// only: the one `baml.json` has when it has one, and otherwise what
    /// writing the field takes.
    fn shown(&mut self, db: &'db dyn baml_compiler2_mir::Db, ty: &NativeTy<'db>) -> u32 {
        let mark = self.body.len();
        if let Ok(at) = self.describe_held(db, ty) {
            return at;
        }
        self.forget(mark);
        match ty {
            NativeTy::Enum(enum_ref) => {
                if let Some(known) = self.shown_enums.get(enum_ref) {
                    return *known;
                }
                let at = self.description(KIND_ENUM, REPR_INT, [0; 3], "", FLAG_SHOWN_ONLY);
                self.shown_enums.insert(*enum_ref, at);
                self.pending.push(Pending {
                    at,
                    what: Described::Enum(*enum_ref),
                    arguments: Vec::new(),
                    layout: Vec::new(),
                    listed: false,
                });
                at
            }
            NativeTy::Optional(inner) if !inner.is_ref() => {
                let inner = self.shown(db, inner);
                self.description(KIND_OPTIONAL, REPR_PAIR, [inner, 0, 0], "", 0)
            }
            // A field that holds instances of one class says which, whether
            // or not `baml.json` reads the class: a native that builds the
            // field's value finds the class there.
            ty if holds_one_class(ty) => self.shown_holder(db, ty),
            // Its header says what it is.
            ty if ty.is_ref() => self.description(KIND_UNKNOWN, repr(ty), [0; 3], "", 0),
            _ => self.description(KIND_OPAQUE, REPR_INT, [0; 3], "", 0),
        }
    }

    /// The description of a type [`holds_one_class`] is true of: the
    /// class's own, under what holds it. A class that cannot be laid out
    /// is described as any reference is.
    fn shown_holder(&mut self, db: &'db dyn baml_compiler2_mir::Db, ty: &NativeTy<'db>) -> u32 {
        match ty {
            NativeTy::Class(class, arguments) => {
                let Ok(fields) = class_layout(db, *class, arguments) else {
                    return self.description(KIND_UNKNOWN, repr(ty), [0; 3], "", 0);
                };
                let known = self.shown_classes.len();
                let at = self.shown_class(db, *class, arguments, &fields);
                if self.shown_classes.len() != known {
                    // Described here first: the backend gives it a type.
                    self.pending.push(Pending {
                        at,
                        what: Described::Class(*class),
                        arguments: arguments.clone(),
                        layout: fields,
                        listed: false,
                    });
                }
                at
            }
            NativeTy::Optional(inner) => {
                let inner = self.shown_holder(db, inner);
                self.description(KIND_OPTIONAL, repr(ty), [inner, 0, 0], "", 0)
            }
            NativeTy::Array(element) => {
                let element = self.shown_holder(db, element);
                self.description(KIND_ARRAY, repr(ty), [element, 0, 0], "", 0)
            }
            NativeTy::Map(key, value) => {
                let key = self.shown(db, key);
                let value = self.shown_holder(db, value);
                self.description(KIND_MAP, repr(ty), [key, value, 0], "", 0)
            }
            _ => self.description(KIND_UNKNOWN, repr(ty), [0; 3], "", 0),
        }
    }

    /// Take back everything described since the body was `mark` bytes: a
    /// class one of whose fields cannot be described, and what was
    /// described on the way to finding that out, which may refer to it.
    fn forget(&mut self, mark: usize) {
        let kept = |at: &u32| (*at as usize) < JSON_BODY + mark;
        self.body.truncate(mark);
        self.classes.retain(|_, at| kept(at));
        self.enums.retain(|_, at| kept(at));
        self.shown_enums.retain(|_, at| kept(at));
        self.shown_classes.retain(|_, at| kept(at));
        self.pending.retain(|pending| kept(&pending.at));
        self.addresses.retain(|(at, _)| kept(at));
    }

    /// Where in the section the address of each entry goes.
    pub(crate) fn addresses(&self) -> &[(u32, u32)] {
        &self.addresses
    }

    /// The classes and enums described since the last call, for
    /// [`Self::identify`] to be told the type id of each.
    pub(crate) fn take_pending(&mut self) -> Vec<Pending<'db>> {
        std::mem::take(&mut self.pending)
    }

    /// Give the description at `at` the type id of what it describes,
    /// and list it for `baml.json` to find by that when `listed`.
    pub(crate) fn identify(&mut self, at: u32, type_id: usize, listed: bool) {
        self.patch(at + word(DESCRIPTION_A), word(type_id));
        if listed {
            self.directory.push((word(type_id), at));
        }
    }

    /// The classes and enums described so far.
    pub(crate) fn described(&self) -> impl Iterator<Item = Described<'db>> + '_ {
        self.classes
            .keys()
            .map(|class| Described::Class(*class))
            .chain(self.enums.keys().map(|enum_ref| Described::Enum(*enum_ref)))
    }

    /// The section, to follow the last type of the table.
    pub(crate) fn section(mut self) -> Vec<u8> {
        self.directory.sort_unstable();
        let directory = self.here();
        let mut section = Vec::with_capacity(JSON_BODY + self.body.len());
        for word in [JSON_VERSION, directory, word(self.directory.len()), 0] {
            section.extend_from_slice(&word.to_ne_bytes());
        }
        section.append(&mut self.body);
        for (type_id, at) in self.directory {
            section.extend_from_slice(&type_id.to_ne_bytes());
            section.extend_from_slice(&at.to_ne_bytes());
        }
        section
    }
}

/// Whether a call of `native` passes descriptions for its type arguments,
/// where any other native is passed type ids: `baml.json`, and the two
/// defaults a `to_json` and a `from_json` fall back on.
pub(crate) fn takes_descriptions(native: &crate::natives::Native) -> bool {
    native.path.starts_with("baml.json.")
        || matches!(
            native.path,
            "baml._to_json_default" | "baml._from_json_structural_default"
        )
}

/// The classes and enums a call of a `baml.json` native can make values of
/// or meet, or what keeps the runtime from writing or reading a type the
/// call names.
pub(crate) fn check_call<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    body: &crate::types::Body<'_, 'db>,
    native: &crate::natives::Native,
    args: &[baml_compiler2_mir::Operand<'db>],
    ntypeargs: usize,
) -> Result<Vec<Described<'db>>, Blocker> {
    if !takes_descriptions(native) {
        return Ok(Vec::new());
    }
    let mut types = JsonTypes::new();
    let (type_arguments, values) = args.split_at(ntypeargs.min(args.len()));
    for argument in type_arguments {
        match body.type_value(argument) {
            // What this function was itself called with.
            Some(baml_compiler2_mir::TyTemplate::TypeArgRef(_)) => {}
            Some(template) if template.is_fully_concrete() => {
                types.describe(db, &template.substitute_symbolic(&[]))?;
            }
            _ => {
                return Err(Blocker::new(
                    "json of a type built from a type parameter",
                    native.path,
                ));
            }
        }
    }
    if let Some(signature) = &native.signature {
        for (argument, ty) in values.iter().zip(signature.params) {
            if matches!(ty, crate::natives::Ty::Typed) {
                let found = crate::types::operand_ty(db, body, argument)?;
                types.describe_native(db, &found)?;
            }
        }
    }
    Ok(types.described().collect())
}
