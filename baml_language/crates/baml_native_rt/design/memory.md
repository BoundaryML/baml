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
| 32..58 | payload of the header. A string caches its hash here (27 bits); a function value holds its call layout id here (see "Tags"); a class object the number of its columns |
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

A table with a row per tag (the method rows of the types that are not classes) is indexed by the id scaled by the load's addressing mode (`ldr x, [table, w_id, uxtw #3]`), with no separate shift. The ids below are fixed when the runtime is compiled and are the same in every program: the collector and the Rust runtime `match` on them with no program in hand, and compiled code compares against them as constants.

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
| `RAW_FUNCTION`, `REALIZED_FUNCTION` | `RawFunctionObject`, `RealizedFunctionObject` | below. Reserved: nothing makes one |
| `CLOSURE`, `BOUND_METHOD`, `HOST_CLOSURE` | `ClosureObject`, `BoundMethodObject`, `HostClosureObject` | below. `HOST_CLOSURE` is reserved |
| `CELL` | `CellObject` | below |
| `FUTURE` | `FutureObject` | below |
| `BIGINT` | `BigintObject` | below |
| `BYTES`, `BYTES_STORAGE` | `BytesObject`, `BytesStorageObject` | below. A `uint8array`, packed |
| `TUPLE` | `TupleObject` | below |
| `ENUM_VARIANT` | `EnumVariantObject` | below. An enum value is a pointer to its variant; not an immediate |
| `CLASS`, `PACKAGE` | `ClassObject`, `PackageObject` | below |
| `OWNED`, `PLAIN` | `OwnedObject`, `PlainObject` | below. Rust data with and without a destructor; `RustInstanceObject` may replace them later |

Gone from the runtime: the boxes (`TYPE_BOX_INT`, `TYPE_BOX_FLOAT`, `TYPE_BOX_BOOL`), the three cell types and the four map-entry types that differed only in which words were pointers, and the array types split by element kind. A slot's own tag now says whether it is a pointer.

Function values carry one more thing in the header. A call through a function value has to know whether the callee takes exactly the arguments the site pushes, because a function with defaulted parameters can stand where a type with fewer parameters is expected ([default-arguments.md](ideas/default-arguments.md)). So the header payload (bits 32..58) of every callable object, `CLOSURE`, `BOUND_METHOD`, `REALIZED_FUNCTION`, `HOST_CLOSURE` and a `RAW_FUNCTION` used as a value, holds its call layout id: a number the compiler gives each distinct parameter layout in the program (the required slots in order, then the named optional ones). A function value's tag carries the same id, so the caller already has it in the register that holds the tag, and every call through a function value compares it with the site's layout, with no load (`calls.md`). This is required for correctness, not a later optimization. The id is written when the object is made and never changes. Ids count from 1, so a header nothing gave a layout fails every check.

Not a fixed id: a class or an enum of the program. Every instance has the one id `INSTANCE`, and the truth about it is its `class_ptr`: `x is Foo` is a tag compare, a load and a pointer compare, and interface dispatch loads the method from the class object's row. An instance's header is `INSTANCE`, with bit 62 when its class is a panic, and nothing else. A class object or an enum variant may live in the heap (they cannot be made at run time today, but will be), so `class_ptr` is traced and nothing hashes or orders by a class's or a variant's address. A per-class id is the kind of shortcut [type-header-fast-path.md](ideas/type-header-fast-path.md) is about.

Also fixed, though not type ids: the classes of the standard library that the runtime builds instances of and must be able to find without being handed them, 25 today (`WELL_KNOWN` in `src/layout.rs`: the panics, `errors.InvalidArgument`, the `json` errors, `errors.Io` and so on). The runtime finds each one's class object in a table of class object addresses the compiler emits (`baml_native_types`, at `PROGRAM_CLASSES`), in the order of `WELL_KNOWN` then `NAMED` (35), zero for a `NAMED` class the program does not name. After it comes one word, the address of the list of the program's static built types (`PROGRAM_BUILT_TYPES`), and then the descriptions `baml.json` and the uncaught report read (`PROGRAM_DESCRIPTIONS`). There is no type table. Bit 62 of the header marks an instance of a panic class; it is set from `CLASS_PANIC` in the class object's `kind`.

## Objects

An object is a 16-byte-aligned dynamically sized type which lives on the heap. It needs to be 16-byte-aligned so it can be aliased with more specific layouts which hold `Value` slots:

