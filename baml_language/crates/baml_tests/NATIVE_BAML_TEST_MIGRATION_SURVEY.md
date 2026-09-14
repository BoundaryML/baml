# Native BAML test migration survey

Survey date: 2026-09-12

Last mechanically reconciled: 2026-09-13

Repository revision: `fcb0b7d03c`

Scope: all active `baml_tests` and `baml_cli` tests; detailed per-test ledger for
`crates/baml_tests/tests/*.rs`; action audit for the supplied 139-entry
greater-than-three-second timing set

Status: implementation and fresh diff review complete

## Implementation checklist

This checklist tracks the implementation against the recommendation, not just
the survey. A box is checked only after the affected narrow nextest/insta run
passes; the final full-workspace validation is tracked separately.

- [x] Step 1: remove 86 tests already covered natively and one dead Rust test.
- [x] Step 2: land the 13 recommended cache/discovery/CLI/shebang reductions
  and the setup-cost improvements that do not reduce test definitions.
- [x] Step 3a: retain eight diagnostic-error PPIR recovery snapshots and remove
  the other 83 generated PPIR tests/snapshots.
- [x] Step 3b: remove the ten recommended redundant MIR tests/snapshots.
- [x] Step 3c: consolidate the five TIR families from 106 definitions to 29.
- [x] Step 4: apply the native migration deduplication rules before porting.
- [x] Step 5: migrate the remaining phase-1 all-native-file cases.
- [x] Step 6: migrate the remaining phase-1 interface cases.
- [x] Step 7: migrate phase-1 cases from mixed suites.
- [x] Step 8: migrate the nine library cases and the CLI
  `assert.approx_equal` case.
- [x] Step 9: migrate diagnostics supported by current reflection APIs.
- [x] Step 10: close the documented reflection gaps and migrate the remaining
  diagnostic cases.
- [x] Step 11: document why every surviving Rust test cannot be native.
- [x] Validation: run the native corpus, affected package suites, snapshot
  unreferenced check, and the full workspace nextest jobs.
- [x] Final review: have a fresh subagent inspect the complete diff and fix all
  confirmed misses, then rerun affected validation.
- [x] PR CI follow-up: repair the markdown, rustfmt, and Clippy pre-commit
  failures and replace the deleted `book_interfaces` workflow target with its
  native suite.
- [x] CodeRabbit follow-up: address all five inline findings, add mounted-
  interface regressions, rerun affected validation, and resolve the threads.
- [x] Second CodeRabbit implementation follow-up: complete defaulted associated
  bindings for mounted qualifier candidates and make the intentional E0139
  blanket-impl diagnostic inventory explicit before inspecting its facts.
- [x] CI performance follow-up: remove the standalone release-mode
  `trace_heap` leg; those tests remain covered by the normal workspace tranche.
- [x] Readability follow-up: replace escaped inline BAML source maps across
  compiler assertion and runtime-package utilities with backtick literals,
  then format, validate, review, and push the result.

Implementation started: 2026-09-12.

Final review completed: 2026-09-13.

Progress:

- PPIR policy now retains and passes exactly the eight planned recovery
  representatives; 83 generated tests and snapshots are gone.
- The ten planned MIR tests/snapshots are removed; all 23 remaining MIR tests
  pass and the 14 remaining snapshots match their assertions.
- The five planned TIR families are consolidated from 106 definitions to 29
  (101 snapshot assertions to 27), preserving every prescribed scenario label.
  The five narrow nextest/insta runs pass.
- The no-port ledger is complete: all 86 existing-native Rust duplicates and
  the dead import-only test are removed; the 493 cases in that checkpoint's
  targeted Rust/native validation passed (one loopback case required ordinary
  non-sandboxed networking).
- Step 2 is complete. The cache oracles are consolidated 4-to-1, the sampled
  no-op no longer builds a database, discovery/profile/skill setup is shared or
  made cache-honest as planned, and the recommended CLI/shebang layer removes
  nine definitions. Rename presentation is now a pure normal/verbose/quiet
  table with one retained generation subprocess; the affected narrow nextest
  runs pass. The nine-case phase-3A diagnostic check now reuses one namespaced
  database, and all 16 emit tests splice the embedded byte-identical stdlib
  prefix rather than recompiling it. The optional emit 16-to-9 consolidation
  remains deferred. The gross reduction is 13; the added presentation unit
  makes the net reduction for this layer 12.
- Phase 1 all-native-file migration is complete. After the three existing-native
  removals, all 81 remaining Rust definitions have native replacements; they
  become 77 native cases after the intended optimizer-pair collapses. Targeted
  native runs pass.
- The nine library candidates and CLI `assert.approx_equal` wrapper are
  migrated. Two string value/length pairs are combined as planned, so ten Rust
  definitions become eight native cases. The host `serde_json` compatibility
  assertion remains in Rust.
- All 21 language-observable `runtime_package_compile.rs` cases now live as
  native BAML tests. The three surviving Rust tests are intentionally host-only:
  they inspect logger drain state or the external structured-throw layout. The
  21 native cases and three Rust survivors pass their focused runs.
- Interface migration is complete. The 105 remaining phase-1 Rust definitions
  become 105 native tests; together with the nine interface cases handled in
  the all-native batch, this completes all 114 planned interface positives.
  All 321 phase-2 interface definitions become 317 native tests after four
  documented duplicate/near-duplicate merges. The native diagnostic selectors
  pass 317/317, including the full 36-accepted/144-rejected scalar arithmetic
  matrix; the 14 non-ignored Rust-only interface survivors also pass.
- Phase 2 migration is complete. The regression, backtick, wildcard, tagged-
  template, invalid-string, interface, and mixed-suite families now have native
  reflected-diagnostic coverage; their focused native runs pass, including
  successful-compilation warnings.
- The reflection fidelity work is implemented and focused validation passes.
  `reflect.Diagnostic` now preserves severity, compiler phase, separate headline
  and primary label, message highlights, primary/secondary annotations, and
  related information while retaining the legacy flattened message. Derived
  line/column and fully rendered source remain intentionally outside the API.
- The cross-package parity gaps found by direct native probes are closed.
  Runtime compilation now preserves E0139 orphan validation and mounted
  `requires` and required-method validation (including E0125 and E0113), local
  implementations for mounted foreign classes dispatch correctly, and
  explicit associated-type witnesses use the type checker's canonical,
  sequential loc-free bindings during method lowering and emit.
- Five mounted-interface native regressions now pass. Three close the original
  migration blockers and let the last two formerly parity-blocked
  `book_interfaces.rs` cases leave Rust (the third was already in the planned
  phase-2 count), so that Rust integration binary is removed completely. The
  two PR-review regressions pin dependent `Self.Item` bindings and E0113 for an
  omitted required method across `reflect.Package.compile`.
- Mixed-suite migration is complete. Its planned phase-1 tranche removes 163
  Rust definitions: 157 receive native replacements and six already had native
  coverage. The 128 newly authored cases in the final mixed-suite batch pass,
  as do all 65 reflected-diagnostic replacements removed from those Rust files.
  Session coverage is split across 25 minimal fixtures rather than repeating a
  large shared prelude.
- The final Rust-residue audit leaves 21 additional language-observable native
  cases: two caught throw/panic `ErrorContext` cases, zero-duration `sys.sleep`,
  one clean spawn-success lifecycle case, ten `spawn_array_race` cases, six
  `future_all_settled` cases, and one reflected invalid-filesystem-mode
  diagnostic. A second finite detached-spawn port was removed because the
  retained infinite-spawn Rust timeout is the stronger oracle. Five unobserved
  spawned-error cases remain in Rust because the native runner intentionally
  reports `testing::unhandled_spawn_error`.
- Fifteen ordinary mounted-package call tests are replaced by two native tests:
  one aggregate behavior test and one focused generic-return regression. The
  sole Rust survivor verifies the host-only privileged builtin-trust artifact.
  Supporting parity work also taught semantic value resolution to ignore
  type-erased link-only PPIR stubs in favor of mounted interface metadata.
- Three additional redundant Rust assertions were removed: a duplicate fully
  qualified bytecode case, a status-code assertion merged into the stronger
  foreign-class field-access bytecode test, and the finite detached-spawn timing
  assertion subsumed by the retained infinite-spawn timeout oracle.
- Every remaining Rust definition now has an explicit capability reason in the
  [companion survivor ledger](NATIVE_BAML_TEST_RUST_SURVIVORS.md), which is
  mechanically reconciled with the source inventory.
- Final diff review caught and restored five pre-existing native cleanup
  run-once/defer/per-instance tests that had been overwritten while adding the
  GC-path port. The cleanup namespace now contains all five original contracts
  plus the new migrated case.
- Final diff review also strengthened the migrated traceback assertions,
  preserved mounted interface requirements, associated-type metadata, and
  generic bounds through runtime compilation, made the cache and discovery
  reductions truly share their expensive setup, exercised structured reflected
  diagnostics through the host boundary, and removed one stale ignored TIR
  specification whose syntax no longer parses. Focused validation for every
  correction passes.
- The final Clippy follow-up keeps the richer diagnostic payload while boxing
  its optional details, reducing `RuntimeCompileDiagnostic` below Clippy's
  large-error threshold. It also updates the migrated discovery-cache assertion
  to use inline format capture. The full workspace/all-target/all-feature
  Clippy command passes.
- The second CodeRabbit pass closes the mounted existential qualifier-default
  gap. Loc-free candidates now fill exported defaults in declaration order with
  progressively pinned `Self` before qualifier proof, while rigid bounds keep
  defaults unfilled. Its regression pins an inherited `Root = string`; the
  mounted blanket test separately asserts that its deliberate orphan-rule error
  is the only diagnostic before inspecting the recovered implementation facts.
- The profiler value-copy tests remain in the all-feature workspace nextest
  tranche, but no longer trigger a separate release build after that tranche.
- The native compiler fixtures now use readable backtick source literals
  instead of escaped one-line strings across assertion, package-compilation,
  mixed-suite, and source-builder helpers. Nested templates retain literal
  backticks/interpolations and generated-source trailing newlines explicitly.
  This follow-up converted 528 ordinary-string program literals across 21
  BAML files. The full 449-file BAML check, the offline native corpus, and the
  1,564-test snapshot-owned CI tranche all pass; a fresh-agent semantic review
  found no payload changes beyond source-only whitespace normalization.
- Full validation is green after final review and PR follow-up: the offline
  native corpus selects 4,488 cases and reports 4,485 passes plus two expected
  tolerated failures (one fail-fast child is intentionally not executed), and
  the post-review snapshot-owned job passes all 1,564 active Rust entries with
  21 ignored and no unreferenced snapshots. The post-review complementary
  workspace job passes all 4,377 active entries with seven skipped. The PR
  follow-up also passes the repository's markdown/workflow hooks, CI-configured
  rustfmt and Clippy, and the three-case native Developer Docs gate. All five
  CodeRabbit findings have focused regression coverage or a stronger retained
  oracle, and all five review threads are answered and resolved.
  `git diff --check` and the `.snap.new` scan are clean.

## Outcome

The `2,628` figure is correct: it is the number of **active** all-feature
nextest entries across `baml_tests` and `baml_cli` at the surveyed baseline. The initial
`1,106` denominator was the cross-platform source inventory of the 95
hand-written `baml_tests` integration binaries, not the whole nextest job.

### Implemented result

The implementation goes beyond the conservative survey. It closes the mounted-
package parity gaps and the E0125 runtime-compile gap, then applies a complete
survivor and overlap audit. As a result, **878 active Rust entries leave through
native migration**: 868 integration entries, nine library entries, and the CLI
`assert.approx_equal` wrapper.

The complete all-feature nextest inventory is now **1,585 listed entries**:
1,564 active and 21 ignored. The active inventory is down **1,064** from 2,628.
The full unprofiled native universe grows from 3,770 to
4,522 selected cases (+752), matching the source-definition increase from 3,728
to 4,480. The default offline profile selects 4,488 of the current cases after
excluding 34 intentionally compile-only fixtures. Thus the logical test-count
reduction is **312**, not 1,064.

| Active slice | Baseline | Current | Removed |
|---|---:|---:|---:|
| `baml_tests` hand-written integration | 1,082 | 210 | 872 |
| `baml_tests` library/generated | 995 | 816 | 179 |
| `baml_cli` | 551 | 538 | 13 |
| **Total** | **2,628** | **1,564** | **1,064** |

The 1,064 removals partition without overlap:

