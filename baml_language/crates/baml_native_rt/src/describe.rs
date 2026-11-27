//! Reading the descriptions the compiler leaves in the program's table of
//! types: what `baml.json` writes and reads values by, and what an uncaught
//! throw of an instance is written from. See [`crate::json_layout`].

#![allow(
    clippy::cast_possible_truncation,
    reason = "every word read here is a `u32` of a description"
)]

use crate::{
    heap::{Mutator, Value, load},
    json_layout::{
        DESCRIPTION_A, DESCRIPTION_B, DESCRIPTION_C, DESCRIPTION_FLAGS, DESCRIPTION_NAME,
        DESCRIPTION_REPR, FIELD_FLAGS, FIELD_MAY_BE_ABSENT, FIELD_OFFSET, FIELD_OMIT_NULL,
        FIELD_RECORD, FIELD_TYPE, FLAG_HOOKS, FLAG_SHOWN_ONLY, HOOKS, KIND_BOOL, KIND_BOOL_IS,
        KIND_CLASS, KIND_ENUM, KIND_FLOAT, KIND_INT, KIND_INT_IS, KIND_NULL, KIND_OPTIONAL,
        KIND_STRING, KIND_STRING_IS, KIND_UNION, KIND_UNKNOWN, KIND_VARIANT, MEMBER_ALIASED,
        MEMBER_DECLARED, MEMBER_SKIP, VARIANT_FLAGS, VARIANT_RECORD,
    },
    layout::{
        CLASS_DESCRIPTION, INSTANCE_CLASS, PROGRAM_CLASSES, PROGRAM_DESCRIPTIONS, STRING_TAGS_END,
        TAG_BOOL, TAG_ENUM_VARIANT, TAG_FLOAT, TAG_INSTANCE, TAG_INT, TAG_NULL, TAG_STRING,
        TYPE_CLASS, TYPE_DESCRIPTION, TYPE_ID_MASK, TYPE_KIND_UNION, TYPE_TAG, VARIANT_ENUM,
        VARIANT_INDEX, WELL_KNOWN, WORD,
    },
    native::{Scope, TypeRef, drive::BamlFn},
};

/// Read the `u32` at `address`.
///
/// # Safety
///
/// `address` is inside the section of descriptions.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read_unaligned() as usize }
}

/// The descriptions of a program.
#[derive(Clone, Copy)]
pub(crate) struct Types {
    /// The program's table of types.
    table: usize,
    /// The section of descriptions.
    section: usize,
}

/// One description.
#[derive(Clone, Copy)]
pub(crate) struct Desc {
    section: usize,
    at: usize,
}

/// A variant of a described enum, or a field of a described class.
pub(crate) struct Member {
    pub(crate) name: &'static str,
    declared: &'static str,
    aliased: &'static str,
    pub(crate) skip: bool,
    /// A field's type, where it is in an instance, and its other flags.
    pub(crate) ty: Option<Desc>,
    pub(crate) offset: usize,
    pub(crate) omit_null: bool,
    pub(crate) may_be_absent: bool,
}

impl Member {
    /// The name it is written under.
    pub(crate) fn key(&self, aliased: bool) -> &'static str {
        if aliased { self.aliased } else { self.declared }
    }

    /// Whether it has a name at all.
    pub(crate) fn shown(&self, aliased: bool) -> bool {
        !(aliased && self.skip)
    }
}

impl Types {
    pub(crate) fn of(scope: &Scope<'_>) -> Self {
        Self::of_table(scope.type_table())
    }

    /// The descriptions of the program whose table of types is at `table`.
    pub(crate) fn of_table(table: usize) -> Self {
        Self {
            table,
            section: table + PROGRAM_DESCRIPTIONS,
        }
    }

    /// The description at the address `at`, which a class object or a type
    /// object holds: nothing for zero.
    fn at(&self, at: usize) -> Option<Desc> {
        (at != 0).then_some(Desc {
            section: self.section,
            at,
        })
    }

    /// The description of the type a call named, when the call named one
    /// and it has one. That of a class or an enum can be one `baml.json`
    /// does not write or read by ([`Desc::shown_only`]).
    pub(crate) fn described(&self, ty: TypeRef) -> Option<Desc> {
        if ty.0 == 0 {
            return None;
        }
        // SAFETY: a type a call names is a live type object.
        self.at(unsafe { load(ty.0 + TYPE_DESCRIPTION) })
    }

