# Collector: where it goes next

Status: proposed

Nothing here is built unless a section says so. The collector as it stands is in `../gc.md`; numbers quoted below come from there and from `../memory.md`. Sections marked **Ruled** record a decision already made; the rest is open.

Contents:

1. What the collector does today, and why that shape
2. Programs that do poorly
3. How V8 and Go differ
4. When a raw pointer goes stale
5. Pinning, and why a two-space collector cannot do it
6. The non-moving region for large byte blocks (ruled)
7. Bytes during I/O (ruled)
8. Rust slices over heap bytes
9. Long Rust calls and several workers: `scope.off_heap`
10. Owned Rust values and how they are dropped
11. `cleanup` finalizers
12. Generations
13. Block-based collector: the likely end state
14. Smaller ideas
15. Order of work
16. Open questions

## 1. What the collector does today, and why that shape

Two spaces. Allocation bumps a pointer through one:

```
FROM: [ A ][ B ][ C ][ D ][ E ][ free............ ]
                               ^ alloc_ptr
TO:   [ empty....................................  ]
```

A collection copies what is reachable into the other space and swaps:

```
FROM: [ A ][ B ][ C ][ D ][ E ]        <- the whole space is declared empty
TO:   [ A'][ C'][ E'][ free.......... ]
```

It never frees one object. It frees by abandoning the whole old space. That is what makes it fast: cost depends only on what is alive, allocation is always a bump (10 instructions per object, collections included, against Go's 146), and there is no write barrier (a reference store is one instruction).

The first space is 8 MiB. The next space is four times the survivors, so a collection runs after about three times the live size has been allocated. If survivors plus the request do not fit, everything is copied a second time into a larger space. The spare space is kept, never returned to the OS.

Costs: 255 instructions fixed, 122 per live object, 0.37 per byte of a large scalar array (copied by capacity, not length), 75 per stack frame of every task, parked or not.

## 2. Programs that do poorly

Cost per collection follows what the program keeps, not what it throws away. The 4x sizing rule bounds the throughput loss (about 122 instructions per 3 objects allocated at full retention). Pause length and memory footprint are what scale badly.

- Long-running servers with in-memory state: a cache, session table or index that lives for the process, with per-request garbage on top. Every collection copies the whole cache again.
- Agents with long histories: messages, tool results and parsed JSON that grow and are never dropped.
- Load-then-process jobs: a large parsed file kept alive while the compute phase allocates temporaries.
- Large collections of small objects: a million small instances is about 122 M instructions per collection.
- Large scalar arrays: far cheaper per byte, but Go pays nothing for a pointer-free slice and we copy it every time.
- Many parked tasks with deep stacks: every stack is walked every time.
- Memory-constrained deployments: two spaces and the 4x rule put peak memory at several times the live data. 1 GB live wants about 4 GB.
- Latency-sensitive code with a big heap: the pause is a full stop proportional to live data. 10 M live objects is a few hundred milliseconds (estimated from the instruction count, not measured).

Programs that do well: short-lived ones, request handlers with little retained state, compute where most objects die young.

Measured so far: only the `live-heap` workload (500,000 live items). Nothing at server scale.

## 3. How V8 and Go differ

V8 is generational. Its young generation is what we have: two spaces, bump allocation, survivors copied. Objects that survive twice are promoted to an old generation collected rarely, by mark-sweep with occasional compaction, mostly concurrently. A long-lived object is copied once or twice and then left alone. The price is a write barrier on every reference store, to track old-to-young pointers. Large objects sit in a space that never moves, and `ArrayBuffer` storage is outside the managed heap.

Ours is V8's young generation with no old generation behind it.

Go never moves objects: concurrent mark and sweep, size classes, no generations. Pauses are short whatever the heap size. Long-lived data is marked every cycle but never copied. Allocation is slow and there is a write barrier while marking. Addresses stay valid, so passing memory to a syscall is free. Go's split between small and large objects is at 32 KiB.

Side by side:

- Allocation: ours and V8 bump a pointer; Go is far slower.
- Long-lived heap: V8 best (touched rarely), Go in the middle (marked, not moved), ours worst (copied every cycle).
- Pauses: Go short and independent of heap size; V8 short young pauses with mostly concurrent old work; ours a full stop proportional to live data.
- Memory: Go defaults to 2x live; ours is up to 4x plus the spare.
- Mutator cost: ours has no barrier; both others pay one.

In our favor on any path: natives already hold no raw heap address across an allocation, so the runtime is written for a moving collector.

## 4. When a raw pointer goes stale

"The collector moves things under you" is three separate events with different fixes.

1. The Rust code allocates. That may collect, and everything moves.
2. The Rust code calls BAML, or parks. Same, plus arbitrary time passes.
3. Another worker triggers a collection while this one is inside Rust. Does not exist with one worker.

Events 1 and 2 happen at points the Rust code chooses, so "read your pointers again after those calls" is a workable rule. `Root` and `Scope` enforce it through lifetimes: a view into the heap borrows the scope shared, and anything that can move objects (`reserve`, `call_baml`, `io`) borrows it exclusively.

```rust
let text = url.get(scope).as_str();
scope.io(job).await;                 // error: `scope` is already borrowed
use(text);
```

The only suspension points in an `async` native are methods on the scope. A bare `.await` on anything else would let a view survive a park, so it aborts and names the native.

Event 3 can happen at any instruction. Two answers only:

- The collector waits for the call to return. Raw pointers stay valid, but one slow call stalls every worker. Fine for a 50-instruction probe, not for a regex or a syscall.
- The collector proceeds, treating the worker as stopped. Then the Rust code must hold nothing that can move.

So raw pointers into a moving heap are safe exactly for calls that are short, do not allocate and do not call back. Everything else needs one of these:

| approach | cost per access | cost elsewhere |
|---|---|---|
| Rooted slot, read again after any safepoint | one load after each safepoint | discipline; Rust can enforce it, C cannot; does not cover event 3 alone |
| Handle table with accessor calls (JNI style) | a call per field or element | very slow for bulk data, which is why JNI grew a pinning escape hatch |
| Pin the object for the call | zero | the collector must tolerate objects that do not move |
| Copy out before, copy in after | zero during the call | a copy per crossing |
| Keep bulk bytes out of the moving heap | zero | a second region with its own bookkeeping |

What crosses in practice decides it:

- Scalars: registers.
- Text and bytes: nearly all the volume. Read-only going in, freshly made coming out.
- Structured objects: only code that knows our layout can use them, and that is our own natives.

So there is no single representation. Structured objects stay raw with roots, for our code only. Foreign packages would get scalars, byte ranges and opaque tokens they can hand back but not open. The hard case narrows to bytes during a long or blocking call: an HTTP body written by a Tokio thread while the task is parked and collections run. Roots do not help there. It is pin, copy, or non-moving storage.

What crosses and how, as settled:

- Scalars in registers.
- Any object into a short call that cannot allocate: raw heap pointer.
- Any object into one of our allocating natives: a slot the collector updates.
- Large byte blocks into anything, other threads included: raw pointer into the non-moving region plus a keep-alive on the block.
- Small byte data into a long call: copied.

## 5. Pinning, and why a two-space collector cannot do it

Pinning means the collector promises not to move an object. Pin C in the picture from section 1:

```
FROM: [ A ][ B ][ C ][ D ][ E ]        <- cannot be declared empty: C is still here
TO:   [ A'][ E'][ free............... ]
```

After the swap FROM is the next destination. It should be one empty block and has C in the middle.

- Allocation is no longer a bump. The hot path is `ptr += size; if ptr > limit`. With a hole the allocator has to know to skip C.
- C is in the wrong space. At the next collection it sits in the destination while everything else is in the source. Every "is this pointer in the space I am evacuating" test answers wrongly for C.

Pinning in a two-space collector means per-object exceptions in a design whose whole trick is having none.

Collectors that pin cut the heap into blocks (section 13). The non-moving region is the same idea decided ahead of time by size instead of at run time by a pin call.

What stands in for pinning today:

- A native holds no heap address across anything that can allocate.
- To hold bytes across a stop, the native copies them out (`BytesRef::hold`). When the region lands, the held value becomes "pointer plus pin on the storage block" and callers do not change.

So every socket or file write costs a copy, and a read lands in a Rust buffer and is copied into the heap.

Elsewhere: Go's heap never moves, so everything is pinned (`runtime.Pinner` exists only for cgo's rules). V8 as above. .NET has explicit pins (`fixed`, `GCHandle`) and a pinned object heap.

