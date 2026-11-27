# Memory

Object layouts, allocation, field and index access, maps, strings. The collector is in `gc.md`. Numbers are instructions retired per operation, counted at `881e4c1dfc` unless they say otherwise; method in `README.md`. "Before" is the layout before two-word values, as these notes had it at `39d3ad4c25`. Go and Node numbers are the ones measured then and were not run again.

The structs below are the layouts as documentation: the runtime reads and writes them through the offsets in `src/layout.rs`, which is the source of truth, and has no such Rust types but `Value`.

## Values

Value slots are the counterpart to the VM's `Value`. They are 16 bytes and 16-byte-aligned.

The first 8 bytes are a header, while the remaining bytes are the payload.

Header format:

| bits | meaning |
|---|---|
| 0..31 | type id, dense, with no shift: `NULL` 0, `BOOL` 1, `INT` 2, `FLOAT` 3, `OMITTED` 4, then the objects |
| 32..58 | payload of the header. A string caches its hash here (27 bits); a function value holds its call layout id here (see "Tags") |
| 59 | collector: forwarded. The word is then `address \| 1 << 59` |
| 60 | collector: a mark used only while slices are settled |
| 61 | free |
| 62 | panic class |
| 63 | busy bit |

A word is 64 bits on every target. The low half of a header is the type id and nothing else, so a 32-bit read of a header or of a tag is the type id with no mask, even while the collector has its bits set (a forwarded header is an address, not a header). While it settles slices the collector keeps a count in bits 32..58 of a string's header, in place of the hash.

The type id determines how the payload is interpreted: the payload is either an inline scalar primitive (`null`, `int`, `float`, `bool`) or a heap pointer. Except for the scalar primitives, the type id is a fast path only, per [type-header-fast-path.md](ideas/type-header-fast-path.md).

The busy bit is a lock on the slot, so that its two words are never seen torn when several workers read and write it. Whoever reads or writes a slot's header holds the bit while it touches the payload, readers included; the exact sequences, the orderings and why a reader cannot merely re-check the header are in [uniform-values-convention.md](ideas/uniform-values-convention.md), "The busy bit protocol".

- A store to a slot whose header may change (a union, `unknown`, an optional, a generic slot, any pointer) sets the bit with an atomic or, retrying while it was already set, stores the payload, then stores the new header with the bit clear.
- A read of such a slot that needs the tag takes the bit the same way, loads the payload, and stores the header back unchanged.
- A slot whose declared type is exactly `int`, `float` or `bool` never changes its header: a store is one atomic store of the payload, a read one atomic load.
- A read that needs only the payload, because the static type fixes the type id, is one atomic load of the payload.
- Not under the protocol: the slots of an object nobody else can reach yet, a future's value (its state word is the lock), frames and registers, and the collector, which runs with the world stopped, never sets the bit and asserts in checked builds that no slot it scans has it.

```rust
#[repr(C, align(16))]
pub struct Value {
    pub header: ValueHeader,
    pub payload: ValuePayload,
}
const _: () = assert!(::std::mem::size_of::<Value>() == 16);
```

## Tags

