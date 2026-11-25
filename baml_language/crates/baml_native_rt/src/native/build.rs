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
//! `baml.json` gives the fields of the instances it decodes, in room it
//! has reserved, the values the same function makes: [`place`].

use super::{Elem, Obj, Root, Scope, TypeId};
use crate::{
    describe::{Desc, Types},
    heap::{self, load, store},
    json_layout::{
        KIND_BOOL, KIND_BOOL_IS, KIND_CLASS, KIND_ENUM, KIND_FLOAT, KIND_INT, KIND_INT_IS,
        KIND_NULL, KIND_OPAQUE, KIND_OPTIONAL, KIND_STRING, KIND_STRING_IS, KIND_UNION,
        KIND_VARIANT, REPR_BOXED,
    },
    layout::{
        BOX_SIZE, BOX_VALUE, HEADER_PANIC, NAMED, STRING_TAGS_END, TAG_BOOL, TAG_FLOAT, TAG_INT,
        TYPE_ID_MASK,
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

impl Got {
    /// What the value `value` is, which its tag says. An enum's variant
    /// in a box is the reference to its box.
    pub(crate) fn of(value: heap::Value) -> Self {
        match value.id() {
            TAG_BOOL => Self::Bool(value.payload & 0xFF != 0),
            TAG_INT => Self::Int(value.payload.cast_signed() as i64),
            TAG_FLOAT => Self::Float(f64::from_bits(value.payload as u64)),
            _ if value.is_pointer() => Self::Ref(value.payload),
            _ => Self::Null,
        }
    }
}

/// The value a slot of representation `repr` holds `got` as. A variant
/// goes as its position into a slot of its enum's own type, and into any
/// other as a box, which `boxed` makes of the enum's type id and the
/// position: the address of the box, or zero from one that only counts,
/// and the slot is then `null`.
pub(crate) fn place(got: Got, repr: u32, boxed: impl FnOnce(usize, usize) -> usize) -> heap::Value {
    match got {
        Got::Null => heap::Value::NULL,
        // SAFETY: what was got is zero or a live object.
        Got::Ref(object) => unsafe { heap::Value::reference(object) },
        Got::Bool(value) => heap::Value::bool(value),
        Got::Int(value) => heap::Value::int(value),
        Got::Float(value) => heap::Value::float(value),
        // SAFETY: zero or a box just made.
        Got::Variant(type_id, index) if repr == REPR_BOXED => unsafe {
            heap::Value::reference(boxed(type_id, index))
        },
        Got::Variant(_, index) => heap::Value::immediate(TAG_INT, index),
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
    /// what it is an optional, an array or a map of, whether or not
    /// `baml.json` reads the class. Nothing for a field that holds no one
    /// class: a scalar, a string, a union, a value of a type parameter.
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
        Self::Ref(root.into())
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

/// An instance being built: every slot `null` until its field is set.
pub struct Build<'s> {
    class: Desc,
    object: Root<'s, Obj>,
    /// The fields set so far, a bit each.
    set: u64,
}

/// Whether a field of the type `ty` may be left unset: its slot is then
/// `null`.
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
        let object = self.mutator().allocate(size);
        // SAFETY: the instance is `size` bytes.
        let object = unsafe {
            crate::class::format(object, type_id | flags, size);
            self.root_raw(object)
        };
        Build {
            class,
            object,
            set: 0,
        }
    }
}

impl Scope<'_> {
    /// The slot of field `field` of `object`.
    fn field_slot(&self, object: super::ObjRef<'_>, field: usize) -> heap::Value {
        let class = self.class_of(object).0;
        let Some((offset, _)) = class.field_place(field) else {
            panic!(
                "a native read field {field} of `{}`, which has none there",
                class.full_name()
            );
        };
        // SAFETY: the description says where the field is in the instance.
        unsafe { heap::Value::load(super::View::raw(object) + offset) }
    }

    /// The `int`, `float` or `bool` in field `field` of `object`, by its
    /// position in the declaration, as the value's bits: `None` for
    /// `null`.
    pub fn field_bits(&self, object: super::ObjRef<'_>, field: usize) -> Option<u64> {
        let value = self.field_slot(object, field);
        match value.id() {
            TAG_BOOL | TAG_INT | TAG_FLOAT => Some(value.payload as u64),
            _ => None,
        }
    }

    /// The object field `field` of `object` refers to, by the field's
    /// position in the declaration: `None` for `null`.
    ///
    /// # Safety
    ///
    /// The field's type is `M`, or `M` or `null`.
    pub unsafe fn field_ref<'a, M: super::Managed>(
        &'a self,
        object: super::ObjRef<'a>,
        field: usize,
    ) -> Option<M::View<'a>> {
        let value = self.field_slot(object, field);
        // SAFETY: the caller's contract: the slot is a reference or
        // `null`.
        (!value.is_null()).then(|| unsafe { super::View::from_raw(value.payload) })
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
    /// value is an enum's variant.
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
                let held = held.value(scope);
                let (object, header) = (held.payload, held.id());
                let fits = match inner.kind() {
                    _ if !held.is_pointer() => false,
                    KIND_CLASS => header == inner.a(),
                    KIND_STRING | KIND_STRING_IS => header < STRING_TAGS_END,
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
        let placed = place(got, ty.repr(), |type_id, position| {
            let boxed = scope.mutator().allocate_object(type_id, BOX_SIZE);
            // SAFETY: an enum's box is a header and the variant's position.
            unsafe { store(boxed + BOX_VALUE, position) };
            boxed
        });
        // SAFETY: the description says where the field is and how its
        // slot holds the value, which was checked to be one of its type.
        unsafe { heap::Value::store(self.object.raw(scope) + offset, placed) };
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