## 6. The non-moving region for large byte blocks

**Ruled:** large byte blocks live in a non-moving region; no real object pinning. Threshold 32 KiB. One mapping per block. Copying small slices out of large parents is deferred ("a micro win for later").

```
moving spaces (small objects):        non-moving region (big byte blocks):
[ A ][ hdr ][ C ]...                  [ 2 MB body ][ 500 KiB file ][ free ]
        |                                  ^
        +-------- data pointer ------------+
```

Everything in the region behaves as if permanently pinned. Nothing in the moving spaces is ever pinned. The two-space collector stays as it is.

### No new object forms

A large string is the existing flat string, allocated somewhere that does not move. Same header, same type id, same offsets:

```
moving spaces                          non-moving region
flat string, 40 B                      flat string, 2 MB
+0 header  +8 lengths  +16 text        +0 header  +8 lengths  +16 text ...
        moves                                   never moves
```

- Compiled code does not change. `text = s + 16` is right for both.
- Slices do not change. A slice's parent is a flat string wherever it lives.
- The cached hash is in the header either way.
- The raw text pointer `s + 16` is stable for the object's life, which is what I/O on another thread needs.

`uint8array` gets it for free: its storage is already a separate object the array points into, so a large storage block is allocated in the region and the 24-byte array header keeps moving.

