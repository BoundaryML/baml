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

**`async`.** The native may stop and go on later. It implies `allocates`.
The native is an `async fn`, and it stops in exactly two ways, both methods
of its scope:

- `scope.call_baml(function, args).await` runs a compiled BAML function,
  such as a class's own `to_json`, and resumes with what it returned. What
  it threw comes back as an `Err` to hand on with `?`.
- `scope.io(job).await` runs async Rust on the runtime's own Tokio threads
  and parks the task meanwhile. The job is `'static + Send`, so it can
  hold no root and no view. The request the native leaves for its driver
  carries the function that starts the job, so the driver names no Tokio:
  a program links it only when one of the natives it calls uses
  `scope.io`. Keep it that way, since linking Tokio costs every start of
  the program about 0.8 M instructions in the dynamic linker, for the
  system symbols Tokio imports, whether or not a job ever runs.

Both borrow the scope exclusively, so no view survives a stop, and the
future must be `Send`, so nothing bound to a thread (a `MutexGuard`, an
`Rc`) does either. Awaiting anything else ends the program with a message
that names the native. Compiled code calls an `async` native as it calls a
BAML function: the call can park the task, and what it throws arrives by
unwinding.

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
or by position in `layout::NAMED` (`scope.named_class`). A program
describes the classes of `NAMED` that the declarations of the natives it
calls name, in what they take, return and throw. Asking for one that no
such declaration names ends the program.

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