    /// Whether `value` is of the type the type object at `ty` is of. A
    /// type with a description is decided by it, and a union without one,
    /// which is one made when the program runs, by its members. Any other
    /// is decided by the tag and the class object its type object names,
    /// when it names a tag: an array of one element type or a function
    /// type has no value this knows of.
    ///
    /// # Safety
    ///
    /// `ty` is a live type object, and `value` a value as a slot holds
    /// one: when its tag is a pointer's, its payload is a live object.
    pub(crate) unsafe fn admits(&self, ty: usize, value: Value) -> bool {
        // SAFETY: the caller's contract; the arguments of a live type
        // object are live type objects, and an instance and a variant hold
        // the address of a class object in their second word.
        unsafe {
            if let Some(described) = self.described(TypeRef(ty)) {
                return described.admits(value);
            }
            if crate::types::kind(ty) == TYPE_KIND_UNION {
                return crate::types::arguments(ty)
                    .iter()
                    .any(|member| self.admits(*member, value));
            }
            let wanted = load(ty + TYPE_TAG);
            let class = load(ty + TYPE_CLASS);
            let is_string = value.is_pointer() && value.id() < STRING_TAGS_END;
            let id = if is_string { TAG_STRING } else { value.id() };
            wanted != 0
                && id == wanted
                && (class == 0 || load(value.payload + INSTANCE_CLASS) == class)
        }
    }

    /// The description `baml.json` writes and reads the object at `object`
    /// by: that of the class of an instance or of the enum of a variant,
    /// when it has one that is not shown only. Nothing for any other
    /// object.
    ///
    /// # Safety
    ///
    /// `object` is a live object.
    pub(crate) unsafe fn of_object(&self, object: usize) -> Option<Desc> {
        // SAFETY: the caller's contract: an instance and a variant hold
        // the address of a class object in their second word.
        unsafe {
            if !matches!(load(object) & TYPE_ID_MASK, TAG_INSTANCE | TAG_ENUM_VARIANT) {
                return None;
            }
            self.of_class(load(object + INSTANCE_CLASS))
                .filter(|described| !described.shown_only())
        }
    }

    /// The description of the class, or of the enum, whose class object is
    /// at `class`, when it has one: one shown only included.
    ///
    /// # Safety
    ///
    /// `class` is a live class object.
    pub(crate) unsafe fn of_class(&self, class: usize) -> Option<Desc> {
        // SAFETY: the caller's contract.
        self.at(unsafe { load(class + CLASS_DESCRIPTION) })
    }

    /// The class object of the class at `position` of the program's list:
    /// those of [`WELL_KNOWN`], and then those of
    /// [`crate::layout::NAMED`]. Zero for one the program does not name.
    fn listed(&self, position: usize) -> usize {
        // SAFETY: the table starts with a word for each.
        unsafe { load(self.table + PROGRAM_CLASSES + position * WORD) }
    }

    /// The class object of the class at `index` of [`WELL_KNOWN`].
    pub(crate) fn well_known(&self, index: usize) -> usize {
        let class = self.listed(index);
        assert!(
            class != 0,
            "the program names the standard library's panics and errors"
        );
        class
    }

    /// The class object of the class at `index` of
    /// [`crate::layout::NAMED`], when the program names it.
    pub(crate) fn named(&self, index: usize) -> Option<usize> {
        Some(self.listed(WELL_KNOWN.len() + index)).filter(|class| *class != 0)
    }
}

impl Desc {
    fn field(self, offset: usize) -> usize {
        // SAFETY: a description is eight words.
        unsafe { word(self.at + offset) }
    }