### What the region needs

One record per block, in a side table:

```rust
struct Block {
    base:  *mut u8,       // page-aligned, its own mapping
    pages: u32,
    pins:  AtomicU32,     // keep-alives held by I/O in flight
}
```

Three changes to the collector:

```rust
// 1. allocation: size decides the home
if size >= LARGE { region.alloc(size) } else { bump(size) }

// 2. tracing: a pointer outside the space being evacuated is marked, not copied
if !from_space.contains(p) { set_mark(p); return p; }

// 3. after each collection: free what was not reached and is not pinned
for b in blocks { if !marked(b) && b.pins == 0 { unmap(b) } else { clear_mark(b) } }
```

The mark is one of the three collector bits already reserved at the bottom of the header. The pin count is in the side table and not in the header, so a Tokio thread never writes to memory the collector reads. `forward` already leaves alone a pointer outside the space being evacuated; what is missing is the mark and the sweep.

### Threshold

| threshold | effect |
|---|---|
| 4 KiB | many blocks, each a whole 16 KiB page on this machine: wasteful |
| 32 KiB | two pages minimum; copying anything smaller each collection is cheap |
| 256 KiB | fewer blocks, but a 200 KiB HTTP body is copied for every send and every collection |

Placement by size, as settled in the bytes design:

- under 32 KiB: moving heap, copied when it crosses into I/O
- 32 KiB and up: the region, zeroed by the OS, zero-copy into I/O in both directions
- Rust-backed (`scope.bytes_over`): package memory, zero-copy always; a kind of storage the collector must neither free nor move

### One mapping per block

Trivial, and memory goes back to the OS at once, at the price of an `mmap` and `munmap` per large block. If the system calls show up in a profile, add a small cache of recently freed blocks. Size classes with a free list are the alternative and were not chosen.

### Limits

- Only large blocks. A 30-byte string still moves and is copied into a long call. A 30-byte copy is cheap.
- Only byte data. A class instance cannot be handed to a long foreign call by address. Foreign code cannot interpret our objects anyway.
- Allocation in the region is slower than a bump, amortized over the bytes then written.

### A small slice keeping a large parent alive (deferred)

```baml
let body = baml.http.get(url).body;     // 2 MB, in the region
let id = body.slice(10, 40);            // 30-byte slice
// body is dropped, id lives for an hour
```

The 2 MB stays mapped because of 30 bytes. The collector already turns slices of an unreached string into strings of their own unless together they are as large as the parent (`settle_slices`). The extension is to apply that whenever the slice is under the threshold. Whether the existing rule applies as written or needs a size condition has not been checked.

### What else the region pays for

- `read_into` and `write_range` on `uint8array` need it for the in-place half.
- Large `int[]` and `float[]` storage could live there too and stop being copied by capacity each cycle. Not decided.
- It does not get in the way of a block-based collector: big blocks stay out of the moving heap in that design too.

## 7. Bytes during I/O

**Ruled:** mutating a `uint8array` during an I/O call on it is allowed. The bytes sent are unspecified, memory stays safe. Same as Go and Node.

Options that were on the table:

| option | program sees | cost |
|---|---|---|
| Allow it (chosen) | unspecified bytes sent, memory safe | none |
| Copy mutable arrays, zero-copy strings | contents as of the call | a copy per `uint8array` write |
| Freeze while in flight | `buf[0] = 255` panics until done | a flag test on every byte store |
| Transfer | `buf` becomes empty, as a transferred `ArrayBuffer` | none, surprising |

Using the pin count to block writers is the "freeze" row and was withdrawn.

The keep-alive is on the storage block, not the array:

```rust
let block = buf.load().storage();
let keep  = heap.keep_alive(block);    // the block survives even if no array points at it
tokio.spawn(async move { socket.write_all(block.bytes()).await; drop(keep); });
```

