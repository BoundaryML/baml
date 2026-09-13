# Rust tests that remain after native BAML migration

Reconciled: 2026-09-13

This is the completion ledger for Step 11 of the
[native BAML test migration survey](NATIVE_BAML_TEST_MIGRATION_SURVEY.md). It
accounts for every Rust test definition that remains in `crates/baml_tests`.
The reason in each row names the capability that a native BAML test cannot
observe. A BAML assertion may be able to repeat part of a test's behavior; that
does not replace a Rust test whose indispensable oracle is bytecode, host state,
an internal API, or elapsed time.

## Reconciled totals

`Active`, `Ignored`, and `Listed/compiled` are nextest counts for this macOS
target; the last column additionally counts source definitions selected only on
Windows.

| Slice | Active | Ignored | Listed/compiled | Cross-platform source definitions |
|---|---:|---:|---:|---:|
| Hand-written integration binaries | 210 | 17 | 227 | 234 |
| Library and generated compiler harness | 816 | 4 | 820 | 820 |
| **All surviving `baml_tests` Rust tests** | **1,026** | **21** | **1,047** | **1,054** |

The seven-definition difference in the integration row is intentional.
`shell.rs` has separate Unix and Windows definitions for
`exec_failing`, `exec_with_args`, `exec_stderr`, `exec_with_cwd`,
`exec_with_stdin`, `exec_with_timeout`, and `shell_with_options`; this macOS
checkout compiles only the Unix side. The 28 definitions in
`src/compiler2_tir/package_interface.rs` are behind `#[cfg(any())]`, compile to
zero nextest entries, and are not counted as survivors.

## Capability key

- **HOST** — the contract is at the Rust/VM boundary: `BexExternalValue`,
  `EngineError`, logger drains, opaque handles, host-selected working directory,
  or direct engine/session state.
- **COMP** — the oracle is compiler/VM structure unavailable through the native
  test API: HIR/TIR/MIR/PPIR, bytecode, pool slots, type metadata, formatter
  output, or serialized artifacts.
- **INC** — the test controls more than a fresh source compilation: incremental
  database history, multi-root/package identity, linker/relinker state, mounted
  source-less units, or deliberately malformed artifacts.
- **OS** — the test creates or inspects host filesystem, environment, process,
  TCP/UDP/HTTP, or stdin/stdout state unavailable inside a hermetic BAML test.
- **TIME** — correctness depends on a host clock, timeout, liveness, scheduling,
  resource-growth, or performance oracle.
- **IGN** — this is an intentionally ignored future-feature specification or a
  manual benchmark. Native BAML tests have no checked-in `#[ignore]` equivalent.

## Hand-written integration survivors

Every row is homogeneous unless its reason explicitly gives a split. Counts are
source definitions, so the `shell.rs` row includes both target-specific sides.

