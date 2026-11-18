//! Reading the descriptions the compiler leaves in the type table.

use crate::{
    json_layout::{
        DESCRIBED, DESCRIPTION_A, DESCRIPTION_B, DESCRIPTION_C, DESCRIPTION_NAME, DESCRIPTION_REPR,
        FIELD_FLAGS, FIELD_MAY_BE_ABSENT, FIELD_OFFSET, FIELD_OMIT_NULL, FIELD_RECORD, FIELD_TYPE,
        JSON_DIRECTORY_AT, JSON_DIRECTORY_COUNT, JSON_SECTION, KIND_CLASS, KIND_ENUM,
        KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS, KIND_UNION, KIND_UNKNOWN, MEMBER_ALIASED,
        MEMBER_DECLARED, MEMBER_SKIP, REPR_REF, VARIANT_FLAGS, VARIANT_RECORD,
    },
    layout::TYPE_STRING,
    native::{Scope, TypeId},
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
pub(super) struct Types {
    /// The type table.
    table: usize,
    /// The section of descriptions, zero when the program has none.
    section: usize,
}

/// One description.
#[derive(Clone, Copy)]
pub(super) struct Desc {
    section: usize,
    at: usize,
}

/// A variant of a described enum, or a field of a described class.
pub(super) struct Member {
    pub(super) name: &'static str,
    declared: &'static str,
    aliased: &'static str,
    pub(super) skip: bool,
    /// A field's type, where it is in an instance, and its other flags.
    pub(super) ty: Option<Desc>,
    pub(super) offset: usize,
    pub(super) omit_null: bool,
    pub(super) may_be_absent: bool,
}

impl Member {
    /// The name it is written under.
    pub(super) fn key(&self, aliased: bool) -> &'static str {
        if aliased { self.aliased } else { self.declared }
    }

    /// Whether it has a name at all.
    pub(super) fn shown(&self, aliased: bool) -> bool {
        !(aliased && self.skip)
    }
}

impl Types {
    pub(super) fn of(scope: &Scope<'_>) -> Self {
        let table = scope.mutator.types();
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
    pub(super) fn described(&self, id: TypeId) -> Option<Desc> {
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
    pub(super) fn of_type_id(&self, type_id: usize) -> Option<Desc> {
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

    /// The size in bytes of an instance of the class whose type id is
    /// `type_id`.
    pub(super) fn instance_size(&self, type_id: usize) -> usize {
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

    pub(super) fn kind(self) -> u32 {
        self.field(0) as u32
    }

    pub(super) fn repr(self) -> u32 {
        self.field(DESCRIPTION_REPR) as u32
    }

    pub(super) fn a(self) -> usize {
        self.field(DESCRIPTION_A)
    }

    pub(super) fn b(self) -> usize {
        self.field(DESCRIPTION_B)
    }

    /// The type's name, as an error message writes it.
    pub(super) fn name(self) -> &'static str {
        self.text(
            self.field(DESCRIPTION_NAME),
            self.field(DESCRIPTION_NAME + 4),
        )
    }

    /// An enum's name with its package, which one message writes.
    pub(super) fn full_name(self) -> &'static str {
        let after = self.section + self.b() + self.count() * VARIANT_RECORD;
        // SAFETY: an enum's variant records are followed by where its
        // full name is.
        unsafe { self.text(word(after), word(after + 4)) }
    }

    pub(super) fn is_enum(self) -> bool {
        self.kind() == KIND_ENUM
    }

    pub(super) fn is_class(self) -> bool {
        self.kind() == KIND_CLASS
    }

    /// Whether the type is `json` or `unknown`: any value at all.
    pub(super) fn is_any(self) -> bool {
        self.kind() == KIND_UNKNOWN
    }

    /// The description `a` or `b` names: an array's elements, a map's
    /// keys or values, what an optional is of, a variant's enum.
    pub(super) fn of_a(self) -> Self {
        self.at(self.a())
    }

    pub(super) fn of_b(self) -> Self {
        self.at(self.b())
    }

    /// The text of a string literal type.
    pub(super) fn literal(self) -> &'static str {
        self.text(self.a(), self.b())
    }

    /// The members of a union, in declaration order, with the members of
    /// a union among them in its place.
    pub(super) fn members(self, into: &mut Vec<Self>) {
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
    pub(super) fn count(self) -> usize {
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
    pub(super) fn variant(self, index: usize) -> Option<Member> {
        (index < self.count())
            .then(|| self.member(self.section + self.b() + index * VARIANT_RECORD, false))
    }

    /// The variants of an enum.
    pub(super) fn variants(self) -> impl Iterator<Item = Member> {
        (0..self.count()).filter_map(move |index| self.variant(index))
    }

    /// The fields of a class, in declaration order.
    pub(super) fn fields(self) -> impl Iterator<Item = Member> {
        (0..self.count())
            .map(move |index| self.member(self.section + self.b() + index * FIELD_RECORD, true))
    }

    /// Whether a map with keys of this type is a JSON object: `string`, or
    /// string literals.
    pub(super) fn is_string_key(self) -> bool {
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
    pub(super) fn not_null(self) -> Self {
        if self.kind() == KIND_OPTIONAL {
            self.of_a()
        } else {
            self
        }
    }
}
