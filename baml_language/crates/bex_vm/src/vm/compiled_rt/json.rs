//! Compiled code's JSON decode and encode: the module's per-type schemas,
//! read in one pass over the text into the shared layout, and written from
//! it.
//!
//! The emitter declares one static [`Codec`] per type a `baml.json.decode<T>`
//! site reaches (`baml_compiler2_rust::json_codec`, which lists the types that
//! get one). [`decode_json`] reads the text through the interpreter's own
//! reader and builds the value as the interpreter's one-pass text decoder
//! (`package_baml::json::decode_text`) does, with the same rules: an unknown
//! key is checked and skipped, a repeated key keeps its last value, an absent
//! key reads as `null` where the field takes it, an integer must fit an
//! `int`. What the schema already knows the interpreter works out per call:
//! a field's key, its slot, and its type's decode. Instances are written in
//! place, fields in their own representation.
//!
//! Exact where it succeeds and silent where it fails: on `None` the site runs
//! the call it replaced, which reports the error `decode` has always
//! reported.

use bex_heap::TlabHolder;
use bex_vm_types::{RealizedTy, Value, object_format::Access};

use super::heap::{FieldBase, class_word, instance_class_word};
use crate::{
    BexVm,
    package_baml::json::{
        encode::write_escaped,
        text::{Entries, Reader, Stop},
    },
};

