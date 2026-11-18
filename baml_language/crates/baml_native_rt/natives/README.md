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
  hold no root and no view.

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