    fn text(self, offset: usize, length: usize) -> &'static str {
        // SAFETY: the compiler put `length` bytes of UTF-8 there, in the
        // executable's data.
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                (self.section + offset) as *const u8,
                length,
            ))
        }
    }

    fn at(self, offset: usize) -> Self {
        Self {
            section: self.section,
            at: self.section + offset,
        }
    }

    pub(crate) fn kind(self) -> u32 {
        self.field(0) as u32
    }

    pub(crate) fn repr(self) -> u32 {
        self.field(DESCRIPTION_REPR) as u32
    }

    pub(crate) fn a(self) -> usize {
        self.field(DESCRIPTION_A)
    }

    /// The class object of a class or of an enum, whose address is in the
    /// word `a` locates.
    pub(crate) fn class_object(self) -> usize {
        debug_assert!(self.is_class() || self.is_enum());
        // SAFETY: the `a` of a class and of an enum is the offset in the
        // section of a word that holds the address.
        unsafe { load(self.section + self.a()) }
    }

    /// Whether the description of a class or an enum says only how a value
    /// is written when it is thrown: `baml.json` has no form for it.
    pub(crate) fn shown_only(self) -> bool {
        (self.is_class() || self.is_enum())
            && self.field(DESCRIPTION_FLAGS) as u32 & FLAG_SHOWN_ONLY != 0
    }

    pub(crate) fn b(self) -> usize {
        self.field(DESCRIPTION_B)
    }

    /// The type's name, as an error message writes it.
    pub(crate) fn name(self) -> &'static str {
        self.text(
            self.field(DESCRIPTION_NAME),
            self.field(DESCRIPTION_NAME + 4),
        )
    }

    /// The name of an enum or a class with its package: what one message
    /// writes of an enum, and an uncaught throw of an instance.
    pub(crate) fn full_name(self) -> &'static str {
        let record = if self.is_class() {
            FIELD_RECORD
        } else {
            VARIANT_RECORD
        };
        let after = self.section + self.b() + self.count() * record;
        // SAFETY: the records of an enum and of a class are followed by
        // where its full name is.
        unsafe { self.text(word(after), word(after + 4)) }
    }

    /// The entry of the class's own `to_json` or `from_json`, by where its
    /// address is after the class's records: `None` for a class that
    /// leaves it to the language, and for anything not a class.
    pub(crate) fn hook(self, which: usize) -> Option<BamlFn> {
        if !self.is_class() || self.field(DESCRIPTION_FLAGS) as u32 & FLAG_HOOKS == 0 {
            return None;
        }
        let after = self.section + self.b() + self.count() * FIELD_RECORD;
        // SAFETY: a class with the flag has, after its records, where
        // both addresses are.
        BamlFn::at(unsafe { load(self.section + word(after + HOOKS) + which) })
    }

    pub(crate) fn is_enum(self) -> bool {
        self.kind() == KIND_ENUM
    }

    pub(crate) fn is_class(self) -> bool {
        self.kind() == KIND_CLASS
    }

    /// Whether the type is `json` or `unknown`: any value at all.
    pub(crate) fn is_any(self) -> bool {
        self.kind() == KIND_UNKNOWN
    }

    /// The description `a` or `b` names: an array's elements, a map's
    /// keys or values, what an optional is of, a variant's enum.
    pub(crate) fn of_a(self) -> Self {
        self.at(self.a())
    }

    pub(crate) fn of_b(self) -> Self {
        self.at(self.b())
    }

    /// The text of a string literal type.
    pub(crate) fn literal(self) -> &'static str {
        self.text(self.a(), self.b())
    }

    /// The members of a union, in declaration order, with the members of
    /// a union among them in its place.
    pub(crate) fn members(self, into: &mut Vec<Self>) {
        match self.kind() {
            KIND_UNION => {
                for index in 0..self.b() {
                    // SAFETY: a union's `a` is a list of `b` offsets.
                    let member = unsafe { word(self.section + self.a() + index * 4) };
                    self.at(member).members(into);
                }
            }
            _ => into.push(self),
        }
    }

    /// How many variants an enum has, or fields a class.
    pub(crate) fn count(self) -> usize {
        self.field(DESCRIPTION_C)
    }

    fn member(self, record: usize, is_field: bool) -> Member {
        let text = |offset: usize| {
            // SAFETY: a record starts with three texts, each an offset
            // and a length.
            unsafe { self.text(word(record + offset), word(record + offset + 4)) }
        };
        // SAFETY: the record is one of this description's.
        unsafe {
            let flags = word(record + if is_field { FIELD_FLAGS } else { VARIANT_FLAGS }) as u32;
            Member {
                name: text(0),
                declared: text(MEMBER_DECLARED),
                aliased: text(MEMBER_ALIASED),
                skip: flags & MEMBER_SKIP != 0,
                ty: is_field.then(|| self.at(word(record + FIELD_TYPE))),
                offset: if is_field {
                    word(record + FIELD_OFFSET)
                } else {
                    0
                },
                omit_null: flags & FIELD_OMIT_NULL != 0,
                may_be_absent: flags & FIELD_MAY_BE_ABSENT != 0,
            }
        }
    }

    /// Variant `index` of an enum, when it has that many.
    pub(crate) fn variant(self, index: usize) -> Option<Member> {
        (index < self.count())
            .then(|| self.member(self.section + self.b() + index * VARIANT_RECORD, false))
    }

    /// The variants of an enum.
    pub(crate) fn variants(self) -> impl Iterator<Item = Member> {
        (0..self.count()).filter_map(move |index| self.variant(index))
    }

    /// The fields of a class, in declaration order.
    pub(crate) fn fields(self) -> impl Iterator<Item = Member> {
        (0..self.count())
            .map(move |index| self.member(self.section + self.b() + index * FIELD_RECORD, true))
    }

    /// Where field `index` of a class is in an instance, and its type:
    /// two words of its record, read without its names.
    pub(crate) fn field_place(self, index: usize) -> Option<(usize, Self)> {
        if index >= self.count() {
            return None;
        }
        let record = self.section + self.b() + index * FIELD_RECORD;
        // SAFETY: the record is one of this class's.
        unsafe {
            Some((
                word(record + FIELD_OFFSET),
                self.at(word(record + FIELD_TYPE)),
            ))
        }
    }

    /// Whether a map with keys of this type is a JSON object: `string`, or
    /// string literals.
    pub(crate) fn is_string_key(self) -> bool {
        match self.kind() {
            KIND_STRING | KIND_STRING_IS => true,
            KIND_UNION => {
                let mut members = Vec::new();
                self.members(&mut members);
                !members.is_empty() && members.iter().all(|member| member.is_string_key())
            }
            _ => false,
        }
    }

    /// Whether `value` is a value of the type, as far as a description
    /// says: nothing says what an array or a map holds, or what is behind
    /// a field nothing writes, so no value is taken to be one of those.
    ///
    /// # Safety
    ///
    /// `value` is a value as a slot holds one: when its tag is a
    /// pointer's, its payload is a live object.
    pub(crate) unsafe fn admits(self, value: Value) -> bool {
        let (tag, payload) = (value.id(), value.payload);
        let is_string = value.is_pointer() && tag < STRING_TAGS_END;
        // SAFETY: the caller's contract: an instance and a variant hold
        // the address of a class object in their second word.
        unsafe {
            match self.kind() {
                KIND_UNKNOWN => true,
                KIND_NULL => tag == TAG_NULL,
                KIND_BOOL => tag == TAG_BOOL,
                KIND_INT => tag == TAG_INT,
                KIND_FLOAT => tag == TAG_FLOAT,
                KIND_STRING => is_string,
                KIND_CLASS => {
                    tag == TAG_INSTANCE && load(payload + INSTANCE_CLASS) == self.class_object()
                }
                KIND_ENUM => {
                    tag == TAG_ENUM_VARIANT && load(payload + VARIANT_ENUM) == self.class_object()
                }
                KIND_VARIANT => {
                    tag == TAG_ENUM_VARIANT
                        && load(payload + VARIANT_ENUM) == self.of_a().class_object()
                        && load(payload + VARIANT_INDEX) == self.b()
                }
                KIND_STRING_IS => is_string && crate::string::text(payload) == self.literal(),
                KIND_INT_IS => tag == TAG_INT && payload == self.a() | self.b() << 32,
                KIND_BOOL_IS => tag == TAG_BOOL && payload & 0xFF == self.a(),
                KIND_OPTIONAL => tag == TAG_NULL || self.of_a().admits(value),
                KIND_UNION => {
                    let mut members = Vec::new();
                    self.members(&mut members);
                    members.into_iter().any(|member| member.admits(value))
                }
                _ => false,
            }
        }
    }

    /// The type with `null` taken out, when it is an optional.
    pub(crate) fn not_null(self) -> Self {
        if self.kind() == KIND_OPTIONAL {
            self.of_a()
        } else {
            self
        }
    }
}

/// Whether the value of tag `tag` and payload `payload` is of the type
/// whose type object is at `type_object`, for a type whose values no one
/// tag and class object tell: a union, an optional, a literal. Its
/// description says, and a type with none has no value this knows of. A
/// leaf: it neither allocates nor throws.
///
/// # Safety
///
/// Called from compiled code with its mutator, a live type object and a
/// value: when the tag is a pointer's, the payload is a live object.
#[unsafe(no_mangle)]
unsafe extern "C" fn baml_rt_is_type(
    worker: &Mutator,
    type_object: usize,
    tag: usize,
    payload: usize,
) -> u8 {
    // SAFETY: the caller's contract.
    u8::from(unsafe { Types::of_table(worker.types()).admits(type_object, Value { tag, payload }) })
}
