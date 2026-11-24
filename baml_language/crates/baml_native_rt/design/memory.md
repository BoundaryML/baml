# Memory

Object layouts, allocation, field and index access, maps, strings. The collector is in `gc.md`. Numbers are instructions retired per operation, measured unless marked estimated; method in `README.md`.

## Layouts

Everything is 8-byte words and 8-byte aligned. A reference is a plain address, zero for `null`. Nothing is tagged. `src/layout.rs` is included by the compiler by path, so both sides use the same constants.

Header, one word in every object:

| bits | meaning |
|---|---|
| 0..2 | collector: bit 0 forwarded, bit 1 a mark used only while slices are settled |
| 3..31 | type id: byte offset of the type in the program's type table |
| 32..59 | free. A string caches its hash here |
| 60..63 | flags: bit 63 panic class, bit 60 float elements |

Compiled code reads the type id as the 32-bit word at the object's address and writes the whole header as one constant.

| object | bytes | Go equivalent | Go bytes |
|---|---|---|---|
| instance of a class with n slots | 8 + 8n | struct of n words | 8n (rounded to a size class) |
| `P { x int }` | 16 | `struct{ x int }` | 8 |
| array (`T[]`) | 24 (header, length, data address) plus storage | slice header (value, not an object) | 24 |
| array storage of capacity c | 16 + 8c (header, capacity) | backing array | 8c |
| `[i, i]` | 24 + 32 = 56, one allocation | `[]int{i, i}` | 16 heap + 24 header |
| string of b bytes | 16 + b rounded up to 8 (header, byte length u32 and char count u32) | string header + bytes | 16 + b |
| string slice | 32 (header, lengths, parent, offset) | string header | 16 |
| closure | 24 (header, code, captures) | funcval + captures | 8 + captures |
| box of `int`/`float`/`bool` in a union | 16 | interface word pair + boxed value | 16 + 8 |
| map | 40 (header, len u32 + used u32, entries, index, mask u32 + epoch u32) | `map` header (swiss) | 48 |
| map entries, capacity c | 16 + 16c + 4c rounded to 8 | group of 8 slots | 8 + 8 x (key + value) |
| map entry, `map<string,int>` | 20 per slot of capacity, plus an index slot once past 8 entries | `map[string]int` slot | 25 per slot of capacity |
| future | 48 | channel of capacity 1 | 96 + element |
| task (not on the heap) | 128, plus a 64 KB stack and 16 KB guard | `g` | about 450, plus a stack from 8 KB |

A `bool` or enum field takes a full word. An optional scalar takes two slots. Go sizes are from the Go 1.26 runtime's types and are not measured here.

Arrays keep elements in a separate storage object so the array's address never changes when it grows. Every element access is therefore two dependent loads (array to data address, data to element) unless the data address is already in a register.

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

The memory is not zeroed. The caller writes the header as one constant and then every field. There is one span and it is the whole space (8 MB to start), so the limit check fails only when a collection is due.

```baml
class P { x int }
function main() -> int {
  let keep: P[] = [P { x: 0 }, /* 8 of them */];
  for (let i = 0; i < N; i += 1) { keep[i & 7] = P { x: i }; };
  return keep[3].x;
}
```

The store into `keep` is what keeps scalar replacement (`optimize/scalar_replace.rs`) from deleting the object.

```asm
  ldr  x4,  [x21]            ; alloc_ptr
  ldr  x14, [x21, #8]        ; alloc_limit
  add  x13, x4, #16          ; end
  cmp  x13, x14
  b.hi alloc_slow            ; cold
  str  x13, [x21]            ; bump
  mov  x0, #400              ; header: type id of P
  str  x0, [x4]
  str  x3, [x4, #8]          ; x = i
```

Six instructions to allocate, two for the header, one per field.

| | N lo | N hi | instr lo | instr hi | per iteration |
|---|---|---|---|---|---|
| native, allocate and store | 10 M | 60 M | 281,582,920 | 1,582,224,067 | 26.01 |
| native, store an int instead | 10 M | 60 M | 174,497,663 | 974,876,205 | 16.00 |
| Go, allocate and store | 10 M | 60 M | 1,534,989,740 | 9,104,700,271 | 151.39 |
| Go, store an int | 10 M | 60 M | 68,822,394 | 319,145,572 | 5.00 |
| Go, `GOGC=off` | 10 M | 20 M | 1,556,006,313 | 3,078,657,222 | 152.26 |
| Node | 20 M | 120 M | 2,852,411,144 | 15,045,766,104 | 121.93 |

