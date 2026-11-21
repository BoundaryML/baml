# The lists of natives

Each `*.txt` file here lists standard library builtins this runtime
implements in Rust, one per line: the builtin's path, then flags. Blank
lines and `#` comments are skipped. The build reads the lists twice: to
generate the trait a native implements and the `extern "C"` symbol compiled
code calls, and to tell the compiler how to call it. A builtin that is not
listed is reported as not ported when a program calls it.

```
baml.String.byte_length
baml.String.repeat allocates
baml.json._stringify async
```

## Passing a `uint8array`

A `uint8array` can be written, by compiled code and by natives, and the
collector moves it. So a native reads one by copying: a byte
(`BytesRef::get`), all of it (`to_vec`), or a range into a buffer or
another array (`copy_to`, `copy_from`, `copy_from_bytes`). A native that
stops, which is any `async` one before its first `scope.io` or
`scope.call_baml`, takes the bytes with `BytesRef::hold` first: the view
cannot be held across the stop, and what compiled code writes into the
array meanwhile is not seen. `hold` copies today, and is the one place
that changes when a stop can keep the array's storage block alive and in
place instead. What it holds is the storage's bytes, never the array: a
`push` gives the array new storage. A result is made through the scope
(`alloc_bytes`, `alloc_zeroed_bytes`).

`BytesRef::as_slice` and `as_mut_slice` lend the bytes in place instead.
They are the only two places a Rust slice is made of bytes in the heap, and
both rely on a single worker: with several workers on one heap another
could write the bytes while the slice is alive. A native uses one only
where it says why a copy would not do.

## What a flag promises

A flag is a promise the native's author makes. The generated signature
holds the author to it where the type system can.

**No flag.** The native cannot touch the heap's allocator. It is passed
borrowed views of its arguments (`StrRef`, `ArrRef`) and no `Scope`, so it
has nothing to allocate with, and it cannot throw, since an error is an
object. The call is a plain C call: no worker argument, and the caller does
not expect anything to have moved.

**`allocates`.** The native may allocate, so a collection may run during
the call and move every object. It is passed a `Scope` and its arguments as
roots, which the collector keeps current. A view borrows the scope shared
and an allocation borrows it exclusively, so the compiler rejects a view
held across an allocation. The call passes the worker first, the caller
treats the call as a point where objects move, and a native whose
declaration can throw returns the thrown object beside its value.

The symbol of such a native is a stub of four instructions that records in
the worker where compiled code called from (its stack pointer, the return
address, its frame pointer) and goes on in the native. A collection walks
the stack from there. Recording it before any Rust runs is what keeps it
from depending on how the Rust compiler lays the native's frame out. A
native with no flag has no stub: it cannot collect, so nothing is recorded
and its call costs what a C call costs.

A native holds as many roots as it makes: the first sixteen are in the
call's own frame, and more spill to chunks on the Rust heap that the
collector also walks. A loop that makes a few roots each time around runs
its body in `scope.scoped(|scope, _| ...)`, which gives them back when the
body returns, and they cannot leave it. A loop that stops in its body
cannot, and uses the unsafe `scope.release_last`.

**`async`.** The native may stop and go on later. It implies `allocates`.
The native is an `async fn`, and it stops in exactly two ways, both methods
of its scope:

- `scope.call_baml(function, args).await` runs a compiled BAML function,
  such as a class's own `to_json`, and resumes with what it returned. What
  it threw comes back as an `Err` to hand on with `?`.
- `scope.io(job).await` runs async Rust on the runtime's own Tokio threads
  and parks the task meanwhile. The job is `'static + Send`, so it can
  hold no root and no view. The request the native leaves carries the
  function that starts the job, so what waits for it names no Tokio: a
  program links it only when one of the natives it calls uses `scope.io`. Keep it that way, since linking Tokio costs every start of
  the program about 0.8 M instructions in the dynamic linker, for the
  system symbols Tokio imports, whether or not a job ever runs.

Both borrow the scope exclusively, so no view survives a stop, and the
future must be `Send`, so nothing bound to a thread (a `MutexGuard`, an
`Rc`) does either. Awaiting anything else ends the program with a message
that names the native. Compiled code calls an `async` native as it calls a
BAML function: the call can park the task, and what it throws arrives by
unwinding.

