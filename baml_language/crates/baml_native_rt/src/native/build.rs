//! Making an instance of any class from Rust, by the class's description:
//! where each field is and how its slot holds a value.
//!
//! A description is of one class at its type arguments, so a generic class
//! is built like any other. A native comes by one in four ways: the type a
//! call names for a type parameter ([`Scope::class_from`]), the class of an
//! object it has ([`Scope::class_of`]), the type of a field of a class it
//! has ([`Class::field_class`]), and for the classes of the standard
//! library the runtime names, the program's table of them
//! ([`Scope::named_class`]).
//!
//! `baml.json` builds the instances it decodes through the same two
//! functions, [`place`] and [`put`], in room it has reserved.

use super::{Elem, Obj, Root, Scope, TypeId};
use crate::{
    describe::{Desc, Types},
    heap::{load, store},
    json_layout::{
        KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT, KIND_INT, KIND_INT_IS,
        KIND_NULL, KIND_OPAQUE, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS, KIND_UNION,
        KIND_VARIANT, REPR_BOXED, REPR_PAIR,
    },
    layout::{
        BOX_SIZE, BOX_VALUE, HEADER_PANIC, NAMED, STRING_TYPES_END, TYPE_BOX_BOOL, TYPE_BOX_FLOAT,
        TYPE_BOX_INT, TYPE_ID_MASK, WORD,
    },
};

/// A value on its way into a slot, before it is put in the form the slot
/// holds it in.
#[derive(Clone, Copy)]
pub(crate) enum Got {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A variant of the enum whose boxes have this type id.
    Variant(usize, usize),
    /// A reference to an object.
    Ref(usize),
}

/// The words a place of representation `repr` holds `got` as: the slot,
/// and for an optional scalar whether there is a value. Where the place
/// holds any of several types, a scalar goes in a box, which `boxed` makes
/// of a type id and the value's bits.
pub(crate) fn place(
    got: Got,
    repr: u32,
    boxed: impl FnOnce(usize, usize) -> usize,
) -> (usize, usize) {
    let (type_id, bits) = match got {
        Got::Null => return (0, 0),
        Got::Ref(object) => return (object, 1),
        Got::Bool(value) => (TYPE_BOX_BOOL, usize::from(value)),
        Got::Int(value) => (TYPE_BOX_INT, value.cast_unsigned() as usize),
        Got::Float(value) => (TYPE_BOX_FLOAT, value.to_bits() as usize),
        Got::Variant(type_id, index) => (type_id, index),
    };
    if repr == REPR_BOXED {
        (boxed(type_id, bits), 1)
    } else {
        (bits, 1)
    }
}

/// Write what [`place`] made into the field at `offset` of `instance`,
/// whose representation is `repr`.
///
/// # Safety
///
/// `instance` is an instance with a field of that representation there,
/// and the words are a value of the field's type.
pub(crate) unsafe fn put(
    instance: usize,
    offset: usize,
    repr: u32,
    (word, present): (usize, usize),
) {
    // SAFETY: the caller's contract: an optional scalar is two slots.
    unsafe {
        store(instance + offset, word);
        if repr == REPR_PAIR {
            store(instance + offset + WORD, present);
        }
    }
}

/// A class at its type arguments, as the program describes it.
#[derive(Clone, Copy)]
pub struct Class(Desc);

impl Class {
    /// The position of the field called `name`, in declaration order.
    pub fn field_named(self, name: &str) -> Option<usize> {
        self.0.fields().position(|field| field.name == name)
    }

    /// The class the type of field `field` names: the field's own type, or
    /// what it is an optional, an array or a map of. Nothing when the
    /// program describes the field only as a reference: the type of a
    /// field is described as far as `baml.json` reads it, and it reads no
    /// class that holds a `bigint`, a function or Rust state.
    pub fn field_class(self, field: usize) -> Option<Self> {
        let (_, mut ty) = self.0.field_place(field)?;
        loop {
            ty = match ty.kind() {
                KIND_CLASS => return Some(Self(ty)),
                KIND_OPTIONAL | crate::json_layout::KIND_ARRAY => ty.of_a(),
                crate::json_layout::KIND_MAP => ty.of_b(),
                _ => return None,
            };
        }
    }
}

