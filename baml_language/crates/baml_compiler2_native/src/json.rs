//! What `baml.json` is told about the types it writes and reads: the
//! section of the type table laid out in the runtime's `json_layout.rs`.
//!
//! A description is made from the type as MIR has it, so that a literal
//! type, an alias for `json` and a field's attributes are still there to
//! describe. What the runtime cannot yet write or read (a type with its own
//! `to_json` or `from_json`, which it would have to call back into compiled
//! code for; a `bigint`; a tuple) is found here, when the program is
//! checked, and reported as a blocker.

use std::collections::HashMap;

use baml_compiler2_hir::loc::DeclRef;
use baml_compiler2_hir_ty::{
    extern_loc::{ClassRef, EnumRef},
    json_attrs::{is_optional, json_overrides, override_of},
    layout::{class_head, class_ref_of, enum_head, enum_ref_of, interface_ref_of},
};
use baml_compiler2_mir::{RuntimeTy, resolved_aliases_for_package};
use baml_type::{HeadDisplay, Literal};

use crate::{
    Blocker,
    json_layout::{
        DESCRIPTION, DESCRIPTION_A, FIELD_MAY_BE_ABSENT, FIELD_OMIT_NULL, FLAG_JSON, JSON_BODY,
        JSON_VERSION, KIND_ARRAY, KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT,
        KIND_INT, KIND_INT_IS, KIND_MAP, KIND_NULL, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS,
        KIND_UNION, KIND_UNKNOWN, KIND_VARIANT, MEMBER_SKIP, REPR_BOOL, REPR_BOXED, REPR_FLOAT,
        REPR_INT, REPR_NULL, REPR_PAIR, REPR_REF,
    },
    layout,
    types::{NativeTy, class_layout, field_slots, native_ty},
};

/// A class or an enum whose type id a description is waiting for.
#[derive(Clone, Copy)]
pub(crate) enum Described<'db> {
    Class(ClassRef<'db>),
    Enum(EnumRef<'db>),
}

/// The descriptions made so far.
pub(crate) struct JsonTypes<'db> {
    /// The section from [`JSON_BODY`] on.
    body: Vec<u8>,
    classes: HashMap<ClassRef<'db>, u32>,
    enums: HashMap<EnumRef<'db>, u32>,
    /// The descriptions whose type id is not yet written.
    pending: Vec<(u32, Described<'db>)>,
    /// The type id of each class and enum described, with its description.
    directory: Vec<(u32, u32)>,
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
            pending: Vec::new(),
            directory: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.body.is_empty()
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
        let [name, length] = self.text(name);
        let at = self.here();
        self.push(&[kind, repr, abc[0], abc[1], abc[2], name, length, flags]);
        debug_assert_eq!(self.here() - at, word(DESCRIPTION));
        at
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
            records.extend(self.text(own));
            records.extend(self.text(declared));
            records.extend(self.text(aliased));
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
        self.pending.push((at, Described::Enum(enum_ref)));
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
        if override_of(json_overrides(db, head.root()), &head).is_some() {
            return blocked("type with its own to_json or from_json", name);
        }
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
        let at = self.description(KIND_CLASS, REPR_REF, [0; 3], &name, 0);
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
            records.extend(self.text(own));
            records.extend(self.text(declared));
            records.extend(self.text(aliased));
            records.extend([field, word(layout::field_offset(slot)), flags]);
        }
        let table = self.here();
        self.push(&records);
        self.patch(at + word(DESCRIPTION_A) + 4, table);
        self.patch(at + word(DESCRIPTION_A) + 8, word(fields.len()));
        self.pending.push((at, Described::Class(class)));
        Ok(at)
    }

    /// Take back everything described since the body was `mark` bytes: a
    /// class one of whose fields cannot be described, and what was
    /// described on the way to finding that out, which may refer to it.
    fn forget(&mut self, mark: usize) {
        let kept = |at: &u32| (*at as usize) < JSON_BODY + mark;
        self.body.truncate(mark);
        self.classes.retain(|_, at| kept(at));
        self.enums.retain(|_, at| kept(at));
        self.pending.retain(|(at, _)| kept(at));
    }

    /// The classes and enums described since the last call, for
    /// [`Self::identify`] to be told the type id of each.
    pub(crate) fn take_pending(&mut self) -> Vec<(u32, Described<'db>)> {
        std::mem::take(&mut self.pending)
    }

    /// Give the description at `at` the type id of what it describes.
    pub(crate) fn identify(&mut self, at: u32, type_id: usize) {
        self.patch(at + word(DESCRIPTION_A), word(type_id));
        self.directory.push((word(type_id), at));
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
/// where any other native is passed type ids.
pub(crate) fn takes_descriptions(native: &crate::natives::Native) -> bool {
    native.path.starts_with("baml.json.")
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