| File | Definitions | Ignored | Capability and indispensable Rust-only oracle |
|---|---:|---:|---|
| `backtick_fmt_value_preservation.rs` | 1 | 0 | **COMP:** runs the formatter and compares exact decoded/backtick source values and bytes. |
| `baml_src.rs` | 1 | 0 | **COMP:** compile-checks the live Prompt Fiddle asset through a cross-workspace `include_str!`; copying its contents into the corpus would no longer test that owned file. |
| `bigints.rs` | 6 | 0 | **HOST:** injects host `Int`/`BigInt` values, including values that cannot be BAML literals, and checks call-boundary range/member selection. |
| `byte_strings.rs` | 1 | 0 | **HOST:** injects a host byte-string external value and checks host-to-VM conversion. |
| `cancel_cascade.rs` | 1 | 0 | **HOST/TIME:** checks the root host result while an unobserved child fails; the native runner separately reports that child as `testing::unhandled_spawn_error`. |
| `convert_boundary_regressions.rs` | 2 | 0 | **COMP:** checks `ProjectDatabase::get_bytecode`'s typed `LoweringError` gate and emitted throws-type metadata, not merely source diagnostics/behavior. |
| `emit_determinism.rs` | 5 | 0 | **COMP/INC:** compares serialized bytecode/artifacts across repeated, parallel, prefix-spliced, and mounted-package emit paths. |
| `env.rs` | 6 | 0 | **OS/HOST/COMP:** mutates the process environment, verifies late binding/provider initialization against host values, and pins three bytecode forms. |
| `errors.rs` | 1 | 0 | **HOST:** snapshots the exact escaping host traceback, including frames and source lines. |
| `exceptions.rs` | 8 | 0 | **HOST/COMP:** mutates bytecode for a stale slot and checks escaping `EngineError`/external-value unions, exit/panic distinctions, and exact host traceback rendering. |
| `floats.rs` | 5 | 0 | **HOST:** supplies host numeric external values and verifies coercion and union-member selection at the ABI boundary. |
| `fs.rs` | 6 | 0 | **OS:** inspects symlink metadata, stored relative targets, and exact Unix modes. The rejected-mode case also proves the host file mode was not mutated. |
| `hir_ty_package_interface.rs` | 7 | 0 | **COMP/INC:** constructs, serializes, mounts, and queries source-less `PackageInterface` values and reserved built-in resolution directly. |
| `http.rs` | 8 | 1 | **OS/COMP/TIME/IGN:** seven active cases own mock/raw TCP peers and assert bytecode, response consumption, traceback, or timeout behavior; the ignored case is a pending external-op error contract. |
| `incremental_typing_repro.rs` | 5 | 2 | **INC/IGN:** three active cases edit a live database or re-exec crash repros; the two ignored child entrypoints are invoked by those parent subprocess tests. |
| `instantiation_interning.rs` | 1 | 0 | **COMP:** checks concrete instantiation pool interning and object identity. |
| `interface_body_anonymity.rs` | 2 | 0 | **COMP/HOST:** inspects pool slots/name maps and proves anonymous bodies cannot be resolved through the host engine entry-point API. |
| `interfaces.rs` | 4 | 2 | **HOST/IGN:** two active cases inspect the concrete external instance field shape; two ignored cases preserve unsupported first-class interface-method dispatch specifications. |
| `interfaces_associated_types.rs` | 6 | 1 | **COMP/IGN:** four tests inspect VM type metadata, one invokes the formatter, and one ignored test preserves a pending runtime type-variable guard contract. |
| `io.rs` | 2 | 0 | **COMP:** snapshots the exact input opcode forms without invoking interactive stdin. |
| `json_auto_derive.rs` | 2 | 0 | **COMP:** inspects emitted bytecode, including the Rust-only `show_auto_derive` compiler option. |
| `link_fail_closed.rs` | 1 | 0 | **INC:** corrupts a compilation unit and checks that the linker rejects a named-function offset. |
| `link_units_oracle.rs` | 5 | 0 | **INC/COMP:** compares separately linked serialized units byte-for-byte with full compilation. |
| `memory_leak_audit.rs` | 3 | 3 | **HOST/TIME/IGN:** manual RSS/object-retention audits over long-lived editing sessions. |
| `mounted_package_calls.rs` | 1 | 0 | **INC/COMP:** constructs, serializes, and mounts a `PackageInterface` artifact with reserved native/intrinsic declarations, then verifies that an ordinary artifact cannot claim compiler-intrinsic trust. |
| `mounted_package_parity.rs` | 10 | 0 | **INC/COMP:** compares source and source-less mounted-unit behavior, metadata, diagnostics, bytes, overlaps, and dependency-link failures. |
| `multi_root_emit.rs` | 5 | 0 | **INC/COMP:** builds multiple workspace roots in one database and inspects per-root programs, isolation, and shared stdlib prefix identity. |
| `narrow_bind.rs` | 1 | 0 | **COMP:** pins the `narrow_bind` opcode in emitted bytecode in addition to runtime behavior. |
| `native_working_dir.rs` | 1 | 0 | **HOST/OS:** installs native sys-ops with a host-selected working directory and proves the process CWD itself is unchanged. |
| `net.rs` | 10 | 0 | **OS/TIME/COMP:** creates host TCP/UDP peers, controls raw bytes/failure/cancellation/timeout ordering, and pins selected network bytecode. |
| `optimization.rs` | 9 | 0 | **COMP:** snapshots exact constant folding and non-folding in emitted bytecode. |
| `optimizer_binary_prefix.rs` | 2 | 0 | **COMP:** inspects optimized instruction prefixes, stack use, ordering, and unwind behavior. |
| `optimizer_effects.rs` | 3 | 0 | **COMP:** compares optimizer levels, preserved effects/panics, stale-proof invalidation, and exact instruction budgets. |
| `optimizer_stack.rs` | 2 | 0 | **COMP:** compares all optimization levels and pins optimized instruction/local counts and runtime behavior. |
| `package_identity.rs` | 10 | 0 | **INC/COMP:** constructs package graphs directly and checks internal identity, provenance, collision, emit, and impl-visibility rules. |
| `parallel_scaling.rs` | 1 | 1 | **TIME/IGN:** manual multi-thread scaling benchmark with host wall-clock measurements. |
| `prompt_tag_e2e.rs` | 2 | 0 | **OS/HOST:** owns a wiremock server and verifies orchestrator streaming requests and rendered output-format traffic. |
| `prompt_tag_runtime.rs` | 3 | 0 | **HOST:** inspects the host `PromptAst` ADT, media kinds, and raw JSON role metadata that cross the VM boundary. |
| `reflect_call_any.rs` | 2 | 0 | **HOST/COMP:** checks an inferred runtime class crossing sys-op/host boundaries and mutates bytecode to simulate an invalid dynamic callee return. |
| `relink_oracle.rs` | 8 | 0 | **INC/COMP:** edits an incremental database and compares relinked bytes, dirty-unit selection, and actual previous-unit reuse with a fresh compile. |
| `rule_method_resolution_total.rs` | 1 | 0 | **COMP/INC:** fabricates an invalid rule missing a required method and verifies a total internal error path that valid BAML source cannot construct. |
| `runtime_classes_and_composites.rs` | 2 | 0 | **HOST:** checks runtime-compiled/anonymous class instances crossing as opaque host handles with no statically addressable class. |
| `runtime_package_compile.rs` | 3 | 0 | **HOST:** observes init through logger-drain state and checks the structured `CompilationError` external-value layout and live callback logs. |
| `runtime_render_identity.rs` | 1 | 0 | **HOST:** observes a deferred recursive-generic output-format failure only when it escapes to the host. |
| `runtime_session.rs` | 6 | 0 | **HOST/INC/TIME:** checks structured host causes, busy/cancel lease recovery, escaped-handle provenance/GC, artifact bounds, and 500-evaluation latency. |
| `shell.rs` | 27 | 0 | **OS/HOST/TIME/COMP:** controls real processes, argv, cwd, stdin, stdout/stderr bytes, PID, cancellation, and timeouts, and pins a pipe bytecode form; seven definitions are target-specific alternate bodies. |
| `sort_compare.rs` | 1 | 0 | **TIME:** uses a 60-second host deadline to guard the 10k-element native primitive-array fast path. |
| `spawn_parallel.rs` | 1 | 0 | **TIME:** host elapsed time distinguishes concurrent sleeping from sequential execution. |
| `spawn_semantics.rs` | 8 | 0 | **HOST/TIME/OS:** one parent-cancellation case and two detached-liveness cases need host timeouts; four unobserved-error cases need a host result oracle that suppresses the native runner's extra failure; one owns a loopback server. |
| `spawn_specialization.rs` | 4 | 0 | **COMP:** inspects generic versus specialized arithmetic opcodes and transitive captured-cell marking. |
| `stdlib_prefix_equivalence.rs` | 2 | 0 | **COMP/INC:** proves prefixed compilation byte-identical at each optimization level and compares whole-project diagnostic gates. |
| `stdlib_splice_guard.rs` | 1 | 0 | **INC/COMP:** corrupts a base artifact slot and verifies fail-closed prefix splicing. |
| `streaming_composite_clients.rs` | 4 | 0 | **OS/TIME:** scripts SSE disconnect/delta timing with wiremock and verifies retry, fallback, and round-robin stream policy. |
| `structured_prompt_requests.rs` | 1 | 0 | **OS/HOST:** re-execs the test in an isolated process to control a late-bound environment variable during provider preview rendering. |
| `toml.rs` | 1 | 1 | **IGN:** preserves a pending VM `continue`/`break` regression; it cannot be an always-run native test while the behavior is broken. |
| `unit_reference_records.rs` | 6 | 0 | **COMP/INC:** inspects emitted compilation-unit reference records, layout flags, and sorted/deduplicated names. |
| `wildcard_expression_holes.rs` | 6 | 6 | **IGN:** future-feature specifications for unsupported expression-position `_` inference; native tests have no ignore facility. |
| **Total** | **234** | **17** | All source definitions are assigned above. |

