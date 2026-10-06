# Build-time program pruning

`baml pack Main` links an image rooted at `Main`. With multiple `--function`
arguments, every selected function is a root. Unreachable declarations and
constants are omitted, while the selected program keeps its original behavior.

```baml
function Main() -> string { "hello" }
function Unused() -> string { "unused" }
```

```sh
baml pack Main --file main.baml --output small
baml pack Main --file main.baml --no-prune --output complete
```

Both executables print `"hello"`. The second includes the complete program and
provides an execution oracle and a diagnostic escape hatch.

## Correctness contract

The shared linker validates and binds immutable compilation units and package
records, then marks original declaration identities, assigns compact output
placements, relocates retained references, rebuilds type tags/switch tables and
package tables, and validates the selected `Program`. It never edits input
bucket arrays or removes entries from an already linked image. Missing output
placements are build errors. Interned generic values keep their canonical
identity, including copies appearing in initialization tails.

Selection retains complete class layouts and every inherent method, including
cleanup. Reached interfaces retain all implementation rules across the link
set, provided/default bodies, receiver patterns, bounds, associated bindings,
and method frames. Implicit structural defaults retain their pooled function
operands alongside ordinary interface defaults. This intentionally does not attempt precise receiver
analysis. All initializer/test tails, let cells, package edges, and initializer
execution order are preserved.

Native edges are classified by `Function.native_key` and `SysOp.path()`, not
function display names. `crates/baml_linker/src/native-keys.txt` is a closed
list of audited operations. The build script derives class-marshalling
requirements from the same builtin declarations used to generate Rust glue.
`native.rs` adds handwritten lookups and conservative constructor/error groups.
Map operations also retain `baml.Hash`, `baml.hash.Hasher`, the native
`DefaultHasher` class and equality dispatch: Rust continuations can invoke
custom nested key hashing/equality without a bytecode call edge. Declared trace
hooks retain `CallHooked` global operands; currently unclassified trace natives
use the ordinary complete-image fallback.

Changing a native implementation requires reviewing its contract here;
new/unclassified native keys retain the full image. Stale audited keys fail the
build. Native dependency lookup cannot silently omit a missing declaration.

Ordinary reflection, runtime compilation, SAP, and other currently unclassified
native behavior retain the complete image. Pure `reflect.Type` formatting is
classified independently; formatting a type does not require reflection over
the complete package surface.

Complete compile-time package-interface blobs are used unchanged to bind and
authenticate input dependencies and remain in reusable cache entries. A pruned
image omits those blobs entirely: they describe declarations no longer present.
Full-image fallback preserves them. No input dependency fingerprint is replaced
by a fingerprint of a partial package.

SDK generation uses `LinkRoots::HostSurface`, which keeps every callable and
exposed type visible from the root package's host viewpoint. This includes
functions invoked by name through the bridge, beyond generated wrappers. Under
the current bridge contract this also exposes stdlib reflection/runtime
compilation, so SDK images generally fall back to a complete program. There is
no claim of an SDK startup improvement until that contract becomes narrower or
reflection reachability becomes explicit.

The Cargo-built wrapper selects its entry point through the same linker.

Pack keeps the existing complete whole-program cache for target/signature
resolution and reuses complete package outputs for selected linking. It never
writes a selected image under run/check's whole-program cache key. Signature
resolution uses `UserFunctionCatalog`, so packing does not execute initializers.

## Verification

- `cargo test -p baml_linker`: placement, relocation, metadata, immutable input,
  generic-value interning, malformed input, and host-root contracts.
- `cargo test -p baml_tests --test pruned_program`: paired execution after artifact
  round-trip for JSON/formatting overrides, structural defaults, custom hashing,
  non-string map keys, callback mutations, equality/GC, CSV, regex, datetimes, media, generics,
  cleanup, file/pipe/TCP interfaces, system randomness, reflection, runtime
  compilation, and SDK invocation by name. Existing
  offline native assertion fixtures are converted to callable roots using the
  parser; registration tails cannot accidentally retain the entire fixture.
- `cargo test -p baml_cli --lib pack_command::tests`: target resolution, signature
  validation, packaging flags and artifact behavior.
- `cargo test -p bex_heap --lib`: heap/GC behavior and a regression for creating
  initial root permits after the caller exhausts Tokio's cooperative budget.

The execution corpus exposed a preexisting constructor deadlock: blocking on an
uncontended async mutex can still stall a Tokio task when cooperative scheduling
requires a yield. Engine construction now registers its initial root holders
through exclusive manager ownership, then shares the manager. The async path and
the construction path share the same registration function and root accounting.

The native JSON test deliberately deletes a retained `ToJson` rule. The resulting
program still passes structural validation but produces different output, proving
that the behavioral oracle catches a silent fallback. Ordinary native tests also
require the selected image to be smaller; a conservative full-image fallback
cannot masquerade as a successful pruning test.

External release smoke checks pack different entry points through one persistent
cache, then run an entry point removed from an earlier packed image. Repeating a
warm pack must produce identical executable bytes. This verifies that selected
images do not pollute the full-program cache.