A tag is the header word of a value. Its type id (the low 32 bits, the id itself with no shift) says how to read the payload, and for a pointer it is always exactly the id in the header of the object pointed at: when the collector makes a string of a slice it rewrites the tag of every slot that points at it. A function value's tag also carries its call layout id, always exactly. Every other bit of a tag is zero: code that needs a string's cached hash or the panic bit reads the object, and bits 59..63 are zero in every tag (bit 63 is set only in a slot's tag word in memory, while a worker holds the slot).

One word that looks like a tag is not stored anywhere: a native that can throw returns a value in two registers, and bit 59 of its tag (`THROWN`, the collector's forwarded bit, which no tag has) says the value is what it threw. The caller tests it with one `tbnz` (`natives.md`).

Which bits of the header a tag keeps, by kind:

| kind | bits of the header in the tag | how compiled code reads it from an object |
|---|---|---|
| immediate | the id | no object |
| callable (`CLOSURE`, `BOUND_METHOD`, `RAW_FUNCTION`, `REALIZED_FUNCTION`, `HOST_CLOSURE`) | 0..58: the id and the call layout id | the 64-bit word |
| every other pointer | 0..31: the id | the 32-bit word, which leaves a string's hash and the panic bit behind |

An object whose type is only known at run time may be a callable: its header is read whole and bits 32..58 kept only when the id is a callable's. There is one place that makes a tag of a header on each side: `tag_of_object` in the compiler (`codegen.rs`) and `layout::tag_of` in the runtime, which `Value::reference` uses. In checked builds the collector ends the program on a slot whose tag is not exactly what its object's header makes. No mask is applied for the collector's bits: they are zero whenever compiled code runs.

The ids below are fixed when the runtime is compiled and are the same in every program: the collector and the Rust runtime `match` on them with no program in hand, and compiled code compares against them as constants.

Immediates. The payload is the value itself, never an address:

| tag | payload | notes |
|---|---|---|
| `NULL` = 0 | always 0 | `T?` has no representation of its own; a `null` is this tag. A zeroed 16 bytes is a `null` |
| `BOOL` = 1 | 0 or 1 | |
| `INT` = 2 | the 64 bits | |
| `FLOAT` = 3 | the 64 bits | |
| `OMITTED` = 4 | 0 | an argument left to its default. Exists only in a call's arguments and in the frame of the function that was passed it, until its default is computed; never stored to an object, never a value a program holds ([default-arguments.md](ideas/default-arguments.md)). The collector in a checked build ends the program on an object slot or a runtime root that holds it |

Everything else is a pointer (an id above 4), and its tag names the layout of the object, one id per struct in "Objects" below. The ids in order from 5: `STRING`, `STRING_SLICE`, `INSTANCE`, `ENUM_VARIANT`, `CLASS`, `ARRAY`, `ARRAY_STORAGE`, `MAP`, `MAP_ENTRIES`, `MAP_INDEX`, `CELL`, `FUTURE`, `CLOSURE`, `BOUND_METHOD`, `BIGINT`, `BYTES`, `BYTES_STORAGE`, `TUPLE`, `TYPE`, `OWNED`, `PLAIN`, `RAW_FUNCTION`, `REALIZED_FUNCTION`, `HOST_CLOSURE`, `PACKAGE`, `RUST_INSTANCE` (30); `TAGS_END` is 31. That is the least the collector needs: which struct to read the object as, and so its size and where its slots and pointers are.

| tag | object | from |
|---|---|---|
| `INSTANCE` | `InstanceObject` | below |
| `RUST_INSTANCE` | `RustInstanceObject` | below. Reserved: nothing makes one |
| `ARRAY`, `ARRAY_STORAGE` | `ArrayObject`, `ArrayStorageObject` | below |
| `MAP`, `MAP_ENTRIES`, `MAP_INDEX` | `MapObject`, `MapEntriesObject`, `MapIndexObject` | below |
| `STRING`, `STRING_SLICE` | `StringObject`, `StringSliceObject` | below. Kept next to each other so "is a string" is one range test |
| `TYPE` | `TypeObject` | below |
| `RAW_FUNCTION`, `REALIZED_FUNCTION` | `RawFunctionObject`, `RealizedFunctionObject` | below. `REALIZED_FUNCTION` is reserved: nothing makes one |
| `CLOSURE`, `BOUND_METHOD`, `HOST_CLOSURE` | `ClosureObject`, `BoundMethodObject`, `HostClosureObject` | below. `HOST_CLOSURE` is reserved |
| `CELL` | `CellObject` | below |
| `FUTURE` | `FutureObject` | below |
| `BIGINT` | `BigintObject` | below |
| `BYTES`, `BYTES_STORAGE` | `BytesObject`, `BytesStorageObject` | below. A `uint8array`, packed |
| `TUPLE` | `TupleObject` | below |
| `ENUM_VARIANT` | `EnumVariantObject` | below. An enum value is a pointer to its variant; not an immediate |
| `CLASS`, `PACKAGE`, `INTERFACE` | `ClassObject`, `PackageObject`, `InterfaceObject` | below. `INTERFACE` has no id yet |
| `OWNED`, `PLAIN` | `OwnedObject`, `PlainObject` | below. Rust data with and without a destructor; `RustInstanceObject` may replace them later |

Gone from the runtime: the boxes (`TYPE_BOX_INT`, `TYPE_BOX_FLOAT`, `TYPE_BOX_BOOL`), the three cell types and the four map-entry types that differed only in which words were pointers, and the array types split by element kind. A slot's own tag now says whether it is a pointer.

Function values carry one more thing in the header. A call through a function value has to know whether the callee takes exactly the arguments the site pushes, because a function with defaulted parameters can stand where a type with fewer parameters is expected ([default-arguments.md](ideas/default-arguments.md)). So the header payload (bits 32..58) of every callable object, `CLOSURE`, `BOUND_METHOD`, `REALIZED_FUNCTION`, `HOST_CLOSURE` and a `RAW_FUNCTION` used as a value, holds its call layout id: a number the compiler gives each distinct parameter layout in the program (the required slots in order, then the named optional ones). A function value's tag carries the same id, so the caller already has it in the register that holds the tag, and every call through a function value compares it with the site's layout, with no load (`calls.md`). This is required for correctness, not a later optimization. The id is written when the object is made and never changes. Ids count from 1, so a header nothing gave a layout fails every check.

Not a fixed id: a class or an enum of the program. Every instance has the one id `INSTANCE`, and the truth about it is its `class_ptr`: `x is Foo` is a tag compare, a load and a pointer compare, and a call of an interface method is resolved through the interface's rules (`InterfaceObject` below), not through the class. An instance's header is `INSTANCE`, with bit 62 when its class is a panic, and nothing else. A class object or an enum variant may live in the heap (they cannot be made at run time today, but will be), so `class_ptr` is traced and nothing hashes or orders by a class's or a variant's address. A per-class id is the kind of shortcut [type-header-fast-path.md](ideas/type-header-fast-path.md) is about.

Also fixed, though not type ids: the classes of the standard library that the runtime builds instances of and must be able to find without being handed them, 25 today (`WELL_KNOWN` in `src/layout.rs`: the panics, `errors.InvalidArgument`, the `json` errors, `errors.Io` and so on). The runtime finds each one's class object in a table of class object addresses the compiler emits (`baml_native_types`, at `PROGRAM_CLASSES`), in the order of `WELL_KNOWN` then `NAMED` (35), zero for a `NAMED` class the program does not name. After it come the descriptions `baml.json` and the uncaught report read (`PROGRAM_DESCRIPTIONS`). There is no type table. Bit 62 of the header marks an instance of a panic class; it is set from `CLASS_PANIC` in the class object's `kind`.

## Objects

An object is a 16-byte-aligned dynamically sized type which lives on the heap. It needs to be 16-byte-aligned so it can be aliased with more specific layouts which hold `Value` slots:

```rust
/// The address of compiled code that a call jumps to. Never a heap address, so the collector leaves it alone.
#[repr(transparent)]
pub struct EntryPoint(NonNull<u8>);

/// A class's position among the program's classes: dense, for a `match` over classes.
#[repr(transparent)]
pub struct ClassIndex(u32);

/// An enum variant's position in its declaration. What a map hashes for an enum key.
#[repr(transparent)]
pub struct VariantIndex(u64);

/// What a class object describes: a class, an enum or a Rust-backed class, and whether it is a panic.
#[repr(transparent)]
pub struct ClassKind(u32);

/// Whether an `await` of the future has already thrown its error.
#[repr(transparent)]
pub struct FutureFlags(u64);

/// An associated type's position in its interface's declaration.
#[repr(transparent)]
pub struct AssocIndex(u32);

/// A field's position among an instance's slots, type parameters included.
#[repr(transparent)]
pub struct SlotIndex(u32);

#[repr(C, align(16))]
pub struct Object {
    pub header: ObjectHeader,
    _payload: [Slot64],
}

/// Can be aliased over an `Object` with an `Instance` layout
#[repr(C, align(16))]
pub struct InstanceObject {
    pub header: ObjectHeader,
    pub class_ptr: NonNull<ClassObject>,
    /// `[generics ++ fields]`. A generic slot is `(TYPE, address of a TypeObject)`; `class_ptr.generic_count` says how many there are.
    pub slots: [Value],
}

/// Can be aliased over an `Object` with a `RustInstance` layout
#[repr(C, align(16))]
pub struct RustInstanceObject {
    pub header: ObjectHeader,
    pub class_ptr: NonNull<ClassObject>,
    /// An enum or something here
    pub data: RustInstanceData,
    /// No need for `Value` headers since they are all type objects
    pub generics: [NonNull<TypeObject>],
}

/// Can be aliased over an `Object` with an `Array` layout
#[repr(C, align(16))]
pub struct ArrayObject {
    pub header: ObjectHeader,
    pub length: u64,
    /// The element type.
    pub ty: NonNull<TypeObject>,
    /// The storage object, not its first element.
    pub ptr: NonNull<ArrayStorageObject>,
}

/// Can be aliased over an `Object` with an `ArrayStorage` layout
#[repr(C, align(16))]
pub struct ArrayStorageObject {
    pub header: ObjectHeader,
    pub capacity: u64,
    /// Length is `capacity`
    pub data: [Value],
}

/// Can be aliased over an `Object` with a `Map` layout
#[repr(C, align(16))]
pub struct MapObject {
    pub header: ObjectHeader,
    pub key_ty: NonNull<TypeObject>,
    pub value_ty: NonNull<TypeObject>,
    pub len: u32,
    pub used: u32,
    pub entries: NonNull<MapEntriesObject>,
    pub index: NonNull<MapIndexObject>,
    /// 2 * capacity - 1
    pub mask: u32,
    pub epoch: u32,
}

/// Can be aliased over an `Object` with a `MapEntries` layout
#[repr(C, align(16))]
pub struct MapEntriesObject {
    pub header: ObjectHeader,
    pub capacity: u64,
    /// Length is `capacity`
    pub records: [MapRecord]
}

/// The hashes are not here: they follow the records as `capacity` `u32`s, so a small map's probe compares all eight at once.
/// The key is a whole `Value`, so the collector scans entries with no map in hand.
#[repr(C, align(16))]
struct MapRecord {
    pub key: Value,
    pub value: Value,
}

/// Can be aliased over an `Object` with a `MapIndex` layout
#[repr(C, align(16))]
pub struct MapIndexObject {
    pub header: ObjectHeader,
    /// 2 * capacity
    pub slot_count: u64,
    pub slots: [u64],
}

/// Can be aliased over an `Object` with a `String` layout
#[repr(C, align(16))]
pub struct StringObject {
    pub header: ObjectHeader,
    pub byte_len: u32,
    pub char_len: u32,
    pub data: str,
}

/// Can be aliased over an `Object` with a `StringSlice` layout
#[repr(C, align(16))]
pub struct StringSliceObject {
    pub header: ObjectHeader,
    pub byte_len: u32,
    pub char_len: u32,
    pub parent: NonNull<StringObject>,
    pub offset: u32,
    // padding: u32
}

/// Can be aliased over an `Object` with a `Type` layout
#[repr(C, align(16))]
pub struct TypeObject {
    pub header: ObjectHeader,
    // /// Once better caching is introduced
    // canonical_id: u64,
    pub ty: RealizedTy,
}
// The compiler places the type object of every type the program names whole in the executable's data, its
// children static. A type made of a function's type arguments (`T?`, `T[]`, `Box<T>`) is built when the program
// runs by substituting them into a `TyTemplate`; that one is in the heap and owns its children. A union is built
// as written, and `Self.Item` is reduced through the matched rule's `interface_assoc`.
// Two type objects are compared by the rules of `baml_type::normalize` (see "Types and rules are copies"), the
// same address being the only shortcut. Nothing is interned.
// The collector runs Rust's `drop` on the `ty` of every type object it does not copy, as it does for an
// `OwnedObject`, and moves one by copying its bytes. The drop runs only while the world is stopped, frees only
// owned children, and no two type objects share owned memory.

/// What `Cow<'static, [T]>` is, with a layout the compiler can write: the items are in the executable's data, or
/// in memory this owns and frees when it is dropped.
#[repr(C)]
pub struct CowSlice<T> { pub ptr: NonNull<T>, pub len: u32, pub owned: bool }

/// The same for one item.
#[repr(C)]
pub struct CowBox<T> { pub ptr: NonNull<T>, pub owned: bool }

/// UTF-8 text that is not a BAML string: a parameter's name, a literal type's text.
pub type Text = CowSlice<u8>;

/// A type with nothing left to fill in. A copy of `baml_type::RealizedTy`, with a layout the compiler can write,
/// children that are static or owned, and the address of the class or interface object where the original has a name.
#[repr(C, u8)]
pub enum RealizedTy {
    Int, Bigint, Float, String, Bool, Null, Uint8Array,
    Media(MediaKind),
    Literal(Literal, Freshness),
    Class(NonNull<ClassObject>, CowSlice<RealizedTy>),
    Interface(NonNull<InterfaceObject>, CowSlice<RealizedTy>, CowSlice<AssocBinding<RealizedTy>>),
    Enum(NonNull<ClassObject>),
    EnumVariant(NonNull<EnumVariantObject>),
    List(CowBox<RealizedTy>),
    Map { key: CowBox<RealizedTy>, value: CowBox<RealizedTy> },
    Union(CowSlice<RealizedTy>),
    Function { params: CowSlice<FunctionParamTy<RealizedTy>>, ret: CowBox<RealizedTy>, throws: CowBox<RealizedTy> },
    Future(CowBox<RealizedTy>, CowBox<RealizedTy>),
    RustType, Type, Resource, PromptAst,
    /// A recursive alias, kept by name in the original: here the type object of what it stands for.
    TypeAlias(NonNull<TypeObject>),
    Unknown, Never,
    Tuple(CowSlice<RealizedTy>),
}

/// A type with holes for the type arguments of the code it appears in. A copy of `baml_type::TyTemplate`: every
/// variant of `RealizedTy` with `TyTemplate` where that has `RealizedTy`, and two more.
#[repr(C, u8)]
pub enum TyTemplate {
    // ... every variant of `RealizedTy`, over `TyTemplate`
    /// The n-th type argument of the frame.
    TypeArgRef(u32),
    /// `base.Item`, reduced through the rule that makes `base` implement `interface`.
    AssociatedTypeProjection { base: CowBox<TyTemplate>, interface: CowBox<InterfaceBound>, member: AssocIndex },
}

#[repr(C)]
pub struct FunctionParamTy<T> { pub name: Option<Text>, pub ty: T, pub mode: FunctionParamMode }
#[repr(u8)] pub enum FunctionParamMode { Required, Optional }
#[repr(u8)] pub enum MediaKind { Image, Audio, Video, Pdf, Generic }
#[repr(u8)] pub enum Freshness { Fresh, Regular }
#[repr(C, u8)]
pub enum Literal { Int(i64), Bigint(CowSlice<u64>), Float(Text), String(Text), Bool(bool) }

/// An associated type pinned to a type. A struct because a Rust tuple has no specified layout, so the compiler could not write one.
#[repr(C)]
pub struct AssocBinding<T> { pub member: AssocIndex, pub ty: T }

/// An interface at given arguments, with the associated types it pins. A copy of `bex_vm_types`'s `InterfaceBound`.
/// `InterfaceBound<RealizedTy>` is the realized form, which the copied normal form and the reduction of `Self.Item` take.
#[repr(C)]
pub struct InterfaceBound<T = TyTemplate> {
    pub interface: NonNull<InterfaceObject>,
    pub args: CowSlice<T>,
    pub assoc: CowSlice<AssocBinding<T>>,
}

/// Can be aliased over an `Object` with a `RawFunction` layout
/// Equivalent to `bex_vm_types::types::Function`
#[repr(C, align(16))]
pub struct RawFunctionObject {
    /// Header payload: the call layout id.
    pub header: ObjectHeader,
    /// At offset 8 in every callable: what a call through this object jumps to. Here, the function itself.
    pub entry: EntryPoint,
    pub owner: NonNull<PackageObject>,
    pub name: NonNull<StringObject>,
    /// How many type arguments a value of this function carries.
    pub type_param_count: u32,
    /// How many variable cells a closure of it captures.
    pub capture_count: u32,
    // ... other fields
}

/// Can be aliased over an `Object` with a `Closure` layout
#[repr(C, align(16))]
/// Header payload: the call layout id. Every callable has its `entry` at offset 8 and is itself the closure word, so one call sequence serves all of them.
pub struct ClosureObject {
    pub header: ObjectHeader,
    /// The function's body, which reads its captures from this object.
    pub entry: EntryPoint,
    pub function: NonNull<RawFunctionObject>,
    /// `[generics ++ variable cells]`: `function.type_param_count` type objects, then `function.capture_count` values each pointing at a `CellObject`.
    /// The collector sizes a closure through `function`, which it may already have copied: it follows the forwarding word.
    pub captures: [Value],
}

/// Can be aliased over an `Object` with a `Cell` layout
#[repr(C, align(16))]
pub struct CellObject {
    pub header: ObjectHeader,
    // padding: u64,
    pub value: Value,
}

/// Can be aliased over an `Object` with a `BoundMethod` layout
#[repr(C, align(16))]
/// Header payload: the layout of the method without its receiver.
pub struct BoundMethodObject {
    pub header: ObjectHeader,
    /// A stub that passes `receiver` and `type_args` and calls the method.
    pub entry: EntryPoint,
    pub function: NonNull<RawFunctionObject>,
    pub receiver: Value,
    /// Length is `function.type_param_count`
    pub type_args: [NonNull<TypeObject>],
}

/// Can be aliased over an `Object` with a `RealizedFunction` layout
/// Equivalent to `bex_vm_types::types::GenericFunction`
#[repr(C, align(16))]
pub struct RealizedFunctionObject {
    pub header: ObjectHeader,
    /// A stub that passes `type_args` and calls the function.
    pub entry: EntryPoint,
    pub function: NonNull<RawFunctionObject>,
    /// Length is `function.type_param_count`
    pub type_args: [NonNull<TypeObject>],
}

/// Can be aliased over an `Object` with a `HostClosure` layout
#[repr(C, align(16))]
pub struct HostClosureObject {
    pub header: ObjectHeader,
    /// A stub that calls the host through `handle`.
    pub entry: EntryPoint,
    pub handle: ::std::sync::Arc<HostValuePayload>,
    pub return_ty: NonNull<TypeObject>,
    pub throws_ty: NonNull<TypeObject>,
    pub arity: usize,
    pub params: [FunctionParam]
}

/// Can be aliased over an `Object` with a `Future` layout
#[repr(C, align(16))]
pub struct FutureObject {
    pub header: ObjectHeader,
    pub state: AtomicUsize,
    pub value: Value,
    pub task: *mut Task,
    /// Whether an `await` has thrown its error. The value's own tag says whether it is a reference.
    pub flags: FutureFlags,
    /// What the error it settled with travels with, for the `await` that throws it again: a trace object. `None` until then.
    pub trace: Option<NonNull<Object>>,
}

/// Tag `BIGINT`. A count and that many limbs, two's complement, least significant first.
#[repr(C, align(16))]
pub struct BigintObject {
    pub header: ObjectHeader,
    pub limb_count: u64,
    pub limbs: [u64],
}

/// Tag `BYTES`. The length never changes; the bytes are in a storage object.
#[repr(C, align(16))]
pub struct BytesObject {
    pub header: ObjectHeader,
    pub length: u64,
    pub ptr: NonNull<BytesStorageObject>,
}

/// Tag `BYTES_STORAGE`. Packed bytes, no slots.
#[repr(C, align(16))]
pub struct BytesStorageObject {
    pub header: ObjectHeader,
    pub capacity: u64,
    pub data: [u8],
}

/// Tag `TUPLE`.
#[repr(C, align(16))]
pub struct TupleObject {
    pub header: ObjectHeader,
    pub count: u64,
    pub slots: [Value],
}

/// Tag `ENUM_VARIANT`. One object per variant; an enum value is its address, and equality is the address.
/// A map hashes an enum key by `index`, never by the address.
#[repr(C, align(16))]
pub struct EnumVariantObject {
    pub header: ObjectHeader,
    /// At offset 8, where an instance has `class_ptr`: the class of either is found the same way.
    pub enum_ptr: NonNull<ClassObject>,
    pub index: VariantIndex,
    pub name: NonNull<StringObject>,
}

/// Tag `CLASS`. Also describes an enum (`kind`).
#[repr(C, align(16))]
pub struct ClassObject {
    pub header: ObjectHeader,
    pub package: NonNull<PackageObject>,
    pub name: NonNull<StringObject>,
    /// Generics then fields: an instance is `16 + 16 * slot_count` bytes.
    pub slot_count: u32,
    pub generic_count: u32,
    pub kind: ClassKind,
    pub index: ClassIndex,
    /// Field names, places and types: what JSON and the uncaught report read. Program data. `None` for a class with nothing to describe.
    pub description: Option<NonNull<u8>>,
    /// An enum's variants, one address each in declaration order. `None` for a class.
    pub variants: Option<NonNull<[NonNull<EnumVariantObject>]>>,
    /// Every method the class declares, in declaration order. Not a value a program can hold: reflection on an
    /// instantiated class (`Box<int>`, never `Box`) makes function values from these, with the class's type
    /// arguments bound. Interface methods are not found here: see `InterfaceObject`.
    pub methods: NonNull<[NonNull<RawFunctionObject>]>,
}

/// Tag `INTERFACE`. A call of an interface method is resolved here, not on the receiver's class: an implementor
/// need not be a class (`implement<T> I for T[]`), and a rule can match a type that did not exist when it was written.
#[repr(C, align(16))]
pub struct InterfaceObject {
    pub header: ObjectHeader,
    pub package: NonNull<PackageObject>,
    pub name: NonNull<StringObject>,
    pub generic_count: u32,
    pub method_count: u32,
    /// How many fields the interface declares: what a rule's `field_links` has one entry for.
    pub field_count: u32,
    /// How many associated types it declares: what an `AssocIndex` is a position among.
    pub assoc_count: u32,
    /// Rules the compiler emitted: in the executable's data, never freed, nothing in them moves.
    pub static_rules: &'static [ImplRule],
    /// Rules added while the program runs: an ordinary Rust list owned by the runtime, outside the heap.
    /// `None` until the first one is added, so a program that never adds a rule tests one word.
    /// The lock is for a worker that adds a rule while another resolves a call.
    pub dynamic_rules: Option<NonNull<RwLock<Vec<ImplRule>>>>,
    /// One per method the interface declares, in declaration order. A method index is a position here.
    pub methods: [InterfaceMethod],
}

pub struct InterfaceMethod {
    pub name: NonNull<StringObject>,
    /// The default body, compiled once against the frame `[Self, interface args..]`. `None` if an implementor must provide it.
    pub default_fn: Option<NonNull<RawFunctionObject>>,
}

/// One `implement ... for ...`. Not a heap object and never a value: no header, no tag.
/// The same struct whether the compiler emitted it or the program added it: the lists of an emitted one are
/// static, those of an added one owned. A rule added later can point at a function object in the heap, so the
/// collector visits the `function` of each method of every rule in `dynamic_rules`.
/// A copy of `bex_vm_types::types::RuntimeImplRule`.
#[repr(C)]
pub struct ImplRule {
    /// The package that declared the rule, which may not be the interface's.
    pub owner: NonNull<PackageObject>,
    /// What implements the interface, over the rule's own type parameters: `Wrap<T>`, `T[]`, `Foo`.
    pub for_ty_pattern: TyTemplate,
    /// Per type parameter of the rule, the interfaces its argument must implement.
    pub generic_param_bounds: CowSlice<CowSlice<InterfaceBound>>,
    pub interface_args: CowSlice<TyTemplate>,
    /// What `Self.Item` reduces through.
    pub interface_assoc: CowSlice<AssocBinding<TyTemplate>>,
    /// Indexed by the interface's method index. `None` adopts the interface's `default_fn`.
    pub methods: CowSlice<Option<MethodImpl>>,
    /// Interface field index to the implementor's slot.
    pub field_links: CowSlice<SlotIndex>,
}

#[repr(C)]
pub struct MethodImpl {
    /// A heap object like any other function object.
    pub function: NonNull<RawFunctionObject>,
    /// The type arguments the method was compiled against, over the rule's own type parameters.
    pub frame: CowSlice<TyTemplate>,
}

/// Tag `PACKAGE`.
#[repr(C, align(16))]
pub struct PackageObject {
    pub header: ObjectHeader,
    pub name: NonNull<StringObject>,
}

/// Tag `OWNED`. A Rust value with a destructor, dropped when the object is not copied.
#[repr(C, align(16))]
pub struct OwnedObject {
    pub header: ObjectHeader,
    pub value: *const (),
    pub kind: &'static RustKind,
}

/// Tag `PLAIN`. `Copy` Rust data held inline.
#[repr(C, align(16))]
pub struct PlainObject {
    pub header: ObjectHeader,
    pub words: u64,
    pub kind: &'static RustKind,
    pub data: [u64],
}
```

## Types and rules are copies

The runtime depends on neither `baml_type` nor `bex_vm_types`. What it needs of them it has its own copy of, written by hand (not with `ty_family!`), with layouts the compiler can write into the executable and children that are static or owned:

| here | copied from |
|---|---|
| `RealizedTy`, `TyTemplate`, `FunctionParamTy`, `Literal`, `Freshness`, `MediaKind` | `baml_type` (`family.rs`), `baml_base` |
| substitution of type arguments into a template, and the reduction of `Self.Item` | `baml_type/src/template.rs` |
| equivalence, subtyping and normal form of realized types | `baml_type/src/normalize.rs` |
| `ImplRule`, `MethodImpl`, `InterfaceBound` | `bex_vm_types/src/types/interface.rs` |
| matching a type against an interface's rules, with their bounds | `bex_vm/src/package_baml/resolve.rs` |
| testing a value against a type | `bex_vm/src/type_match.rs` |

Only what realized types and templates need is copied: nothing of inference, type variables by name or the interned pool.

Every file that holds a copy starts with a note naming the file it was copied from and saying that a change to either must be made to both. That note is the only guard on the logic. The types have a second one: the native compiler turns `baml_type`'s templates into these to emit them, in a `match` with no catch-all arm, so a variant added to the original stops the compiler from building until the copy has it.

## Sizes

The sizes of the structs in "Objects". Where the code differs today ("Not built"), the row says so. Go sizes are from the Go 1.26 runtime's types and are not measured here.

| object | bytes | before | Go equivalent | Go bytes |
|---|---|---|---|---|
| instance of a class with n slots | 16 + 16n | 8 + 8n | struct of n words | 8n (rounded to a size class) |
| `P { x int }` | 32 | 16 | `struct{ x int }` | 8 |
| array (`T[]`) | 32 (header, length, `ty`, storage address) plus storage | 24 | slice header (value, not an object) | 24 |
| array storage of capacity c | 16 + 16c (header, capacity) | 16 + 8c | backing array | 8c |
| `[i, i]` | 32 + 48 = 80, one allocation | 56 | `[]int{i, i}` | 16 heap + 24 header |
| string of b bytes | 16 + b rounded up to 16 | rounded up to 8 | string header + bytes | 16 + b |
| string slice | 32 | 32 | string header | 16 |
| cell | 32 | 16 | | |
| closure with g type arguments and c captures | 32 + 16(g + c) (header, entry, function, 8 of padding) | 24 | funcval + captures | 8 + captures |
| bound method with t type arguments | 48 + 8t rounded to 16 (header, entry, function, 8 of padding, receiver) | | | |
| tuple of n | 16 + 16n | | | |
| map | 64 (header, `key_ty`, `value_ty`, len u32 + used u32, entries, index, mask u32 + epoch u32, padding) | 40 | `map` header (swiss) | 48 |
| map entries, capacity c | 16 + 32c + 4c rounded to 16 | 16 + 16c + 4c rounded to 8 | group of 8 slots | 8 + 8 x (key + value) |
| map entry, `map<string,int>` | 36 per slot of capacity, plus an index slot once past 8 entries | 20 | `map[string]int` slot | 25 per slot of capacity |
| future | 64 | 48 | channel of capacity 1 | 96 + element |
| class object | 80 (header, package, name, slot and generic counts, kind and index, description, variants and methods as an address and a length each). Today 56 + 8m rounded to 16, with a row of m method columns | none | | |
| interface object with m methods | 64 + 16m (header, package, name, four counts, static rules as an address and a length, dynamic rules) | none | | |
| enum variant object | 32 | none (a position) | | |
| task (not on the heap) | 128, plus a 64 KB stack and 16 KB guard | same | `g` | about 450, plus a stack from 8 KB |

Every slot is 16 bytes whatever it holds: a `bool`, an enum, an `int?`, a union. An optional scalar is one slot (it was two words). A union or `unknown` holding an `int` is the slot itself; there are no boxes.

Arrays keep elements in a separate storage object so the array's address never changes when it grows. Every element access is therefore two dependent loads (array to storage address, storage to element) unless the storage address is already in a register.

## Allocation fast path

Compiled code bumps the worker's pointer inline. `codegen.rs`, `allocate`:

```rust
let object = load(mutator + MUTATOR_ALLOC_PTR);
let limit  = load(mutator + MUTATOR_ALLOC_LIMIT);
let end    = iadd_imm(object, size);
brif(icmp(UnsignedGreaterThan, end, limit), slow, bump);
// bump:
store(end, mutator + MUTATOR_ALLOC_PTR);
// slow (cold): scratch = size; call baml_rt_alloc_slow; object = scratch
```

The memory is not zeroed. The caller writes the header, the class object's address and then both words of every slot, with plain stores: nobody else can reach the object yet. There is one span and it is the whole space (8 MB to start), so the limit check fails only when a collection is due.

```baml
class P { x: int }
function main() -> int {
  let keep: P[] = [P { x: 0 }, /* 8 of them */];
  for (let i = 0; i < N; i += 1) { keep[i & 7] = P { x: i }; };
  return keep[3].x;
}
```

The store into `keep` is what keeps scalar replacement (`optimize/scalar_replace.rs`) from deleting the object.

```asm
  ldr  x3, [x21]             ; alloc_ptr
  ldr  x1, [x21, #0x8]       ; alloc_limit
  add  x0, x3, #0x20         ; end: 32 bytes
  cmp  x0, x1
  b.hi alloc_slow            ; cold
  str  x0, [x21]             ; bump
  mov  x4, #0x7              ; header: INSTANCE
  str  x4, [x3]
  str  x19, [x3, #0x8]       ; the class object of P, kept in a register across the loop
  mov  x4, #0x2              ; x: tag INT
  str  x4, [x3, #0x10]
  str  x2, [x3, #0x18]       ; x: payload = i
```

Twelve instructions: six to allocate, two for the header, one for the class word, three for the field (two where the tag is already in a register). Before it was nine.

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native, allocate and store | 10 M | 60 M | 405,352,105 | 2,311,843,437 | 38.12 |
| native, store an int instead | 10 M | 60 M | 195,748,493 | 1,095,843,402 | 18.00 |
| native, before, allocate and store | 10 M | 60 M | 281,582,920 | 1,582,224,067 | 26.01 |
| native, before, store an int instead | 10 M | 60 M | 174,497,663 | 974,876,205 | 16.00 |
| Go, allocate and store (at `39d3ad4c25`) | 10 M | 60 M | 1,534,989,740 | 9,104,700,271 | 151.39 |
| Go, store an int (at `39d3ad4c25`) | 10 M | 60 M | 68,822,394 | 319,145,572 | 5.00 |
| Node (at `39d3ad4c25`) | 20 M | 120 M | 2,852,411,144 | 15,045,766,104 | 121.93 |

The difference of the two native loops is 20.12 where it was 10.01, but it is no longer only the allocation. Counted in the listing: the object is 12 instructions (9 before); storing its address into `keep[i & 7]` is a locked store of both words, 9 instructions, where storing an `int` there is 2; and the 0.12 is the collections, one per 8 MB, which is 229 of them for 60 M objects of 32 bytes (114 before). So an object costs about 12 and the rest is the store.

```go
type P struct{ x int }
var sinkP [8]*P
for i := 0; i < n; i++ { sinkP[i&7] = &P{i} }
```

## Arrays and strings are allocated by a runtime call

An array literal calls `baml_rt_array_new(worker, length)` (`src/array.rs`): one allocation for the array and its storage, every element made `null`, then compiled code writes both words of each element with plain stores. A string concatenation calls `baml_rt_string_concat` (`src/string.rs`). Both go through a 4-instruction anchor stub (see `natives.md`).

```baml
keep[i & 7] = [i, i];                             // array
keep[i & 7] = parts[i & 1] + parts[(i + 1) & 1];  // 7 + 16 bytes
```

| | per iteration | before | notes |
|---|---|---|---|
| native array `[i, i]` | 102.03 (1,042,937,573 at 10 M; 6,144,875,870 at 60 M) | 88.03 | 84 over the store-an-int loop, 72 before. The call's share was 61 in a trace of the old layout; not traced again |
| Go `[]int{i, i}` (at `39d3ad4c25`) | 210.41 | | |
| native concat | 232.04 (2,342,817,644; 13,945,251,261) | 222.05 | the probe's two strings were written again (7 and 16 bytes of ASCII); the old ones are lost |
| Go concat (at `39d3ad4c25`) | 392.77 | | |

Weak point, unchanged: a two-element array literal costs several times an object of the same size. The length is a constant at the call site, so the allocation and both headers could be emitted inline as for a class instance.

## Allocation slow path

`baml_rt_alloc_slow` (`src/stubs.rs`) is called with a convention under which the callee preserves every register, so the fast path keeps nothing in callee-saved registers on its account. The stub saves `x0`-`x17` and `q0`-`q31`, calls `heap::alloc_slow`, restores them. Its 59 instructions were counted from the source at `39d3ad4c25` and not counted again. It runs once per space: every 8 MB of allocation at the start, less often as the space grows to four times the live data. Objects are twice the bytes they were, so it runs about twice as often for the same program.

## Field load and store

A slot is two words, tag at the slot's offset and payload 8 above it. What compiled code emits depends on whether the slot's static type fixes its tag, and on the busy bit protocol ("Values" above). There is no write barrier: the collector stops everything and copies.

| access | emitted | instructions |
|---|---|---|
| load, type fixes the tag (`int`, a class, a `string`, ...) | `add x, obj, #off+8; ldar` | 2 |
| store to an `int`, `float` or `bool` slot | `add; stlr` of the payload | 2 |
| load of a pair (a union, an optional, `unknown`, a function value) | lock, `ldar` payload, `stlr` the tag back | 7, and a constant |
| store to any other slot, a pointer of fixed type included | lock, `stlr` payload, `stlr` tag | 7, a constant, and the tag |
| any store into an object not yet reachable | `str`, `str` | 2 |

The atomic instructions take no offset, so each access computes the address first. Before two-word values and the protocol every one of these was one instruction.

```baml
let p = ps[i & 1]; p.x = p.x + p.y;            // scalar fields
let p = ps[i & 1]; p.next = ps[(i + 1) & 1];   // reference field, next: Q?
```

```asm
  add     x0, x1, #0x18          ; p.x: slot 0 is at 16, its payload at 24
  ldar    x2, [x0]
  add     x1, x1, #0x28          ; p.y
  ldar    x1, [x1]
  add     x1, x2, x1
  stlr    x1, [x0]               ; p.x = ...: the tag does not change

  mov     x7, #0x7               ; p.next = q: the tag of q, INSTANCE
  mov     x9, #-0x8000000000000000
  add     x10, x3, #0x30         ; the slot of next
  ldsetal x9, x10, [x10]         ; take the busy bit
  tbnz    x10, #0x3f, <the ldsetal>
  add     x10, x3, #0x38
  stlr    x8, [x10]              ; payload
  add     x11, x3, #0x30
  stlr    x7, [x11]              ; tag: the release
```

| | native per iteration | before | Go per iteration (at `39d3ad4c25`) |
|---|---|---|---|
| scalar field update loop | 24.00 (255,814,219 at 10 M; 1,455,993,089 at 60 M) | 20.00 | 9.02 |
| reference store loop | 34.00 (355,786,880; 2,056,043,172) | 22.01 | 14.00 |

The scalar loop gained 4: three loads that are `add; ldar` where they were `ldr`, and the index's own `add`. The reference loop gained 12: the store is 9 where it was 1, and the two element loads and their indexes are 2 each where they were 1.

Weak point: a store of a pointer whose static type fixes the tag still takes the lock and writes both words. An exempt class for such slots (one atomic payload store) is named and not built in `ideas/uniform-values-convention.md`.

## Array index

```asm
  ldr  x1, [x0, #0x8]        ; length
  ldr  x3, [x0, #0x18]       ; storage object
  and  x0, x2, #0x7          ; index
  cmp  x0, x1
  b.hs slow                  ; cold: negative index wraps, else IndexOutOfBounds
  mov  x4, x0
  add  x0, x3, x4, lsl #4    ; 16 bytes an element; no addressing mode scales by 16
  add  x0, x0, #0x18         ; storage header and capacity, then the payload word
  stlr x2, [x0]              ; keep[i & 7] = i
```

Two instructions for the bounds check, three for the store (one before), two more when the length and storage address are not already in registers. The compiler caches the pair per block (`codegen.rs`, `array_view`) and drops it at every call that can allocate, since a collection moves the storage.

`keep[i & 7] = i` with `keep: int[]` is 18.00 instructions per iteration (195,748,493 at 10 M; 1,095,843,402 at 60 M), 16.00 before, 5.00 in Go (at `39d3ad4c25`).

## Maps

A map is an insertion-ordered table (`src/map.rs`): an entries object with records in insertion order and a parallel array of 32-bit hashes, and, past 8 entries, an index object of slots holding hash and record number. A record is two values, the key and the value, 32 bytes. Up to 8 entries are searched without an index by comparing all hashes at once (they start at offset 272 of the entries).

The key's kind is read from its tag: a string by text, a `float` by canonical bits, an `int` or `bool` by bits, an enum variant by its address and hashed by its index. Lookup of a string or `int` key is a hand-written assembly probe with a private convention (`baml_rt_map_find_str`, `baml_rt_map_find_int`); a `float` or enum key calls `baml_rt_map_find(map, tag, payload)`. Compiled code reads the string's cached hash from its header inline (27 bits) and calls `baml_rt_str_hash` only when it is zero. A probe reads a record's key payload with one `ldar` and returns the address of the value's slot or zero.

```baml
s += m[keys[i % n]];     // map<string, int>
```

```asm
  ldr  w1, [x0, #0x4]        ; high half of the key's header
  and  w2, w1, #0x7ffffff    ; cached hash
  mov  w1, w2
  cbz  x1, hash_slow         ; cold: bl baml_rt_str_hash
  ldur x0, [sp, #0x10]       ; map   (spilled: a root across the cold call)
  ldur x1, [sp, #0x20]       ; key
  bl   _baml_rt_map_find_str
  cmp  x0, #0x0
  b.eq not_found             ; cold: MapKeyNotFound
  add  x0, x0, #0x8
  ldar x1, [x0]              ; the value's payload
```

The probes were written again from the description in these notes (keys `key0`, `key1`, ..., the baseline taking `keys[i % n].length()`); the originals are lost, so a difference of a few instructions from "before" may be the probe.

| operation | native | before | Go (at `39d3ad4c25`) |
|---|---|---|---|
| get, string key, 8 keys | 35 (63.00 - 28.00) | 39 (65.02 - 26.00) | 155 |
| get, string key, 1000 keys | 33 (61.97 - 29.00) | 37 (63.20 - 26.01) | 138 |
| get, int key, 1000 keys | 49.46 for the loop | 48.73 for the loop | 108.51 for the loop |
| set existing key, string, 8 keys | 305 (333.03 - 28.00) | 289 (315.11 - 26.00) | 170 |
| set existing key, string, 1000 keys | 284 (313.23 - 29.00) | 268 (294.49 - 26.01) | 172 |
| insert new int key, growing to 6 M | 562.40 | 452.20 | 417.69 |

Raw counts (N lo 5 M, N hi 30 M):

```text
native map_get.8      330,768,177   1,905,879,750
native map_get.1000   327,092,088   1,876,569,951
native map_base.8     155,785,786     855,820,626
native map_base.1000  161,724,335     886,777,855
native map_set.8    1,680,883,548  10,006,694,572
native map_set.1000 1,581,583,742   9,412,470,033
native map_int_get.1000 265,071,619 1,501,773,308
native map_insert (1 M, 6 M) 504,333,714 3,316,348,428
```

Lookup is where it was. Store is still behind Go: `m[k] = v` does not use the probe. It calls the generic native `baml.Map.set`, which now takes the key and the value as two words each and no type ids:

```rust
// generated wrapper, natives_generated.rs
unsafe extern "C" fn baml_map_set(mutator: &mut Mutator, map: usize,
                                  key_tag: u64, key: u64, value_tag: u64, value: u64) -> Value {
    let slots = RootSlots::new();
    let mut scope = unsafe { Scope::new(mutator, &slots) };
    let map = unsafe { scope.root_raw(map) };
    let key = unsafe { scope.elem(Value { tag: key_tag as usize, payload: key as usize }) };
    let value = unsafe { scope.elem(Value { tag: value_tag as usize, payload: value as usize }) };
    let value = <Natives as NativeBamlMap>::set(&mut scope, map, key, value);
    value.map_or(Value::NULL, |value| value)
}
```

The known alternative is what `get` already does: probe inline, store through the returned slot address, and call the runtime only to insert. Insert costs 110 more than before; not traced. A record is 32 bytes where it was 16, so growing copies twice the bytes and the space fills twice as fast.

## Not built

The structs in "Objects" are the intended shape. Where the code differs today:

- Of the copies in "Types and rules are copies", two do not exist: matching a type against an interface's rules, and testing a value against a type. The others are in `src/ty`, the compiler can write them (`ty_emit.rs`), and nothing uses them yet.
- `TypeObject` is not a `RealizedTy`. It is `{ tag, class, description, kind, count, arguments }`, built only for unions, found or made in a table outside the heap (`baml_rt_type_intern`, seeded from the program's `PROGRAM_BUILT_TYPES` list) that is never freed, with members ordered by address. To be replaced, and the collector does not yet drop a type object.
- The `ty` word of an array and `key_ty`, `value_ty` of a map are written as zero, and an instance's type parameter slots as `(NULL, 0)`. So a type test against an array type or between two instantiations of one class is refused when compiled (`ideas/uniform-values-convention.md`, status).
- A call of an interface method is to be resolved through the interface's rules every time. Remembering the answer comes only after type objects have canonical ids.
- There is no `InterfaceObject` and no `ImplRule`. A class object ends in a row of code addresses, one per method column of the whole program, and an interface call loads the receiver's class and jumps through its column; the types that are not classes have a row per tag. So an implementation chosen by a type argument (`for Box<int>` against `for Box<string>`) is refused. A class object has no list of its methods.
- An array's `ptr` is zero until it first holds an element, where the intended shape always points at a storage object.
- `RustInstanceObject`, `RealizedFunctionObject`, `HostClosureObject`: the tags are reserved and nothing makes one.
- Inline allocation of arrays, strings and maps.
- Per-worker allocation spans carved from a shared space (`threads.md`).
- Large-object space: an object larger than the space forces a collection into a bigger space.
- Packed fields: every field is 16 bytes.