That was designed for an array that could grow during a write and replace its storage. `uint8array` is now fixed length, so storage is never replaced; the keep-alive still covers the array being dropped while a write is in flight.

Reads are race-free as long as they return fresh arrays: the runtime allocates the block, the Tokio thread fills it, and only then does an array pointing at it become visible. A "read into my buffer" function brings the mutation race back, which the ruling above already allows.

The Tokio side never touches the BAML heap and never runs BAML. It produces a plain Rust value and the native turns that into BAML objects once back on a worker. What can cross to it is owned data or a keep-alive on a region block.

Until the region exists, everything is copied (`BytesRef::hold`). That is the interim state, not the design.

## 8. Rust slices over heap bytes

The borrow rule stops the collector case. It does not stop someone writing the bytes while Rust holds `&[u8]`.

- One worker: sound. A native that does not stop runs to completion with no BAML interleaved.
- Across a stop: covered by section 7.
- Several workers on one heap: not sound. Worker B can run `arr[i] = x` while worker A is inside `sha256(view.as_slice())`. In Go that is a benign-looking race. In Rust a `&[u8]` that changes underneath is undefined behavior.

The rule: Rust reads bytes another thread can write through raw pointers. The view's primary API is pointer and length (`len`, `get`, `copy_to`, `to_vec`, `copy_from`, `copy_from_bytes`). A borrowed `&[u8]` is the documented exception, valid only with one worker.

`natives/README.md` says `BytesRef::as_slice` and `as_mut_slice` are the only two such slices. `../threads.md` items 26 to 31 list more (`ArrRef::words`, `native/array.rs` `elements()`, `OpaqueRef::plain`). Crates like `sha2` want `&[u8]`, so with several workers each of those callers needs a copy, a region pointer with a rule about writers, or a language rule that the array has one owner. Not decided.

## 9. Long Rust calls and several workers: `scope.off_heap`

With several workers a collection needs every worker stopped, and a worker inside a long Rust call is not. "Long running" is not a flag on the native; it is a region inside it:

```rust
fn is_match<'s>(scope: &mut Scope<'s>, pattern: Root<'s, Str>, text: Root<'s, Str>) -> bool {
    let pattern = pattern.get(scope).as_str().to_owned();
    let text    = text.get(scope).as_str().to_owned();
    scope.off_heap(move || {                       // 'static: no Roots, no views
        Regex::new(&pattern).is_ok_and(|re| re.is_match(&text))
    })
}
```

Inside `off_heap` the worker tells the collector "treat me as stopped", runs the closure on the same thread, and checks in again on return, waiting out a collection if one is running. Nothing the closure held could have moved, since the compiler made sure it held nothing from the heap. No thread hop. The cost is the copies going in; large inputs would use the region.

Built today as a plain call of the closure (`drive.rs`), and nothing calls it yet. Unenforced: a long loop nobody wrapped.

The rest of what a collection with several workers needs is in `../threads.md` (items 1 to 8) and `../control.md`: a stop request compiled code can see at every loop and call, every worker's anchor and roots walked, per-worker allocation spans from a shared space. Today nothing sets the poll word, and a loop of direct calls has no safepoint.

## 10. Owned Rust values and how they are dropped

Built. Recorded here because it constrains any new collector.

About 40 stdlib classes carry a hidden `$rust_type` field. Two forms:

Plain bytes, stored inline (`Sha256`, `DefaultHasher`, the random generators): an ordinary object the collector copies and follows nothing from. Requires `Copy`: no destructor, no self-pointers. Nothing new in the collector.

Owned values behind a pointer (`File`, sockets, `Response`, `Regex`, `Subprocess`):

```
+0   header    kind OWNED: no outgoing references
+8   pointer   Arc<T>, raw
+16  vtable    &'static { drop_fn, type_name }
```

For I/O the native clones the `Arc` into the Tokio job, so nothing is pinned or kept alive on the BAML side.

A copying collector never visits dead objects, so it keeps a list:

```rust
worker.owned.retain_mut(|address| match forwarded(*address) {
    Some(new) => { *address = new; true }
    None      => { drop_owned(*address); false }
});
```

Cost per collection is proportional to live owned objects, which is why hashers are inline. Resources are released only at a collection, so a collection also runs after 128 owned values have been made since the last one, or twice the number found alive. A generational design must keep both: the list walk for whichever generation is collected, and the pressure trigger.

## 11. `cleanup` finalizers

Not built. Native refuses to compile construction of a class with a `cleanup` method, which includes the stdlib's `Subprocess`.

