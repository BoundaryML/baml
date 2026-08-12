use std::{any::Any, sync::Arc};

use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{
    ArrayContainer, BoundMethod, Class, CollectorRef, Enum, Function, GenericFunction, HostClosure,
    Instance, MapContainer, Uint8ArrayContainer, UnscheduledFuture, Value, Variant,
    types::{
        Array, Cell, Closure, FunctionType, FutureType, InterfaceDef, Map, Package, RuntimeImplRule,
    },
};

/// Any data that the Baml program can reference and is allocated on heap.
///
/// `Vm` (in `bex_vm` crate) should own objects and give references to them to the running Baml
/// program. Internally, in the `Vm` code, note that by reference I don't mean
/// a Rust reference (& or &mut), but rather a [`usize`] that is used to index
/// into the `Vm::objects` pool.
///
/// Read `Vm::objects` for more information.
#[derive(Clone, Debug)]
pub enum Object {
    /// Package object.
    Package(Box<Package>),

    /// Function object.
    Function(Box<Function>),

    /// Represents an interface definition. Not a value object.
    Interface(Box<InterfaceDef>),

    /// Represents an implementation rule for an interface.
    ImplRule(Box<RuntimeImplRule>),

    /// Class object.
    Class(Box<Class>),

    /// Class instance object.
    Instance(Instance),

    /// Enum object.
    Enum(Box<Enum>),

    /// Enum value object.
    Variant(Variant),

    /// A closure: a function paired with captured variable cells.
    Closure(Closure),

    /// A method bound to a specific receiver instance.
    /// Created by `MakeBoundMethod`. The receiver is inserted as `self`
    /// at call time by `CallIndirect`.
    BoundMethod(BoundMethod),

    /// A generic function instantiation carrying concrete type arguments
    /// (`foo<int>` referenced as a value, not called). Pooled and interned at
    /// compile time so identical instantiations share one object
    /// (pointer-stable `foo<int> === foo<int>`). When called, the VM resolves
    /// `function` via the global table and seeds `frame.type_args` from
    /// `type_args`, so type-reifying bodies (`reflect.type_of<T>`, json natives)
    /// work through the value.
    GenericFunction(GenericFunction),

    /// A host-language callable bound to a BAML function type.
    ///
    /// Created at the FFI boundary when a `HostValue` is passed for a
    /// `RuntimeTy::Function` parameter. Calling it (`CallIndirect`) dispatches
    /// `SysOp::BamlHostCallHostValue`, which fires the bridge's
    /// `HostDispatchFn` and awaits the host's response.
    HostClosure(HostClosure),

    /// A mutable cell holding a single captured value.
    Cell(Cell),

    /// Heap allocated string backed by [`bex_str::BexStr`].
    ///
    /// `BexStr` avoids allocating for small strings (up to ~22 bytes inline)
    /// and reference-counts larger ones, making clone cheap.
    String(bex_str::BexStr),

    /// Heap-allocated arbitrary-precision integer.
    ///
    /// `Value: Copy` so bigints must live on the heap behind an `Arc`. The
    /// `Arc` lets multiple values share the same allocation (e.g. after a
    /// `let y = x` assignment) without deep-copying the underlying digit slice.
    Bigint(std::sync::Arc<num_bigint::BigInt>),

    /// Byte array (uint8array). Wrapped in [`Uint8ArrayContainer`] so the
    /// underlying `Vec<u8>` is protected by a [`LazyBiasedMutex`](`crate::lazy_biased_mutex::LazyBiasedMutex`) against
    /// racing mutation under `spawn`.
    Uint8Array(Uint8ArrayContainer),

    /// List of values. Wrapped in [`ArrayContainer`] so the underlying
    /// `Vec<Value>` is protected by a [`LazyBiasedMutex`](`crate::lazy_biased_mutex::LazyBiasedMutex`) against racing
    /// mutation under `spawn`.
    Array(Array),

    /// Map of values. Wrapped in [`MapContainer`] so the underlying
    /// `IndexMap` is protected by a [`LazyBiasedMutex`](`crate::lazy_biased_mutex::LazyBiasedMutex`) against racing
    /// mutation under `spawn`.
    Map(Map),

    /// Boxed 64-bit float. Floats are heap-allocated because `Value`
    /// itself is a single tagged 64-bit word with no inline encoding
    /// for full-precision f64. Allocation rate is low in practice —
    /// BAML programs are integer-and-object-heavy.
    Float(f64),