### Mixed-file splits worth preserving

- The five unobserved-error cases are
  `cancel_cascade::fire_and_forget_error_does_not_replace_unrelated_await` and
  the four `spawn_semantics` cases whose names start with
  `never_awaited_`/`racing_never_awaited_`. Direct native ports returned the
  desired root values but also produced `testing::unhandled_spawn_error`, so
  they would not be equivalent passing tests.
- `fs::fs_chmod_rejects_out_of_range_mode` overlaps the native range-error
  assertions only at the language level. Its retained assertion is that the
  rejected operation left the host file's mode exactly `0o600`.
- The two ignored `incremental_typing_repro` definitions are not redundant
  dormant tests: the active parent tests launch those exact child entrypoints
  in subprocesses to turn a compiler crash/abort into an assertable status.
- Fifteen ordinary mounted-call behavior tests moved to native BAML via
  `reflect.Package.compile(files, packages = ...)`. The one survivor in
  `mounted_package_calls.rs` deliberately builds a source-less artifact with a
  reserved native call, which `reflect.Package.compile` cannot create from
  valid source.

## Library and generated survivors

These are grouped at the layer whose internal representation they pin. A native
BAML test may cover a user-visible consequence, but it cannot replace these
representation, query, snapshot, or host-harness contracts.

| Group | Compiled | Ignored | Why it remains Rust-only |
|---|---:|---:|---|
| Generated project tests | 292 | 0 | 112 exact rendered diagnostic snapshots; 8 PPIR recovery snapshots; 15 incremental-edit and 15 node-reuse checks; 4 lossless-CST checks; 138 formatter goldens. Reflection does not expose rendered source diagnostics, CST identity/trivia, PPIR, or formatter output. |
| Compiler emit | 16 | 0 | Exact emitted `Program`, bytecode, slots, and serialization invariants. |
| Compiler HIR | 72 | 0 | HIR scopes, lookup/query identity, source maps, and incremental behavior. |
| Compiler HIR types | 5 | 0 | Internal type/package-interface lowering and identity. |
| Compiler MIR | 23 | 0 | Selected MIR shapes and optimizer/lowering invariants after deleting the redundant tutorial goldens. |
| Compiler PPIR | 3 | 0 | Focused ordering, artifact compatibility, and query-equivalence internals. |
| Compiler TIR | 338 | 4 | Typed-IR inference, recursion, effects, stream expansion, and internal type snapshots/matrices after consolidation. The four ignored cases are pending compiler specifications. |
| Corpus snapshot driver | 4 | 0 | Snapshot-policy/inventory, exact rendered diagnostics, selected compiler phases, formatter idempotency, and the shared single-compile harness. |
| Engine harness | 5 | 0 | Host logger, external values, exact bytecode display, and engine-call behavior. |
| Incremental harness | 26 | 0 | Parser/query mutation sequences, event history, node reuse, and invalidation. |
| String compatibility | 1 | 0 | Passes a VM-produced string to host `serde_json` and verifies wire compatibility. |
| Type-spec harness | 34 | 0 | Internal coherence/type tables, fixture sweep, and pattern/type-checker corpus invariants. |
| Test utility | 1 | 0 | Directly tests the Rust incremental-parser string-edit helper. |
| **Total** | **820** | **4** | **816 active library/generated entries remain.** |

## Mechanical reconciliation

The integration table was checked against the live source with a read-only
`diff` between per-file counts extracted from this table and counts produced by
`rg` for `#[test]`, `#[tokio::test]`, and parameterized Tokio test attributes.
It reconciles exactly: 57 files, 234 definitions, 17 ignored, and no missing or
extra file.

The compiled inventory was checked with:

```sh
cargo nextest list -p baml_tests --all-features --tests --message-format json
```

On this macOS checkout it reports exactly 1,047 tests: 820 library/generated
and 227 integration entries. Of those, 21 are ignored (4 library and 17
integration), leaving 1,026 active Rust tests. Adding the seven Windows-only
source definitions produces the 1,054 cross-platform ledger total above.