```rust
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
    /// `[generics ++ fields]`. A generic slot is `(NULL, 0)` today: not built.
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
    /// Zero today: not built.
    pub ty: *const TypeObject,
    /// The storage object, not its first element. Null while the array has never held an element.
    pub ptr: *mut ArrayStorageObject,
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
    /// Both zero today: not built.
    pub key_ty: *const TypeObject,
    pub value_ty: *const TypeObject,
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
    /// The tag its values have, when a tag compare decides `x is T`; zero when it takes more.
    pub tag: u64,
    /// The class object to compare `class_ptr` with, or null.
    pub class: *const ClassObject,
    /// What `baml.json` and the slow path of `x is T` read: program data, or null.
    pub description: *const u8,
    /// `TYPE_KIND_NAMED` (0) or `TYPE_KIND_UNION` (1).
    pub kind: u32,
    pub count: u32,
    /// A union's members, sorted, with no repeats and no union among them.
    pub arguments: [*const TypeObject],
}
// The compiler places the type object of every type the program names in the executable's data.
// A union built from a function's type arguments when the program runs (`T?`) is found or made by
// `baml_rt_type_intern`: one object per type, outside the heap, never moved or freed.

/// Can be aliased over an `Object` with a `RawFunction` layout
/// Equivalent to `bex_vm_types::types::Function`
#[repr(C, align(16))]
pub struct RawFunctionObject {
    pub header: ObjectHeader,
    pub owner: NonNull<PackageObject>,
    // ... other fields
}

/// Can be aliased over an `Object` with a `Closure` layout
#[repr(C, align(16))]
/// Header payload: the call layout id. Every callable has its code address at offset 8 and is itself the closure word, so one call sequence serves all of them.
pub struct ClosureObject {
    pub header: ObjectHeader,
    pub code: usize,
    /// Zero today.
    pub function: *const RawFunctionObject,
    pub capture_count: u64,
    /// Variable cells, each a value pointing at a `CellObject`. No type argument is captured today.
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
    pub code: usize,
    pub function: NonNull<RawFunctionObject>,
    /// Zero today, and no type argument follows the receiver.
    pub type_arg_count: u64,
    pub receiver: Value,
    pub type_args: [NonNull<TypeObject>],
}

/// Can be aliased over an `Object` with a `RealizedFunction` layout
/// Equivalent to `bex_vm_types::types::GenericFunction`
#[repr(C, align(16))]
pub struct RealizedFunctionObject {
    pub header: ObjectHeader,
    pub code: usize,
    pub function: NonNull<RawFunctionObject>,
    pub type_args: [NonNull<TypeObject>],
}

/// Can be aliased over an `Object` with a `HostClosure` layout
#[repr(C, align(16))]
pub struct HostClosureObject {
    pub header: ObjectHeader,
    pub code: usize,
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
    pub flags: usize,
    pub trace: usize,
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
    /// At offset 8, where an instance has `class_ptr`: dispatch reads the method row the same way.
    pub enum_ptr: NonNull<ClassObject>,
    pub index: u64,
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
    /// Class, enum, Rust-backed; panic class.
    pub kind: u32,
    /// Dense, for a `match` over classes.
    pub index: u32,
    /// Field names, places and types: what JSON and the uncaught report read. Program data.
    pub description: *const u8,
    /// An enum's variants, or null.
    pub variants: *const NonNull<EnumVariantObject>,
    /// One code address per method column of the program, zero where the class has none.
    pub methods: [usize],
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

## Sizes

Counted from `src/layout.rs` at `881e4c1dfc`. Go sizes are from the Go 1.26 runtime's types and are not measured here.

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
| closure with c captures | 32 + 16c (header, code, function, capture count) | 24 | funcval + captures | 8 + captures |
| bound method | 48 (header, code, function, type argument count, receiver) | | | |
| tuple of n | 16 + 16n | | | |
| map | 64 (header, `key_ty`, `value_ty`, len u32 + used u32, entries, index, mask u32 + epoch u32, padding) | 40 | `map` header (swiss) | 48 |
| map entries, capacity c | 16 + 32c + 4c rounded to 16 | 16 + 16c + 4c rounded to 8 | group of 8 slots | 8 + 8 x (key + value) |
| map entry, `map<string,int>` | 36 per slot of capacity, plus an index slot once past 8 entries | 20 | `map[string]int` slot | 25 per slot of capacity |
| future | 64 | 48 | channel of capacity 1 | 96 + element |
| class object with m method columns | 56 + 8m rounded to 16 | none | | |
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

- The `ty` word of an array and `key_ty`, `value_ty` of a map: written as zero. An instance's type parameter slots: written as `(NULL, 0)`. So a type test against an array type or between two instantiations of one class is refused when compiled (`ideas/uniform-values-convention.md`, status).
- `RustInstanceObject`, `RawFunctionObject`, `RealizedFunctionObject`, `HostClosureObject`: the tags are reserved and nothing makes one. A closure's `function` word is zero.
- Inline allocation of arrays, strings and maps.
- Per-worker allocation spans carved from a shared space (`threads.md`).
- Large-object space: an object larger than the space forces a collection into a bigger space.
- Packed fields: every field is 16 bytes.