### What drives an `async` native

Compiled code. The compiler makes a stub for each `async` native a program
calls, with the convention of a BAML function. The stub calls the native's
symbol, which makes the native's state and returns. Then it loops: it calls
`baml_rt_native_poll`, which polls the native once and says what to do
next, and does that itself: return the value, throw the error again, call
the compiled function the native asked for, or wait for its I/O
(`baml_rt_io_wait`).

Every call into the runtime returns before anything that can stop the
task. So while a task is stopped, or runs BAML a native called, no frame of
the runtime is on its stack: the frames are compiled BAML, the stubs
between, and the stub the task booted in. A task that stops can go on on
another thread and under another worker, and a Rust frame would go on with
whatever the Rust compiler chose to keep, in a register or a stack slot, of
the thread and the worker it stopped on. No reading of the source rules
that out, so the frames are not there. The stub reads the worker from its
pinned register for every call, and the poll tells the native's scope.

The runtime with checks holds every stop to this (`await`, sleep, I/O,
`spawn`): it walks the stopping task's stack from its last frame to its
first and ends the program, naming the function, if a return address is
neither in compiled code nor one of a short list of stubs.

A native's own state is held to it by the compiler: its future must be
`Send`, and a stop takes the scope exclusively.

Long work that touches no heap object goes inside
`scope.off_heap(move || ...)` in an `allocates` native. With one worker that
is a plain call. With several, a collection needs every worker stopped, and
`off_heap` is where a worker says it already is, so the others do not wait
for the work to end. That is why there is no `long_running` flag: the
native marks the part that runs long, not the whole call.

## Classes with Rust state

A class of the standard library with a `$rust_type` field keeps Rust state.
The field refers to a heap object of one of two forms, and the native that
makes the instance chooses which:

- **Plain**, `scope.alloc_plain(value)` for a `T: Copy + 'static`. The
  value's bytes are in the heap object, which the collector copies like any
  object without references. Nothing is dropped. A native reads and writes
  it in place through `view.plain::<T>()`, a `&Cell<T>` that borrows the
  view, so it cannot outlive an allocation. `baml.hash.DefaultHasher` is
  one.
- **Owned**, `scope.alloc_owned(Arc<T>)` for a `T: Send + Sync + 'static`.
  The heap object holds the `Arc` and the address of what the runtime knows
  of `T`, which is also how a read checks the type: one compare. A native
  borrows `&T` with `view.owned::<T>()`, and one that allocates or stops
  takes its own `Arc` with `view.share::<T>()`. The heap lists these
  objects. The collection that finds one dead drops its `Arc`, and what is
  left when the program ends is dropped then. Making 128 of them, or twice
  as many as the last collection found alive, brings on a collection, since
  the values are outside the heap and would otherwise never fill it.
  `baml.regex.Regex` is one.

A native takes an instance of a standard library class as `ObjRef` or
`Root<Obj>` and reads the field with `instance.field::<Opaque>(index)`.

## Making an instance of a class

`scope.build(class)` starts an instance of any class, `set(scope, index,
value)` or `set_named` gives a field its value, and `finish()` returns it.
The runtime writes each field the way the class's layout holds it, and ends
the program if a value does not fit its field or a field that cannot be
`null` was left unset. `baml.json` writes the instances it decodes with the
same two functions.

A `Class` is one class at its type arguments, as the program describes it.
A native gets one from the type a call names (`scope.class_from`), from an
object (`scope.class_of`), from the type of a field (`class.field_class`),
or by position in `layout::NAMED` (`scope.named_class`). A field that holds
instances of one class says which, whether or not `baml.json` can read the
class: a native that builds a `Timeout` finds the `Duration` of its limit
in the field, though a `Duration` holds a `bigint`. A program
describes the classes of `NAMED` that the declarations of the natives it
calls name, in what they take, return and throw. Asking for one that no
such declaration names ends the program.

## Comparing two values

`baml.ops.equals_equals` is `==` on the values no instruction compares, and
`baml._equals_structural_default` is the `eq` of a class that writes none.
Both are one walk of the two values (`src/native/structural.rs`). A call
passes the description of each argument's static type, as a call of
`baml.json` does, and zero for a type `baml.json` has no form for: the walk
reads a description where it has one, and otherwise the value's header, and
for an object nothing describes the words the collector follows.