## Measurement contract

Compare two fresh packs of the same source and entry points with the same native
host and build settings, one default and one `--no-prune`. Run each binary in a
fresh process, measuring immediately before process launch until after exit.
Include loader work, artifact decoding, engine construction, execution, child
commands, telemetry shutdown and destruction. Use matching executable basenames,
an isolated home, a fixed identical CLI child, and disabled update checks.

Run the production comparison with:

```sh
python3 scripts/runtime-startup/measure-pruning.py \
  --compiler target/release/baml-cli --host target/release/baml-pack-host \
  --child /absolute/path/to/fixed/baml-cli \
  --output target/pruning-production --runs 100
```

The harness packs an empty program, a program exercising defaults/closures/JSON,
and the real BAML wrapper, then measures wrapper version and child-command paths.

Report median/nearest-rank p95 from shuffled paired runs, peak root-process RSS,
CPU time, full binary and artifact size, and retained function/type counts.
Warm executable/file caches consistently, test telemetry off and default local
recording separately, and retain raw samples and exact artifact/host hashes.
Do not benchmark while compiler/build workers are active. These are warm-cache
launch measurements on one machine, not cold disk or cross-platform claims.

## Production measurement — 2026-10-04 (before the October 6 canary sync)

Apple M2 Max, macOS 15.6.1, Rust 1.98.0. Both variants use the same native host,
including the compiler, built with the repository’s release profile: opt-level 3,
fat LTO, one codegen unit, panic unwind, stripped symbols. BAML compilation uses
`OptLevel::Two`. These measurements use the production linker and normal artifact
validation, with five warmups and 100 fresh launches per cell (1,600 measured
launches total). No samples are discarded.

The source is the production pruning working tree based on
`843837715c15adfaa65666b34588ea270cb6ed0f`; exact compiler, host, fixed CLI child,
harness and executable hashes are saved in [the measurement summary](pruning-20261004.json).
[Raw samples](pruning-20261004.csv) include timing, CPU, RSS, faults and available
OS counters. Millisecond columns end in `_ms`; memory/I/O columns end in `_bytes`.
An optional counter of `-1` means unavailable. RSS is root-process peak RSS,
not simultaneous aggregate memory across the process tree.

With default local telemetry:

| Execution | Full median / p95 (ms) | Pruned median / p95 (ms) | Full median peak RSS (MiB) | Pruned median peak RSS (MiB) |
|---|---:|---:|---:|---:|
| Empty program | 20.63 / 36.19 | 8.67 / 12.25 | 30.48 | 11.88 |
| Defaults, closures, JSON, errors | 20.76 / 26.55 | 8.87 / 10.06 | 31.24 | 12.78 |
| Wrapper `--version` | 26.62 / 31.12 | 14.81 / 16.19 | 32.61 | 15.33 |
| Wrapper `run --help` | 23.96 / 37.03 | 14.25 / 16.16 | 32.48 | 14.95 |

With telemetry disabled:

| Execution | Full median / p95 (ms) | Pruned median / p95 (ms) |
|---|---:|---:|
| Empty program | 18.72 / 19.95 | 7.90 / 8.70 |
| Defaults, closures, JSON, errors | 19.02 / 20.82 | 8.08 / 9.21 |
| Wrapper `--version` | 24.63 / 26.53 | 13.84 / 16.04 |
| Wrapper `run --help` | 21.95 / 24.84 | 13.34 / 14.37 |

Image reduction (full → pruned):

| Program | Objects | Functions | Types | Serialized Program bytes |
|---|---:|---:|---:|---:|
| Empty | 4,127 → 562 | 1,662 → 330 | 513 → 110 | 2,928,231 → 376,772 |
| Features | 4,133 → 568 | 1,666 → 334 | 514 → 111 | 2,932,156 → 380,198 |
| Wrapper | 4,439 → 1,026 | 1,713 → 495 | 517 → 136 | 3,092,582 → 612,482 |

Types count complete class, interface, enum and alias declarations; functions
include bytecode and native declarations. Constants contribute to object counts.

The wrapper retains **444 of 1,661 stdlib functions** and **132 of 513 stdlib
types**. All rules of each retained interface are kept: 94 implementation rules
remain, compared with 243 in the full program. Conservative interface retention
still allows about 73% of stdlib functions and 74% of stdlib types to be omitted.

| Executable | Full size (MiB) | Pruned size (MiB) |
|---|---:|---:|---:|
| Empty | 31.53 | 29.07 |
| Features | 31.53 | 29.07 |
| Wrapper | 31.72 | 29.32 |

The current bridge’s broad callable surface keeps SDK images complete. These
performance numbers apply to packed applications; no SDK pruning gain is claimed.

The final verification passed 43 linker tests, 40 pack tests, 99 heap tests,
and 19 behavioral tests. Native and JSON/formatting fixture corpora run at both
BAML optimization levels One and Two; other behavioral tests use Two. Core crates
and the execution-test target pass Clippy with warnings denied. Format checking,
release packing/execution, cache isolation and deterministic warm packing pass.
