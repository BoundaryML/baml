# Startup and size

What a compiled program costs before its first line runs, and how large
it is. Instructions are whole-process `instructions retired` from
`/usr/bin/time -l`, best of 7, kernel included; method in `README.md`.

## What runs

`main` is the C entry point, exported by the runtime (`src/lib.rs`). There
is no Rust `lang_start`.

```rust
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let stress = std::env::var_os("BAML_GC_STRESS").is_some();
    let migrate = std::env::var_os("BAML_MIGRATE_STRESS").is_some();
    let program = Program { stack_maps: ..., types: ..., unwind: ..., functions: ... };
    map::randomize_seed();                         // CCRandomGenerateBytes
    let mut mutator = Mutator::new(program, stress);   // 8 MB space, decode the stack maps
    let status = sched::run(&raw mut mutator, baml_native_entry as usize, migrate);
    mutator.drop_owned();
    status
}
```

`Mutator::new` allocates the first space (8 MB, untouched until used) and
decodes the stack maps (`gc.md`). `sched::run` maps the main task's 8 MB
stack and switches to it. `baml_native_entry` is compiled code: it calls
the user's `main` inside a handler that takes everything, and prints the
result. The handler table and the function table are decoded lazily, on
the first throw.

The link is against one of four runtime archives (`link.rs`): with or
without `baml.http`, lean or checked. A program that names no
`baml_rt_baml_http_*` symbol gets the archive without reqwest and rustls.
Dead code is stripped by the linker (`-dead_strip`).

## Startup

```baml
function main() -> int { 42 }
```

| program | instructions | binary bytes | dylibs | imported symbols |
|---|---|---|---|---|
| C `int main(){return 42;}` | 11,073,643 | 16,832 | 1 | 0 |
| C `printf("42\n")` | 11,368,801 | 33,432 | 1 | 1 |
| native `main() -> int { 42 }` | 14,514,192 | 573,464 | 2 (libSystem, libiconv) | 66 |
| Go `func main() {}` | 16,246,923 | 1,749,218 | 1 | 42 |
| Go `os.Exit(42)` | 17,927,966 | 2,214,386 | 2 | 57 |
| Go `fmt.Println(42)` | 18,184,609 | 2,475,682 | 2 | 57 |
| Node `console.log(42)` | 363,927,537 | | | |
| native, one `baml.http.fetch` (connection refused) | 29,534,719 | 4,581,864 | 4 (+Security, CoreFoundation) | 153 |
| Go, `net/http` linked, not called | 34,575,095 | 8,103,042 | 4 | 90 |

The floor on this machine is 11.07 M: `exec`, dyld, and libSystem's
initializers for a program that does nothing. The native return-42 is
3.44 M above the floor. Go's empty program is 5.17 M above it.

Where the 3.44 M goes:

| part | instructions | how known |
|---|---|---|
| runtime code from the stack-map decode to `exit`, user mode | 8,901, of which 5,437 in `malloc`/`free` and 1,168 in pthread calls | single-step trace |
| `main` to the first instruction of `baml_native_entry`, kernel included | 207,873 | `proc_pid_rusage` read at two breakpoints |
| `baml_native_entry` to `exit`, kernel included | 148,327 | same |
| before `main`: dyld binding 66 imports and loading libiconv, page-ins of a 573 KB image, over the C baseline | about 3.1 M | by subtraction, estimated |

So the runtime's own initialization is about 9,000 user instructions.
Between `main` and the user's code the kernel does about 200,000: the
`mmap` and `mprotect` of the main stack, first-touch faults on the stack,
the space, the task record and the runtime's data pages. Everything else
is the dynamic linker.

The two `getenv` calls for the stress variables cost about 700
instructions together.

What dominates and what is known to help: startup is dyld. Each imported
symbol and each dylib costs. `natives/README.md` records that linking
Tokio costs every start about 0.8 M and a second thread's symbols about
0.6 M, which is why Tokio is linked only when a native that uses
`scope.io` is called and why migration stress lives in a separate
archive. The `baml.http` archive doubles startup (29.5 M) for the TLS
library's setup and two system frameworks, whether or not a request is
sent; in this probe one request is attempted and refused, which is
included.

Compared with Go: native starts in 81% of the instructions of Go's
`os.Exit(42)` program and 89% of Go's empty one. Go initializes its
scheduler, heap arenas and several threads before `main`.

## Size

| program | bytes |
|---|---|
| native return-42 | 573,464 |
| native, three speedtest programs (`json-parser-15`, `agent-loop-300`, `request-handler-2k`) | 649,344; 738,656; 773,960 |
| native with `baml.http` | 4,581,864 |
| Go empty / `os.Exit` / `fmt` | 1,749,218 / 2,214,386 / 2,475,682 |
| Go with `net/http` | 8,103,042 |

The compiled object is small (7,496 bytes for return-42, 36,248 for the
JSON parser); the rest is the runtime that survives dead stripping: the
scheduler, the collector, the unwinder, `std`'s formatting and I/O, the
panic machinery with its symbolizer (`addr2line`, `gimli`), and whichever
natives the program names. `nm` lists 998 symbols in return-42.

Weak points: 573 KB for return-42 is mostly Rust `std` and the backtrace
symbolizer, which a `panic = "abort"` runtime keeps only to print a
backtrace on a runtime bug. libiconv is linked for one of the
dependencies and costs a dylib load on every start. Neither has been
trimmed.

## Not built

- A static or prebound link that avoids dyld's symbol binding.
- Lazy creation of the main stack or the first space.
- A build of the runtime without the panic symbolizer.
