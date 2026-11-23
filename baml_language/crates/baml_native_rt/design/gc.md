# The collector

A stop-the-world copying collector, how it finds roots, and what it
costs. Allocation is in `memory.md`. Numbers are instructions retired,
measured unless marked estimated; method in `README.md`.

## Design

Cheney's algorithm over two spaces (`src/heap.rs`). Live objects are
copied into a fresh space, every reference is updated, the old space is
kept as a spare for the next collection. No generations, no marking, no
write barrier, no concurrency. Every object moves at every collection.

```rust
pub(crate) fn collect(&mut self, needed: usize) {
    let used = self.alloc_ptr - self.heap.space.start;
    self.copy_into(used);                       // into the spare, or a new space
    let live = self.alloc_ptr - self.heap.space.start;
    self.heap.next_space = self.heap.space.size.max((live + needed) * SPACE_PER_LIVE);  // 4x live
    if live + needed > self.heap.space.size { self.copy_into(live + needed); }          // move again, larger
}
```

The first space is 8 MB. After a collection the next space is at least
four times what survived, so copying stays a fraction of allocating. When
the survivors do not leave room for the request, everything is copied a
second time into a larger space. Spaces come from `std::alloc` and are
not returned while they are the spare.

A collection also runs when 128 owned Rust values (files, sockets) have
been made since the last one, or twice the number found alive, so their
destructors run without waiting for 8 MB of garbage.

A slice of a string does not keep the string alive: slices of an
unreached string are flattened into strings of their own unless together
they are as large as the parent (`settle_slices`).

## Roots

Roots are precise. Three sources, walked in `copy_into`:

```rust
copy.stack(&self.heap.stack_maps, self.anchor, base);        // the running task
for task in self.sched.tasks() {
    let roots = crate::sched::task_roots(task, self.cur_task);
    for slot in roots.slots { copy.forward(slot); }           // task.future, task.entry
    if let Some((result, [sp, pc, fp], base)) = roots.stack { // a task that is not running
        copy.forward(result);                                 // the switch frame's result word
        copy.stack(&self.heap.stack_maps, Anchor { sp, pc, fp }, base);
    }
    crate::native::drive::each_root(task, |slot| copy.forward(slot));   // async natives' root slots
}
for &slot in &self.roots { copy.forward(slot); }              // the runtime's registered slots
```

The running task's walk starts from the anchor: the stack pointer, return
address and frame pointer of the compiled frame that last called the
runtime, recorded by a stub before any Rust ran (`natives.md`). Every
other task's walk starts from its switch frame. Every task's stack is
walked at every collection, parked or not.

### Stack maps

Cranelift records, for each call in compiled code, which frame slots hold
references across it (`declare_value_needs_stack_map`). A reference that
is live across a call is stored to its slot before the call and loaded
back after it, since the callee may have moved the object. The compiler
emits one table, `baml_native_stack_maps`: a count of functions, then per
function its address and its call sites, each a return-address offset and
a list of slot offsets from the caller's `sp`. Every entry is a 64-bit
word.