A class with an `eq` of its own is compared by it. The compiler makes an
entry for each such `eq`, which takes two instances and returns the answer's
one byte, and lists them in `baml_native_equals`: a count, then the type id
and the entry of each class, sorted by type id. Only a program that calls
one of the two natives defines the list, and only they read it. The walk
calls nothing: it takes such a pair to be equal and hands it out, and the
native calls the `eq` of each pair once the walk has found nothing else
unequal.

What the walk does not know, and compares as it finds it:

- An enum with an `eq` of its own, and a generic class with one: both are
  compared as if they had none.
- The elements of an array of values, and the values of a map, whose
  static type nothing says (an `unknown` holding a `float[]`): word by
  word, so a NaN equals only itself bit for bit, and the two zeros differ.

## A native the declaration says throws nothing

A native listed without `async` throws only when its declaration lets it:
the symbol of one that throws nothing returns the bare value. A builtin that
panics all the same (a count below zero) is listed as `async`, whose symbol
can always throw, and never stops. `baml.random.Rng$for$SystemRandom.random`
is one.

## `baml.http` and the two builds of the runtime

`baml.http` sends its requests with `reqwest` over `rustls`, set up as
`sys_native` sets them up: the crate's `aws-crypto` and `ring-crypto`
features choose the TLS provider the same way. The TLS library runs code
when a program that links it starts, and on macOS it brings two system
frameworks for the program to load. The linker keeps both even when
nothing sends a request, which costs every start about 11 M instructions
and the executable about 700 KB.

So the compiler builds the runtime twice, with the `http` feature and
without, and links the one with it only into a program whose object names
a native of `baml.http`. The build without the feature skips the natives
whose path starts with `baml.http.`. A native of another namespace that
needs TLS has to be named beside it, in the runtime's `build.rs` and in
the compiler's `link.rs`.

## The runtime tests link, and migration stress

The compiler builds each runtime a second time with the `checks` feature
(`RuntimeBuild::Checked`, or `native_pack --checked`). A program ships with
the other. The conformance suites run every case on both: as it ships,
plain and collecting on every allocation, and on the one with checks,
migrating and migrating while collecting on every allocation.

With `BAML_MIGRATE_STRESS` set, the runtime with checks serves as its one
worker on two threads in turns: every task that stops (an `await`, a sleep,
I/O, a `spawn`) goes on on the other thread, and under a worker that has
moved to another address, the one it left made unreadable. A native that
keeps anything of its thread or of its worker across a stop then fails at
once, where with one thread it would never notice. It finds what is held
across a stop. It does not find a race between two workers running at the
same time: only one thread runs at any moment.

The second thread is why the mode is not in the runtime a program ships
with: importing what starts a thread costs every start about 0.6 M
instructions, whether or not a thread is ever started.

`BAML_GC_STRESS`, which collects on every allocation, works in both.

The runtime with checks also holds the compiler to its stack maps. The
compiler lists every call in compiled code: those across which a frame slot
holds a reference, with the slots, and then those across which none does.
Only the runtime with checks reads the second list, so it costs the start
of a program that ships nothing. With it, a return address into compiled
code that has no entry is a map the compiler lost, and the collection that
meets one ends the program naming the function. Without the check such a
frame reads as one that holds no roots, and its objects are lost quietly.

## These files are temporary

The same facts belong on the BAML declarations, as annotations. Today the
standard library carries `//baml:mut_vm` and `//baml:may_yield`, which
describe how the bytecode VM calls a builtin, and the VM's build reads
them. Once the bytecode VM is removed, the flags here replace those two
annotations and these files go away. Until then nothing here changes what
the VM's build reads.

## Planned, not built

**`small_stack`.** A native that a build step has proven to fit a byte
budget of stack. It will be called directly on a task's stack, while every
other native moves to the worker's own stack for the length of the call.
It arrives together with task stacks that start at 2 KiB and grow by
copying, which is what makes the budget matter: today a task's stack is
fixed and large enough for any native to run on.