    Future(crate::Future),
    /// Only used for requesting scheduling of a future, passed from VM to engine.
    /// Boxed: under `heap_debug` the instrumented `HeapPtr` makes the inline
    /// payload (closure + name + config pointers) push `Object` past its
    /// 64-byte interpreter-hot-loop budget.
    UnscheduledFuture(Box<UnscheduledFuture>),

    /// Opaque Rust-managed data, accessed via `Arc<dyn Any>` downcast.
    /// Used for `$rust_type` fields in builtin classes (including media classes Pdf, Audio, Video, Image).
    RustData(Arc<dyn Any + Send + Sync>),

    /// Collector object (opaque handle to `bex_events::Collector`).
    Collector(CollectorRef),

    /// A type descriptor value — wraps a `baml_type::RealizedTy`.
    Type(Box<baml_type::RealizedTy>),

    #[cfg(feature = "heap_debug")]
    Sentinel(crate::types::SentinelKind),
}

const _: () = assert!(
    std::mem::size_of::<Object>() <= 64,
    "Object enum size regression — expected <= 64 bytes"
);

impl Object {
    /// The inner [`Package`] if this is an [`Object::Package`].
    #[inline]
    pub fn as_package(&self) -> Option<&Package> {
        match self {
            Object::Package(p) => Some(&**p),
            _ => None,
        }
    }

    /// The inner [`InterfaceDef`] if this is an [`Object::Interface`].
    #[inline]
    pub fn as_interface(&self) -> Option<&InterfaceDef> {
        match self {
            Object::Interface(i) => Some(&**i),
            _ => None,
        }
    }

    /// The inner [`RuntimeImplRule`] if this is an [`Object::ImplRule`].
    #[inline]
    pub fn as_impl_rule(&self) -> Option<&RuntimeImplRule> {
        match self {
            Object::ImplRule(r) => Some(&**r),
            _ => None,
        }
    }
}

// Custom borsh for Object: RustData and Collector contain non-serializable
// trait objects (Arc<dyn Any>). They should never appear in a compiled Program.
#[derive(BorshSerialize, BorshDeserialize)]
enum ObjectWire {
    Function(Box<Function>),
    Interface(Box<InterfaceDef>),
    Package(Box<Package>),
    ImplRule(Box<RuntimeImplRule>),
    Class(Box<Class>),
    Instance(Instance),
    Enum(Box<Enum>),
    Variant(Variant),
    Closure(Closure),
    BoundMethod(BoundMethod),
    GenericFunction(GenericFunction),
    Cell(Cell),
    String(String),
    // `Arc<BigInt>` isn't directly Borsh-derivable (and we want the same
    // wire form `ConstValue::Bigint` uses), so the proxy holds the inner
    // `BigInt` by value. `num_bigint::BigInt` has no native borsh impl, so
    // route through `baml_base::core_types::borsh_bigint`.
    Bigint(
        #[borsh(
            serialize_with = "baml_base::core_types::borsh_bigint::serialize",
            deserialize_with = "baml_base::core_types::borsh_bigint::deserialize"
        )]
        num_bigint::BigInt,
    ),
    Uint8Array(Vec<u8>),
    Array(Box<baml_type::RealizedTy>, Vec<Value>),
    Map(
        Box<baml_type::RealizedTy>,
        Box<baml_type::RealizedTy>,
        IndexMap<String, Value>,
    ),
    Float(f64),
    Future(crate::Future),
    // Boxed like the other large variants (and like `Object`'s own
    // `UnscheduledFuture`): the struct carries the spawn's `Future<T, E>` type
    // arguments, which are `RealizedTy`s, so inline it would set the whole
    // enum's size. Borsh treats `Box<T>` transparently, so the wire form is
    // unchanged.
    UnscheduledFuture(Box<UnscheduledFuture>),
    Type(Box<baml_type::RealizedTy>),
}