/// How one type decodes.
pub enum Codec {
    Int,
    Float,
    /// `int | float`: an integral number that fits is an `int`.
    IntOrFloat,
    Bool,
    String,
    /// One string literal.
    Literal(&'static str),
    /// A union of string literals.
    Literals(&'static [&'static str]),
    /// `T | null`.
    Optional(&'static Codec),
    List(&'static Codec),
    /// `map<string, V>`.
    Map(&'static Codec),
    Class(&'static ClassCodec),
    /// Classes told apart by a string-literal field: at most one decodes a
    /// given object, so they are tried in declaration order.
    Classes(&'static [&'static ClassCodec]),
}

/// How a class decodes: its object index and its fields, in declaration
/// order.
pub struct ClassCodec {
    pub class: usize,
    pub fields: &'static [FieldCodec],
}

/// One field: its JSON key, its type's decode, and its slot.
pub struct FieldCodec {
    pub key: &'static str,
    pub codec: &'static Codec,
    pub offset: u32,
    pub slot: Slot,
    /// The field takes `null`, which an absent key reads as.
    pub absent_null: bool,
    /// `@json.omit_null`: a `null` value is not written.
    pub omit_null: bool,
}

/// A field slot's representation.
#[derive(Clone, Copy)]
pub enum Slot {
    Int,
    Float,
    Bool,
    Ptr,
    Tagged,
}

/// A decoded value, unboxed while it is a scalar bound for a raw slot.
#[derive(Clone, Copy)]
enum Decoded {
    Int(i64),
    Float(f64),
    Bool(bool),
    Value(Value),
}

/// `text` decoded as `ty` through `codec`, the schema the emitter declared for
/// `ty`; `None` when the call `decode` makes must decide.
pub fn decode_json(
    vm: &mut BexVm,
    text: Value,
    ty: &RealizedTy,
    codec: &'static Codec,
) -> Option<Value> {
    let text = vm.as_string(&text).ok()?.clone();
    let mut decoder = Decoder {
        reader: Reader::new(text.as_str()),
    };
    let value = decoder.value(vm, codec, Some(ty)).ok()?;
    decoder.reader.finish().ok()?;
    Some(boxed(vm, value))
}

struct Decoder<'a> {
    reader: Reader<'a>,
}

/// The integer a number token writes, when it is integral and fits an
/// `int`: the interpreter's rule for `int`.
fn int(number: crate::package_baml::json::text::Number<'_>) -> Option<i64> {
    number.as_i64().and_then(Value::try_int).and_then(|int| int.as_int())
}

fn float_value(
    vm: &mut BexVm,
    number: crate::package_baml::json::text::Number<'_>,
) -> Result<Decoded, Stop> {
    let float = number.as_f64().ok_or(Stop)?;
    Ok(Decoded::Value(Value::object(vm.alloc_float(float))))
}

fn boxed(vm: &mut BexVm, decoded: Decoded) -> Value {
    match decoded {
        Decoded::Int(int) => Value::int(int),
        Decoded::Float(float) => Value::object(vm.alloc_float(float)),
        Decoded::Bool(bool) => Value::bool(bool),
        Decoded::Value(value) => value,
    }
}

impl Decoder<'_> {
    /// The value at the cursor. `ty` is the type being decoded where a list
    /// or map needs its element types; a class finds its fields' own.
    fn value(
        &mut self,
        vm: &mut BexVm,
        codec: &'static Codec,
        ty: Option<&RealizedTy>,
    ) -> Result<Decoded, Stop> {
        let reader = &mut self.reader;
        Ok(match codec {
            Codec::Int => Decoded::Int(int(reader.number()?).ok_or(Stop)?),
            Codec::Float => Decoded::Float(reader.number()?.as_f64().ok_or(Stop)?),
            Codec::IntOrFloat => {
                let number = reader.number()?;
                match int(number) {
                    Some(int) => Decoded::Value(Value::int(int)),
                    None => float_value(vm, number)?,
                }
            }
            Codec::Bool => Decoded::Bool(reader.bool()?),
            Codec::String => {
                let text = reader.string()?;
                Decoded::Value(Value::object(vm.alloc_string(text)))
            }
            Codec::Literal(expected) => self.literal(vm, &[expected])?,
            Codec::Literals(expected) => self.literal(vm, expected)?,
            Codec::Optional(inner) => self.optional(vm, inner, ty)?,
            Codec::List(element) => self.list(vm, element, ty)?,
            Codec::Map(value) => self.map(vm, value, ty)?,
            Codec::Class(class) => self.class(vm, class)?,
            Codec::Classes(classes) => self.classes(vm, classes)?,
        })
    }

    /// `null`, or the value as `inner`.
    fn optional(
        &mut self,
        vm: &mut BexVm,
        inner: &'static Codec,
        ty: Option<&RealizedTy>,
    ) -> Result<Decoded, Stop> {
        if self.reader.peek() == Some(b'n') {
            self.reader.null()?;
            return Ok(Decoded::Value(Value::NULL));
        }
        let inner_ty = ty.map(RealizedTy::strip_null);
        self.value(vm, inner, inner_ty.as_ref())
    }

    fn literal(&mut self, vm: &mut BexVm, expected: &[&'static str]) -> Result<Decoded, Stop> {
        let written = self.reader.string()?;
        let text = expected.iter().find(|text| **text == written).ok_or(Stop)?;
        Ok(Decoded::Value(Value::object(vm.alloc_string(*text))))
    }

    fn list(
        &mut self,
        vm: &mut BexVm,
        element: &'static Codec,
        ty: Option<&RealizedTy>,
    ) -> Result<Decoded, Stop> {
        let Some(RealizedTy::List(element_ty)) = ty else {
            return Err(Stop);
        };
        self.reader.open_array()?;
        let mut items = Vec::new();
        while self.reader.next_element(items.is_empty())? {
            let item = self.value(vm, element, Some(element_ty))?;
            items.push(boxed(vm, item));
        }
        Ok(Decoded::Value(Value::object(
            vm.alloc_array(element_ty.as_ref().clone(), items),
        )))
    }

    fn map(
        &mut self,
        vm: &mut BexVm,
        value: &'static Codec,
        ty: Option<&RealizedTy>,
    ) -> Result<Decoded, Stop> {
        let Some(RealizedTy::Map {
            key: key_ty,
            value: value_ty,
        }) = ty
        else {
            return Err(Stop);
        };
        self.reader.open_object()?;
        let mut entries = Entries::default();
        while let Some(written) = self.reader.next_key(entries.is_empty())? {
            let key = bex_vm_types::BexStr::from(written);
            let decoded = self.value(vm, value, Some(value_ty))?;
            let decoded = boxed(vm, decoded);
            entries.insert(key, decoded);
        }
        Ok(Decoded::Value(entries.alloc(
            vm,
            key_ty.as_ref().clone(),
            value_ty.as_ref().clone(),
        )))
    }

    /// The first of `classes` that decodes the object at the cursor.
    fn classes(
        &mut self,
        vm: &mut BexVm,
        classes: &'static [&'static ClassCodec],
    ) -> Result<Decoded, Stop> {
        let start = self.reader.mark();
        for class in classes {
            match self.class(vm, class) {
                Ok(decoded) => return Ok(decoded),
                Err(Stop) => self.reader.reset(start),
            }
        }
        Err(Stop)
    }

    /// An instance of `codec`'s class, read by its fields' keys.
    fn class(&mut self, vm: &mut BexVm, codec: &'static ClassCodec) -> Result<Decoded, Stop> {
        let slots = self.fields(vm, codec)?;
        let mut values: smallvec::SmallVec<[Decoded; 8]> = smallvec::SmallVec::new();
        for (field, slot) in codec.fields.iter().zip(slots) {
            values.push(match slot {
                Some(value) => value,
                None if field.absent_null => Decoded::Value(Value::NULL),
                None => return Err(Stop),
            });
        }
        instance(vm, codec, &values).map(Decoded::Value)
    }

    /// The object at the cursor read into `codec`'s fields, by key: an
    /// unknown key skipped, a repeated one keeping its last value.
    fn fields(
        &mut self,
        vm: &mut BexVm,
        codec: &'static ClassCodec,
    ) -> Result<smallvec::SmallVec<[Option<Decoded>; 8]>, Stop> {
        self.reader.open_object()?;
        let mut slots: smallvec::SmallVec<[Option<Decoded>; 8]> =
            smallvec::smallvec![None; codec.fields.len()];
        let mut first = true;
        while let Some(key) = self.reader.next_key(first)? {
            first = false;
            let Some(field) = codec.fields.iter().position(|field| field.key == key) else {
                self.reader.skip_value()?;
                continue;
            };
            let ty = self.field_type(vm, codec, field);
            slots[field] = Some(self.value(vm, codec.fields[field].codec, ty.as_ref())?);
        }
        Ok(slots)
    }

    /// The type of `codec`'s field `field`, where its decode needs one: a
    /// list or map, possibly optional.
    fn field_type(&self, vm: &mut BexVm, codec: &ClassCodec, field: usize) -> Option<RealizedTy> {
        let mut inner = codec.fields[field].codec;
        while let Codec::Optional(next) = inner {
            inner = next;
        }
        if !matches!(inner, Codec::List(_) | Codec::Map(_)) {
            return None;
        }
        vm.class_field_type(codec.class, field)
    }
}

/// An instance of `codec`'s class with `values`, each written to its slot in
/// its own representation.
fn instance(vm: &mut BexVm, codec: &ClassCodec, values: &[Decoded]) -> Result<Value, Stop> {
    let class = vm.heap.compile_time_ptr(codec.class);
    let shape = vm.instance_shape_at(codec.class, class).ok_or(Stop)?;
    let (instance, base) = vm.alloc_shaped(shape);
    for (field, &value) in codec.fields.iter().zip(values) {
        // SAFETY: the instance is fresh and its layout is the class's, whose
        // slots the emitter read the offsets and kinds of.
        unsafe { write(vm, base, field, value) }?;
    }
    Ok(instance)
}

/// # Safety
/// `base` is an instance with `field`'s slot.
unsafe fn write(vm: &mut BexVm, base: FieldBase, field: &FieldCodec, value: Decoded) -> Result<(), Stop> {
    let offset = field.offset;
    // SAFETY: forwarded to the caller.
    unsafe {
        match (field.slot, value) {
            (Slot::Int, Decoded::Int(int)) => base.set_int(offset, int),
            (Slot::Float, Decoded::Float(float)) => base.set_float(offset, float),
            (Slot::Bool, Decoded::Bool(bool)) => base.set_bool(offset, bool),
            (Slot::Ptr, Decoded::Value(value)) => base.set_ptr(offset, value),
            (Slot::Tagged, value) => {
                let value = boxed(vm, value);
                vm.store_tagged_at(base, offset, value).map_err(|_| Stop)?;
            }
            _ => return Err(Stop),
        }
    }
    Ok(())
}

/// `value`, typed by the type `codec` is the schema of, as compact JSON text,
/// written from its slots as the interpreter's structural walk writes it
/// (`package_baml::json::encode`): fields in declaration order, map entries
/// in insertion order, `serde_json`'s escapes, a float's shortest form.
/// `omit_nulls` leaves out a field holding `null`.
///
/// `None` where the interpreter's walk must decide: a float with no JSON
/// form, a value of a class the schema does not name. Types with a
/// `baml.ToJson` override, or a class that can contain itself, get no schema.
pub fn encode_json(
    vm: &mut BexVm,
    value: Value,
    codec: &'static Codec,
    omit_nulls: bool,
) -> Option<Value> {
    let mut encoder = Encoder {
        out: Vec::new(),
        omit_nulls,
    };
    encoder.value(vm, value, codec).ok()?;
    let text = String::from_utf8(encoder.out).ok()?;
    Some(Value::object(vm.alloc_string(text)))
}

struct Encoder {
    out: Vec<u8>,
    omit_nulls: bool,
}

impl Encoder {
    fn value(&mut self, vm: &mut BexVm, value: Value, codec: &'static Codec) -> Result<(), Stop> {
        match codec {
            Codec::Int => self.int(value.as_int().ok_or(Stop)?),
            Codec::Float => self.float(crate::vm::value_as_float(value).ok_or(Stop)?)?,
            Codec::IntOrFloat => match value.as_int() {
                Some(int) => self.int(int),
                None => self.float(crate::vm::value_as_float(value).ok_or(Stop)?)?,
            },
            Codec::Bool => self.bool(value.as_bool().ok_or(Stop)?),
            Codec::String | Codec::Literal(_) | Codec::Literals(_) => {
                let text = vm.as_string(&value).map_err(|_| Stop)?;
                write_escaped(&mut self.out, text.as_str());
            }
            Codec::Optional(_) if value.is_null() => self.out.extend_from_slice(b"null"),
            Codec::Optional(inner) => self.value(vm, value, inner)?,
            Codec::List(element) => self.list(vm, value, element)?,
            Codec::Map(entry) => self.map(vm, value, entry)?,
            Codec::Class(class) => self.instance(vm, value, class)?,
            Codec::Classes(classes) => {
                let word = instance_class_word(value);
                let class = classes
                    .iter()
                    .find(|class| class_word(vm.heap.compile_time_ptr(class.class)) == word)
                    .ok_or(Stop)?;
                self.instance(vm, value, class)?;
            }
        }
        Ok(())
    }