What a collection would have to do:

```rust
for obj in finalizable.drain_dead() {          // a list of instances whose class has cleanup
    let obj = copy_to_new_space(obj);          // dead, but cleanup(self) needs it: keep it one more round
    scheduler.spawn(cleanup_fn(obj), obj);     // the collector cannot run BAML, so queue a task
}
// really freed at a later collection, once that task has finished
```

The list and the walk are the same as for owned values. New parts: a registration at every construction, a second copying phase, and a collection creating a task from a bare function and argument (tasks are only made from compiled spawn plans today).

Decisions needed:

- whether pending cleanups run at exit
- what an uncaught panic inside one does
- whether forcing a collection runs them before returning (the interpreter's test expects that)

## 12. Generations

The fix for section 2. The current collector becomes the nursery. Survivors of two collections are promoted to an old space collected rarely.

Needs:

- A write barrier on reference stores: a card table or a remembered set for old-to-young pointers. Every reference store grows from one instruction.
- A collector for the old space: mark-sweep, mark-compact, or copy.
- A promotion rule.

Keeps the 10-instruction allocation and cheap young collections. The barrier is the cost, paid by every program, including those that do well today.

The Go design (non-moving, concurrent) is not proposed. It gives up bump allocation and rewrites most of the heap. It would make several workers and pinning simpler.

## 13. Block-based collector: the likely end state

Cut the heap into blocks instead of two spaces:

```
[ block 0 ][ block 1 ][ block 2 ][ block 3 ][ block 4 ]
   32 KiB each, each independently "being evacuated" or "staying put"
```

A collection picks per block whether to copy the live objects out or leave the block alone. Pinning an object marks its block as staying for this round. Allocation is still a bump, within one block at a time. Roughly how .NET, the JVM's region collectors and Immix work.

Costs:

- "Is this object being moved" becomes a table lookup by block, not one compare against a space boundary.
- A block kept by one pinned object keeps its dead neighbors until the pin drops.
- A substantially larger collector, a guess of several times the current size.

It covers pinning of small objects and structured data, and is the usual base for a generational or concurrent collector. The region in section 6 does not conflict with it.

## 14. Smaller ideas

From `../gc.md` and `../memory.md`, independent of the above:

- Stack maps: emit the table sorted and in the runtime's layout so decode is zero. Today it is decoded eagerly at every start (about 17 instructions per word), entries are 8 bytes where 4 would do, and the vectors grow from empty.
- Stack walk: one binary search per frame per collection. A cache keyed by return address, or a side table hashed by `pc`, removes the logarithm. Tasks parked in the same call chain repeat the search.
- Parked stacks: skip a task's stack when it has not run since the last collection. Needs old-to-young tracking for its slots, so it belongs with generations.
- Scalar array storage is copied by capacity, and capacity doubles on `push`, so up to half the copy is unused tail. Copy by length.
- Reference storage is scanned to capacity, which relies on slots past the length being zero. A test covers popped slots; map compaction is not covered.
- Return memory to the OS: the spare space is kept forever.
- Peak memory: a second copy runs when survivors do not fit, so the worst case holds three spaces.
- Roots found without the anchor stub: the three stores on every `allocates` call could go if the collector found the compiled frame lazily, which needs an unwinder for Rust frames.
- No tail calls in runtime functions that read the current frame: one was miscompiled into a tail call and a collection missed the caller's roots. Patched by forcing the frame; the class of bug remains.
- A missing stack map is fatal only in the checked build.

## 15. Order of work

Proposed, not agreed:

1. The non-moving region (section 6). Ruled on, smallest, removes the I/O copies, unblocks `read_into`. No barrier.
2. The stop request and safepoints for several workers (`../threads.md`), with `off_heap` made real.
3. Finalizers (section 11), once the three decisions are made.
4. Generations (section 12), after measuring a server-scale workload and the barrier.
5. The block design (section 13) only if pinning of small objects or concurrent collection is wanted.

## 16. Open questions

- What the write barrier costs per reference store, on the field-store probe in `../memory.md` (22 instructions per iteration today; Go 14 with its barrier).
- Promotion rule and old-space collector.
- Whether pointer-free `int[]` and `float[]` storage goes in the region.
- Whether the slice rule applies to region parents as written.
- `&[u8]` over heap bytes with several workers (section 8).
- The three finalizer decisions (section 11).
- How the barrier and remembered set are shared between workers.
- A server-scale measurement: hundreds of MB live with steady garbage.
- `uint8array` slices and views were never designed; the slice layout is strings only.
