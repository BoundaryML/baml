//! The library's interface as the runtime holds it: the table of exports
//! (`codegen/host.rs` in the compiler) and the descriptions of the types it
//! names (`json_layout.rs`), read once at init into plain Rust values.
//!
//! What a host passes is checked against them where it lands in a typed
//! place: an argument against its parameter, a field against its field.
//! A value of the wrong type is refused, never stored, because compiled
//! code trusts static types.

use std::collections::HashMap;

use crate::{
    heap::{Value, load},
    json_layout::{
        DESCRIPTION_A, DESCRIPTION_B, DESCRIPTION_C, FIELD_OFFSET, FIELD_RECORD, FIELD_TYPE,
        KIND_ARRAY, KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT, KIND_INT,
        KIND_INT_IS, KIND_MAP, KIND_NULL, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS, KIND_UNION,
        KIND_UNKNOWN, KIND_VARIANT,
    },
    layout::{
        ARRAY_LENGTH, ARRAY_PTR, INSTANCE_CLASS, PROGRAM_DESCRIPTIONS, STORAGE_ELEMENTS,
        STRING_TAGS_END, TAG_ARRAY, TAG_BOOL, TAG_ENUM_VARIANT, TAG_FLOAT, TAG_INSTANCE, TAG_INT,
        TAG_MAP, TAG_NULL, TAG_STRING, VALUE, VARIANT_ENUM, VARIANT_INDEX,
    },
};

/// A type, by its place in [`Interface::types`].
pub(super) type Ty = usize;

/// What a value must be to land where a type is declared.
pub(super) enum HostTy {
    /// Anything.
    Any,
    Int,
    Float,
    Bool,
    Null,
    String,
    StringIs(&'static [u8]),
    IntIs(i64),
    BoolIs(bool),
    /// A variant of the enum whose class object is this.
    Enum(usize),
    /// This variant of the enum whose class object is this.
    Variant(usize, usize),
    /// An instance of the class whose class object is this.
    Class(usize),
    Array(Ty),
    Map(Ty, Ty),
    Optional(Ty),
    Union(Vec<Ty>),
}

pub(super) struct Function {
    pub(super) entry: usize,
    /// Each parameter's type, and whether it has a default.
    pub(super) params: Vec<(Ty, bool)>,
}

pub(super) struct Field {
    /// Where its slot is in an instance.
    pub(super) offset: usize,
    pub(super) ty: Ty,
}

pub(super) struct ExportedType {
    /// Its class object, which the compiler placed and which never moves.
    pub(super) class: usize,
    pub(super) is_enum: bool,
    /// A class's fields in slot order.
    pub(super) fields: Vec<Field>,
    /// An enum's variant count.
    pub(super) variants: usize,
}

pub(super) struct Interface {
    pub(super) functions: Vec<Function>,
    pub(super) exported: Vec<ExportedType>,
    /// The number of each class or enum, by its class object.
    pub(super) by_class: HashMap<usize, usize>,
    types: Vec<HostTy>,
}

/// The table of exports, as `codegen/host.rs` lays it out.
pub(super) const EXPORTS_VERSION: u32 = 2;
const HEADER: usize = 32;
pub(super) const FUNCTION_RECORD: usize = 56;
const TYPE_RECORD: usize = 32;
const PARAM_RECORD: usize = 8;
pub(super) const FUNCTION_HASH: usize = 8;
pub(super) const FUNCTION_ARITY: usize = 16;
pub(super) const FUNCTION_NAME: usize = 20;
pub(super) const FUNCTION_SIGNATURE: usize = 28;
const FUNCTION_PARAMS: usize = 44;

/// Read the `u32` at `address`.
///
/// # Safety
///
/// `address` is inside the table of exports or the descriptions.
unsafe fn word(address: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (address as *const u32).read_unaligned() as usize }
}