    fn int(&mut self, int: i64) {
        self.out
            .extend_from_slice(itoa::Buffer::new().format(int).as_bytes());
    }

    fn float(&mut self, float: f64) -> Result<(), Stop> {
        if !float.is_finite() {
            return Err(Stop);
        }
        self.out
            .extend_from_slice(zmij::Buffer::new().format_finite(float).as_bytes());
        Ok(())
    }

    fn bool(&mut self, bool: bool) {
        self.out
            .extend_from_slice(if bool { b"true" } else { b"false" });
    }

    fn list(&mut self, vm: &mut BexVm, value: Value, element: &'static Codec) -> Result<(), Stop> {
        let ptr = value.as_object_ptr().ok_or(Stop)?;
        let list = vm.list_at(ptr).ok_or(Stop)?;
        let items = vm.list_values(list, Access::Shared);
        self.out.push(b'[');
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                self.out.push(b',');
            }
            self.value(vm, item, element)?;
        }
        self.out.push(b']');
        Ok(())
    }

    fn map(&mut self, vm: &mut BexVm, value: Value, entry: &'static Codec) -> Result<(), Stop> {
        let ptr = value.as_object_ptr().ok_or(Stop)?;
        let map = vm.map_at(ptr).ok_or(Stop)?;
        let entries = vm.map_snapshot(map, Access::Shared).snapshot_entries();
        self.out.push(b'{');
        for (i, (key, item)) in entries.into_iter().enumerate() {
            if i > 0 {
                self.out.push(b',');
            }
            let key = vm.as_string(&key).map_err(|_| Stop)?;
            write_escaped(&mut self.out, key.as_str());
            self.out.push(b':');
            self.value(vm, item, entry)?;
        }
        self.out.push(b'}');
        Ok(())
    }

    fn instance(&mut self, vm: &mut BexVm, value: Value, codec: &'static ClassCodec) -> Result<(), Stop> {
        let class = vm.heap.compile_time_ptr(codec.class);
        if instance_class_word(value) != class_word(class) {
            return Err(Stop);
        }
        // SAFETY: an instance of the class the schema was laid out for.
        let base = unsafe { FieldBase::typed(value) }.ok_or(Stop)?;
        self.out.push(b'{');
        let mut first = true;
        for field in codec.fields {
            // SAFETY: the field's slot, as the emitter read it from the layout.
            let slot = unsafe { read(vm, base, field) };
            let omitted = matches!(slot, Decoded::Value(held) if held.is_null())
                && (self.omit_nulls || field.omit_null);
            if !omitted {
                self.field(vm, field, slot, first)?;
                first = false;
            }
        }
        self.out.push(b'}');
        Ok(())
    }

    /// One entry of an instance: its key and the value its slot held.
    fn field(
        &mut self,
        vm: &mut BexVm,
        field: &FieldCodec,
        slot: Decoded,
        first: bool,
    ) -> Result<(), Stop> {
        if !first {
            self.out.push(b',');
        }
        write_escaped(&mut self.out, field.key);
        self.out.push(b':');
        match slot {
            Decoded::Int(int) => self.int(int),
            Decoded::Float(float) => self.float(float)?,
            Decoded::Bool(bool) => self.bool(bool),
            Decoded::Value(held) => self.value(vm, held, field.codec)?,
        }
        Ok(())
    }
}