The runtime decodes the table once, in `Mutator::new`, into a sorted
vector, and keeps it for the life of the process:

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
    for &offset in maps.slots(pc) { self.forward(sp + offset as usize); }
    if fp >= base { break; }
    sp = fp + 2 * WORD;        // a caller's sp at a call is just above the callee's frame record
    pc = load(fp + WORD);
    fp = load(fp);
}
```

That relies on every function keeping a frame record and on no tail
calls.

### Decode cost

Decode is per process, not per collection. Single-step count from the
first instruction that reads the table to the next call after the decode,
`malloc` excluded:

| program | functions | call sites with roots | slots | table bytes | decode instructions |
|---|---|---|---|---|---|
| return-42 | 0 | 0 | 0 | 8 | 11 |
| `agent-loop-300` | 4 | 54 | 206 | 2,584 | 6,666 (loop 5,455, sort 480, vector growth 731) |
| `request-handler-2k` | 4 | 80 | 167 | 2,688 | 7,182 (loop 5,679, sort 714, vector growth 789) |
| `json-parser-15` | 7 | 110 | 341 | 4,608 | 11,535 (loop 9,699, sort 984, vector growth 852) |

About 17 instructions per table word in the loop, plus the sort, plus 8
to 10 vector regrowths, each a `realloc`. With the allocator counted the
three programs are 18,090, 19,809 and 25,597 under the debugger, where
`malloc` runs slower paths than outside it. Earlier counts of this
decode in three programs were 37, 27,013 and 445,674: an empty table, a
table the size of `json-parser-15`'s with the allocator included, and one
about 17 times larger (estimated from the per-word rate). The cost is
linear in the table and paid at every start, used or not.

Weak points: the table is 8 bytes per entry where 4 would do, it is
decoded eagerly though a program that never collects never needs it, and
the vectors are grown from empty although both counts could be emitted.
Emitting the table already sorted and in the runtime's layout would make
decode zero.

## Cost of a collection

`baml.sys.collect_garbage()` forces one. All probes call it N times.

```baml
class Node { v int  next Node? }
let head: Node? = null;
for (let i = 0; i < M; i += 1) { head = Node { v: i, next: head }; };
for (let i = 0; i < N; i += 1) { baml.sys.collect_garbage(); s += 1; };
```

### Fixed cost

Nothing live, one frame, no roots: 255.03 instructions per collection (N
100,000: 39,965,563; 600,000: 167,484,616). Single-step trace, 244:

| part | instructions |
|---|---|
| anchor stub and the native's wrapper | 9 |
| `Mutator::collect` | 34 |
| `copy_into`: take the spare space, set up, the empty scan, swap spaces | 123 |
| `Copy::stack`: one frame, one binary search | 44 |
| `Copy::forward`: the main task's two slots | 34 |

No allocation, no system call, no page touched. Go's `runtime.GC()` with
nothing live is 1,122,109 instructions (N 2,000: 2,194,559,900; 12,000:
13,415,648,436), 553,984 with `GOMAXPROCS=1`. A forced Go cycle is a full
concurrent mark and sweep with two stop-the-world phases and wakes every
P; it is not what Go pays per unit of allocation, but it is the fixed
cost of a cycle.

### Per live object

A linked list of M `Node`s (24 bytes each), collected N times:

| M | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100,000 | 20 | 60 | 264,406,846 | 752,585,620 | 12,204,469 |
| 300,000 | 20 | 60 | 764,476,000 | 2,228,544,621 | 36,601,716 |

122 instructions per live object per collection, 5.1 per byte for this
object size. A trace with 20 live nodes gives 128 per node:

| part | instructions per object |
|---|---|
| `forward`: load the slot, range check, header, type lookup, slice test, size by kind | 58 |
| `memmove` call for 24 bytes | 26 |
| the scan loop in `finish`: type lookup, kind dispatch, per-field offsets from the type table, size again | 38 |

Go, same list: 376 per object (38,665,029 and 113,805,572 per cycle),
350 with one P. Go marks and sweeps and does not move.

So per live object, per collection, native is 3 times cheaper. Per unit
of allocation the comparison turns on how often each collects. Native
collects when the space is full and the space is four times the
survivors, so each collection copies at most a quarter of the space:
at full retention that is about 122 instructions per 3 objects allocated
(estimated from the sizing rule), falling to nothing as the survival rate
falls. There are no generations, so a large long-lived graph is copied
at every collection. The `live-heap` workload (500,000 live items) is
the case this design is worst at.

### Per byte of large objects

One `int[]` of M elements, built by `push`:

| M | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100,000 | 20 | 60 | 21,582,622 | 29,472,559 | 197,248 |
| 300,000 | 20 | 60 | 44,313,065 | 75,846,089 | 788,326 |

2.96 instructions per element, 0.37 per byte. Storage is copied by
capacity, not length, and capacity doubles on `push`, so up to half of
that is unused tail. Go does not scan or move a pointer-free slice: its
cycle costs the same at both sizes (1,071,755 and 1,036,546).

### Per frame

Recursion M deep, each frame holding one reference across the call, N
collections at the bottom:

| M frames | N lo | N hi | instr lo | instr hi | per collection |
|---|---|---|---|---|---|
| 100 | 2,000 | 12,000 | 30,511,219 | 110,561,209 | 8,005 |
| 1,100 | 2,000 | 12,000 | 180,591,995 | 1,010,654,079 | 83,006 |

75 instructions per frame. A trace with 20 frames gives 87: 57 in
`Copy::stack` (the frame-record loads and one binary search of the site
table, which grows with the logarithm of the number of call sites) and 29
in `forward` for slots whose object is already copied.

Go, same shape: 716 per frame (1,165,601 and 1,881,814 per cycle with
200 and 1,200 collections), 703 with one P. Go's stack scan decodes
pcdata per frame and its frames are larger.

Weak point: the lookup is a binary search per frame per collection.
Tasks parked deep in the same call chain repeat it for every task. A
cache keyed by return address, or an index stored next to the return
address in a side table hashed by `pc`, removes the logarithm.

## Stop mechanism

There is none to speak of, because there is one worker. A collection
runs inside whatever runtime call ran out of space, on the stack of the
task that made the call, with every other task already stopped at a
switch. The poll word in the worker exists for stopping other workers
and nothing sets it (`control.md`, `threads.md`).

## Not built

- Generations, or any way to avoid copying long-lived data.
- A large-object space or pinning. A native cannot hold the address of a
  heap object across an allocation; a `uint8array` is copied to be held
  across a stop (`BytesRef::hold`).
- Parallel or concurrent collection.
- Returning memory to the OS: the spare space is kept.
- Collection with more than one worker (`threads.md`).