impl BorshSerialize for Object {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let proxy = match self {
            Self::Function(v) => ObjectWire::Function(v.clone()),
            Self::Interface(v) => ObjectWire::Interface(v.clone()),
            Self::Package(v) => ObjectWire::Package(v.clone()),
            Self::ImplRule(v) => ObjectWire::ImplRule(v.clone()),
            Self::Class(v) => ObjectWire::Class(v.clone()),
            Self::Instance(v) => ObjectWire::Instance(v.clone()),
            Self::Enum(v) => ObjectWire::Enum(v.clone()),
            Self::Variant(v) => ObjectWire::Variant(v.clone()),
            Self::Closure(v) => ObjectWire::Closure(v.clone()),
            Self::BoundMethod(v) => ObjectWire::BoundMethod(v.clone()),
            Self::GenericFunction(v) => ObjectWire::GenericFunction(v.clone()),
            Self::Cell(v) => ObjectWire::Cell(v.clone()),
            Self::String(v) => ObjectWire::String(v.to_string()),
            Self::Bigint(v) => ObjectWire::Bigint((**v).clone()),
            Self::Uint8Array(v) => ObjectWire::Uint8Array(v.lock().clone()),
            Self::Array(v) => ObjectWire::Array(v.element_ty.clone(), v.data.lock().clone()),
            Self::Map(v) => ObjectWire::Map(
                v.key_ty.clone(),
                v.value_ty.clone(),
                v.to_index_map()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
            Self::Float(v) => ObjectWire::Float(*v),
            Self::Future(v) => ObjectWire::Future(v.clone()),
            Self::UnscheduledFuture(v) => ObjectWire::UnscheduledFuture(v.clone()),
            Self::Type(v) => ObjectWire::Type(v.clone()),
            Self::RustData(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "RustData cannot be serialized",
                ));
            }
            Self::Collector(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Collector cannot be serialized",
                ));
            }
            Self::HostClosure(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "HostClosure cannot be serialized",
                ));
            }
            #[cfg(feature = "heap_debug")]
            Self::Sentinel(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Sentinel cannot be serialized",
                ));
            }
        };
        proxy.serialize(writer)
    }
}

impl BorshDeserialize for Object {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let proxy = ObjectWire::deserialize_reader(reader)?;
        Ok(match proxy {
            ObjectWire::Function(v) => Self::Function(v),
            ObjectWire::Interface(v) => Self::Interface(v),
            ObjectWire::Package(v) => Self::Package(v),
            ObjectWire::ImplRule(v) => Self::ImplRule(v),
            ObjectWire::Class(v) => Self::Class(v),
            ObjectWire::Instance(v) => Self::Instance(v),
            ObjectWire::Enum(v) => Self::Enum(v),
            ObjectWire::Variant(v) => Self::Variant(v),
            ObjectWire::Closure(v) => Self::Closure(v),
            ObjectWire::BoundMethod(v) => Self::BoundMethod(v),
            ObjectWire::GenericFunction(v) => Self::GenericFunction(v),
            ObjectWire::Cell(v) => Self::Cell(v),
            ObjectWire::String(v) => Self::String(bex_str::BexStr::from(v)),
            ObjectWire::Bigint(v) => Self::Bigint(std::sync::Arc::new(v)),
            ObjectWire::Uint8Array(v) => Self::Uint8Array(Uint8ArrayContainer::new(v)),
            ObjectWire::Array(ty, data) => Self::Array(Array {
                element_ty: ty,
                data: ArrayContainer::new(data),
            }),
            ObjectWire::Map(key_ty, value_ty, data) => Self::Map(Map {
                key_ty,
                value_ty,
                data: MapContainer::new(
                    data.into_iter()
                        .map(|(k, v)| (bex_str::BexStr::from(k), v))
                        .collect::<IndexMap<bex_str::BexStr, Value>>()
                        .into(),
                ),
            }),
            ObjectWire::Float(v) => Self::Float(v),
            ObjectWire::Future(v) => Self::Future(v),
            ObjectWire::UnscheduledFuture(v) => Self::UnscheduledFuture(v),
            ObjectWire::Type(v) => Self::Type(v),
        })
    }
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Function(function) => function.fmt(f),
            Object::Interface(interface) => write!(f, "<interface {}>", interface.name),
            Object::Package(_) => write!(f, "<package>"),
            Object::ImplRule(_) => write!(f, "<impl_rule>"),
            Object::Class(class) => class.fmt(f),
            Object::Instance(instance) => instance.fmt(f),
            Object::Enum(enm) => enm.fmt(f),
            Object::Variant(value) => value.fmt(f),
            Object::Closure(closure) => {
                let captures_len = closure.captures.len();
                write!(f, "<closure captures={captures_len}>")
            }
            Object::BoundMethod(_) => write!(f, "<bound_method>"),
            Object::GenericFunction(gf) => {
                write!(f, "<generic_function type_args={}>", gf.type_args.len())
            }
            Object::HostClosure(_) => write!(f, "<host_closure>"),
            Object::Cell(cell) => write!(f, "<cell {}>", cell.load()),
            Object::String(string) => string.fmt(f),
            Object::Bigint(bi) => write!(f, "{bi}"),
            Object::Uint8Array(bytes) => write!(f, "<uint8array len={}>", bytes.len()),
            Object::Array(array) => write!(
                f,
                "<array[{}] len={}>",
                array.element_ty,
                array.data.lock().len()
            ),
            Object::Map(map) => write!(
                f,
                "<map[{}: {}] len={}>",
                map.key_ty,
                map.value_ty,
                map.data.lock().len()
            ),
            Object::RustData(_) => write!(f, "<rust_data>"),
            Object::Collector(_) => write!(f, "<collector>"),
            Object::Type(ty) => write!(f, "<type: {ty}>"),
            Object::Future(future) => write!(f, "{}", future.read()),
            Object::UnscheduledFuture(_) => write!(f, "<unscheduled: spawn>"),
            Object::Float(v) => write!(f, "{v}"),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(kind) => write!(f, "<sentinel {kind:?}>"),
            // Object::BamlType(type_ir) => write!(f, "<baml type: {type_ir}>"),
        }
    }
}
/// Object type lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ObjectType {
    /// Top type of the lattice. It is castable to any of the other
    /// types.
    Any,
    Instance,
    Uint8Array,
    Array,
    Map,
    Function(FunctionType),
    Interface,
    Package,
    ImplRule,
    Closure,
    Cell,
    Class,
    String,
    Bigint,
    Enum,
    Variant,
    Future(FutureType),
    UnscheduledFuture,
    Collector,
    Type,
    RustData,
    Float,
}

