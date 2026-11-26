# The collector

A stop-the-world copying collector, how it finds roots, and what it costs. Allocation is in `memory.md`. Numbers are instructions retired; method in `README.md`. Native totals are counted at `881e4c1dfc` unless they say otherwise. "Before" is the layout before two-word values, at `39d3ad4c25`. The single-step traces and every Go number are from then and were not run again; each says so.

## Design

Cheney's algorithm over two spaces (`src/heap.rs`). Live objects are copied into a fresh space, every reference is updated, the old space is kept as a spare for the next collection. No generations, no marking, no write barrier, no concurrency. Every object moves at every collection.

```rust
pub(crate) fn collect(&mut self, needed: usize) {
    let used = self.alloc_ptr - self.heap.space.start;
    self.copy_into(used);                       // into the spare, or a new space
    let live = self.alloc_ptr - self.heap.space.start;
    self.heap.next_space = self.heap.space.size.max((live + needed) * SPACE_PER_LIVE);  // 4x live
    if live + needed > self.heap.space.size { self.copy_into(live + needed); }          // move again, larger
}
```

The first space is 8 MB. After a collection the next space is at least four times what survived, so copying stays a fraction of allocating. When the survivors do not leave room for the request, everything is copied a second time into a larger space. Spaces come from `std::alloc` and are not returned while they are the spare.

A collection also runs when 128 owned Rust values (files, sockets) have been made since the last one, or twice the number found alive, so their destructors run without waiting for 8 MB of garbage.

A slice of a string does not keep the string alive: slices of an unreached string are flattened into strings of their own unless together they are as large as the parent (`settle_slices`). A slice made a string changes its type id, and a pointer's tag is always the id in its object's header, so the collector then gives the tag `STRING` to every slot that pointed at the slice: heap slots, the value entries of frames, the runtime's value roots. While it settles, the count of a string's unreached slices is kept in bits 32..58 of the string's header, where its hash was.

## What an object is, to the collector

The tag in an object's header says its layout, and nothing else is consulted: there is no type table. `Copy::size` and `Copy::finish` are one `match` on the type id each. An instance's size is `16 + 16 * slot_count`, with the count read from its class object (through the forwarding address when the class object was already copied). The scan, per tag:

| tag | followed |
|---|---|
| `INSTANCE` | the class object word, then every slot as a value |
| `ARRAY`, `BYTES` | the storage word |
| `ARRAY_STORAGE` | every element to the capacity, as a value |
| `MAP` | the entries and index words |
| `MAP_ENTRIES` | the key and the value of every record to the capacity, as values |
| `CELL`, `TUPLE`, `CLOSURE` | their values |
| `BOUND_METHOD` | the receiver |
| `FUTURE` | the value, and the trace word |
| `ENUM_VARIANT` | its enum's class object and its name |
| `CLASS` | its package and name, and for an enum each word of its variants table |
| `PACKAGE`, `TYPE` | the name; the class object and the arguments |
| strings, `MAP_INDEX`, `BIGINT`, `BYTES_STORAGE`, `OWNED`, `PLAIN` | nothing |

A value is followed when its tag is a pointer's (an id above `OMITTED`): one load and one compare per slot, whatever the slot's static type. So an `int[]` is scanned element by element where its storage used to be copied and not looked at. Class, variant, package and type objects are in the executable's data today and are skipped as any address outside the space is, but nothing relies on that: a test builds them in the heap and collects them.

The collector never takes a slot's busy bit: the world is stopped at safepoints and no locked sequence contains one.

## Roots

Roots are precise. Three sources, walked in `copy_into`:

```rust
copy.stack(&self.heap.stack_maps, self.anchor, base);        // the running task
for task in self.sched.tasks() {
    let roots = crate::sched::task_roots(task, self.cur_task);
    for slot in roots.slots { copy.forward_word(slot); }      // task.future, task.entry
    if let Some((result, [sp, pc, fp], base)) = roots.stack { // a task that is not running
        copy.forward_word(result);                            // the switch frame's result word
        copy.stack(&self.heap.stack_maps, Anchor { sp, pc, fp }, base);
    }
    crate::native::drive::each_root(task, |slot| copy.forward_value(slot));   // async natives' root slots
}
for &slot in &self.roots {                                    // the runtime's registered slots
    if slot & ROOT_IS_VALUE == 0 { copy.forward_word(slot); } else { copy.forward_value(slot & !ROOT_IS_VALUE); }
}
```