/// What a native gives a field.
#[derive(Clone, Copy)]
pub enum Value<'s> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A variant of the field's enum, by its position in the declaration.
    Variant(usize),
    /// An object, which the field's type must admit.
    Ref(Elem<'s>),
}

impl<'s, M> From<Root<'s, M>> for Value<'s> {
    fn from(root: Root<'s, M>) -> Self {
        Self::Ref(Elem {
            slot: root.0,
            is_ref: true,
        })
    }
}

impl<'s, M> From<Option<Root<'s, M>>> for Value<'s> {
    fn from(root: Option<Root<'s, M>>) -> Self {
        root.map_or(Self::Null, Self::from)
    }
}

impl From<i64> for Value<'_> {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<Option<i64>> for Value<'_> {
    fn from(value: Option<i64>) -> Self {
        value.map_or(Self::Null, Self::Int)
    }
}

impl From<f64> for Value<'_> {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<bool> for Value<'_> {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// An instance being built: every slot zero until its field is set.
pub struct Build<'s> {
    class: Desc,
    object: Root<'s, Obj>,
    /// The fields set so far, a bit each.
    set: u64,
}

/// Whether a field of the type `ty` may be left unset: its zero slot is
/// then `null`.
fn admits_null(ty: Desc) -> bool {
    match ty.kind() {
        KIND_NULL | KIND_OPTIONAL => true,
        KIND_UNION => {
            let mut members = Vec::new();
            ty.members(&mut members);
            members.iter().any(|member| member.kind() == KIND_NULL)
        }
        _ => ty.is_any(),
    }
}

impl<'s> Scope<'s> {
    /// The class of the standard library at `index` of
    /// [`crate::layout::NAMED`]. The program describes it when it calls a
    /// native whose declaration names it.
    pub fn named_class(&self, index: usize) -> Class {
        let types = Types::of(self);
        match types.named(index).and_then(|id| types.of_class(id)) {
            Some(class) => Class(class),
            None => panic!(
                "a native builds `baml.{}.{}`, which its declaration does not name",
                NAMED[index].0, NAMED[index].1
            ),
        }
    }

    /// The class `object` is an instance of.
    pub fn class_of(&self, object: super::ObjRef<'_>) -> Class {
        // SAFETY: a view is of a live instance, whose header is its type.
        let type_id = unsafe { load(super::View::raw(object)) } & TYPE_ID_MASK;
        Class(
            Types::of(self)
                .of_class(type_id)
                .expect("every class has a description"),
        )
    }

    /// The class a call names by `ty`, when it names one.
    pub fn class_from(&self, ty: TypeId) -> Option<Class> {
        let types = Types::of(self);
        let described = types.described(ty).map(Desc::not_null)?;
        described.is_class().then_some(Class(described))
    }

    /// Start an instance of `class`. May collect.
    pub fn build(&mut self, class: Class) -> Build<'s> {
        let class = class.0;
        assert!(
            class.count() <= u64::BITS as usize,
            "a class built from Rust has at most 64 fields"
        );
        let types = Types::of(self);
        let type_id = class.a();
        let size = types.instance_size(type_id);
        let flags = if types.is_panic(type_id) {
            HEADER_PANIC
        } else {
            0
        };
        let object = self.mutator().allocate_object(type_id | flags, size);
        // SAFETY: the instance is `size` bytes, and zero is a slot's empty
        // value whatever it holds.
        let object = unsafe {
            std::ptr::write_bytes((object + WORD) as *mut u8, 0, size - WORD);
            self.root_raw(object)
        };
        Build {
            class,
            object,
            set: 0,
        }
    }
}