Native: 10 instructions per object, collections included (60 M objects of 16 bytes is 114 collections with almost nothing live). Go: 146, and the same with the collector off, so that is `mallocgc` itself for an 8-byte object on this machine (27 ns per allocation wall). Go's count covers all its threads and the kernel.

```go
type P struct{ x int }
var sinkP [8]*P
for i := 0; i < n; i++ { sinkP[i&7] = &P{i} }
```

## Arrays and strings are allocated by a runtime call

An array literal calls `baml_rt_array_new` (`src/array.rs`): one allocation for the array and its storage, then a `bzero` of the elements. A string concatenation calls `baml_rt_string_concat` (`src/string.rs`). Both go through a 4-instruction anchor stub (see `natives.md`).

```baml
keep[i & 7] = [i, i];                         // array
keep[i & 7] = parts[i & 1] + parts[(i + 1) & 1];  // 7 + 16 bytes
```

| | per iteration | per operation | where it goes (single-step trace) |
|---|---|---|---|
| native array `[i, i]` | 88.03 (901,565,431 at 10 M; 5,303,154,732 at 60 M) | about 72 | 61 in the call: stub 4, `array_new` 37, `bzero` 20; the rest is argument setup and the two element stores |
| Go `[]int{i, i}` | 210.41 (2,119,963,964; 12,640,880,851) | about 205 | |
| native concat | 222.05 (2,243,068,019; 13,345,676,338) | about 196 | 184 in the call: stub 4, `string_concat` 116, `memmove` 64 |
| Go concat | 392.77 (3,947,933,854; 23,586,733,744) | about 383 | |

Weak point: a two-element array literal costs 7 times an object of the same size class. The length is a constant at the call site, so the allocation and both headers could be emitted inline as for a class instance. `string_concat` computes the character count of the result and handles slices on both sides; 116 instructions for two short flat strings is mostly that.

## Allocation slow path

`baml_rt_alloc_slow` (`src/stubs.rs`) is called with a convention under which the callee preserves every register, so the fast path keeps nothing in callee-saved registers on its account. The stub saves `x0`-`x17` and `q0`-`q31` (25 `stp`), calls `heap::alloc_slow`, restores them:

```rust
pub(crate) extern "C" fn alloc_slow(mutator: &mut Mutator, frame: usize) {
    mutator.enter(frame);                       // record the anchor from the stub's frame record
    mutator.scratch = mutator.allocate(mutator.scratch);   // collects, then bumps
}
```

Cost: 59 instructions in the stub (counted from the source, not traced) plus one collection (`gc.md`: 235 fixed, 122 per live object). It runs once per space: every 8 MB of allocation at the start, less often as the space grows to four times the live data.

## Field load and store

One instruction each. There is no write barrier: the collector stops everything and copies, so it needs none.

```baml
let p = ps[i & 1]; p.x = p.x + p.y;      // scalar fields
let p = ps[i & 1]; p.next = ps[(i + 1) & 1];   // reference field
```

```asm
  ldr  x1, [x0, #8]          ; p.x
  ldr  x3, [x0, #16]         ; p.y
  add  x1, x1, x3
  str  x1, [x0, #8]          ; p.x = ...

  str  x0, [x3, #16]         ; p.next = q      (no barrier)
```

| | native per iteration | Go per iteration |
|---|---|---|
| scalar field update loop | 20.00 (214,508,728 at 10 M; 1,214,617,865 at 60 M) | 9.02 (109,092,634; 560,543,206) |
| reference store loop | 22.01 (234,515,435; 1,335,052,936) | 14.00 (158,958,808; 859,423,999) |

The field operations are the same count in both. Go pays 5 more for the reference store (the write-barrier flag test and the nil check); native pays more everywhere else in the loop: the poll (2), and the array's length and data pointer reloaded from the array object on every iteration (3 to 5).

## Array index

```asm
  ldr  x1,  [x0, #8]         ; length
  ldr  x15, [x0, #16]        ; data address
  and  x0, x19, #1           ; index
  cmp  x0, x1
  b.hs slow                  ; cold: negative index wraps, else IndexOutOfBounds
  ldr  x2, [x15, x0, lsl #3]
```

Two instructions for the bounds check, one for the load, two more when the length and data address are not already in registers. The compiler caches the pair per block (`codegen.rs`, `array_view`) and drops it at every call that can allocate, since a collection moves the storage. A negative index counts from the end; that is the cold block.

`keep[i & 7] = i` with `keep: int[]` is 16.00 instructions per iteration native (174,497,663 at 10 M; 974,876,205 at 60 M) and 5.00 in Go (68,822,394; 319,145,572). Go proves `i & 7 < 8` and drops the check, has no poll, and keeps the slice in registers. Cranelift does none of the three here.