impl ObjectType {
    pub fn of(ob: &Object) -> Self {
        match ob {
            Object::Function(func) => Self::Function(FunctionType::from(&func.kind)),
            Object::Interface(_) => Self::Interface,
            Object::Package(_) => Self::Package,
            Object::ImplRule(_) => Self::ImplRule,
            Object::Closure(_) => Self::Closure,
            Object::BoundMethod(_) => Self::Closure, // Treat as callable like closures
            Object::GenericFunction(_) => Self::Closure, // Callable like closures
            Object::HostClosure(_) => Self::Closure, // Callable like closures

            Object::Cell(_) => Self::Cell,
            Object::Class(_) => Self::Class,
            Object::Instance(_) => Self::Instance,
            Object::Enum(_) => Self::Enum,
            Object::Variant(_) => Self::Enum,
            Object::String(_) => Self::String,
            Object::Bigint(_) => Self::Bigint,
            Object::Uint8Array(_) => Self::Uint8Array,
            Object::Array(_) => Self::Array,
            Object::Map(_) => Self::Map,
            Object::RustData(_) => Self::RustData,
            Object::Collector(_) => Self::Collector,
            Object::Type(_) => Self::Type,
            Object::Future(fut) => Self::Future(fut.into()),
            Object::UnscheduledFuture(_) => Self::UnscheduledFuture,
            Object::Float(_) => Self::Float,
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Self::Any,
            // Object::BamlType(_) => Self::Any, // TODO
        }
    }
}

impl From<FutureType> for ObjectType {
    fn from(value: FutureType) -> Self {
        ObjectType::Future(value)
    }
}

impl From<FunctionType> for ObjectType {
    fn from(value: FunctionType) -> Self {
        ObjectType::Function(value)
    }
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Any => write!(f, "any"),
            ObjectType::Instance => write!(f, "instance"),
            ObjectType::Array => write!(f, "array"),
            ObjectType::Map => write!(f, "map"),
            ObjectType::Function(function_type) => write!(f, "{function_type}"),
            ObjectType::Interface => write!(f, "interface"),
            ObjectType::Package => write!(f, "package"),
            ObjectType::ImplRule => write!(f, "impl_rule"),
            ObjectType::Closure => write!(f, "closure"),
            ObjectType::Cell => write!(f, "cell"),
            ObjectType::Class => write!(f, "class"),
            ObjectType::Enum => write!(f, "enum"),
            ObjectType::Variant => write!(f, "variant"),
            ObjectType::Future(future_type) => write!(f, "{future_type}"),
            ObjectType::UnscheduledFuture => write!(f, "unscheduled_future"),
            ObjectType::String => write!(f, "string"),
            ObjectType::Bigint => write!(f, "bigint"),
            ObjectType::Uint8Array => write!(f, "uint8array"),
            ObjectType::Collector => write!(f, "collector"),
            ObjectType::Type => write!(f, "type"),
            ObjectType::RustData => write!(f, "rust_data"),
            ObjectType::Float => write!(f, "float"),
        }
    }
}