/// The address of the first function record.
pub(super) fn function_record(exports: usize, index: usize) -> usize {
    exports + HEADER + index * FUNCTION_RECORD
}

struct Reader {
    /// The section of descriptions.
    section: usize,
    types: Vec<HostTy>,
    by_offset: HashMap<usize, Ty>,
}

impl Reader {
    /// The type the description at `offset` of the section describes.
    ///
    /// # Safety
    ///
    /// `offset` is a description's, in the compiler's format.
    unsafe fn ty(&mut self, offset: usize) -> Ty {
        if let Some(ty) = self.by_offset.get(&offset) {
            return *ty;
        }
        // A union can name itself through a class only, which is checked by
        // its class object alone: no description is read while it is read.
        let place = self.types.len();
        self.types.push(HostTy::Any);
        self.by_offset.insert(offset, place);
        // SAFETY: the caller's contract.
        let ty = unsafe {
            let at = self.section + offset;
            let (a, b) = (word(at + DESCRIPTION_A), word(at + DESCRIPTION_B));
            #[expect(clippy::cast_possible_truncation, reason = "a kind is small")]
            match word(at) as u32 {
                KIND_INT => HostTy::Int,
                KIND_FLOAT => HostTy::Float,
                KIND_BOOL => HostTy::Bool,
                KIND_NULL => HostTy::Null,
                KIND_STRING => HostTy::String,
                KIND_UNKNOWN => HostTy::Any,
                KIND_STRING_IS => HostTy::StringIs(std::slice::from_raw_parts(
                    (self.section + a) as *const u8,
                    b,
                )),
                KIND_INT_IS => HostTy::IntIs(((b as u64) << 32 | a as u64).cast_signed()),
                KIND_BOOL_IS => HostTy::BoolIs(a != 0),
                KIND_ENUM => HostTy::Enum(load(self.section + a)),
                KIND_CLASS => HostTy::Class(load(self.section + a)),
                KIND_VARIANT => {
                    let of = self.section + a;
                    HostTy::Variant(load(self.section + word(of + DESCRIPTION_A)), b)
                }
                KIND_ARRAY => HostTy::Array(self.ty(a)),
                KIND_MAP => {
                    let key = self.ty(a);
                    HostTy::Map(key, self.ty(b))
                }
                KIND_OPTIONAL => HostTy::Optional(self.ty(a)),
                KIND_UNION => HostTy::Union(
                    (0..b)
                        .map(|member| self.ty(word(self.section + a + 4 * member)))
                        .collect(),
                ),
                // Refused when the library was packed.
                _ => HostTy::Any,
            }
        };
        self.types[place] = ty;
        place
    }

    /// A class's fields, from its description at `offset`.
    ///
    /// # Safety
    ///
    /// `offset` is a class description's.
    unsafe fn fields(&mut self, offset: usize) -> Vec<Field> {
        // SAFETY: the caller's contract.
        unsafe {
            let at = self.section + offset;
            let (records, count) = (word(at + DESCRIPTION_B), word(at + DESCRIPTION_C));
            (0..count)
                .map(|index| {
                    let record = self.section + records + index * FIELD_RECORD;
                    Field {
                        offset: word(record + FIELD_OFFSET),
                        ty: self.ty(word(record + FIELD_TYPE)),
                    }
                })
                .collect()
        }
    }
}