## Maps

A map is an insertion-ordered table (`src/map.rs`): an entries object with records in insertion order and a parallel array of 32-bit hashes, and, past 8 entries, an index object of slots holding hash and record number. Up to 8 entries are searched without an index by comparing all hashes at once.

Lookup is a hand-written assembly probe with a private convention (`baml_rt_map_find_str`, `baml_rt_map_find_int`, in `map.rs`). Compiled code reads the string's cached hash from its header inline and calls `baml_rt_str_hash` only when it is zero. The probe returns the address of the value slot or zero.

```baml
s += m[keys[i % n]];     // map<string, int>
```

```asm
  ldr  w1, [x0, #4]          ; high half of the key's header
  and  w2, w1, #0x0fffffff   ; cached hash
  mov  w1, w2
  cbz  x1, hash_slow         ; cold: bl baml_rt_str_hash
  ldur x0, [sp, #16]         ; map   (spilled: a root across the cold call)
  ldur x1, [sp, #32]         ; key
  bl   baml_rt_map_find_str  ; 20 instructions for a hit in a small map (trace)
  cmp  x0, #0
  b.eq not_found             ; cold: MapKeyNotFound
  ldr  x0, [x0]              ; the value
```

| operation | native | Go | notes |
|---|---|---|---|
| get, string key, 8 keys | 39 (65.02 - 26.00) | 155 (164.56 - 10.01) | |
| get, string key, 1000 keys | 37 (63.20 - 26.01) | 138 (147.86 - 10.01) | Node `Map.get`: about 190 |
| get, int key, 1000 keys | about 42 (48.73 - 7) | about 103 (108.51 - 5) | baseline estimated from the empty loop |
| set existing key, string, 8 keys | 289 (315.11 - 26.00) | 170 (180.19 - 10.01) | |
| set existing key, string, 1000 keys | 268 (294.49 - 26.01) | 172 (182.06 - 10.01) | |
| insert new int key, growing to 6 M | 452.20 | 417.69 | |

Raw counts (N lo 5 M, N hi 30 M; baseline is the same loop taking `keys[i % n].length()`):

```text
native map_get.8     339,517,544   1,965,217,307
native map_get.1000  330,138,084   1,910,260,844
native map_base.8    144,475,345     794,544,273
native map_set.8   1,596,490,945   9,474,370,502
native map_set.1000 1,496,135,127  8,858,475,130
native map_int_get.1000 256,356,097 1,474,778,703
native map_insert (1 M, 6 M) 432,095,626 2,693,131,034
go map_get.8         841,919,636   4,956,107,107
go map_get.1000      759,595,668   4,456,218,575
go map_base.8         68,925,334     319,357,781
go map_set.8         919,752,185   5,424,649,077
go map_set.1000      928,329,997   5,479,837,602
go map_int_get.1000  561,439,722   3,274,332,194
go map_insert (1 M, 6 M) 623,255,825 2,711,746,007
```

Lookup is ahead of Go by 3 to 4 times: the hash is cached in the string, the probe is 20 instructions, and nothing is spilled around it.

Store is behind Go. `m[k] = v` does not use the probe. It calls the generic native `baml.Map.set`, which takes two type ids and values as a union holds them:

```rust
// generated wrapper, natives_generated.rs
unsafe extern "C" fn baml_map_set(mutator: &mut Mutator, type_0: u32, type_1: u32,
                                  map: usize, key: usize, value: usize) -> OptionalScalar {
    let slots = RootSlots::new();
    let mut scope = unsafe { Scope::new(mutator, &slots) };
    let types = [TypeId(type_0), TypeId(type_1)];
    let map = unsafe { scope.root_raw(map) };
    let key = unsafe { scope.elem_raw(key, types[0]) };
    let value = unsafe { scope.elem_raw(value, types[1]) };
    let value = <Natives as NativeBamlMap>::set(&mut scope, types, map, key, value);
    OptionalScalar::from(value)
}
```

Single-step trace of one store to an existing key: 244 instructions, 4 in the anchor stub, 178 in the wrapper and the inlined `set` (root slots, type id tests, boxing rules, the store), 21 in `str_hash`, 41 in `find`. The known alternative is what `get` already does: probe inline, store through the returned slot address, and call the runtime only to insert.

## Not built

- Inline allocation of arrays, strings and maps.
- Per-worker allocation spans carved from a shared space (`threads.md`).
- Large-object space: an object larger than the space forces a collection into a bigger space.
- Packed fields: every field is a word.