/// `field`'s slot of `base`, raw scalars unboxed.
///
/// # Safety
/// `base` is an instance with `field`'s slot.
unsafe fn read(vm: &mut BexVm, base: FieldBase, field: &FieldCodec) -> Decoded {
    // SAFETY: forwarded to the caller.
    unsafe {
        match field.slot {
            Slot::Int => Decoded::Int(base.int(field.offset)),
            Slot::Float => Decoded::Float(base.float(field.offset)),
            Slot::Bool => Decoded::Bool(base.bool(field.offset)),
            Slot::Ptr => Decoded::Value(base.ptr(field.offset)),
            Slot::Tagged => Decoded::Value(vm.load_tagged_at(base, field.offset)),
        }
    }
}

impl BexVm {
    /// The realized type of field `field` of the compile-time class at object
    /// index `class`, cached per VM: what a list or map field's decode
    /// allocates its elements as.
    fn class_field_type(&mut self, class: usize, field: usize) -> Option<RealizedTy> {
        if let Some(ty) = self.compiled.json_field_types.get(&(class, field)) {
            return Some(ty.clone());
        }
        let ptr = self.heap.compile_time_ptr(class);
        let bex_vm_types::Object::Class(declaration) = self.get_object(ptr) else {
            return None;
        };
        let template = declaration.fields.get(field)?.field_template.clone();
        let ty = self.realize_field_ty(&template, &[]);
        self.compiled
            .json_field_types
            .insert((class, field), ty.clone());
        Some(ty)
    }
}