impl Interface {
    /// Read the table of exports at `exports` and the descriptions in the
    /// table of types at `types`.
    ///
    /// # Safety
    ///
    /// Both are the compiled library's, in the compiler's formats.
    pub(super) unsafe fn read(exports: usize, types: usize) -> Self {
        let mut reader = Reader {
            section: types + PROGRAM_DESCRIPTIONS,
            types: Vec::new(),
            by_offset: HashMap::new(),
        };
        // SAFETY: the caller's contract.
        unsafe {
            let count = word(exports + 4);
            let functions = (0..count)
                .map(|index| {
                    let record = function_record(exports, index);
                    let arity = word(record + FUNCTION_ARITY);
                    let params_at = exports + word(record + FUNCTION_PARAMS);
                    let params = (0..arity)
                        .map(|place| {
                            let param = params_at + place * PARAM_RECORD;
                            (reader.ty(word(param)), word(param + 4) & 1 != 0)
                        })
                        .collect();
                    Function {
                        entry: load(record),
                        params,
                    }
                })
                .collect();
            let (type_count, types_at) = (word(exports + 16), word(exports + 20));
            let mut by_class = HashMap::new();
            let exported = (0..type_count)
                .map(|index| {
                    let record = exports + types_at + index * TYPE_RECORD;
                    let class = load(record);
                    let is_enum = word(record + 16) == 1;
                    let described = word(record + 20);
                    let count = word(record + 24);
                    by_class.insert(class, index);
                    ExportedType {
                        class,
                        is_enum,
                        fields: if is_enum {
                            Vec::new()
                        } else {
                            reader.fields(described)
                        },
                        variants: if is_enum { count } else { 0 },
                    }
                })
                .collect();
            Self {
                functions,
                exported,
                by_class,
                types: reader.types,
            }
        }
    }

    /// Whether `value`, a value as a slot holds it, is of type `ty`. An
    /// instance is checked by its class alone: every instance was made by
    /// compiled code or by a checked `instance_new`, so its fields are
    /// right. An array or a map is checked element by element.
    ///
    /// # Safety
    ///
    /// `value` is a live value. Nothing allocates while this runs.
    pub(super) unsafe fn fits(&self, value: Value, ty: Ty) -> bool {
        let id = value.tag;
        let is_string = (TAG_STRING..STRING_TAGS_END).contains(&id);
        // SAFETY: the caller's contract; a value's payload is its object
        // when its tag says so.
        unsafe {
            match &self.types[ty] {
                HostTy::Any => true,
                HostTy::Int => id == TAG_INT,
                HostTy::Float => id == TAG_FLOAT,
                HostTy::Bool => id == TAG_BOOL,
                HostTy::Null => id == TAG_NULL,
                HostTy::String => is_string,
                HostTy::StringIs(text) => {
                    is_string && crate::string::text(value.payload).as_bytes() == *text
                }
                HostTy::IntIs(expected) => {
                    id == TAG_INT && value.payload.cast_signed() as i64 == *expected
                }
                HostTy::BoolIs(expected) => {
                    id == TAG_BOOL && (value.payload & 0xFF != 0) == *expected
                }
                HostTy::Enum(class) => {
                    id == TAG_ENUM_VARIANT && load(value.payload + VARIANT_ENUM) == *class
                }
                HostTy::Variant(class, index) => {
                    id == TAG_ENUM_VARIANT
                        && load(value.payload + VARIANT_ENUM) == *class
                        && load(value.payload + VARIANT_INDEX) == *index
                }
                HostTy::Class(class) => {
                    id == TAG_INSTANCE && load(value.payload + INSTANCE_CLASS) == *class
                }
                HostTy::Array(element) => {
                    if id != TAG_ARRAY {
                        return false;
                    }
                    let length = load(value.payload + ARRAY_LENGTH);
                    let storage = load(value.payload + ARRAY_PTR);
                    (0..length).all(|index| {
                        let slot = storage + STORAGE_ELEMENTS + index * VALUE;
                        self.fits(Value::unshared(slot), *element)
                    })
                }
                HostTy::Map(key, item) => {
                    if id != TAG_MAP {
                        return false;
                    }
                    let mut all = true;
                    crate::map::for_each(value.payload, |k, v| {
                        all = all && self.fits(k, *key) && self.fits(v, *item);
                    });
                    all
                }
                HostTy::Optional(inner) => id == TAG_NULL || self.fits(value, *inner),
                HostTy::Union(members) => members.iter().any(|member| self.fits(value, *member)),
            }
        }
    }
}
