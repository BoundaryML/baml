//! Reading the descriptions the compiler leaves in the type table: what
//! `baml.json` writes and reads values by, and what an uncaught throw of
//! an instance is written from. See [`crate::json_layout`].

#![allow(
    clippy::cast_possible_truncation,
    reason = "every word read here is a `u32` of the type table"
)]

use crate::{
    heap::load,
    json_layout::{
        DESCRIBED, DESCRIPTION_A, DESCRIPTION_B, DESCRIPTION_C, DESCRIPTION_FLAGS,
        DESCRIPTION_NAME, DESCRIPTION_REPR, FIELD_FLAGS, FIELD_MAY_BE_ABSENT, FIELD_OFFSET,
        FIELD_OMIT_NULL, FIELD_RECORD, FIELD_TYPE, FLAG_HOOKS, HOOKS, JSON_DIRECTORY_AT,
        JSON_DIRECTORY_COUNT, JSON_SECTION, KIND_CLASS, KIND_ENUM, KIND_OPTIONAL, KIND_STRING,
        KIND_STRING_IS, KIND_UNION, KIND_UNKNOWN, MEMBER_ALIASED, MEMBER_DECLARED, MEMBER_SKIP,
        REPR_REF, VARIANT_FLAGS, VARIANT_RECORD,
    },
    layout::{KIND_FIXED, TYPE_DESCRIBED, TYPE_STRING, WELL_KNOWN, WELL_KNOWN_PANICS},
    native::{Scope, TypeId, drive::BamlFn},
};

/// Read the `u32` at `address`.
///
/// # Safety
///
/// `address` is inside the type table.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read_unaligned() as usize }
}

/// The description of `string`, for a call that names the type by its
/// type id: a section of its own, the name after the description.
static STRING: [u32; 10] = [
    KIND_STRING,
    REPR_REF,
    0,
    0,
    0,
    32,
    6,
    0,
    u32::from_le_bytes(*b"stri"),
    u32::from_le_bytes(*b"ng\0\0"),
];

/// The descriptions of a program.
#[derive(Clone, Copy)]
pub(crate) struct Types {
    /// The type table.
    table: usize,
    /// The section of descriptions, zero when the program has none.
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

    /// The descriptions of the program whose type table is at `table`.
    pub(crate) fn of_table(table: usize) -> Self {
        // SAFETY: the table starts with eight bytes no type lives at, the
        // last four of which locate the section.
        let section = unsafe { word(table + JSON_SECTION) };
        Self {
            table,
            section: if section == 0 { 0 } else { table + section },
        }
    }

    /// What a call passed for a type: a description, the type id of a
    /// class, an enum or `string`, or nothing.
    pub(crate) fn described(&self, id: TypeId) -> Option<Desc> {
        let id = id.0;
        if id & DESCRIBED != 0 && self.section != 0 {
            return Some(Desc {
                section: self.section,
                at: self.section + (id & !DESCRIBED) as usize,
            });
        }
        match id as usize {
            0 => None,
            TYPE_STRING => Some(Desc {
                section: STRING.as_ptr() as usize,
                at: STRING.as_ptr() as usize,
            }),
            other => self.of_type_id(other),
        }
    }

    /// The description of the class or enum whose instances or boxes have
    /// the type id `type_id`.
    pub(crate) fn of_type_id(&self, type_id: usize) -> Option<Desc> {
        if self.section == 0 {
            return None;
        }
        // SAFETY: the section's header locates its directory, which is
        // sorted by type id.
        unsafe {
            let directory = self.section + word(self.section + JSON_DIRECTORY_AT);
            let (mut low, mut high) = (0, word(self.section + JSON_DIRECTORY_COUNT));
            while low < high {
                let middle = usize::midpoint(low, high);
                let found = word(directory + middle * 8);
                if found == type_id {
                    return Some(Desc {
                        section: self.section,
                        at: self.section + word(directory + middle * 8 + 4),
                    });
                }
                if found < type_id {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
        }
        None
    }

    /// The description of the class whose instances have the type id
    /// `type_id`, which the class's type locates: every class has one,
    /// whether or not `baml.json` can write its instances.
    pub(crate) fn of_class(&self, type_id: usize) -> Option<Desc> {
        // SAFETY: a type is four words: the first its kind, and the last
        // of a class's where its description is.
        let (kind, described) =
            unsafe { (word(self.table + type_id), word(self.table + type_id + 12)) };
        (self.section != 0
            && kind == KIND_FIXED as usize
            && described & TYPE_DESCRIBED as usize != 0)
            .then(|| Desc {
                section: self.section,
                at: self.section + (described & !(TYPE_DESCRIBED as usize)),
            })
    }

    /// The size in bytes of an object of the fixed type `type_id`, and the
    /// offsets of the words of it the collector follows.
    pub(crate) fn fixed(&self, type_id: usize) -> (usize, &'static [u32]) {
        // SAFETY: a fixed type is its four words and then the offset of
        // each of its references, in the executable's data.
        unsafe {
            let entry = self.table + type_id;
            let references = std::slice::from_raw_parts(
                (entry + crate::layout::TYPE_HEADER) as *const u32,
                word(entry + 8),
            );
            (word(entry + 4), references)
        }
    }

    /// The list of types the runtime knows by position, which the table's
    /// first word locates: a count, then the type id of each.
    fn listed(&self, position: usize) -> Option<usize> {
        // SAFETY: the type table starts with where the list is.
        unsafe {
            let list = self.table + word(self.table);
            (word(self.table) != 0 && position < word(list)).then(|| word(list + 4 + position * 4))
        }
    }

    /// The type id of the class at `index` of [`crate::layout::NAMED`],
    /// when the program describes it.
    pub(crate) fn named(&self, index: usize) -> Option<usize> {
        self.listed(WELL_KNOWN.len() + index).filter(|id| *id != 0)
    }

    /// Whether `type_id` is the type of a panic: one of the first of
    /// [`WELL_KNOWN`].
    pub(crate) fn is_panic(&self, type_id: usize) -> bool {
        (0..WELL_KNOWN_PANICS).any(|class| self.listed(class) == Some(type_id))
    }

    /// The size in bytes of an instance of the class whose type id is
    /// `type_id`.
    pub(crate) fn instance_size(&self, type_id: usize) -> usize {
        // SAFETY: a class's type is four words, the second its size.
        unsafe { word(self.table + type_id + 4) }
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

    /// The type with `null` taken out, when it is an optional.
    pub(crate) fn not_null(self) -> Self {
        if self.kind() == KIND_OPTIONAL {
            self.of_a()
        } else {
            self
        }
    }
}