- **878** Rust entries removed through native migration;
- **1** dead Rust entry;
- **170** compiler-test reductions: 83 generated PPIR, ten MIR, and 77 TIR;
- **15** other reductions: 12 net cache/discovery/CLI/shebang reductions (the
  survey's gross 13 is offset by one new pure generation-presentation unit) and
  three redundant Rust assertions found by the final overlap and PR audits.

Against the 878 Rust entries removed through migration, the native universe
grows by 752 selected cases after deduplication, table/matrix consolidation,
and the two mounted-interface review regressions, a net reduction of 126 across
that part of the program. Adding the dead test, 170 compiler reductions, and 15
other reductions gives the exact **312 logical
tests removed**.

The final integration-layer disposition is:

| Integration action | Definitions | Share | Meaning |
|---|---:|---:|---|
| Delete: already covered natively | 86 | 7.8% | Stronger existing native tests make a new port unnecessary |
| Delete: dead | 1 | 0.1% | Import-only smoke test with no behavior |
| Delete: redundant assertion | 3 | 0.3% | Duplicated bytecode/assertion/timing coverage |
| Add a phase-1 native port | 379 | 34.3% | BAML-observable behavior or successful compilation |
| Add a phase-2 native diagnostic port | 403 | 36.4% | Use reflected compiler diagnostics |
| Outside native migration | 234 | 21.2% | Host, compiler/VM internals, OS/I/O, concurrency, optimizer matrices, or ignored-test support |
| **Total** | **1,106** | **100%** | |

Across the full original active inventory, the corrected capability split is:

| Full active-test disposition | Tests | Share |
|---|---:|---:|
| Phase 1: native behavior/build or existing native coverage | 474 | 18.0% |
| Phase 2: reflected diagnostics | 404 | 15.4% |
| Dead | 1 | <0.1% |
| Redundant duplicates | 3 | 0.1% |
| Outside native migration | 1,746 | 66.4% |
| **Total** | **2,628** | **100%** |

Thus **872 active integration entries are removed** and 210 active macOS
entries remain. The cross-platform source inventory has 234 surviving
definitions across 57 files: 17 are ignored specifications and seven are
Windows-only, so macOS lists 227 of them and selects 210. The
[companion Rust-survivor ledger](NATIVE_BAML_TEST_RUST_SURVIVORS.md) gives the
non-native capability reason for every remaining definition.

Three useful answers to “how many tests can we delete?” are now concrete:

- **1,064 active nextest entries were removed** from the original 2,628;
- **312 logical tests were eliminated net** after adding 752 native cases;
- **90 integration tests required no new replacement at all** because 86 had
  stronger native coverage already, one was dead, and three were redundant.

The inventory counts source-defined cases after expanding the macros in
`map_aliasing.rs`, counts ignored tests, and counts both sides of target-specific
`cfg` declarations. At the baseline revision, macOS listed 1,099 integration
tests because seven Windows-only `shell.rs` definitions were not built. The
1,106 total is the useful cross-platform migration denominator. It excludes the
package's library tests and the generated project/corpus snapshot tests.

### Relationship to the 2,628-test CI count

The 1,106 source-definition count is not the whole snapshot-job denominator.
At the surveyed baseline revision, the command below listed 2,650 tests and
selected 2,628 active tests after excluding 22 ignored cases:

```sh
cargo nextest list -p baml_tests -p baml_cli --all-features --message-format json
```

| Package/slice | Active | Ignored | Listed total |
|---|---:|---:|---:|
| `baml_tests` hand-written integration binaries | 1,082 | 17 | 1,099 |
| `baml_tests` library/generated harness | 995 | 5 | 1,000 |
| `baml_cli` | 551 | 0 | 551 |
| **Total** | **2,628** | **22** | **2,650** |

The cross-platform source inventory has 1,106 integration definitions rather
than the 1,099 listed on macOS because it also counts seven Windows-only
`shell.rs` cases. The migration tables below initially focus on the hand-written
integration binaries because they contain the per-test BAML compiles,
`baml_test!` wrappers, and interface diagnostic helpers under discussion. The
library/generated and CLI slices are audited separately below so this scope
choice is explicit.

The integration action totals reconcile as follows:

| Existing native | Phase-1 ports | Phase-2 ports | Rust | Dead | Redundant | Total |
|---:|---:|---:|---:|---:|---:|---:|
| 86 | 379 | 403 | 234 | 1 | 3 | **1,106** |

The final active nextest slices reconcile directly:

| Active slice | Baseline | Current | Removed |
|---|---:|---:|---:|
| `baml_tests` integration binaries | 1,082 | 210 | 872 |
| `baml_tests` library/generated harness | 995 | 816 | 179 |
| `baml_cli` | 551 | 538 | 13 |
| **Total** | **2,628** | **1,564** | **1,064** |

### Audit of the previously omitted 1,546 active tests

#### `baml_tests` library/generated harness: 995 active

| Category | Total | Active | Ignored | Native disposition |
|---|---:|---:|---:|---|
| Generated `projects/` harness | 375 | 375 | 0 | Not native; generated PPIR is separately reducible |
| Corpus compiler snapshots and policy | 4 | 4 | 0 | Keep |
| Compiler/internal unit tests | 606 | 601 | 5 | Not native; MIR/TIR contain separate redundancy |
| Whole-test native candidates | 9 | 9 | 0 | Port: eight phase 1, one phase 2 |
| Rust-only or partial-port engine/string tests | 6 | 6 | 0 | Retain or split; Rust assertion remains |
| **Total** | **1,000** | **995** | **5** | |

The generated 375 consist of:

- `broken_syntax`: 48 = 21 diagnostic snapshot/tier cases, 12 incremental
  edit checks, 12 node-reuse checks, and three lossless-CST checks;
- `diagnostic_errors`: 327 = 91 PPIR snapshots, 91 diagnostic
  snapshot/tier/span cases, three incremental checks, three node-reuse checks,
  one lossless-CST check, and 138 formatter snapshot/idempotence checks.

The 606 compiler/internal tests consist of emit 16, HIR 72, HIR-ty 5, MIR 33,
PPIR 3, TIR 416, incremental 26, type-spec 34, and utilities 1. Their assertions
are intentionally below the language-observable boundary. All five ignored
library tests are TIR placeholders or stale-syntax cases.

The nine credible whole-test native ports are:

- phase 1 engine cases:
  `narrowed_function_value_reorders_named_optionals`,
  `narrowed_function_value_as_concrete_call_argument`, and
  `narrowed_function_value_as_generic_call_argument`;
- phase 1 string cases:
  `quoted_string_escape_sequences_decode_to_exact_bytes`,
  `quoted_string_escape_lengths_match`,
  `escaped_backslash_at_string_boundary`,
  `escaped_backslash_boundary_lengths`, and
  `replace_all_with_backslash_arguments`;
- phase 2:
  `invalid_strings_still_produce_errors`, whose current contract is only that
  three invalid snippets each produce an error.

Port the two exact-value/length string pairs as two combined native matrices,
not four one-for-one tests. This is the source of two of the 11 hard-exact
collapses counted below. Keep `json_body_is_parseable_by_serde` in Rust: its
meaningful contract is compatibility with the host `serde_json` parser.

There are no *additional native-coverage* deletion candidates in this slice.
The closest cross-language overlap,
`compiler2_tir::array_rest::rest_binding_generic_element_type`, pins the
absence of legacy/type diagnostic text in Rust; the same-named native execution
case is not a full replacement. That says nothing about redundancy among the
Rust compiler tests themselves; those results are below.

The 112 generated diagnostic snapshot cases (21 `broken_syntax` plus 91
`diagnostic_errors`) are a later experiment pool, not part of the nine counted
ports. They also pin rendered severity, phase, spans/path, trivia-tight spans,
and parse-versus-semantic tier invariants that reflection does not currently
expose losslessly.

#### `baml_cli`: 551 active

One CLI test becomes a native migration candidate after the duplicate audit:
`test_assert_approx_equal_accepts_float_tolerance` only adds the language-level
`assert.approx_equal` success case; generic CLI pass status and aggregate
summary are already exercised by other `baml test` E2Es. Move that assertion to
the native corpus and remove the Rust wrapper. The other 550 are outside the
whole-test native boundary before the separate cleanup below:

| CLI category | Active |
|---|---:|
| Command/project/run/test/pack orchestration | 185 |
| Describe/help/paint/output/reporting | 133 |
| Bytecode/diagnostic cache and file signatures | 75 |
| Telemetry/auth/feedback | 37 |
| Agent/skill behavior | 18 |
| True process-level E2E contracts | 102 |
| Existing `baml_corpus::baml_test` native-corpus driver | 1 |
| **Total** | **551** |

The remaining language-flavored CLI E2Es still assert exit codes, stdout/stderr,
filesystem/project discovery, caching, packaging, filtering, or reporting.
Keep the single corpus wrapper: it is how CI runs all native BAML tests.

## Survey-time independent compiler-test redundancy audit

This audit answers a different question from native portability: which tests
inside the surveyed baseline's 1,787-entry non-native remainder are duplicated
or over-granular?
Here, “removed” means a net reduction in active Rust test definitions. For TIR,
most of that reduction comes from combining related fixtures into themed,
table-driven snapshots, not from abandoning the semantic cases. The TIR bounds
are implementation proposals: equivalence is not proven until the combined
fixtures are written, their labeled output is reviewed, and nextest/insta pass.

| Layer | Baseline active tests | Snapshot assertions | Conservative reduction | Recommended reduction | Policy ceiling |
|---|---:|---:|---:|---:|---:|
| Generated diagnostic-error PPIR | 91 | 91 | 81 | 83 | 91 |
| MIR unit tests | 33 | 24 | 8 | 10 | 15 |
| TIR unit tests | 411 | 197 | 68 | 77 | 85 |
| **Total** | **535** | **312** | **157** | **170** | **191** |

The ordinary coverage-preserving planning range is **157–178**: the
conservative 157; the recommended 170; or 178 if eight additional
cross-suite-subsumed TIR cases are removed while retaining the recommended
PPIR and MIR goldens. The 191 ceiling additionally removes every
diagnostic-error PPIR golden and five MIR goldens whose behavior is covered
elsewhere but whose exact error-recovery/lowering text is otherwise unique. It
is a policy choice, not the recommended plan.

| Combined plan | Native entries leaving | Compiler cleanup | Other redundancy/setup cleanup | Projected entries leaving | Projected active entries left | Net logical reduction |
|---|---:|---:|---:|---:|---:|---:|
| Conservative cleanup | 841 | 157 | 9 | 1,007 | 1,621 | 264 |
| **Recommended** | **841** | **170** | **13** | **1,024** | **1,604** | **282** |
| Expanded TIR cleanup | 841 | 178 | 13 | 1,032 | 1,596 | 290 |
| Architecture-dependent candidates | 841 | 170 | 27 | 1,038 | 1,590 | 296 |

The last column uses 98 native-program net reductions for the conservative row
and 99 for the other rows. “Other redundancy/setup cleanup” is nine at the
conservative boundary (cache -2, discovery -1, CLI -6), 13 at the recommended
layered-dedup/cache 4-to-1 boundary, and 27 if the seven emit and seven pack/run
candidate consolidations described below also prove clean in implementation.
The much larger “entries leaving” column is useful for nextest runtime and
inventory size, but native BAML tests remain real test cases inside the
already-counted corpus driver.

### Generated diagnostic-error PPIR: keep 8, remove 83

There are exactly 91 `projects/diagnostic_errors` projects, 91 generated
`test_03_ppir` functions, 91 `03_ppir.snap` files, and a separate set of 91
`test_05_diagnostics` tests/snapshots. `build.rs` selects PPIR for every
diagnostic-error project at lines 493–503 and emits the test at lines 546–600;
the diagnostics generator begins at line 603.

The diagnostic pass already drives inference using PPIR's canonical expanded
scope and body-owner graph (`baml_db/src/check.rs`, lines 436–480). Therefore
the diagnostic snapshot also protects “can build and traverse the recovered
program without panicking.” The PPIR snapshot's distinct value is the exact
textual shape of error recovery. Blanket coverage of all 91 projects is much
broader than that value requires.

Keep these eight recovery-oriented representatives:

- `attribute_validation`;
- `duplicate_method_no_cascade`;
- `unknown_type_error`;
- `patterns_class_destructure_namespaces`;
- `scoped_type_binding_forms`;
- `namespaces_stream_direct_ref`;
- `llm_reserved_params`; and
- `test_with_runner_ambiguity`.

That removes **83 active tests and 83 snapshots** while retaining syntax/name,
duplicate suppression, type recovery, pattern recovery, scoped bindings,
stream expansion, LLM lowering, and test-runner ambiguity. A more conservative
ten-golden set also keeps `stream_types` and `requires_clause_shapes`, removing
81. The eight-golden set retains 231 of the current 5,032 snapshot lines and
7,806 of 210,063 bytes.

Do **not** justify deleting all 91 solely from the six PPIR examples in
`corpus_snapshot_policy.rs` (lines 13–44). Those six are valid programs, while
the corpus has a hard zero-error gate (`corpus.rs`, lines 298–312), so they
cannot exercise malformed-program recovery. Removing all 91 is reasonable only
if the project deliberately drops that golden contract or first adds focused
invalid-source PPIR recovery tests.

Warm narrow nextest measurements put all 91 generated PPIR tests at about
3.34 seconds wall / 39.4 seconds summed test CPU, versus 0.46 seconds wall /
3.43 seconds CPU for the proposed eight. The main win is maintenance and
snapshot surface—roughly 4,801 golden lines—not just wall time.

Implement this through an explicit, rationale-bearing project allowlist in
`build.rs` or a sibling policy module. Regenerate `generated_tests.rs`; do not
edit it by hand. Delete every orphaned snapshot in the same change because CI
runs insta with `--unreferenced=reject`.

### MIR: remove 10 of 33

MIR has 33 active tests: 24 snapshots and nine focused structural assertions.
The conservative eight-test deletion is the tutorial-style golden block in
`compiler2_mir/mod.rs`, lines 445–646:

- `literal_return`, `binary_add`, `let_binding`, and `function_call`;
- `if_else`, `while_loop`, and `match_expr`; and
- `object_construction`.

These shapes are already executed end-to-end by native corpus tests, and the
selective corpus policy still keeps eight MIR snapshots spanning 15 functions,
including loops, matches, direct calls, concrete/runtime type loading, lambdas,
exceptions, short-circuit joins, and prompt bodies
(`corpus_snapshot_policy.rs`, lines 46–94).

The recommended plan also removes `reflect_type_of_class` and
`reflect_type_of_bare_typevar` (lines 744–788). The selective corpus MIR
snapshots already pin both `load_type(User)` and `load_type(#0)` shapes, while
native reflection cases cover behavior. That yields **10 removed tests**.

An aggressive 15-test ceiling additionally removes
`reflect_type_of_array`, `reflect_type_of_array_of_typevar`,
`optional_default_prologue_and_source_omission`,
`optional_named_gap_and_explicit_null`, and
`optional_dropping_function_value`. Their behavior is covered natively, but
their feature-specific raw MIR text is not fully duplicated, so they are not in
the recommendation. In particular, retain
`optional_named_reordered_args_evaluate_in_source_order`: because its callees
are pure, the MIR snapshot is what actually checks evaluation order. Also keep
the positive array-rest goldens, which pin exact slice bounds and guards.

### TIR: proposed consolidation of 106 definitions to 29

The enabled TIR modules contain 416 source-defined tests, five ignored and 411
active. They contain exactly 197 snapshot assertions. A narrow nextest run
passed all 411 active tests in 12.69 seconds during this survey.

The recommended design has an exact accounting ledger:

| TIR family | Current definitions | Current snapshots | Themed targets | Net definitions removed |
|---|---:|---:|---:|---:|
| Phase 6 builtin/media return types plus missing-member checks | 30 | 26 | 4 | 26 |
| Explicit type arguments | 16 | 15 | 7 | 9 |
| Recursive type variants | 24 | 24 | 8 | 16 |
| Stream expansion | 23 | 23 | 6 | 17 |
| Phase 7 basic narrowing snapshots | 13 | 13 | 4 | 9 |
| **Total** | **106** | **101** | **29** | **77** |

#### Exact recommended TIR grouping ledger

Phase 6 becomes four tests:

1. `builtin_container_string_method_matrix` folds
   `snapshot_builtin_method_calls`, `array_length_returns_int`, both
   `array_at_returns_element_type_*` tests, `array_join_returns_string`,
   `map_keys_returns_key_type_array`, `map_values_returns_value_type_array`,
   `map_has_returns_bool`, `map_length_returns_int`,
   `string_length_returns_int`, `string_split_returns_string_array`,
   `string_includes_returns_bool`, `string_to_lower_case_returns_string`, and
   the three `let_inferred_from_*` tests.
2. `media_instance_method_matrix` folds `image_url_returns_optional_string`,
   `image_base64_returns_string`, `image_mime_type_returns_optional_string`,
   `pdf_url_returns_optional_string`, `audio_base64_returns_string`, and
   `video_file_returns_optional_string`.
3. `media_static_constructor_matrix` folds `image_static_from_url`,
   `pdf_static_from_base64`, `audio_static_from_file`, and
   `video_static_from_url`.
4. `builtin_missing_method_matrix` folds the image, array, map, and string
   `*_missing_method_produces_unresolved_member` tests. Strengthen this target
   to assert all four receiver/member-specific diagnostics instead of one
   generic `contains("has no member")` check.

Explicit type arguments become seven tests:

1. `type_binding_rendering_matrix` retains
   `type_binding_renders_both_right_hand_side_kinds`.
2. `direct_explicit_type_argument_matrix` folds
   `explicit_type_arg_binds_directly`, `bare_inference_picks_literal_type`,
   `wrong_type_arg_arity_nongeneric`, `wrong_type_arg_arity_too_many`, and
   `explicit_two_type_args`. Repair the explicit-versus-inferred fixture so
   the two rendered results actually differ.
3. `single_parameter_instantiation_value_matrix` folds
   `generic_apply_value_is_specialized`,
   `generic_apply_value_rejects_wrong_arg`,
   `generic_apply_value_accepts_right_arg`, and
   `generic_apply_value_arity_mismatch`.
4. `multiple_parameter_instantiation_value_matrix` folds
   `generic_apply_two_type_args_specialized` and
   `generic_apply_two_type_args_rejects_wrong_arg`.
5. `parenthesized_instantiation_recovery` retains
   `generic_apply_through_parenthesized_receiver`; this checks recovery, while
   `interfaces.rs::parenthesized_generic_apply_is_rejected` owns the primary
   diagnostic.
6. `ambient_type_variable_instantiation_matrix` folds
   `instantiation_value_call_keeps_ambient_typevar_rigid` and
   `instantiation_value_call_preserves_valid_inference`.
7. `generic_lambda_instantiation_recovery` retains
   `paren_generic_lambda_instantiation`.

Recursive types become eight tests:

1. `type_alias_unguarded_scc_matrix` folds direct, mutual, and three-node alias
   cycles: `type_alias_direct_self_reference`, `type_alias_mutual_recursion`,
   and `type_alias_indirect_cycle_three`.
2. `invalid_alias_used_downstream` retains
   `type_alias_cycle_used_in_function`.
3. `type_alias_nonstructural_guard_matrix` folds
   `type_alias_optional_self_reference`, `type_alias_union_with_base_case`, and
   `type_alias_mutual_cycle_through_optional`.
4. `type_alias_structural_guard_matrix` folds
   `type_alias_valid_recursive_via_container`, `type_alias_list_in_union`,
   `type_alias_optional_list_self_reference`, `type_alias_map_in_union`, and
   `type_alias_mutual_cycle_through_list`.
5. `class_required_cycle_matrix` folds `class_field_self_reference`,
   `class_field_mutual_reference`, `class_required_field_mutual_cycle`,
   `class_required_field_self_cycle`, and
   `class_required_field_three_way_cycle`, dropping the duplicated self- and
   two-node-cycle scenarios inside the combined fixture.
6. `class_structural_guard_matrix` folds
   `class_optional_field_breaks_cycle`, `class_list_field_breaks_cycle`, and
   `class_map_field_breaks_cycle`.
7. `class_cycle_alias_transparency_matrix` folds
   `class_cycle_through_type_alias` and
   `class_cycle_broken_by_alias_to_optional`.
8. `class_union_cycle_matrix` folds
   `class_union_field_all_variants_same_class` and
   `class_union_field_different_variants_breaks_cycle`.

Stream expansion becomes six tests:

1. `stream_default_expansion_matrix` folds `primitives_get_null_union`,
   `enum_field_unchanged`, `class_field_gets_stream_prefix`,
   `literal_fields_unchanged`, `list_field_recurses`,
   `map_field_recurses_value`, `union_field_recurses_variants`,
   `optional_expands_to_union_with_null`, `type_alias_expansion`, and
   `recursive_class`.
2. `stream_annotation_semantics_matrix` folds
   `stream_done_field_keeps_type_as_is`, `stream_done_block_attr`,
   `stream_not_null_field`, `stream_not_null_block_attr_on_referenced_class`,
   and `stream_done_and_not_null`, with the repairs described below.
3. `stream_cross_file_reference` retains `cross_file_class_reference`.
4. `stream_pending_default_alias_matrix` folds
   `type_alias_to_list_in_union_gets_correct_pending_default`,
   `chained_alias_to_list_in_union`, and
   `type_alias_to_map_in_union_gets_empty_map_default`.
5. `stream_field_metadata_matrix` folds
   `field_alias_preserved_on_stream_class` and
   `field_description_preserved_stream_done_stripped`.
6. `stream_generic_argument_matrix` folds
   `stream_companion_preserves_generic_args_in_class_field` and
   `stream_companion_preserves_generic_args_for_llm_return_type`.

Phase 7 narrowing becomes four tests:

1. `null_condition_narrowing_matrix` folds
   `narrow_ne_null_then_branch_is_non_nullable`, `narrow_ne_null_rhs_form`,
   `narrow_eq_null_else_branch_is_non_nullable`,
   `narrow_truthiness_then_branch_non_null`, and
   `narrow_negated_eq_null_then_branch_non_null`.
2. `early_return_narrowing_matrix` folds
   `early_return_null_check_narrows_rest_of_block`,
   `early_return_ne_null_check_narrows_rest_of_block`,
   `snapshot_narrowing_patterns`, and `early_return_string_null_check`.
3. `narrowed_value_consumer_matrix` folds
   `narrowed_type_captured_in_let_binding` and
   `narrowed_int_arithmetic_no_error`.
4. `narrowed_assignment_matrix` folds
   `assign_wrong_type_in_null_branch_is_error` and
   `assign_method_result_in_null_branch_works`.

If the combined fixtures validate as expected, this removes **77 active test
definitions and 74 snapshot assertions**, taking TIR from 411 to 334 active
tests and from 197 to 123 snapshot assertions. The other three are the
reduction of four structurally identical phase-6 missing-member tests to one
parameterized assertion. Each target should render labeled fixture sections so
a snapshot failure still identifies the semantic case.

Strong overlap findings exist even before the broader grouping. In phase 6,
the three `let_inferred_from_*` cases and `snapshot_builtin_method_calls`
overlap the direct builtin-return tests; the consolidated matrix should retain
their extra let-propagation and multiple-call contexts but not separate test
definitions. In explicit arguments,
`generic_apply_value_is_specialized` and
`generic_apply_two_type_args_specialized` are subsumed by the corresponding
stored-call accept/reject cases. The recursion matrix has two exact semantic
duplicate pairs: `class_field_self_reference` /
`class_required_field_self_cycle` and `class_field_mutual_reference` /
`class_required_field_mutual_cycle`.

In phase 7, four focused basics are candidates to disappear after their source
contexts are retained in stronger consumer or aggregate fixtures:
`narrow_ne_null_then_branch_is_non_nullable` in the arithmetic consumer,
`narrow_eq_null_else_branch_is_non_nullable` in the assignment cases, and
`early_return_null_check_narrows_rest_of_block` plus
`narrowed_type_captured_in_let_binding` in `snapshot_narrowing_patterns`.

At survey time, `bare_inference_picks_literal_type` had byte-identical expected
TIR to `explicit_type_arg_binds_directly`, despite its comment claiming a
literal-versus-`int` distinction. The consolidated implementation repairs the
assertion and now distinguishes the inferred literal `42`.

The stream family also contained false-confidence coverage that needed repair
during consolidation. `stream_not_null_field`,
`stream_not_null_block_attr_on_referenced_class`, and the `not_null` part of
`stream_done_and_not_null` used the obsolete `@stream.not_null` spelling. The
compiler recognizes `stream.done`, `stream.must_exist`, and
`stream.with_state` (`baml_compiler2_hir/src/builder.rs`, line 14), and full
diagnostics already report `stream.not_null` as unknown; the TIR renderer
simply does not expose that validation diagnostic. The consolidated matrix now
uses valid `@stream.must_exist`/`@@stream.must_exist` cases and gives the block-
attribute case an actual holder/reference, so the snapshot proves expansion.
Block attributes are consumed on referenced type paths in
`baml_compiler2_ppir/src/expand.rs`, lines 669–686. The 23-to-6 consolidation
therefore includes the required semantic repairs rather than a blind snapshot
merge.

The 68-test conservative cut stops before phase 7. The recommended 77 includes
it. An aggressive but still defensible pass can remove eight more
cross-suite-subsumed cases, for an **85-test TIR ceiling**. The initially
suggested 99 is not supportable: it would leave only seven tests across these
five distinct families and necessarily discard unique invariants. Broader
phase-3a diagnostic-matrix consolidation remains worth a separate audit, but
it is not counted here.

Two source-only cleanup findings do not change the 2,628 active denominator:

- `phase8_exceptions::typed_any_and_unknown_catch_bindings_are_rejected` is
  deleted because its typed-catch syntax no longer parses and its asserted
  semantic diagnostics are therefore unreachable. The other four ignored
  future-facing TIR tests remain.
- `compiler2_tir/package_interface.rs` contains 28 `#[test]` definitions—16 at
  file scope and 12 in its `mounted` submodule—but is disconnected by
  `#[cfg(any())]` in `compiler2_tir/mod.rs`, lines 15–18.
  It remains intentionally archived pending a separate port from its
  superseded TIR API to HIR-ty. Those 28 are not among the 416 enabled
  definitions or the 2,628 listed active tests.

### Compiler layers to keep focused

- Keep the 72 HIR and five HIR-ty tests. They primarily protect scope, lookup,
  query, and incremental invariants rather than broad output matrices.
- Keep the three standalone PPIR tests. They cover namespace discovery order,
  serialized artifact/version and declaration order, and body-owner query
  equivalence—not blanket goldens.
- Keep the assertions in the 16 emit tests: they pin exact bytecode and data
  structures. A setup-focused audit does identify a defensible 16-to-9
  consolidation, described below, but it is not counted in the recommended 170
  until the combined fixtures and snapshots are reviewed.
- The incremental, type-specification, and utility tests likewise have no bulk
  reduction counted here.

## Supplied slow-test setup audit

The timing log behind the supplied summary contains 1,545 complete nextest
entries, plus one truncated entry, rather than the full 2,628-entry inventory.
Exactly 139 complete entries exceed three seconds and sum to 18.9 minutes of
*accumulated test duration*. That is neither suite wall time nor a stable
benchmark: isolated reruns on this checkout moved the sampled-cache no-op from
6.863 seconds to 2.6–2.9 seconds, the mixed-file cache oracle from 20.123 to
8.3–8.4 seconds, and the empty-project case from 5.067 to 1.19–1.25 seconds.
The profile is still useful for finding repeated setup, but the three-second
membership should not become a correctness policy.

The categories in the supplied profile sum exactly to 139. This is their
disposition after reconciling them with the native-migration and redundancy
plans:

| Timed family | >3 s | Native overlap | Recommended action | Definition effect |
|---|---:|---|---|---:|
| Bytecode/diagnostics cache | 50 | None | Preserve fresh-oracle semantics; make the sampled no-op cheap; combine the four duplicate mixed-file scenarios | -3 |
| CLI exit/generation | 30 | One phase-1 port | Keep distinct process contracts; make eight layered-coverage deletions/consolidations; move `assert.approx_equal` to native BAML; reduce repeated generation where presentation can be tested purely | -8 logical; 9 Rust entries |
| Discovery/profile | 11 | None | Fold the cache-knob case into the unfiltered oracle; remove its extra cold run; share the bytecode cache in non-cache profile tests | -1 |
| Pack/run command unit | 10 | None | Keep the empty-project boundary; optionally table the four pack and five run resolver cases | 0 now; -7 candidate |
| Other CLI integration | 8 | None | Keep the distinct process/artifact seams; replace two unnecessarily compiling skill probes with `fmt --dry-run` | 0 |
| Compiler emit/MIR/TIR | 21 | None | Keep internal contracts; seed emit setup, combine exact emit families, and reuse one DB inside the nine-case TIR test | 0 now; -7 candidate |
| String/backtick regressions | 8 | All eight | Migrate; one is already covered natively and the two quoted-string tests become one native matrix | already in 841; -2 logical |
| Corpus snapshot | 1 | Infrastructure, not a port | Keep its existing single-DB, selective-snapshot design | 0 |

The 12 recommended non-native reductions in this table plus one adjacent
under-threshold shebang duplicate are counted in the 282-test headline. The
optional emit and pack/run consolidations would add 14 more, raising the
architecture-dependent total to 296. They are kept outside
the headline because they trade away per-case nextest failure granularity and
need implementation-time labeled-case review.

### Named examples: exact decision

| Timed test | Decision | Why |
|---|---|---|
| `plan_reuse_mixed_file_function_sig_edit_stays_minimal` | Keep the oracle; combine its setup | Initial store, edited relink, and an independently built fresh DB are the contract. Combine it with the field-reorder and diagnostics-oracle twins, but never use the served/seeded DB as the honest side. |
| `maybe_sampled_verify_skips_the_hit_path_without_building_a_db` | Keep; remove almost all setup | `maybe_sampled_verify(None, …)` returns before touching a plan or DB. Open a minimal `CacheContext` or extract the `None` gate and retain the panic closure; do not call `sampled_setup`. |
| `list_unfiltered_cold_equals_warm` | Keep; reduce seven related subprocesses to about three | Assert nonempty/nested output on the helper's existing cold result rather than creating a fourth project/cache. Fold the cache-knob assertions into the same unfiltered fixture. |
| `quoted_string_escape_sequences_decode_to_exact_bytes` + `quoted_string_escape_lengths_match` | Replace with one phase-1 native matrix | Both use the same source and currently compile it 11 times. One native fixture preserves both value and length checks while sharing the corpus compile. |
| `cross_site_let_has_no_spurious_diagnostic` | Delete without a new port | `ns_backtick_strings::backtick_cross_site_let_binds_into_concat_scope` already executes the exact expression and asserts `"hi"`; the corpus snapshot also sees its diagnostics. |
| `generate_go_removes_stale_owned_files_and_preserves_unknown_files` | Fold into the retained Go generation smoke, keeping two passes | The before/edit/after transition is real, but the separate definition duplicates writer-unit and Go file-set coverage. One combined Go E2E can check first output, user-file preservation, edit, stale removal, and final output in two passes instead of four passes across three tests. |
| `generate_reports_identifier_renames_in_normal_verbose_and_quiet_modes` | Keep one pipeline integration; extract mode rendering | Run full generation once. Test normal/verbose/quiet rename presentation as a pure table over the reporter/rendering seam rather than compiling and generating three times. |
| `test_load_project_empty_dir_errors` | Keep and monitor | It is the unique real project-load boundary. The supplied five seconds did not reproduce in two isolated runs; add a pre-DB empty-source seam only if a phase benchmark confirms the DB/stdlib open is material. |
| `test_logs_flag_flushes_stdout_during_long_running_test` | Keep its synchronization window | The 750 ms sleep ensures the first log is observable while the child is still alive. Replace it only with a deterministic test hook/barrier, not a shorter arbitrary sleep. |

All eight timed string/backtick entries have an exact migration action:

- port `invalid_strings_still_produce_errors` as one three-snippet reflected-
  diagnostic matrix;
- merge the two timed quoted-string value/length definitions into one phase-1
  native matrix;
- port `mismatched_for_close_diagnoses`, `stray_else_diagnoses`, and
  `stray_endfor_diagnoses` through reflected failure diagnostics;
- for `empty_interpolation_diagnoses`, keep the already-snapshotted untagged
  warning and add only the prompt-path warning through public
  `Package.compile(...).diagnostics()`; and
- delete `cross_site_let_has_no_spurious_diagnostic` against its exact existing
  native execution case.

The adjacent, below-threshold audit also merges the string boundary
value/length pair and drops `unresolved_name_in_interp_reports_cleanly` in favor
of its stronger untagged-plus-prompt sibling. Those two extra findings are why
the global native-dedup arithmetic changes by four rather than only the two
logical reductions visible inside the timed eight.

### Cache tests: retain independence, consolidate duplicate scenarios

The 50 slow cache entries are 33 of the 44 `bytecode_cache::tests`, all 16
`diagnostics_cache_oracle` tests, and
`diagnostics_cache::tests::serialize_builtin_diagnostics_round_trips_the_live_set`.
None can migrate to native BAML because their assertions concern reuse plans,
serialized programs and compilation units, on-disk cache state, or honest
versus seeded databases.

There are two direct actions:

1. Replace `sampled_setup` in
   `maybe_sampled_verify_skips_the_hit_path_without_building_a_db` with a cheap
   context-only setup. The current helper compiles/stores v1, constructs a
   reuse plan, and constructs a fresh honest DB, then the test discards all of
   that before exercising an early return.
2. Combine these four definitions into one labeled two-variant scenario:
   `plan_reuse_mixed_file_function_sig_edit_stays_minimal`,
   `plan_reuse_mixed_file_field_reorder_fires_sentinel`,
   `oracle_mixed_file_function_sig_edit`, and
   `oracle_mixed_file_field_reorder`. Compile/store the common v1 input once;
   for each edit, build a new served v2 DB and a *different, unseeded* honest v2
   DB, then assert the dirty partition, diagnostics parity, program bytes, and
   every compilation unit. That is a 4-to-1 reduction; a lower-risk 4-to-2
   version is the conservative count.

Keep the remaining 45 definitions' semantic checks. Runtime work can still be
reduced by grouping scenarios that share an immutable v1 manifest and by
seeding the build-constant stdlib interface in tests that are not themselves
testing stdlib caching. Never remove the independent fresh DB from relink,
diagnostic, or sampled-verification oracles; comparing a seed against itself
would make those tests vacuous.

### CLI generation and exit-code tests

Twenty-nine of the 30 assert CLI concerns; the exception is
`test_assert_approx_equal_accepts_float_tolerance`. Its only unique behavior is
that the builtin accepts ordinary floating-point rounding, so move that case to
native BAML. Other E2Es already pin a successful CLI test's exit code and
`1 passed, 0 failed, 1 total` summary.

After consolidation, 21 of these Rust definitions should remain, one native
case replaces the approximate-equality wrapper, and eight logical definitions
disappear:

- Delete `check_valid_project_returns_zero_exit_code`; the identical valid
  fixture succeeds through the stronger cwd-default case, while a separate
  diagnostic-highlighting E2E also covers explicit `check --from .` success.
- Delete `generate_multiple_compilation_errors_returns_nonzero_exit_code`. It
  asserts only exit code 4, which the single-error case pins more strongly along
  with rendered diagnostics. If aggregate multi-error rendering matters,
  strengthen it to assert both diagnostics instead of deleting it.
- Fold `generate_valid_project_returns_zero_exit_code`'s version/progress
  assertions into the retained always-run Python rename generation. Separately,
  combine `generate_go_writes_sdk_through_cli` and both Go lifecycle tests into
  one two-pass Go scenario proving generated content, first-run unmanaged-file
  preservation, and stale-owned-file removal. The writer layer already has
  exact first-manifest and stale-owned-file unit tests, and Go codegen tests pin
  whether `types.go` belongs in the generated file set. This removes three of
  the four definitions and reduces their five generation invocations to two;
  the always-run progress assertion rides on the already-retained rename E2E
  rather than depending on `gofmt` availability.
- Delete `generate_rust_default_output_stays_inside_project`; generator
  discovery already asserts the exact project-root default and other E2Es prove
  successful filesystem codegen.
- Delete the full-set
  `test_filtered_testset_run_honors_pass_rate_runner_for_selected_set`. Its
  selector chooses the same complete set as the unfiltered PassRate case; the
  focused filtered-leaf case retains the filter-to-parent-runner seam.
- Delete `run_list_accepts_explicit_unmarked_source_root`. Project-load unit
  tests cover the exact explicit unmarked-root path, and
  `test_accepts_explicit_source_directory_as_project` retains the public CLI
  integration boundary.
- Extract rename-result presentation from full generation. One integration
  **verbose** pass should prove the generator supplies rename data to the
  reporter and emits the exact detail; a pure table should pin normal summary,
  verbose detail, and quiet suppression. This changes subprocess count, not
  test-definition count.
- Reduce `run_log_sources_surface_filtered_logs_for_targets_and_expressions`
  from five subprocesses to two: one conversion-target call can jointly cover
  CLI-over-environment precedence, filtering, and FromJson/target/ToJson log
  order; one expression call retains the `-e` path. Generic default/environment
  logging is covered elsewhere.
- Preserve the live-process assertion in the streaming-log case, but replace
  the 750 ms sleep and separate cache-warm process with a deterministic
  marker-file or test-hook handshake if one is added: the child logs start and
  waits, the parent observes that line while it is alive, then releases it.
- Retain the remaining 18 timed definitions unchanged:
  `check_compilation_error_returns_nonzero_exit_code`,
  `run_expr_ignores_unrelated_project_compile_errors`,
  `generate_compilation_error_returns_nonzero_exit_code`,
  `check_defaults_from_to_current_directory`,
  `run_expr_without_baml_toml_picks_up_baml_src_context`,
  `generate_rust_language_naming_convention_returns_diagnostic`,
  `generate_without_baml_toml_reports_no_generators`,
  `run_file_script_mode_passes_args_after_separator_as_argv`,
  `run_execute_function_without_baml_toml_succeeds`,
  `run_expression_serialization_failure_returns_target_error`,
  `run_list_compilation_error_returns_nonzero_exit_code`,
  `test_accepts_explicit_source_directory_as_project`,
  `shebang_can_name_a_specific_function`,
  `test_assert_equal_failure_shows_values_without_internal_span_debug`,
  `test_legacy_custom_runner_does_not_invent_identity_for_skipped_leaf`,
  `generate_go_writes_sdk_through_cli`,
  `test_filtered_testset_leaf_runs_under_parent_runner`, and
  `test_fail_fast_does_not_report_skipped_leaf_as_passed`.

The recommended eight-definition reduction uses layered coverage. A stricter
process-boundary policy may retain the Rust-generator smoke and the two Go
lifecycle definitions; that lowers the CLI logical reduction from eight to
five. Regardless of that policy, the multi-error exit-code test is a direct
strict duplicate. The common process runner already uses the shared
content-addressed bytecode cache.

One adjacent 2.942-second case is also a direct deletion:
`executable_baml_script_runs_via_kernel_shebang` only proves that a kernel
shebang reaches `main`. The stronger timed
`run_file_script_mode_passes_args_after_separator_as_argv` already proves that
same kernel/implicit-main path plus argument passthrough, while
`shebang_can_name_a_specific_function` covers the other dispatch form. This
extra deletion is outside the 30-row timing category but is counted in the
13-test setup-cleanup headline.

The single-generation Go cases still taking 16–18 seconds point below the test
layer: profile the generation pipeline and the per-file `gofmt` process loop.
Batching or parallelizing formatting is a product optimization and must retain
deterministic file/error reporting.

### Discovery and profiles

The exact 11 are two discovery-cache E2Es
(`list_unfiltered_cold_equals_warm` and
`no_discovery_cache_knob_is_output_neutral`) and all nine tests in
`test_profiles_e2e.rs` from this timing run:
`default_named_and_no_profile_select_expected_canonical_ids`,
`cli_include_narrows_profile_instead_of_oring_with_it`,
`bad_profiles_and_old_slash_selectors_are_actionable`,
`profile_exclusion_prunes_a_lazy_testset_before_expansion`,
`includes_layer_contradictions_and_broad_excludes_prune_lazy_collectors`,
`double_colon_is_reserved_inside_declared_test_names`,
`nested_reserved_names_are_discovery_errors_not_sentinel_tests`,
`console_leaf_ids_match_list_ids_for_passes_and_failures`, and
`failed_expansion_is_not_cached_and_literal_slash_selector_is_legal`.

- Make an unfiltered-specific oracle do a cold/populating run, a warm run, and
  a knob-forced honest run on one fixture. Assert the nested leaf on the first
  output. This replaces the current seven runs across two tests with about
  three and removes one test definition.
- The filtered cold/warm matrix was below three seconds in the supplied run but
  has the same setup smell: remove the duplicate `root::suite::*` row and share
  one project plus one unfiltered cache population across the eight distinct
  selectors. Keep an honest forced-discovery run and a warm cached run per
  selector so the BAML and Rust filters remain compared.
- For profile tests whose purpose is selection/lazy expansion rather than cache
  behavior, use `common::shared_cache_dir()` for bytecode and force discovery
  honest with `BAML_NO_DISCOVERY_CACHE`. Keep
  `failed_expansion_is_not_cached_and_literal_slash_selector_is_legal`
  isolated. Test duration should fall without merging distinct profile
  contracts.

### Pack/run resolver and other CLI tests

The ten pack/run unit entries consist of the empty-project test plus four
namespaced pack-resolution and five namespaced run-resolution tests. They build
full engines to inspect read-only function-name catalogs; they are not native
language tests.

Keep the empty-project case. For the other nine, compile one comprehensive
fixture per module and run labeled argument/result tables against it: four pack
definitions (`test_pack_namespaced_positional`,
`test_pack_namespaced_multi_function`,
`test_pack_namespaced_subcommand_name_collision_errors`, and
`test_pack_namespaced_resolves_via_bare_name`) become one. Five run definitions
(`test_run_subcommand_resolves_namespaced_target`,
`test_run_subcommand_multi_namespaced`,
`test_run_subcommand_namespaced_name_collision_errors`,
`test_run_subcommand_namespaced_resolves_via_bare_name`, and
`test_run_subcommand_unknown_function_errors`) become one. That is a
provisional seven-definition reduction and nine-to-two engine-build reduction.
A cleaner long-term seam would make target resolution consume a lightweight
function catalog and retain one real-engine adapter smoke test per command.

The eight other CLI entries are six `pack_e2e` cases
(`pack_e2e_root_main`, `pack_e2e_current_package_compiles_and_runs_skill`,
`pack_e2e_function_target`, `pack_e2e_hermetic_baml_file`,
`pack_e2e_output_format_debug_baked_in`, and
`pack_e2e_manifest_less_baml_src`) plus
`init_warns_then_embedded_install_silences_authoring_commands` and
`selected_project_uses_its_skill`. Keep all eight definitions:

- each pack case crosses a distinct artifact/process seam; strengthen
  `pack_e2e_output_format_debug_baked_in` because its current
  `contains("hello")` assertion cannot distinguish debug from JSON output;
- in `init_warns_then_embedded_install_silences_authoring_commands`, replace
  the final full `generate` probe with the equally skill-gated `fmt --dry-run`;
  and
- use `fmt --dry-run --project …` rather than a cold `check` in
  `selected_project_uses_its_skill`, preserving selected-project lookup without
  compiling the project.

### The 21 compiler tests are not the planned MIR/TIR deletions

The exact group is all 16 emit tests, the three mounted MIR tests, and two TIR
tests. None is among the recommended ten MIR or 77 TIR reductions above.

- Keep `mounted_await_any_kind_is_trusted_only_for_precompiled_packages`,
  `mounted_intrinsic_kinds_are_trusted_only_for_precompiled_packages`, and
  `mounted_loc_free_runtime_call_target_is_explicit`. They pin precompiled-
  package trust and raw runtime-call-target invariants.
- Keep `compiler2_tir::inference::own_class_method_lookup_matches_exported_implicit_self_type`.
  It is a unique own-lookup versus exported-interface parity check.
- `compiler2_tir::phase3a::new_mode_failures_have_good_diagnostics` now keeps
  all nine assertions in isolated namespaces inside one database, so it pays
  the standard-library setup once while retaining diagnostics grouped by file.
  It remains one test definition; this is a runtime-only change.
- The emit helper now uses the existing byte-identical build-time stdlib prefix
  for both typed interfaces and bytecode. The focused 16-test run passes, and
  the separate all-opt-level equivalence oracle confirms prefixed output is
  byte-identical to an honest compile. The optional fixture consolidation is
  still deferred: five basic/builtin/enum/class/no-test cases could become one
  program-shape test; two optional-default checks could become one; two LocalId
  cases could share one source; and the single/multiple-let cases could collapse
  into the stronger multiple fixture with the package-init-order assertion
  carried over. That would reduce emit from 16 definitions/compiles to nine.
  Keep the typed-pattern, positive test-chainer, and three interface-field/link
  tests focused.

Finally, keep `corpus::corpus_snapshots`. It already follows the desired cost
shape: one database for the whole corpus, zero-error validation, policy-selected
PPIR/MIR rendering, and one bytecode emit. Splitting or replacing it with native
execution tests would lose internal golden coverage and repeat setup.

## Classification rubric

### Phase 1: migrate without expanding diagnostics reflection

A test belongs here when all of its meaningful assertions can be made in BAML:

- it compiles a valid BAML program and checks its returned value or catchable
  throw;
- it checks reflection, a runtime package, or a Session through public BAML
  APIs and Rust only compares the returned bool/string/value;
- it asserts that a valid snippet builds; or
- a native test already covers the same behavior, so the Rust copy can be
  removed after confirming there is no host-specific assertion.

For compile-success cases, prefer placing valid source directly in the corpus.
The corpus compilation itself is the assertion; a no-op `test` block is not
needed. Use `ns_compiler.AssertCompiles` only when the source needs isolation
from the shared namespace or mounted dependencies. Runtime behavior should use
an ordinary native `test` block.

### Phase 2: migrate through reflected compiler diagnostics

A test belongs here when its purpose is to compile invalid or partially valid
source and assert diagnostic code, message, primary source span, count, order,
or absence. The existing helper is already in
`baml_src/ns_compiler/assertions.baml`.

Use `reflect.Package._compile`, not public `Package.compile`, for these tests:

- `_compile` throws the typed `reflect.errors.CompilationError`;
- it performs checking and bytecode emission but avoids package installation
  and top-level initialization; and
- successful compile-only assertions do not need `_finish`.

Use `Package.compile(...).diagnostics()` only for diagnostics emitted by a
successful compilation, such as warnings/advisories.

### Outside native migration

If a test is retained, it must stay in Rust when the assertion is specifically
about the items below. This category is a portability result, not a verdict
that the test is nonredundant:

- `BexExternalValue`, opaque heap handles, host error/cause structure, or FFI
  conversion;
- raw `Program`, IR, bytecode, metadata, source locations, serialization, or
  compiler query/incremental behavior;
- exact optimization levels or bytecode mutation;
- filesystem, environment, shell, HTTP/network, process, platform, or working
  directory behavior;
- scheduling, races, cancellation, latency, memory, GC reachability, or other
  host instrumentation;
- mounted unit/link/relink parity that cannot be constructed through public
  reflection APIs; or
- a deliberately ignored future test, because native BAML tests do not yet
  have an equivalent to Rust's `#[ignore]`.

## Survey-time interface-focused inventory

This section records the baseline classification that drove implementation;
the final disposition in [Outcome](#outcome) is authoritative. The interface
family was the largest and best migration target. Of 534
definitions, 512 are native-capable and one is dead, so 513 can leave Rust and
21 remain. Importantly, 76 of the 512 already have stronger native coverage;
only 114 new phase-1 ports and 322 diagnostic ports are needed before
deduplication.

| File | Existing native | Phase-1 ports | Phase-2 ports | Rust | Dead |
|---|---:|---:|---:|---:|---:|
| `interfaces.rs` | 52 | 52 | 229 | 4 | 1 |
| `interfaces_associated_types.rs` | 17 | 48 | 61 | 6 | 0 |
| `interfaces_class_generics.rs` | 3 | 4 | 26 | 0 | 0 |
| `runtime_interface_witnesses.rs` | 1 | 8 | 0 | 0 | 0 |
| `interface_type_alias_iteration.rs` | 0 | 1 | 0 | 0 | 0 |
| `from_json_interface.rs` | 1 | 1 | 1 | 0 | 0 |
| `to_json_interface.rs` | 1 | 0 | 1 | 0 | 0 |
| `to_string_interface.rs` | 1 | 0 | 1 | 0 | 0 |
| `interface_body_anonymity.rs` | 0 | 0 | 0 | 2 | 0 |
| `hir_ty_package_interface.rs` | 0 | 0 | 2 | 7 | 0 |
| `book_interfaces.rs` | 0 | 0 | 1 | 2 | 0 |
| **Total** | **76** | **114** | **322** | **21** | **1** |

### Exact interface split

Before subtracting existing native coverage, `interfaces.rs` has 104
phase-1-capable definitions. Fifty-two are already covered, leaving 52 actual
ports. The capability set is deterministic to classify without listing 338
names:

- Phase 1 is 102 of the 103 tests that call `assert_no_interface_errors`,
  `assert_no_compile_errors`, or `assert_zero_compile_errors`, plus
  `user_scenario_requires_field_check` and
  `form1_blanket_has_no_compile_errors_at_all`. The exception is the mixed
  scalar arithmetic matrix below.
- Phase 2 is every remaining diagnostic assertion except the four Rust-only
  cases below.
- `scalar_arithmetic_matches_builtin_impl_matrix` is Phase 2 rather than Phase
  1: its accepted scalar rows share one Rust definition with 138 diagnostic
  rejection assertions, so the native replacement must preserve the complete
  mixed matrix.
- Four nominally positive LLM fixtures reference an undeclared `GPT4o` because
  they currently assert only the absence of interface error codes. A direct
  corpus port must add a local test client or preserve the isolated filtered
  assertion: `llm_function_can_return_interface_type`,
  `llm_function_with_interface_array_return_compiles`,
  `llm_function_with_interface_in_union_return_compiles`, and
  `llm_function_takes_interface_typed_parameter_compiles`.
- Delete `_unused_imports_compile` when the Rust runtime imports disappear.

The four `interfaces.rs` tests that remain Rust are:

- `aliased_interface_fields_do_not_create_concrete_runtime_slots`
- `interface_return_uses_concrete_implementor_field_shape`
- ignored `fuzz_bug01_method_ref_required_method_crashes`
- ignored `fuzz_bug02_method_ref_default_dispatches_to_override`

The first two inspect the host `BexExternalValue::Instance` field map. The
last two need ignored-test support.

`interfaces_associated_types.rs` is similarly mechanical. Its 65
phase-1-capable definitions split into 17 already covered and 48 new ports:

- Phase 1 is all 64 tests using `assert_zero_compile_errors` or
  `assert_zero_compile_errors_multi`, plus
  `reflection_bounded_impl_cycle_terminates`. The latter was verified in an
  isolated reflected package and Session; `app.main()` returns `0`.
- Phase 2 is the other 61 diagnostic assertions.
- Keep these six in Rust:
  `vm_metadata_resolves_concrete_associated_type_projection_return`,
  `vm_metadata_resolves_self_associated_type_return_in_implements_method`,
  `vm_metadata_preserves_unresolved_generic_associated_projection_symbolically`,
  `vm_metadata_displays_interface_default_method_self_type`,
  `formatter_accepts_associated_type_syntax`, and ignored
  `runtime_guard_accepts_generic_requested_associated_type_var`.

The seven phase-1-capable tests in `interfaces_class_generics.rs` are:

- `class_generic_bound_exposes_interface_members`
- `class_generic_bound_accepts_implementing_class`
- `class_generic_bound_through_requires_chain_exposes_all_members`
- `class_generic_bound_is_visible_inside_lambda_body`
- `class_generic_bound_substitutes_other_class_type_params`
- `class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field`
- `method_generic_bound_referencing_class_param_is_satisfied`

The first two and
`method_generic_bound_referencing_class_param_is_satisfied` already have direct
native coverage, leaving four new phase-1 ports. The other 26 are phase-2
diagnostic tests.

The smaller interface files have these exact phase-1-capable cases:

- all nine tests in `runtime_interface_witnesses.rs`, one of which already has
  native coverage;
- `interface_type_alias_iteration::interface_method_array_alias_is_iterable`;
- `from_json_interface::from_json_interface_compiles` (already covered);
- `from_json_interface::static_from_json_charges_only_json_decode_error`;
- `to_json_interface::to_json_via_interface_is_allowed` (already covered); and
- `to_string_interface::to_string_via_interface_is_allowed` (already covered).

Their phase-2 cases are:

- `from_json_interface::empty_implementor_is_rejected`;
- `to_json_interface::direct_to_json_method_is_banned`;
- `to_string_interface::direct_to_string_method_is_banned`;
- `book_interfaces::foreign_field_mapping_requires_the_class_owner`;
- `hir_ty_package_interface::bare_type_is_not_a_value_type_annotation`; and
- `hir_ty_package_interface::mounted_type_validation_and_package_shadowing_are_fail_closed`
  (its valid subcase can optionally be split into phase 1).

The remaining interface-related Rust tests are:

- both tests in `interface_body_anonymity.rs`, which inspect raw `Program`
  pools/globals/name maps and host engine name resolution;
- `hir_ty_package_interface::mounted_interface_skew_is_rejected_before_installation`;
- `hir_ty_package_interface::enriched_interface_is_symbolic_loc_free_and_borsh_stable`;
- `hir_ty_package_interface::mounted_lookup_returns_owned_exported_results_without_source_locs`;
- `hir_ty_package_interface::reflect_resolves_as_an_ordinary_builtin_package`;
- `hir_ty_package_interface::reflect_package_resolution_uses_ordinary_builtin_items`;
- `hir_ty_package_interface::mounted_witnesses_members_defaults_and_symbolic_calls_type_check_source_less`;
- `hir_ty_package_interface::mounted_reserved_builtin_reports_normal_and_optional_calls`;
- `book_interfaces::local_notifier_can_be_implemented_for_foreign_webhook`; and
- `book_interfaces::foreign_interface_and_foreign_receiver_violate_local_ownership`.

At survey time, the last two exposed reflection parity failures rather than
inherently Rust-only behavior. The implementation closed both gaps and removed
both Rust tests.

## Other integration suites

### Files that are entirely phase-1-capable

Every test in these files can move without new diagnostic reflection work.
This is a capability table before subtracting existing coverage: three of the
84 definitions (one runtime-interface witness and two type-value-equality
cases) already have native replacements, leaving 81 new ports before collapsing
same-source optimizer pairs.

| File | Tests |
|---|---:|
| `builder_witness_parity.rs` | 3 |
| `compiled_package_identity.rs` | 1 |
| `errorcontext.rs` | 4 |
| `generic_union_returns.rs` | 4 |
| `interface_type_alias_iteration.rs` | 1 |
| `json_typed_union.rs` | 3 |
| `loop_reassignment.rs` | 2 |
| `map_aliasing.rs` | 11 |
| `output_format_non_data.rs` | 11 |
| `parse_companions.rs` | 3 |
| `runtime_identity_seams.rs` | 1 |
| `runtime_interface_witnesses.rs` | 9 |
| `runtime_package_api_consistency.rs` | 4 |
| `runtime_package_extraction.rs` | 2 |
| `short_circuit_locals.rs` | 4 |
| `stdlib_runner_and_stream_batching.rs` | 3 |
| `to_baml_witness_roundtrip.rs` | 5 |
| `type_kinds.rs` | 8 |
| `type_value_equality.rs` | 5 |
| **Total** | **84** |

Some non-obvious classifications are intentional:

- `errorcontext.rs` only unwraps a returned string in Rust and applies
  contains/split/length assertions that BAML can express.
- `output_format_non_data.rs` does not make LLM network requests. It uses
  `@render_prompt`, runtime type construction, and BAML-side catches; Rust
  compares returned strings.
- `runtime_package_api_consistency.rs` exercises public reflection package APIs
  inside BAML; Rust only checks bool/string results.

### Files that are entirely phase 2

| File | Tests |
|---|---:|
| `tostring_sugar.rs` | 1 |
| `type_error_repro.rs` | 7 |
| **Total** | **8** |

The comment in `type_error_repro.baml` saying these cases cannot be expressed
as BAML tests is obsolete now that the reflected compiler helper exists.

### Mixed files

This table is also capability-oriented. It uses the refined interface results
and the corrected classification of four `runtime_session` tests that compare
ordinary compiler diagnostics. Eight of its 167 phase-1 definitions already
have native replacements: three JSON/string-interface cases and five
`sort_compare` cases. Thus it contains 159 new phase-1 ports before other
deduplication.

| File | Phase 1 | Phase 2 | Rust |
|---|---:|---:|---:|
| `anyclass_reflection.rs` | 5 | 2 | 0 |
| `backtick_block_diagnostics.rs` | 2 | 14 | 0 |
| `book_interfaces.rs` | 0 | 1 | 2 |
| `cleanup.rs` | 1 | 4 | 0 |
| `constructor_consistency.rs` | 2 | 1 | 0 |
| `convert_boundary_regressions.rs` | 2 | 0 | 2 |
| `from_json_interface.rs` | 2 | 1 | 0 |
| `match_union_typevar.rs` | 3 | 1 | 0 |
| `reflect_call_any.rs` | 32 | 0 | 2 |
| `runtime_builders_and_pending_types.rs` | 7 | 2 | 0 |
| `runtime_classes_and_composites.rs` | 9 | 1 | 2 |
| `runtime_diagnostic_consistency.rs` | 1 | 2 | 0 |
| `runtime_package_compile.rs` | 21 | 0 | 3 |
| `runtime_session.rs` | 34 | 4 | 6 |
| `runtime_type_bindings.rs` | 24 | 2 | 0 |
| `sort_compare.rs` | 11 | 13 | 1 |
| `tagged_template_lowering.rs` | 5 | 10 | 0 |
| `to_json_interface.rs` | 1 | 1 | 0 |
| `to_string_interface.rs` | 1 | 1 | 0 |
| `wildcard_expression_holes.rs` | 0 | 3 | 6 |
| `wildcard_type_inference.rs` | 4 | 12 | 0 |
| `hir_ty_package_interface.rs` | 0 | 2 | 7 |
| **Total** | **167** | **77** | **31** |

The exact phase-1 and phase-2 names for the non-interface mixed files are in
the appendix, so this table can serve as a migration checklist without
repeating hundreds of large-interface diagnostic names.

### Rust-dominant files

This planning inventory is superseded by the completed
[Rust-survivor ledger](NATIVE_BAML_TEST_RUST_SURVIVORS.md). The final residue is
234 cross-platform integration definitions across 57 files, plus 820
library/generated definitions. The ledger names every integration survivor,
groups every library/generated survivor, assigns a concrete non-native
capability code, and records the source construct that proves the classification.

## Diagnostic benchmark

The benchmark used a temporary harness outside the repository. It compiled one
valid host BAML program and one `BexEngine`, then repeatedly compiled the same
representative missing-interface-method case (E0113). Each measured path used
three warmups and 21 samples in the debug/test profile; two runs agreed.

| Path | Median |
|---|---:|
| Precompiled-engine no-op call | 0.18 ms |
| `reflect.Package._compile` failure plus code/message/span inspection | 85-87 ms |
| Public `reflect.Package.compile` failure plus the same inspection | 85-87 ms |
| Rust `setup_test_db` plus `check_user_files` | 523-535 ms |
| One real Rust interface diagnostic test through nextest | 489 ms |
| Fresh valid `baml_test!` wrapper that performs the inner reflected compile | roughly 700-1,000 ms |

The two repeated medians were:

- run 1: `_compile` 85.4 ms, `compile` 85.4 ms, Rust 526.2 ms;
- run 2: `_compile` 87.2 ms, `compile` 86.9 ms, Rust 534.9 ms.

Once the native corpus program is compiled, the reflected diagnostic path is
about **6.1 times faster per case** than the current Rust diagnostic helper.
Public `compile` is not faster or slower on a compile failure because it never
reaches `_finish`.

There is an important apples-to-apples qualification. `baml_test!` is not a
diagnostic-capture API: invalid outer source panics in
`compile_source_with_opt`. To obtain structured diagnostics through that macro,
the outer source must be valid and call `reflect.Package._compile` on an inner
string, paying for both compilations. Native corpus tests amortize that outer
compilation across the suite.

A one-test CLI invocation still pays to compile/load the whole native corpus.
On this checkout that outer cost was roughly 36-55 seconds, while the selected
inner compiler test took about 0.16-0.21 seconds. Running 51 compiler tests took
about 39 seconds versus about 37 seconds for one, demonstrating the intended
amortization. Native diagnostics are therefore much cheaper for the full CI
suite, but a cold one-test developer invocation can have higher wall time.

This matches the precedent in commit `6217713154`: 370 execution tests moved to
the native corpus, where a shared compile replaced one full compile in every
nextest process.

## Diagnostic fidelity

The reflected path uses the same compiler stream. As part of this migration,
`reflect.Diagnostic` was expanded so native tests can inspect the structured
compiler fields that the Rust tests use.

| Diagnostic information | Available through reflection? |
|---|---|
| Code | Yes, exact |
| Message plus primary label | Yes, flattened through the same `message_with_primary_label()` path |
| Primary filename | Yes, project-relative runtime path |
| Primary byte start/end | Yes |
| Count and stable order | Yes |
| Severity | Yes, exact `error` / `warning` / `info` literal |
| Compiler phase | Yes, exact optional `parse` / `hir` / `validation` / `type` literal |
| Secondary annotations/spans/labels | Yes, including primary/secondary identity and label highlights |
| Related locations | Yes, including optional external file path |
| Message highlights | Yes, byte ranges and semantic kinds |
| Separate headline and primary label | Yes; the legacy flattened `message` is also retained |
| Line/column or full rendered source | **No**; only byte offsets are exposed |

E0113 was checked empirically: code and flattened message were identical.
Offsets also matched when the source bytes matched. Backtick literals can trim
an initial newline, so a port must preserve source bytes if it asserts offsets.

The runtime compiler owns this metadata before its transient database is
dropped, and both reflected-package and engine-external diagnostic
materialization preserve it. Derived line/column coordinates and the fully
rendered source excerpt remain unavailable; tests whose contract is the
renderer itself therefore stay in Rust.

## Current reflection gaps and migration hazards

1. **Successful warnings: closed.** `CompileSuccessDiagnostics` uses public
   `Package.compile(...).diagnostics()`, and native cases cover empty
   interpolation plus the const-for advisory. Severity is now available for
   reliable filtering.

2. **Runtime compiler parity is not universal.** The reflected runtime compiler
   resolves against source-less mounted packages. The repository testing guide
   already records that checks which walk stdlib bodies or declaration sites,
   including E0153/E0163 paths, can differ from the full source compiler. Every
   diagnostic family should get one parity check before bulk deletion.

3. **E0139 parity: closed.** Reflected runtime compilation now applies the same
   mounted-interface ownership/header validation as source compilation, without
   duplicating diagnostics for source-backed impls.

4. **Mounted dispatch parity: closed.** Runtime virtual dispatch searches both
   the receiver owner's package graph and the lexical runtime package, so a
   local interface implementation for a mounted foreign class resolves.

5. **Mounted `requires` parity: closed.** Runtime compilation realizes exported
   mounted-interface requirements and emits E0125 when an impl omits a required
   interface, matching the source compiler.

6. **Phase assertions: closed.** Native diagnostics expose the exact compiler
   phase, so the tagged-template ports can preserve intentional phase checks.

7. **Filtered positive checks.** `assert_no_interface_errors` means "no
   E0112-E0132 errors," not necessarily "the whole snippet compiles." Normalize
   those fixtures before placing them directly in the corpus, or reproduce the
   explicit code filter through reflection.

## Immediate no-port deletion ledger

At survey time, these were the 87 Rust definitions that could be removed without first adding a
new native test. The first 86 are covered by stronger existing native execution
tests; the last is dead. The slow-test audit separately found one redundant CLI
layer worth eight deletions/consolidations, giving a projected 95 definitions
without new native replacements at the recommended layered-dedup boundary.
The four cache/discovery setup consolidations bring that cross-survey figure to
99; deleting the adjacent simple shebang duplicate makes it 100 before the
separate compiler-internal cleanup is added. The conservative
CLI boundary retains three generator lifecycle/smoke cases, reducing those
figures by three. The final authoritative integration result is 90: 86 existing-
native duplicates, one dead test, and three redundant retained-Rust assertions.

| Source group | Existing-native deletions | Dead | Total |
|---|---:|---:|---:|
| `interfaces.rs` | 52 | 1 | 53 |
| `interfaces_associated_types.rs` | 17 | 0 | 17 |
| `interfaces_class_generics.rs` | 3 | 0 | 3 |
| Runtime/JSON/string interface files | 4 | 0 | 4 |
| Other integration files | 10 | 0 | 10 |
| **Total** | **86** | **1** | **87** |

### `interfaces.rs`: 52 existing-native deletions

| Rust test | Existing native test or coverage |
|---|---|
| `basic_interface_parses` | `ns_interfaces::match_destructures_interface_fields` |
| `interface_with_only_fields_parses` | `ns_interfaces::interface_fields_constructed_with_class_keys` |
| `interface_default_method_inherited` | `ns_interfaces::fuzz_bug17_inherited_default_method_callable_on_class_var` |
| `interface_requires_aggregates_contracts` | `ns_interfaces::interface_field_via_requires_chain_runtime`; interface-typed dispatch |
| `generic_interface_parses` | `ns_interfaces::fuzz_bug07_generic_default_method_body_can_use_type_param` |
| `class_can_implement_multiple_interfaces` | `ns_interfaces::self_as_projection_call_inside_unrelated_block` |
| `orphan_rule_allows_local_interface_blanket` | `ns_interfaces::form2_dispatches_through_interface_typed_var` |
| `class_own_field_can_shadow_interface_field_with_different_type` | `ns_interfaces::class_own_field_shadowing_interface_field_is_separate_at_runtime` |
| `class_field_auto_satisfies_interface_field_with_empty_implements_block` | `ns_interfaces::interface_fields_constructed_with_class_keys` |
| `unrelated_interfaces_same_field_different_types_are_separate_namespaces` | `ns_interfaces::same_field_name_different_interface_types_not_conflicting_runtime` |
| `empty_implements_block_with_all_defaults_is_ok` | inherited/default generic-rule dispatch tests in `ns_interfaces` |
| `interface_with_only_required_methods_parses` | `ns_interfaces::interface_typed_var_dispatches_to_concrete` |
| `class_can_have_methods_outside_of_implements` | `ns_interfaces::class_own_method_callable_from_implements_block` |
| `class_can_be_passed_to_interface_param_when_implements` | `ns_interfaces::fuzz_bug10_generic_interface_as_function_param_dispatches` |
| `calling_method_through_interface_typed_param` | `ns_interfaces::fuzz_bug21_interface_param_field_uses_own_view_in_requires_chain` |
| `class_with_same_named_methods_from_two_interfaces_compiles` | `ns_interfaces::as_projection_same_signature_runtime` |
| `distinct_method_names_across_interfaces_is_not_ambiguous` | `ns_interfaces::self_as_projection_call_inside_unrelated_block` |
| `match_with_catchall_on_interface_is_exhaustive` | `ns_interfaces::match_open_interface_with_wildcard_works` |
| `match_narrows_interface_to_concrete_class` | `ns_interfaces::match_narrows_to_concrete_field_access` |
| `reflect_class_implements_interface` | `ns_interfaces::reflect_implements_true_for_implementor` |
| `reflect_implemented_by_is_reverse` | `ns_interfaces::reflect_implemented_by_inverse_runtime` |
| `calling_implements_block_method_works` | `ns_interfaces::class_own_method_callable_from_implements_block` |
| `nominal_subtype_via_requires_chain` | `ns_interfaces::cast_to_parent_interface_via_requires_runtime` |
| `default_method_returning_self_is_allowed` | `ns_interfaces::self_return_type_carries_concrete_class` |
| `interface_fields_auto_link_from_class_fields` | `ns_interfaces::interface_fields_constructed_with_class_keys` |
| `interface_fields_auto_link_with_method` | `ns_interfaces::match_destructures_interface_fields` |
| `all_required_parents_satisfied_is_ok` | `ns_interfaces::interface_field_via_requires_chain_runtime` |
| `user_scenario_requires_field_check` | `ns_interfaces::interface_field_via_requires_chain_runtime` |
| `out_of_body_implements_for_class_compiles` | out-of-body reflection and default-call tests in `ns_interfaces` |
| `out_of_body_implement_singular_keyword_compiles` | `ns_comparison_driver::driver_dispatches_custom_class_equals` |
| `out_of_body_implements_for_primitive_method_only_compiles` | `ns_interfaces::out_of_body_implements_for_primitive_as_projection_runtime` |
| `out_of_body_implements_for_primitive_satisfies_interface_type` | `ns_interfaces::out_of_body_implements_for_primitive_as_projection_runtime` |
| `out_of_body_implements_for_primitive_as_projection_compiles` | `ns_interfaces::out_of_body_implements_for_primitive_as_projection_runtime` |
| `out_of_body_dispatch_through_interface_typed_var` | `ns_interfaces::default_call_from_out_of_body_override_runtime` |
| `requires_chain_interface_subtype_is_ok` | `ns_interfaces::cast_to_parent_interface_via_requires_runtime` |
| `form1_syntax_parses_without_errors` | `ns_interfaces::form1_dispatches_through_interface_typed_var` |
| `form1_blanket_has_no_compile_errors_at_all` | `ns_interfaces::form1_dispatches_through_interface_typed_var` |
| `form1_bounded_syntax_parses_without_errors` | `ns_interfaces::form1_bounded_generic_receiver_dispatches_when_bound_satisfied` |
| `form2_syntax_parses_without_errors` | `ns_interfaces::form2_dispatches_through_interface_typed_var` |
| `existing_concrete_implements_for_still_works` | `ns_interfaces::out_of_body_implements_for_primitive_as_projection_runtime` |
| `implements_for_concrete_container_target_is_allowed` | `ns_interfaces::generic_rule_for_list_receiver_dispatches` |
| `inferred_interface_method_reference_enforces_receiver_bound` | `ns_item_projections::bare_interface_method_value_forms` |
| `ordering_on_same_concrete_primitive_is_ok` | `ns_operators::less_than` |
| `ordering_on_user_class_implementing_compare_is_ok` | `ns_operators::ordering_on_user_class_dispatches_compare` |
| `ordering_on_bounded_type_var_is_ok` | `ns_operators::generic_ordering_matches_opcodes` |
| `arithmetic_on_user_type_implementing_add_is_ok` | `ns_operators::operator_add_user_class` |
| `arithmetic_output_type_is_the_impl_output` | `ns_operators::operator_output_differs_from_self` |
| `negate_on_user_type_implementing_negate_is_ok` | `ns_operators::operator_negate_user_class` |
| `arithmetic_on_union_all_pairs_valid_is_ok` | `ns_operators::operator_union_dispatch` |
| `arithmetic_on_mixed_primitive_union_uses_cartesian_product` | `ns_operators::operator_mixed_primitive_union_add` |
| `negate_output_type_is_the_impl_output` | `ns_operators::operator_negate_output_differs_from_self` |
| `compound_assign_on_user_type_with_self_output_is_ok` | `ns_operators::operator_compound_assign_user_class` |

Delete `_unused_imports_compile` from the same file as dead code; it has no
fixture or assertion beyond keeping now-unneeded Rust imports referenced.

### Associated types and class generics: 20 existing-native deletions

| Rust test | Existing native test or coverage |
|---|---|
| `associated_type_declaration_forms_compile` | declaration/default/bound execution across `ns_interfaces_associated_types` and `ns_projection_patterns` |
| `default_method_may_return_self_call_yielding_associated_type` | `default_method_self_call_yielding_associated_type_runs` |
| `fully_bound_associated_type_interface_values_expose_projected_methods` | `runtime_dispatch_substitutes_class_typevar_in_associated_type_binding` |
| `associated_type_projection_from_generic_interface_bound_compiles` | `blanket_impl_self_associated_projection_uses_bounded_typevar` |
| `associated_type_binding_in_generic_bound_concretizes_projection` | blanket self/concrete projection runtime tests |
| `qualified_projection_on_typevar_accepts_proven_interface_bound` | `upcast_of_bounded_typevar_preserves_associated_bindings` |
| `qualified_associated_type_projection_disambiguates_generic_interfaces` | `foreign_instantiation_projection_keeps_its_own_realization` |
| `associated_type_default_can_reference_interface_generic` | `associated_type_default_typevar_satisfies_declared_bound` |
| `associated_type_binding_overrides_default_on_interface_value` | default-member pin override/reflection tests in `ns_projection_patterns` |
| `match_narrowing_distinguishes_interface_associated_bindings_in_union` | `runtime_match_filters_by_associated_type_binding` |
| `interface_destructure_head_accepts_associated_bindings` | `runtime_destructure_filters_by_associated_type_binding` |
| `out_of_body_implements_can_bind_associated_types` | blanket self/concrete projection runtime tests |
| `implements_block_associated_type_binding_is_honored` | `runtime_dispatch_substitutes_class_typevar_in_associated_type_binding` |
| `abstract_associated_projection_uses_declared_bound_for_members` | `abstract_associated_projection_bound_method_dispatch_runs` |
| `abstract_associated_projection_pins_self_for_bound_methods` | `abstract_associated_projection_bound_method_dispatch_runs` |
| `default_associated_interface_omission_is_positive_for_default_witness` | default-member default/reflection tests in `ns_projection_patterns` |
| `required_parent_associated_binding_controls_member_type` | `upcast_of_bounded_typevar_preserves_associated_bindings` |
| `class_generic_bound_exposes_interface_members` | `ns_interfaces_class_generics::class_generic_bound_member_access_runs_in_class_method` |
| `class_generic_bound_accepts_implementing_class` | `ns_interfaces_class_generics::class_generic_bound_member_access_runs_in_class_method` |
| `method_generic_bound_referencing_class_param_is_satisfied` | `ns_interfaces_class_generics::method_associated_projection_preserves_class_generic` |

### Smaller interface and non-interface files: 14 existing-native deletions

| Rust test | Existing native test or coverage |
|---|---|
| `runtime_interface_witnesses::open_interface_occurrence_fails_at_render_boundary` | `ns_runtime_render_identity` open-interface second-specialization rejection |
| `from_json_interface::from_json_interface_compiles` | override-dispatch tests in `ns_from_json_interface` |
| `to_json_interface::to_json_via_interface_is_allowed` | override-dispatch tests in `ns_to_json_interface` |
| `to_string_interface::to_string_via_interface_is_allowed` | override/direct-call tests in `ns_to_string_interface` |
| `type_value_equality::permuted_union_double_equals_is_canonical` | `ns_type_reflection::union_order_eq_canonical_in_function` |
| `type_value_equality::of_value_reuses_the_static_class_identity` | `ns_reflect_type_of::of_value_class_instance` |
| `fs::fs_symlink_onto_an_existing_path_errors` | `ns_fs::fs_symlink_onto_existing_path_errors` |
| `spawn_semantics::racing_never_awaited_successful_spawn_returns_cleanly` | `ns_spawn_throws::racing_successful_spawn_returns_cleanly` |
| `sort_compare::spike_1a_projection_valued_assoc_binding_in_blanket_impl_compiles` | `ns_comparable_sort::spike_1a_projection_normalizes_to_never_at_concrete_callsite` |
| `sort_compare::spike_1a_symbolic_projection_in_generic_signature_compiles` | `ns_comparable_sort::spike_1a_symbolic_projection_propagates_through_generic_code` |
| `sort_compare::user_class_cmp_direct_call_mir_optimized` | `ns_comparable_sort::phase2_user_class_compare_direct_call_in_main` |
| `sort_compare::element_is_dispatch_compiles` | `ns_comparable_sort::element_is_dispatch_runtime_both_branches` |
| `sort_compare::element_is_dispatch_int_callsite_normalizes_to_never` | `ns_comparable_sort::element_is_dispatch_runtime_both_branches` |
| `backtick_block_diagnostics::cross_site_let_has_no_spurious_diagnostic` | `ns_backtick_strings::backtick_cross_site_let_binds_into_concat_scope` executes the exact expression and the corpus diagnostic snapshot guards against hidden diagnostics |

## Duplicate and overlap survey

Deduplicate before porting phase 1. Otherwise this migration will preserve the
current compile cost in a different form.

The expanded static audit found 16 gross collapse groups. Four groups are
already wholly inside the 86 existing-native deletions, so they do not save an
additional test. Of the remaining 12, 11 are exact, matrix, or strict-
subsumption collapses and one is the near-identical E0126 pair. This is why the
net deletion answers are 98 hard-exact and 99 recommended, rather than 87 +
16.

### Byte-equivalent Rust fixture pairs

These pairs use the same BAML source apart from whitespace/comments and should
become one assertion each:

- `basic_interface_parses` and `interface_fields_auto_link_with_method`
- `interface_with_only_fields_parses` and
  `class_field_auto_satisfies_interface_field_with_empty_implements_block`
- `out_of_body_implements_for_primitive_method_only_compiles` and
  `existing_concrete_implements_for_still_works`
- `form1_syntax_parses_without_errors` and
  `form1_blanket_has_no_compile_errors_at_all`
- `conjunction_ambiguous_source_fields_keep_field_diagnostic` and
  `conjunction_ambiguous_source_fields_diagnostic_names_ambiguity`
- `ordering_on_different_concrete_types_is_rejected` and
  `ordering_diagnostic_renders_operator_symbol`
- `unresolvable_type_arg_in_implements_clause_is_error` and
  `implements_target_arg_error_is_reported_exactly_once`; keep the latter and
  carry over the former's E0002 assertion
- `phase5_class_without_compare_sort_is_compile_error` and
  `phase6_sort_error_message_names_the_array_type`; keep the stricter phase-6
  message assertion

`out_of_body_implements_for_primitive_field_bearing_interface_is_error` and
`wf3_out_of_body_primitive_field_bearing_is_e0126_pins` are also near-identical;
one only adds a trivial `main`.

The first four pairs above are already counted in existing-native coverage.
The next four save one new native definition each. The E0126 near-pair saves
one more under the recommended policy.

### Additional same-source slow-path collapses

The timing-led audit adds three hard-exact reductions that were not in the
initial integration-only duplicate pass:

- Drop `unresolved_name_in_interp_reports_cleanly` when porting
  `unresolved_name_in_interp_reports`. The former checks only the untagged
  `${ nope }` case; the latter checks that identical case plus the prompt path.
- Merge `quoted_string_escape_sequences_decode_to_exact_bytes` and
  `quoted_string_escape_lengths_match` into one native value-and-length matrix.
  They share one source string and currently invoke `baml_test!` 11 times.
- Merge `escaped_backslash_at_string_boundary` and
  `escaped_backslash_boundary_lengths` into one native boundary matrix over the
  same source. Keep `replace_all_with_backslash_arguments` separate.

These save three logical native definitions. They do not change the 841
current nextest entries that can leave, because all six Rust definitions were
already in that migration count.

### Exact or stronger existing native coverage

High-confidence direct duplicates include:

- `type_value_equality::permuted_union_double_equals_is_canonical` ->
  `ns_type_reflection::union_order_eq_canonical_in_function`
- `type_value_equality::of_value_reuses_the_static_class_identity` ->
  `ns_reflect_type_of::of_value_class_instance`
- `sort_compare::user_class_cmp_direct_call_mir_optimized` ->
  `ns_comparable_sort::phase2_user_class_compare_direct_call_in_main`
- `fs::fs_symlink_onto_an_existing_path_errors` ->
  `ns_fs::fs_symlink_onto_existing_path_errors`
- `spawn_semantics::racing_never_awaited_successful_spawn_returns_cleanly` ->
  `ns_spawn_throws::racing_successful_spawn_returns_cleanly`

Substantial interface coverage is already stronger natively:

- field auto-linking, field shadowing, and same-name fields across unrelated
  interfaces in `ns_interfaces::interfaces`;
- interface-typed dispatch, concrete match narrowing, `implements` reflection,
  reverse `implemented_by`, and `requires`-chain casts;
- form-1/form-2 blanket dispatch and out-of-body primitive projection/calls;
- generic class-bound member access and associated-projection substitution;
- associated-type default-method dispatch, abstract projection bounds, nested
  substitution, match/destructure filtering, and bounded defaults;
- `from_json`, `to_json`, and `to_string` interface dispatch; and
- the positive operator/ordering matrix in `ns_operators` and
  `ns_comparison_driver`.

For these clusters, delete the Rust test when its only extra assertion is
compile success. Preserve a focused Rust test only where it checks host shape,
metadata, or a specific optimization level.

### Same-source optimizer matrices

These should become one native behavioral test unless Opt1 coverage is itself
the contract:

- both `loop_reassignment` tests use identical source at Opt1 and Opt2;
- two pairs in `short_circuit_locals` use identical source at Opt1 and Opt2;
- `map_aliasing::int_list_identity_survives_callee_mutation_in_loop` and its
  `_at_o2` variant use identical source.

These four matrices contribute four of the 11 incremental hard-exact
collapses. The native corpus currently compiles at Opt2. If preserving explicit
Opt1 coverage is a requirement, keep the Opt1 Rust representatives and reduce
the reported net savings by four: 94 hard-exact or 95 recommended.

An additional non-exact consolidation is available: the three
`implements_target_associated_type_binding_errors*` recovery tests can compile
three fixtures in one table-driven native test. That saves two definitions but
is intentionally outside the exact/recommended duplicate count.

`optimizer_stack::optimizer_preserves_behavior_at_each_level` overlaps native
Opt2 behavior, but Rust uniquely covers Opt0 and Opt1. `prompt_tag_runtime`
overlaps native prompt role/media behavior, but its host `PromptAstSimple`
matching is a distinct Rust contract.

### Low-effort additions, not duplicates

- `generic_union_returns.rs` already points at BAML source containing its four
  helpers; add four native tests beside them.
- `short_circuit_locals.rs` and `type_kinds.rs` exercise functions already
  present under `ns_fixtures`; those fixture files only need test blocks.

## Recommended order of work

1. **No-new-coverage removals.** Delete the 86 existing-native Rust duplicates
   and dead import smoke test. Apply the CLI layered-dedup set at the chosen
   process-boundary policy; keep the Go lifecycle as one combined two-pass E2E
   if end-to-end cleanup coverage is desired. Defer the one CLI case that needs
   a native replacement (`assert.approx_equal`) to phase 1d.
2. **Setup-cost quick wins.** Make the sampled-cache `None` test context-only;
   combine the four mixed cache oracles; fold the discovery knob into the
   unfiltered oracle; share profile bytecode cache; and replace the two skill
   probes with `fmt --dry-run`. Extract rename reporting before reducing its
   three generation passes. Measure phase timings, not only nextest duration.
3. **Compiler-test cleanup.** Replace the generated PPIR blanket with the
   eight-project recovery allowlist, remove the ten identified MIR goldens, and
   consolidate the five TIR families from 106 definitions to 29. Land these as
   separate changes so snapshot deletions and runtime effects are reviewable.
4. **Native dedup gate.** Merge the 11 hard-exact/strict-subsumption groups,
   including the two string matrices and weaker backtick unresolved-name case.
   Decide which optimizer-level matrices are intentional before creating native
   copies.
5. **Phase 1a: all-native files.** Port the 81 not-already-covered definitions
   in the phase-1-capable table, collapsing the optimizer pairs as chosen. The
   existing BAML fixture files make several of these nearly mechanical.
6. **Phase 1b: interface positives.** Migrate the 114 remaining interface
   phase-1 definitions in
   small coherent groups: fields, `requires`, blankets/out-of-body impls,
   class generics, associated types, and runtime witnesses. Reuse existing
   native coverage instead of porting duplicates.
7. **Phase 1c: other mixed suites.** Migrate the remaining BAML-observable
   runtime/package/Session/reflection cases listed in the appendix.
8. **Phase 1d: library and CLI candidates.** Port the eight engine/string cases
   from the library audit and the `assert.approx_equal` CLI wrapper. Merge the
   two pairs of string value/length checks while doing so.
9. **Phase 2a: high-fidelity diagnostics.** Port code/message/count/order and
   primary-span tests through `ns_compiler` helpers, with one Rust-vs-reflect
   parity probe per diagnostic family.
10. **Phase 2b: close reflection gaps.** Expose severity/phase/secondary
   locations if those assertions should move; fix E0139 and mounted dispatch;
   add a successful-warning helper and use public compile diagnostics for the
   two current warning cases.
11. **Rust residue.** Add or retain a short comment on each survivor naming the
   host/compiler capability that makes native BAML insufficient.

Run native migrations through the local CLI and validate Rust removals with
nextest, following `TEST_INSTRUCTIONS.md` and the repository's `AGENTS.md`.

## Appendix: exact mixed-suite ledger

The interface-owned mixed names are listed above. The following ledger covers
the other mixed files. A name under Phase 1 is an immediate native candidate;
a name under Phase 2 needs the reflected diagnostic path. Rust-only exceptions
are called out explicitly.

### `anyclass_reflection.rs` (5 / 2 / 0)

Phase 1:

- `requires_and_bounded_impl_membership_agree_for_real_classes`
- `runtime_minted_class_narrows_and_exercises_the_complete_surface`
- `membership_is_class_only_and_kind_views_are_ordinary_classes`
- `reflected_membership_and_static_field_handles_agree_with_narrowing`
- `concrete_members_keep_precedence_until_explicitly_narrowed`

Phase 2:

- `requires_any_class_rejects_a_primitive_implementor`
- `any_class_blanket_bound_does_not_expose_members_on_nonclasses`

### `backtick_block_diagnostics.rs` (2 / 14 / 0)

Phase 1:

- `well_formed_blocks_have_no_structural_diagnostic`
- `cross_site_let_has_no_spurious_diagnostic` — delete without a port; exact
  native execution coverage already exists

Phase 2:

- `unclosed_for_diagnoses`
- `unclosed_if_diagnoses`
- `mismatched_for_close_diagnoses`
- `mismatched_if_close_diagnoses`
- `stray_endfor_diagnoses`
- `stray_endif_diagnoses`
- `stray_else_diagnoses`
- `out_of_order_else_branches_diagnose`
- `empty_interpolation_diagnoses`
- `for_header_accepts_const_binding`
- `unresolved_name_in_interp_reports_cleanly`
- `unresolved_name_in_interp_reports`
- `unresolved_name_in_statement_only_interp_reports`
- `nullable_interp_still_reports_on_original_span_only`

Drop `unresolved_name_in_interp_reports_cleanly` when porting the stronger
`unresolved_name_in_interp_reports`, which contains the same untagged case plus
the prompt case. `empty_interpolation_diagnoses` and
`for_header_accepts_const_binding` are successful-warning exceptions and must
use public `Package.compile(...).diagnostics()`, not `CompileDiagnostics`'s
`_compile` failure path.

### `cleanup.rs` (1 / 4 / 0)

Phase 1:

- `baml_collect_garbage_runs_unreachable_cleanup_before_returning`

Phase 2:

- `cleanup_with_extra_param_is_compile_error`
- `cleanup_with_non_void_return_is_compile_error`
- `cleanup_with_default_on_self_is_compile_error`
- `cleanup_with_throws_is_compile_error`

### `constructor_consistency.rs` (2 / 1 / 0)

Phase 1:

- `reserved_words_are_rejected_as_constructor_names`
- `enum_constructor_has_its_precise_declared_kind`

Phase 2:

- `removed_reader_spellings_are_ordinary_unresolved_names`

The first phase-1 case already catches a reflection-constructor
`CompilationError` inside otherwise valid executable BAML; it does not need an
invalid outer program.

### `convert_boundary_regressions.rs` (2 / 0 / 2)

Phase 1:

- `throw_of_non_literal_expression_compiles`
- `generic_llm_function_with_generic_return_compiles`

Rust-only:

- `error_bearing_program_returns_recoverable_error`
- `thrown_parameter_named_like_a_catch_binding_is_not_a_rethrow`

The Rust cases inspect lowering/bytecode internals and emitted throw metadata.

### `match_union_typevar.rs` (3 / 1 / 0)

Phase 1:

- `tag_or_value_union_typevar_match_compiles`
- `typevar_arm_first_leaves_concrete_arms_reachable`
- `typevar_union_with_class_sibling_compiles`

Phase 2:

- `tag_or_value_without_typevar_arm_is_non_exhaustive`

### `reflect_call_any.rs` (32 / 0 / 2)

Phase 1:

- `call_any_infers_pins_from_function_value`
- `call_any_inferred_and_explicit_class_returns_match`
- `call_any_inferred_and_explicit_list_returns_match`
- `call_any_inferred_and_explicit_map_returns_match`
- `call_any_inferred_and_explicit_throws_match`
- `call_any_dispatches_named_args`
- `json_serializes_unknown_call_any_results_by_runtime_value`
- `json_rejects_unknown_uint8array_call_any_results`
- `call_any_absent_optional_fires_callee_default`
- `call_any_rejects_missing_key_and_type_mismatches`
- `call_any_widens_int_for_float_param`
- `call_any_invalid_argument_error_carries_types`
- `call_any_propagates_callee_typed_throw`
- `call_any_heterogeneous_tool_map_dispatch`
- `call_any_lambda_callee`
- `call_any_bound_method_callee`
- `signature_reports_types_and_param_split`
- `signature_bound_method_drops_receiver`
- `instantiated_generic_function_reflects_precisely_and_dispatches`
- `pinned_call_any_declares_unspecialized_generic_compilation_error`
- `signature_object_literal_construction`
- `a_scoped_runtime_type_reifies_as_the_callee_type_argument`
- `runtime_enum_renders_and_alias_round_trips_through_sap`
- `nested_unreflect_runtime_type_renders_through_a_generic_wrapper`
- `runtime_enum_identity_and_metadata_are_preserved`
- `duplicate_runtime_enum_value_uses_compiler_diagnostic`
- `empty_runtime_enum_fails_at_the_render_boundary`
- `a_scoped_runtime_type_is_an_ordinary_streaming_type_argument`
- `get_function_refuses_an_unspecialized_generic_through_any_function`
- `call_any_still_invokes_a_non_generic_companion`
- `get_function_refuses_an_unspecialized_generic_companion`
- `get_function_still_extracts_a_non_generic_companion`

Rust-only:

- `call_any_inferred_class_return_crosses_sys_op_and_host_boundaries`
- `call_any_rejects_a_return_outside_inferred_r`

The first checks a host boundary; the second mutates bytecode so a callee lies
about its return type.

### `runtime_builders_and_pending_types.rs` (7 / 2 / 0)

Phase 1:

- `self_recursive_employee_renders_and_parses`
- `mutually_recursive_group_freezes_together_and_is_idempotent`
- `frozen_mutation_and_unresolved_call_name_the_builder`
- `duplicate_field_structured_diagnostic_has_a_null_span`
- `recursive_pending_fields_carry_metadata`
- `recursive_pending_field_metadata_reaches_the_rendered_schema`
- `metadata_survives_an_already_resolved_pending_reference`

Phase 2:

- `pending_type_is_not_a_static_subtype_of_type`
- `a_type_binding_operand_must_be_a_type_value`

### `runtime_classes_and_composites.rs` (9 / 1 / 2)

Phase 1:

- `scenario_2_saved_form_class_renders_parses_and_assert_reads`
- `scenario_3_tool_union_dispatches_by_runtime_class`
- `empty_runtime_union_throws_the_reserved_diagnostic`
- `class_order_identity_composites_and_to_baml_are_canonical`
- `constructed_type_to_baml_compiles_to_equivalent_new_identity`
- `runtime_class_readback_preserves_exact_type_and_all_metadata`
- `runtime_class_validation_is_eager_and_uses_compiler_diagnostics`
- `same_fields_in_different_orders_render_independently`
- `get_field_missing_and_wrong_type_throw_compilation_diagnostics`

Phase 2:

- `unknown_get_field_method_is_rejected_at_compile_time`

Rust-only:

- `an_anonymous_class_instance_crosses_as_an_opaque_handle`
- `a_runtime_compiled_class_instance_crosses_as_an_opaque_handle`

Both Rust cases inspect host `TaggedHeapHandleKind::RuntimeValue` values.

### `runtime_diagnostic_consistency.rs` (1 / 2 / 0)

Phase 1:

- `e5_structured_diagnostics_have_null_spans`

Phase 2:

- `m1_bare_computed_generic_argument_diagnostic_names_unreflect`
- `e1_duplicate_serialized_key_matches_static_code_and_message`

### `runtime_package_compile.rs` (21 / 0 / 3)

Phase 1:

- `method_bearing_interface_crosses_a_mount`
- `mounted_method_stubs_do_not_shadow_their_owner`
- `runtime_compiled_to_string_on_a_reflected_type_dispatches`
- `package_finish_refuses_session_compile_artifact`
- `scenario_5_compiles_parses_and_encodes_runtime_schema`
- `runtime_compiled_code_dispatches_through_static_stdlib_impls`
- `exact_runtime_types_do_not_regress_static_generic_json_calls`
- `render_prompt_uses_runtime_package_schema`
- `package_declarations_are_created_once_and_compiles_are_generative`
- `runtime_package_and_declarations_survive_major_collection`
- `runtime_compile_honors_namespaces_and_mounted_dependencies`
- `mounted_runtime_types_do_not_leak_into_phantom_stub_diagnostics`
- `get_function_absence_is_null`
- `get_function_mismatch_throws_compiler_subtyping_diagnostic`
- `unspecialized_generic_get_function_reports_reflection_limit`
- `function_listing_omits_unspecialized_generics`
- `generic_function_companion_extraction_reports_reflection_limit`
- `generic_function_companion_remains_in_function_listing`
- `alias_maps_are_order_independent_and_cannot_shadow_stdlib`
- `package_tests_enumerate_invocable_zero_arg_functions`
- `one_package_under_two_aliases_is_one_package`

Rust-only:

- `successful_compile_runs_init_before_returning_package`
- `rejected_compile_returns_real_diagnostic_without_running_init`
- `scenario_6_extracts_and_calls_the_live_aliased_function`

These assert host report/logger state or the host structured-throw
representation.

### `runtime_session.rs` (34 / 4 / 6)

Phase 1:

- `session_compile_artifact_is_consumed_once`
- `session_finish_refuses_package_compile_artifact`
- `session_synthetic_step_names_do_not_collide_with_user_bindings`
- `session_commits_a_binding_and_returns_a_contracted_value`
- `session_reimports_instance_fields_without_changing_the_value`
- `session_reads_fields_from_mounted_return_types_inline_and_after_binding`
- `session_compiler_sees_with_types_exports_from_dependencies`
- `session_export_alias_preserves_nested_mounted_field_types`
- `scenario_7_verbatim_semantics_and_containment`
- `compilation_diagnostics_use_stable_submission_names`
- `package_current_is_not_available_in_session_source`
- `runtime_contracts_check_before_execution`
- `declaration_redefinition_is_newest_wins_without_relinking_old_code`
- `session_client_declarations_do_not_perform_network_io`
- `binding_typed_session_step_values_publish_as_unknown`
- `scoped_runtime_type_bindings_persist_and_rebind_between_submissions`
- `identical_declarations_in_two_sessions_are_distinct_types`
- `host_interface_dispatch_uses_session_class_provenance`
- `mutually_recursive_session_lets_diagnose_without_panicking`
- `session_let_named_json_does_not_shadow_json_package_paths`
- `session_top_level_lets_widen_literal_initializers`
- `session_let_widening_is_visible_through_member_resolution`
- `session_let_widening_moves_match_exhaustiveness_to_the_base_type`
- `session_let_rebinding_across_submissions_is_unaffected`
- `session_let_annotations_are_still_rejected`
- `session_let_narrowing_still_sees_the_literal`
- `method_calls_on_session_let_bindings_dispatch`
- `method_calls_on_a_session_binding_work_in_its_own_submission`
- `session_binding_field_access_and_indexing_still_work`
- `client_declaration_methods_dispatch`
- `a_session_let_still_shadows_at_a_new_type`
- `a_session_assignment_does_not_shadow_a_user_binding`
- `session_assignment_values_keep_their_shape`
- `session_assignments_at_the_binding_type_are_unaffected`

Phase 2:

- `session_assignment_at_another_type_fails_like_ordinary_code`
- `the_session_assignment_crash_shape_dies_at_compile_time`
- `session_compound_assignments_check_like_ordinary_code`
- `a_session_assignment_is_no_stricter_than_ordinary_code`

Rust-only:

- `session_evaluation_error_preserves_structured_cause_for_host`
- `session_evaluation_error_preserves_string_cause_for_host`
- `concurrent_session_eval_throws_busy_and_recovers`
- `cancelled_session_eval_releases_lease_and_preserves_committed_prefix`
- `escaped_session_type_retains_provenance_only_while_handle_is_live`
- `five_hundred_evals_have_flat_latency_and_bounded_artifacts`

### `runtime_type_bindings.rs` (24 / 2 / 0)

Phase 1:

- `with_types_carries_runtime_witnesses_into_consumer_compilation`
- `scenario_four_pattern_two_mounts_a_runtime_type_as_a_static_name`
- `scoped_type_binding_evaluates_once_types_contracts_and_leaves_as_unknown`
- `type_bindings_work_in_lambdas_and_nested_shadowing_uses_distinct_slots`
- `with_types_rejects_collisions_with_existing_exports`
- `with_types_rejects_non_identifier_keys`
- `interface_impl_methods_keep_runtime_type_definitions`
- `agent_run_parses_a_reflected_output_type`
- `agent_runner_renders_reflected_output_schemas_on_the_wire`
- `minted_type_identity_survives_interface_dispatch`
- `interface_impl_methods_look_up_a_type_keyed_registry`
- `dispatch_identity_separates_distinct_mints_and_leaves_static_generics_alone`
- `runtime_package_declarations_keep_definitions_and_identity`
- `static_class_slots_are_not_answered_from_a_same_named_runtime_definition`
- `same_named_declarations_from_two_packages_keep_separate_identities`
- `dispatch_identity_covers_owner_and_method_slots_together`
- `dispatch_identity_covers_a_runtime_enum_slot`
- `a_package_declarations_identity_never_reaches_rendered_output`
- `docstrings_render_for_static_minted_and_mounted_types`
- `a_coercion_error_names_a_runtime_class_as_its_source_spelled_it`
- `a_decode_error_names_a_runtime_class_as_its_source_spelled_it`
- `a_runtime_compile_diagnostic_names_a_mounted_runtime_class`
- `a_runtime_type_test_does_not_match_another_packages_same_named_class`
- `an_output_format_schema_describes_each_packages_own_class`

Phase 2:

- `top_level_runtime_type_binding_is_rejected`
- `type_binding_name_is_not_visible_outside_its_block`

### `sort_compare.rs` (11 / 13 / 1)

Phase 1:

- `spike_1a_projection_valued_assoc_binding_in_blanket_impl_compiles`
- `spike_1a_symbolic_projection_in_generic_signature_compiles`
- `user_class_cmp_direct_call_mir_optimized`
- `phase3_never_binding_callsite_normalizes_to_never`
- `phase3_error_binding_callsite_throws_it`
- `phase3_out_of_body_impl_on_builtin_normalizes_to_never`
- `phase3_out_of_body_impl_on_builtin_with_error_throws_it`
- `phase3_defaulted_assoc_override_satisfies_bare_bound`
- `element_is_dispatch_compiles`
- `element_is_dispatch_int_callsite_normalizes_to_never`
- `element_is_dispatch_float_callsite_normalizes_to_never`

Phase 2:

- `spike_1a_projection_with_concrete_error_requires_handling`
- `spike_1a_diagnostics_path_survives_non_qualifying_projection`
- `spike_1b_non_qualifying_element_is_compile_error_at_callsite`
- `spike_1c_interface_typed_values_cannot_call_two_self_method`
- `missing_undefaulted_assoc_binding_is_error`
- `phase3_error_binding_unhandled_is_compile_error`
- `phase5_union_int_float_sort_is_compile_error`
- `phase5_optional_element_sort_is_compile_error`
- `phase5_sort_by_key_nullable_key_is_compile_error`
- `phase5_union_int_string_sort_is_compile_error`
- `phase5_class_without_compare_sort_is_compile_error`
- `match_dispatch_array_type_arms_reachable_but_t_is_not_refined`
- `phase6_sort_error_message_names_the_array_type`

Rust-only:

- `perf_large_int_array_uses_native_fast_path`

### `tagged_template_lowering.rs` (5 / 10 / 0)

Phase 1:

- `tagged_template_with_interp_for_and_if_survives_front_end`
- `tagged_template_as_let_initializer_survives_front_end`
- `tagged_tag_marked_valid_no_errors`
- `tagged_interp_resolves_body_lambda_param`
- `tagged_for_binding_in_scope_inside_for_body`

Phase 2:

- `tagged_tag_unmarked_function_errors`
- `tagged_tag_unresolved_name_reports_once`
- `tagged_tag_not_a_function_errors`
- `tagged_tag_bad_body_param_wrong_name_errors`
- `tagged_tag_bad_body_param_wrong_return_errors`
- `tagged_tag_missing_body_param_errors`
- `tagged_interp_unknown_name_reports_unresolved`
- `tagged_for_binding_out_of_scope_after_endfor`
- `tagged_for_non_iterable_collection_reports_not_iterable`
- `tagged_body_param_does_not_leak_after_template`

### `wildcard_expression_holes.rs` (0 / 3 / 6)

Phase 2:

- `wildcard_turbofish_uninferable_is_rejected`
- `wildcard_generic_apply_value_is_rejected`
- `wildcard_upcast_target_is_rejected`

Rust-only because these are ignored future-feature tests:

- `wildcard_turbofish_race_work_wins`
- `wildcard_turbofish_race_timer_fires`
- `wildcard_object_ctor_infers_from_field`
- `wildcard_object_ctor_mixed_partial`
- `wildcard_object_ctor_nested_hole`
- `wildcard_object_ctor_phantom_param_recovers`

### `wildcard_type_inference.rs` (4 / 12 / 0)

Phase 1:

- `wildcard_future_error_is_inferred_and_runs`
- `wildcard_map_value_is_inferred`
- `wildcard_nested_union_member_is_inferred`
- `throws_wildcard_absorbs_stdlib_throws`

Phase 2:

- `wildcard_future_error_is_not_erased`
- `wildcard_never_annotation_still_rejected`
- `wildcard_does_not_mask_wrong_explicit_arg`
- `wildcard_in_return_type_is_rejected`
- `wildcard_in_param_type_is_rejected`
- `wildcard_in_field_type_is_rejected`
- `wildcard_in_generic_bound_is_rejected`
- `wildcard_in_requires_clause_is_rejected`
- `wildcard_nested_in_throws_is_rejected`
- `wildcard_in_interface_method_throws_is_rejected`
- `throws_wildcard_caller_sees_full_union`
- `throws_plain_stays_exhaustive`