The running task's walk starts from the anchor: the stack pointer, return address and frame pointer of the compiled frame that last called the runtime, recorded by a stub before any Rust ran (`natives.md`). Every other task's walk starts from its switch frame. Every task's stack is walked at every collection, parked or not.

A root is one of two things. A word is null or an address (`forward_word`): a task's future and closure, the switch frame's result, an exact pointer in a frame. A value is a tag and a payload (`forward_value`): the root slots of a native (`RootSlots` hold values, and a root's tag says whether it is followed), and a registered root of the mutator whose address has bit 0 set.

### Stack maps

Cranelift records, for each call in compiled code, which frame slots hold references across it (`declare_value_needs_stack_map`). A reference that is live across a call is stored to its slot before the call and loaded back after it, since the callee may have moved the object. The compiler emits one table, `baml_native_stack_maps`: a count of functions, then per function its address and its call sites, each a return-address offset and a list of slot words. Every entry is a 64-bit word. A slot word is the offset from the caller's `sp`, a multiple of 8, and its bit 0 (`STACK_MAP_VALUE`) says what is there: clear, one word that is null or an address (a local whose type fixes its tag: a class, an array, a `string`, an enum variant, a type argument); set, a 16-byte value, tag at the offset and payload 8 above it (a union, an optional, `unknown`, a type parameter's value, a function value). The compiler holds such a value as one `I128` and sets the bit from the entry's type.

The runtime decodes the table once, in `Mutator::new`, into a sorted vector, and keeps it for the life of the process:

```rust
struct StackMaps {
    sites: Vec<(usize, u32, u32)>,   // (return address, first offset, count), sorted
    offsets: Vec<u32>,
}

fn slots(&self, return_address: usize) -> &[u32] {
    match self.sites.binary_search_by_key(&return_address, |site| site.0) {
        Ok(index) => { let (_, first, count) = self.sites[index]; &self.offsets[first as usize..(first + count) as usize] }
        Err(_) => &[],               // a frame of the runtime; with checks, fatal inside compiled code
    }
}
```

The walk follows frame records, not unwind info:

```rust
while sp != 0 {
    for &entry in maps.slots(pc) {
        let entry = entry as usize;
        if entry & STACK_MAP_VALUE == 0 { self.forward_word(sp + entry); }
        else { self.forward_held(sp + (entry & !STACK_MAP_VALUE)); }   // the payload, when the tag is a pointer's
    }
    if fp >= base { break; }
    sp = fp + 2 * WORD;        // a caller's sp at a call is just above the callee's frame record
    pc = load(fp + WORD);
    fp = load(fp);
}
```

That relies on every function keeping a frame record and on no tail calls. Arguments passed on the stack need no entry: the callee loads them at entry and lists its own copies, and its frame record is pushed at its entry `sp`, so `fp + 16` is still the caller's `sp` at the call (`tests/conformance/calls.baml` has a case under collection stress).

### What a checked build asserts

The runtime built with `checks` ends the program, naming the slot, when a value the collector meets (an object slot, a frame's value entry, a runtime root) has the busy bit; has a pointer's tag that is not exactly what `layout::tag_of` makes of its object's header (the type id, and a callable's call layout id); has an immediate's tag with any bit above the id; or is `OMITTED` anywhere but a frame. It also ends the program on a return address into compiled code that has no stack map entry (`natives/README.md`).

### Decode cost

Decode is per process, not per collection. The table's format did not change; these counts are from `39d3ad4c25` and were not taken again. Single-step count from the first instruction that reads the table to the next call after the decode, `malloc` excluded:

| program | functions | call sites with roots | slots | table bytes | decode instructions |
|---|---|---|---|---|---|
| return-42 | 0 | 0 | 0 | 8 | 11 |
| `agent-loop-300` | 4 | 54 | 206 | 2,584 | 6,666 (loop 5,455, sort 480, vector growth 731) |
| `request-handler-2k` | 4 | 80 | 167 | 2,688 | 7,182 (loop 5,679, sort 714, vector growth 789) |
| `json-parser-15` | 7 | 110 | 341 | 4,608 | 11,535 (loop 9,699, sort 984, vector growth 852) |

About 17 instructions per table word in the loop, plus the sort, plus 8 to 10 vector regrowths, each a `realloc`. With the allocator counted the three programs are 18,090, 19,809 and 25,597 under the debugger, where `malloc` runs slower paths than outside it. Earlier counts of this decode in three programs were 37, 27,013 and 445,674: an empty table, a table the size of `json-parser-15`'s with the allocator included, and one about 17 times larger (estimated from the per-word rate). The cost is linear in the table and paid at every start, used or not.

Weak points: the table is 8 bytes per entry where 4 would do, it is decoded eagerly though a program that never collects never needs it, and the vectors are grown from empty although both counts could be emitted. Emitting the table already sorted and in the runtime's layout would make decode zero.

## Cost of a collection

`baml.sys.collect_garbage()` forces one. All probes call it N times.

A program run with `BAML_GC_STATS=1` measures its own collections and writes one line on stderr when it ends, `baml-gc-stats {json}` (`src/gc_stats.rs`, `natives/README.md`). The fields: `collections`; `pause_ns_total`, `pause_ns_max` and `pause_ns`, the pauses themselves (the first 20,000), each the wall time of one call of `collect` on the mutator's thread, both copying passes when it resizes, without the time spent dropping owned values; `bytes_copied`, what every copying pass moved; `heap_capacity_peak`, the most bytes the spaces held from the allocator at once (both, while one is copied into the other); `owned_dropped_by_collections` and `owned_drop_ns_total`, the owned Rust values collections found dead and the time their destructors took, reported apart from the pauses; and `owned_dropped_at_exit`, the owned values still held when the program ended. Without the variable a collection pays one load and one predictable branch. The numbers below were counted without it.

```baml
class Node { v int  next Node? }
let head: Node? = null;
for (let i = 0; i < M; i += 1) { head = Node { v: i, next: head }; };
for (let i = 0; i < N; i += 1) { baml.sys.collect_garbage(); s += 1; };
```

### Fixed cost

Nothing live, one frame, no roots: 222.19 instructions per collection (N 100,000: 37,847,300; 600,000: 148,943,115), 255.03 before. Single-step trace of the old layout, 244 (not traced again):

| part | instructions |
|---|---|
| anchor stub and the native's wrapper | 9 |
| `Mutator::collect` | 34 |
| `copy_into`: take the spare space, set up, the empty scan, swap spaces | 123 |
| `Copy::stack`: one frame, one binary search | 44 |
| `Copy::forward`: the main task's two slots | 34 |

No allocation, no system call, no page touched. Go's `runtime.GC()` with nothing live is 1,122,109 instructions (N 2,000: 2,194,559,900; 12,000: 13,415,648,436), 553,984 with `GOMAXPROCS=1`. A forced Go cycle is a full concurrent mark and sweep with two stop-the-world phases and wakes every P; it is not what Go pays per unit of allocation, but it is the fixed cost of a cycle.

### Per live object

A linked list of M `Node`s (24 bytes each), collected N times:

| M | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100,000 | 20 | 60 | 379,954,645 | 1,088,009,936 | 17,701,382 |
| 300,000 | 20 | 60 | 1,177,636,448 | 3,301,989,449 | 53,108,825 |
| 100,000, before | 20 | 60 | 264,406,846 | 752,585,620 | 12,204,469 |
| 300,000, before | 20 | 60 | 764,476,000 | 2,228,544,621 | 36,601,716 |

177 instructions per live object per collection, where it was 122. A `Node` is 48 bytes now and was 24, so that is 3.7 per byte where it was 5.1. Not traced: from the code, the copy moves twice the bytes, the size is read through the class object, the class word is forwarded, and each of the two slots has its tag loaded and tested before its payload is followed. A trace of the old layout with 20 live nodes gave 128 per node:

| part | instructions per object |
|---|---|
| `forward`: load the slot, range check, header, type lookup, slice test, size by kind | 58 |
| `memmove` call for 24 bytes | 26 |
| the scan loop in `finish`: type lookup, kind dispatch, per-field offsets from the type table, size again | 38 |

Go, same list (at `39d3ad4c25`): 376 per object (38,665,029 and 113,805,572 per cycle), 350 with one P. Go marks and sweeps and does not move.

So per live object, per collection, native is about 2 times cheaper (3 times before). Per unit of allocation the comparison turns on how often each collects. Native collects when the space is full and the space is four times the survivors, so each collection copies at most a quarter of the space: at full retention that is about 177 instructions per 3 objects allocated (estimated from the sizing rule), falling to nothing as the survival rate falls. There are no generations, so a large long-lived graph is copied at every collection. The `live-heap` workload (500,000 live items) is the case this design is worst at.

### Per byte of large objects

One `int[]` of M elements, built by `push`:

| M | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100,000 | 20 | 60 | 48,397,726 | 95,621,604 | 1,180,597 |
| 300,000 | 20 | 60 | 146,286,653 | 336,750,080 | 4,761,586 |
| 100,000, before | 20 | 60 | 21,582,622 | 29,472,559 | 197,248 |
| 300,000, before | 20 | 60 | 44,313,065 | 75,846,089 | 788,326 |

17.9 instructions per element where it was 2.96, and 1.12 per byte where it was 0.37 (an element is 16 bytes, it was 8). This is the largest change the value model made to any number in these notes. The storage of an `int[]` was copied and never looked at; now every slot to the capacity has its tag loaded and tested. Storage is copied and scanned by capacity, not length, and capacity doubles on `push`, so up to half of that is unused tail. Known alternative, not built: a storage whose elements are all immediates could say so in its header payload and be skipped; it would have to be cleared by the first store of a pointer. Go does not scan or move a pointer-free slice: its cycle costs the same at both sizes (1,071,755 and 1,036,546, at `39d3ad4c25`).

### Per frame

Recursion M deep, each frame holding one reference across the call, N collections at the bottom:

| M frames | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100 | 2,000 | 12,000 | 27,648,474 | 87,160,845 | 5,951 |
| 1,100 | 2,000 | 12,000 | 137,701,450 | 747,272,540 | 60,957 |
| 100, before | 2,000 | 12,000 | 30,511,219 | 110,561,209 | 8,005 |
| 1,100, before | 2,000 | 12,000 | 180,591,995 | 1,010,654,079 | 83,006 |

55 instructions per frame where it was 75. The probe was written again (each frame holds the one `Node` it was passed across its call, a word entry); the old one is lost, so the difference may be the probe as much as the collector. A frame holding a value entry was not measured. A trace of the old layout with 20 frames gave 87: 57 in `Copy::stack` (the frame-record loads and one binary search of the site table, which grows with the logarithm of the number of call sites) and 29 in `forward` for slots whose object is already copied.

Go, same shape (at `39d3ad4c25`): 716 per frame (1,165,601 and 1,881,814 per cycle with 200 and 1,200 collections), 703 with one P. Go's stack scan decodes pcdata per frame and its frames are larger.

Weak point: the lookup is a binary search per frame per collection. Tasks parked deep in the same call chain repeat it for every task. A cache keyed by return address, or an index stored next to the return address in a side table hashed by `pc`, removes the logarithm.

## Stop mechanism

There is none to speak of, because there is one worker. A collection runs inside whatever runtime call ran out of space, on the stack of the task that made the call, with every other task already stopped at a switch. The poll word in the worker exists for stopping other workers and nothing sets it (`control.md`, `threads.md`).

## Not built

- Generations, or any way to avoid copying long-lived data.
- A large-object space or pinning. A native cannot hold the address of a heap object across an allocation; a `uint8array` is copied to be held across a stop (`BytesRef::hold`).
- Parallel or concurrent collection.
- Returning memory to the OS: the spare space is kept.
- Collection with more than one worker (`threads.md`).