impl<'s> Build<'s> {
    /// End the program: a native gave a field what it cannot hold.
    #[cold]
    fn misfit(&self, field: usize, what: &str) -> ! {
        let name = self
            .class
            .fields()
            .nth(field)
            .map_or("a field it does not have", |field| field.name);
        panic!(
            "a native building `{}` {what}: {name}",
            self.class.full_name()
        );
    }

    /// Give field `field`, by its position in the declaration, `value`.
    /// May collect, when the field holds any of several types and the
    /// value is a scalar.
    pub fn set(&mut self, scope: &mut Scope<'s>, field: usize, value: impl Into<Value<'s>>) {
        let Some((offset, ty)) = self.class.field_place(field) else {
            self.misfit(field, "set");
        };
        let inner = ty.not_null();
        let boxes = ty.repr() == REPR_BOXED;
        let got = match value.into() {
            Value::Null if admits_null(ty) => Got::Null,
            Value::Null => self.misfit(field, "gave `null` to a field that holds none"),
            Value::Bool(value) if boxes || matches!(inner.kind(), KIND_BOOL | KIND_BOOL_IS) => {
                Got::Bool(value)
            }
            Value::Int(value) if boxes || matches!(inner.kind(), KIND_INT | KIND_INT_IS) => {
                Got::Int(value)
            }
            Value::Float(value) if boxes || inner.kind() == KIND_FLOAT => Got::Float(value),
            Value::Variant(index) if inner.kind() == KIND_ENUM && index < inner.count() => {
                Got::Variant(inner.a(), index)
            }
            Value::Variant(index) if inner.kind() == KIND_VARIANT && index == inner.b() => {
                Got::Variant(inner.of_a().a(), index)
            }
            Value::Ref(held) => {
                let object = held.get(scope);
                // SAFETY: a root holds a live object or zero.
                let header = if object == 0 {
                    0
                } else {
                    unsafe { load(object) & TYPE_ID_MASK }
                };
                let fits = match inner.kind() {
                    _ if object == 0 => false,
                    KIND_CLASS => header == inner.a(),
                    KIND_STRING | KIND_STRING_IS => header < STRING_TYPES_END,
                    KIND_BOOL | KIND_INT | KIND_FLOAT | KIND_ENUM | KIND_NULL | KIND_BOOL_IS
                    | KIND_INT_IS | KIND_VARIANT => false,
                    _ => true,
                };
                if !fits {
                    self.misfit(field, "gave an object to a field that does not hold it");
                }
                Got::Ref(object)
            }
            _ => self.misfit(field, "gave a scalar to a field that does not hold it"),
        };
        // A reference is placed as it is, so nothing is allocated between
        // reading it from its root and storing it.
        let placed = place(got, ty.repr(), |type_id, bits| {
            let boxed = scope.mutator().allocate_object(type_id, BOX_SIZE);
            // SAFETY: a box is a header and one value.
            unsafe { store(boxed + BOX_VALUE, bits) };
            boxed
        });
        // SAFETY: the description says where the field is and how its
        // slot holds the value, which was checked to be one of its type.
        unsafe { put(self.object.raw(scope), offset, ty.repr(), placed) };
        self.set |= 1 << field;
    }

    /// [`Self::set`], for the field called `name`.
    pub fn set_named(&mut self, scope: &mut Scope<'s>, name: &str, value: impl Into<Value<'s>>) {
        match Class(self.class).field_named(name) {
            Some(field) => self.set(scope, field, value),
            None => panic!(
                "a native building `{}` set a field it does not have: {name}",
                self.class.full_name()
            ),
        }
    }

    /// The instance. Every field whose type does not admit `null` must
    /// have been set: compiled code reads such a slot without a test.
    pub fn finish(self) -> Root<'s, Obj> {
        for field in 0..self.class.count() {
            let unset = self.set >> field & 1 == 0;
            if unset
                && let Some((_, ty)) = self.class.field_place(field)
                && (ty.kind() == KIND_OPAQUE || !admits_null(ty))
            {
                self.misfit(field, "left a field unset");
            }
        }
        self.object
    }
}
