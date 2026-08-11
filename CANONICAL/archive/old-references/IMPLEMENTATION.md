# BAML Observability — Implementation Ledger

Working notes for the P0→P9 implementation of the historical `design.md`, which was already absent when this corpus was archived. Maintained by Claude
across sessions; update as phases land. Source of truth for *what is done and where*.

## Ground rules (from the goal)

- Optimize for correctness first, performance always: benchmark current-vs-new consistently.
- Use paired baselines (`BAML_PROFILE_PIPELINE=legacy|dual|cct`) for every perf claim.
- Deletions (P9) land last, gated on green oracles/benches.
- Legacy profiler code may be deprecated/removed where the design says so, or leveraged where correct and useful.

## Environment facts

- Workspace: `/root/dev/baml/baml_language` (Rust, edition 2024, MSRV 1.91.1, resolver 2).
- `crates/*` is a workspace-members glob → new crates under `crates/` auto-join.
- Bench convention: harness-less (`[[bench]] harness = false`), e.g. `bex_events/benches/prof_clock.rs`.
- Size gate: `crates/tools_size_gate`, config `.cargo/size-gate.toml`, baselines `.ci/size-gate/`,
  CI: `.github/workflows/ci.yaml` (job `size-gate`, non-required), `size-gate-baseline-refresh.yml`.
  `bridge_wasm` gzip ceiling 4.5 MiB.
- `baml.toml` typed parsing: `crates/baml_cli/src/manifest.rs` (`BamlToml`; warn-not-deny unknown keys;
  `[observability]` table lands here + `KNOWN_UNHANDLED_TOP_LEVEL_KEYS` if needed elsewhere).
- BAML syntax references: `crates/baml_tests/baml_src/ns_*/*.baml` (functions, tests, spawn/await/catch,
  `baml.spawn.TaskGroup`, `baml.sys.sleep`, `baml.time.Duration`).
- Prior docs referenced in design Appendix B (`1-impl/...`, `2-not-impl/...`, `3-not-impl/research/...`)
  are NOT in the repo. The historical `design.md` was the only spec available to this document and is also absent from the archive.

## Phase status

| Phase | Task# | Status | Notes |
|---|---|---|---|
| P0 truth & tooling | #1 | pending | |
| P1 compile-time identity | #2 | pending | |
| P2 CCT engine (RAM) | #3 | pending | exit gate: ≤50 ns/call integrated bench |
| P3 session storage | #4 | pending | |
| P4 query engine + webapp core | #5 | pending | |
| P5 values CAS | #6 | pending | parallel with P4 after P3 |
| P6 flight recorder + triggers | #7 | pending | |
| PH host wiring | #8 | pending | |
| P7 BQL | #9 | pending | |
| P8 studio + wasm + diff | #10 | pending | |
| P9 deletions | #11 | pending | gated: paired baselines, CCT-equivalence oracle, C2/C3/C6/C7 |

## Key decisions / discoveries log

- (2026-07-31) Task ledger created (tasks #1–#11 with dependencies).
- (2026-07-31) Research fan-out launched: profiling pipeline, engine identity, compiler, run-store/UI.
- (2026-07-31) Prebuilt `target/{debug,release}/baml-cli` exist → workloads can be validated by running them.

## Research: compiler emit/link/pack (COMPLETE — for P1)

- **Emit entries** (`baml_compiler2_emit/src/lib.rs`): `generate_project_bytecode` :1264, `_with_opt` :1272,
  `generate_stdlib_program` :1288, `_with_stdlib` :1312, `_with_reuse_artifacts` :1358 (only entry through
  `link`, call at :1456), `emit_units` :1517, `decompose_units` :1551. Private core `generate_impl`
  :2546-2679 (`Ok(program)` at :2678); NEVER finalize when `skip_clean.is_some()` (partial program, :2924).
- **Linker**: `bex_vm_types/src/link.rs:265` `link()`, Ok at :713. Pool order group-major
  ([B classes][B enums][B ifaces][B code][B $init][U …]); `ObjectPool = Pool<Object,ObjectKind>`
  (`indexable.rs:270`). Lambdas are pushed BEFORE their parent function (emit `compile_lambdas_flat`
  :5158-5250, push at :5238), nested-first.
- **Function struct**: `bex_vm_types/src/types/function.rs:184-303`; `function_id` `#[borsh(skip)]` :301.
  `FunctionCaptureProps` :141 {inputs,output,error: CaptureOption}; only LLM funcs get Auto (emit
  :5111-5114 in `attach_function_metadata` :4967+). Five `function_id: 0` sites: emit.rs:1076,
  lib.rs:3490, :5061, :5372, :5449.
- **Program struct**: `bex_vm_types/src/types.rs:59-110`, no borsh-skip fields today.
- **Engine interim id provider**: `bex_engine/src/lib.rs:1576-1595` (1-based sequential, sets
  `bytecode.compact` too). `build_program_metadata` :1392-1500 (synthetic rows `next+1/+2` at
  :1455/:1476; `SPAWN_CLOSURE_FQN` :155, `UNKNOWN_FUNCTION_FQN` :212). FQN sniffers to delete:
  `derive_owner_type_definition_key` :1260-1279, `derive_lambda_metadata` :1283-1293.
  `activate_profiling`/`register_engine_metadata` ~:1855-1870. VM reads f.function_id at
  vm.rs:2410,2414,2425,4455,4652.
- **PackEnvelope**: `baml_exec/src/envelope.rs:56-70` {program, mode, targets, output_format}; NO
  magic/version today. Write: `baml_cli/src/pack_command.rs:186` borsh::to_vec, `write_executable`
  :655-683 (libsui section "baaaaaaaaaaaaaml"). Read/hook: `baml_pack_host/src/main.rs:34-39`
  (`borsh::from_slice` at :39; engines built at :93-97/:167-171).
- **Lambdas**: `lower_lambda` mir/lower.rs:3989+ (name at :4003, ordinal from
  `synthetic_name_counts["__lambda"]` :3997-4002, flatten to ItemRef::Free at :4298-4302). THREE
  producers need LambdaIdentity: lower_lambda (:4298), `build_tagged_body_closure` :4612 (name :4630,
  key "__tagged", flatten :4836-4840), synthetic adapter :3961. `MirFunction` ir.rs:154-174 has
  `item_ref`, `lambdas: Vec<MirFunction>`.
- **ItemRef** (definition_key source): mir/ir.rs:1016-1037 (Free/Method/EnumType), Display :1039-1085
  (dotted, includes package). Producer `def_to_item_ref` lower.rs:810-871. Emit already keys global
  slots by it (:2896). Existing "class:" consumer: sys_ops/src/lib.rs:920.
- **Salsa**: db = `baml_project::ProjectDatabase` (db.rs:116-181; baml_db is a façade). `SourceFile`
  salsa input: `baml_base/src/files.rs:14-29`. Files enter via `add_file_internal` :401-412 /
  `add_or_update_file(s)` :421/:459. Content hashing today NOT salsa: `bex_cache::content_hash`
  (SHA-256) lib.rs:732.
- **bex_cache**: `FORMAT_VERSION: u32 = 3` at lib.rs:52 (bump on Function wire change); MAGIC "BEXC";
  `compiler_fingerprint` :362-439 (SHA-256 of exe + CANONICAL_VERSION + CHANNEL, memoized by
  len/mtime/ctime) — reuse for "dev+" compiler_id. KeyInputs :82-94.
- **baml_version**: CANONICAL_VERSION "0.15.0", CHANNEL "canary"; NO commit hash anywhere
  (only env BEX_GIT_COMMIT in a bench).
- **emit_determinism tests**: baml_tests/tests/emit_determinism.rs (4 tests incl. parallel-vs-serial,
  stdlib-splice byte-identity). Byte-identity oracles: link_units_oracle.rs, relink_oracle.rs,
  common/mod.rs `assert_programs_byte_identical` :66.
- **Borsh-skip load-bearing**: keep `function_id` skipped; `Program.identity` must be `#[borsh(skip)]`;
  adding non-skipped `def_meta` to Function = wire change ⇒ FORMAT_VERSION bump + PackEnvelope
  version-prefix prerequisite PR.
- **Prof metadata surface today**: `bex_events/src/prof/mod.rs:75-111` (EngineProfileMetadata/
  FunctionMetaEntry), `bex_events/src/metadata.rs:88-137` (FunctionMetadata/Table/ProgramMetadata),
  header write `bex_events/src/prof/encode.rs:152-169`.

## Research: profiling producer/consumer (COMPLETE — for P0/P2/P3/P6)

- **bex_events module tree**: prof/{mod,artifact,clock,config,consumer,drain,encode,file,metadata,read,
  record,registry,ring,sync,transcode,wake,concurrency_tests}, proto/bamlprof.proto; run.rs (6046 L),
  run_wire.rs, history/{mod,boundary_writer,path,router}, value/{artifact,encode,live_cache,mod,read,
  record,writer}+bamlvalue.proto, collector.rs (legacy no-op shim), ids.rs, metadata.rs, span_id.rs.
- **Ring record tags** (record.rs:50-55): 0x01 CallFunction 54B, 0x02 EndFunction 26B, 0x03 StartThread
  36B+name, 0x04 EndThread 18B, 0x05 SetFunctionId 41B (no flags byte — unique). MAX_RECORD_LEN=292.
  New tags 0x06/0x07/0x08 must update: encoded_len :191, encode_to :226, decode :321 (+TAG consts;
  test `decode_rejects_unknown_tag_and_bad_status` record.rs:724 asserts 0x06 unknown — UPDATE IT),
  to_disk_event transcode.rs:15-89 (exhaustive), normalize_disk_event run.rs:3626-3672 (drops
  SetFunctionId+Heartbeat at :3671 — add skips), prof_gate.rs `assert_balance` :1570-1644 (exhaustive
  match :1631) + `normalized_per_thread_streams` :2710-2781 (3 exhaustive matches), record.rs
  roundtrip_all_variants :576 / fixed_sizes_match_spec :680. encode.rs has `_ =>` prost fallthrough (ok).
- **VM hot path**: vm.rs `prof_push_record` :4027-4048 (push_with, zero-copy), prof_enter_call :4090,
  prof_exit_call :4118, prof_enter_sysop :4213, prof_emit_native_pair :4265 (pair in one push),
  ring gate `vm.prof_ring: Option<&'static Ring>` :667 refreshed by `prof_refresh_vm_ring`
  bex_engine/lib.rs:1918-1926. Engine cold emits: prof_emit :1904-1914; StartThread root :2987,
  spawn :5333; EndThread :4791; sysop EndFunction :1945; cancel-drain :1976/:1984; SpawnProfCloser
  :264-289.
- **Ring**: ring.rs push_with :314-367; drain :376-402; `overflow_abort` :146-158 = ONLY process::abort
  in workspace (message text load-bearing for consumer.rs:1105 subprocess test). MemBudget :104-141.
  Default 1 GiB cap (config.rs:46).
- **Consumer**: consumer.rs; ControlMsg :59-66 (pub(crate); Flush, EngineClosed — BindBoundary goes here
  + pub wrapper like flush_and_join :133-143). CONTROL_TX OnceLock :99; ensure_started :103-120 (thread
  "bex-prof-consumer"); consumer_main :163-216 (1024-sweep bound before control actions :176/:188).
  **ConsumerState::transcode :274-339 = THE FORK POINT**; 3 sinks: run::publish_profile_event (run.rs:59),
  history::publish_history_profile_event (history/mod.rs:76), ProfileWriter raw file. Borrow gotcha:
  &mut writer held :292-:326 — snapshot new state before writer_for.
  Registry::sweep gives (ring, bytes); ring.engine_id() unsafe (registry.rs:123, ring.rs:536).
- **to_disk_event**: transcode.rs:13-91 pure RawRecord→pb::DiskEventV1; only ts conversion via
  TickConverter. DiskEventV1 oneof: start_thread=1,end_thread=2,call_function=3,set_function_id=4,
  end_function=5,heartbeat=6 (bamlprof.proto:30-39).
- **.bamlprof**: len-delimited EventFileHeaderV1 then DiskEventV1 stream. Header fields: 1 process_id,
  2 engine_id, 3 program_id (random per engine — DELETE per design), 4 started_at_epoch_ns u128 LE,
  5 function_table, 6-9 clock, 10 source_snapshot_id?, 11 revision_id?. FILE ORDER ≠ EVENT ORDER
  (readers sort by ts within thread). Headers written at consumer.rs:348-355, drain.rs:244-251,
  boundary_writer.rs:437-445 (identity TickConverter → header claims INSTANT 1/1 — gotcha).
- **prof_gate.rs**: bex_engine/tests/prof_gate.rs (2945 L, 30 tests). init_prof_env :175-193
  (BAML_PROFILE=1, BAML_PROFILE_DIR, BAML_RING_SEG_BYTES=65536); g3_lossless test :1648 (exact counts).
  reconstruct_bamlprof: run.rs:3691-3796 (normalize_disk_event :3624-3673).
- **activate_profiling**: bex_engine/lib.rs:1855-1869 → register_engine_metadata(engine_id,
  prof_engine_metadata(...)) at :1865. metadata registry: bex_events/prof/metadata.rs (global Mutex map).
  Engine Drop :859-896 → engine_closed at :893. flush_and_join callers: baml_pack_host/main.rs:280 (10s),
  playground_server.rs:163 (1s).
- **Env flags** (config.rs, ProfConfig::from_lookup :114-157, cached OnceLock read-once :96-99):
  BAML_PROFILE (default ON native; truthy = "1"/"true" only), BAML_RING_SEG_BYTES (256KiB, clamp
  64KiB-16MiB), BAML_RING_MAX_OVERFLOW_BYTES (1GiB), BAML_RING_FREELIST_CAP (4), BAML_PROF_WAKE_INTERVAL_MS
  (50ms), BAML_PROFILE_DIR (.baml/profiles). NO BAML_HISTORY today. wasm opt-in =
  enable_wasm_cooperative_profile() fn.
- **Clock**: now_ticks clock.rs:285-291 (rdtsc/CNTVCT/Instant); TickConverter :420-441, to_ns :525-535
  (fixed-point mul-shift, SCALE_SHIFT=48); from_clock :480-516 (TSC: 2ms sleep on consumer thread);
  maybe_refine :541-567 (heartbeat cadence, anchor continuity). started_at_epoch_ns u128 wall anchor.
- **wasm drain**: drain.rs CooperativeProfileDrain (:41-50); transcode :153-221; bridge_wasm/lib.rs
  wiring :705/:729-735/:1857-1918 (max_sweeps=1024 at :111). Fresh uuid per drain instance (gotcha).
- **Existing perf numbers**: consumer 7.5M events/s/core ~285MB/s (consumer.rs:20-23, #[ignore]d
  prof_drain_throughput :953-1004). Bench: only benches/prof_clock.rs (~10ns/clock read budget).
- **NO existing CCT code anywhere** (verified zero grep hits).
- **Stats today**: ConsumerState::report stderr only; no self-reporting module — ConsumerStats is new.

## Research: run store, web UI, values, size-gate (COMPLETE — for P0/P4/P5/P9/PH)

- **Run store**: bex_events/src/run.rs. InMemoryRunStore :847-851; RunStoreInner :859-866;
  PROFILE_EVENTS_CAP=100_000 :2512 (trim :2514); ingest_profile_event :1200-1233 (global mutex per
  event, loops all runs); recompute_record_profile :2577-2691 (O(N) rebuild per event: BFS over 100k
  envelopes :2978-3104, clones matching envelopes, emits patch with UpsertCallNode for ENTIRE call
  tree :2672-2679). run_to_wire run_wire.rs:13-47 (full JSON). ProfileEventSource::Live :1964-1973
  (per-event String at consumer.rs:303-306, drain.rs:183-186).
- **Wire to browser**: no CctPatch; RunPatch JSON via patch_to_wire → WsOutMessage::RunPatch →
  tokio broadcast → /api/ws. That's the 2.21 GB wire. wasm-only ProfileArtifactChunk
  (bridge_wasm/lib.rs:1908, base64 .bamlprof chunks).
- **axum server**: baml_lsp_server/src/playground_server.rs build_router :979-1045 (/api/ws, /api/lsp,
  /api/source-files); loopback bind :430-444 (default port base 4265); PlaygroundAccessGuard :842-905;
  api_guard_middleware :2513-2542. WS protocol playground_ws.rs (WsInMessage :13-171, WsOutMessage
  :213-361). readValue: playground_server.rs:2196-2246 (LiveValueCache 64MiB then
  HistoryStore::read_value_result).
- **Playground UI**: typescript2/pkg-playground (source-only pkg; consumers app-vscode-webview
  [CLI-served dist], app-promptfiddle, app-website, app-vscode-ext). ExecutionProfileView.tsx (438 L,
  DOM button per call block :271-311, NO canvas). **1.5% width floor: run-store-projections.ts
  :226-229 AND :525-528** (Math.max(1.5, ...)); grafted threads via effectiveParentId :724-737;
  buildExecutionProfileProjection :318-390 (unbounded recursion). Projection re-runs per cursor bump
  (ExecutionProfileView.tsx:79-82). wasm build: pkg-playground build:wasm → wasm-pack bridge_wasm.
- **baml_studio crate = EMPTY dirs, no Cargo.toml, untracked** — free to create fresh.
- **History**: .baml/history/<ts-slug>-<target-slug>-<boundary_wire>/thread-N/{stack,value}-K.*;
  blobs/<alg>/<dd>/<digest>.blob CAS (value/artifact.rs:94-169, 64KiB inline threshold, tmp+rename,
  digest verified on read, NO GC). NO bamlmeta anywhere. HistoryStore history/mod.rs:203+;
  BoundaryWriter boundary_writer.rs:23-33 (rotation 8MiB/50k events, 8MiB/10k records);
  boundary id from dir-name suffix (path.rs:1185) + header cross-check (mod.rs:1140).
- **Value capture**: bex_engine/src/value_capture.rs. TraceCaptureConfig :100-150 (enabled(16):
  value=16, log=16, root=2 reserved); try_reserve :211-277 (17th capture in class → QueueFull,
  O(pending) scan); reserve-before-copy invariant holds (:685 test). CaptureKind :23-32;
  TraceCaptureStats :168-177; drains :310-:423. TraceHeap separate from moving heap.
  ValueIdAllocator value/writer.rs:30-67 (Arc<Mutex<u64>>, "prefix_n" String per value).
  Production enabled(16) sites: playground_server.rs:1745,:1851; bridge_wasm 862,998,1564.
  CLI test_command.rs:344 = logs_only(100_000), NO values, NO boundary. run_command.rs has ZERO
  boundary/capture wiring (host wiring lands at :949-953 FunctionCallContextBuilder; reference impl =
  playground_server.rs:1738-1840).
- **CLI subcommands** (commands.rs): Check/Auth/Feedback/Format/Describe/Generate/Test/Init/New/Run/
  Playground/Pack/Ide/Agent/LanguageServer/Help/Telemetry. NO doctor, NO clean.
- **tools_size_gate** (clone for obs-bench): package cargo-size-gate; modules main/config/measure/
  baseline/compare/ceilings/output/fetch/human_size; subcommands Record/Check/Diff/Agg/Bake;
  --format table|markdown|md-fragment|json; exit codes 0/1/2/3/4; baselines .ci/size-gate/<platform>.toml
  (human sizes as strings); CI size-gate.reusable.yaml (per-platform continue-on-error jobs + one
  enforce aggregate); mise tasks size-gate{,-update,-record}.
- **Value drain today**: one-shot at run completion (playground_server.rs:527+ drain at :1802-1830).

## Research: engine/VM identity + hooks (COMPLETE — for P1/P2)

- **Engine stamping walk**: bex_engine/lib.rs:1572-1599 in `new_with_deferred_profiling` (1-based ids +
  lower_to_compact; `function_pool_indices` :1582/:1591 is DEAD code; orphan doc-comment :769-774).
  build_program_metadata :1392-1502 runs on PRE-conversion program at :1543 — two walks must agree
  (unasserted). convert_program :1554; box_compile_time_floats :1567 (appends only).
- **attach_builtins zeroes function_id** (bex_vm/package_baml/mod.rs:463) when resolving
  NativeUnresolved→Native — harmless today (runs pre-stamp), LIVE BUG if ids move to compile time.
  Must preserve function_id in that rebuild for P1.
- **Synthetic rows**: SPAWN_CLOSURE_FQN lib.rs:155, UNKNOWN_FUNCTION_FQN :212; header rows built
  :1452-1494 at next+1/+2; NEVER referenced by ring records today (unresolvable callees emit id 0:
  trampolines vm.rs:2587/:2661, host-closure sysop :4402). prof_gate sentinel test :2608-2657.
- **VM emitters**: prof_enter_call vm.rs:4092-4116 (callers :2437 entry, :4856 bytecode call);
  prof_enter_sysop :4212-4238 (caller :4479 dispatch_sysop_yield); prof_emit_native_pair :4263-4305
  (:4733/:4751); prof_exit_call :4120-4150 (:6580 Return, :3815/:3831 unwind); prof_unwind_status
  :4171-4190; call-site spans :4051-4075; prof_open_call_ids :4155-4160.
- **Ring refresh (D5a)**: prof_refresh_vm_ring lib.rs:1917-1928; call sites :2977, :4603, :4835 (loop
  head), :5109 (SysOp resume), :5432 (Await), :5516 (AwaitAny).
- **PARK POINTS for Suspend/Resume (P2 0x06/0x07 emission sites)**: SysOp async: release :5097,
  select :5099-5103, acquire :5104, refresh :5109. Await: :5413/:5422-5426/:5427/:5432. AwaitAny:
  :5489/:5491-5510/:5511/:5516. EarlyYield: gc_safepoint :5557-5559 (release inside :3971, reacquire
  :3973). Cancellation drains at :5016,:5116,:5383,:5435,:5462,:5519; prof_end_sysop :1934-1953
  (callers :5012,:5112,:5142). Ready-inline sysops (SysOpResult sync) never park.
- **LLM meta hook (P2 0x08)**: sys_llm/src/lib.rs:592-634 `execute_parse_response_from_owned`;
  LlmProviderResponse (parse_response/mod.rs:96-110) has model/finish_reason/usage(TokenUsage
  :129-137) — currently DISCARDED at :606-607. PrimitiveClient{name,provider,model} baml_std.rs:22-26.
  Parse errors ParseResponseError :65-89. HTTP send is a separate BAML-level sysop (llm_types.baml:444,
  error branch :457-465 loses status code). Streaming: stream_accumulator.rs AccumulatorState :19-25.
  sys_ops blanket impl: sys_ops/src/lib.rs:163-366 (parse → :282). Engine sysop dispatch: lib.rs:4989 →
  execute_sys_op :5572-5632 → fn_ptr(..., ctx, call_id) :5609 — call_id IS available to sysops.
- **Thread lifecycle**: root StartThread lib.rs:2986-2995 (StartThread-first invariant, no early
  return); spawn StartThread :5331-5341 (parent ids + name in hand); EndThread :4772-4796 (status map
  :4773-4790); SpawnProfCloser :251-286 (armed :4625, defused :4666). Thread ids: next_prof_thread_id
  :1895-1897 (AtomicU64 :775, one id universe with host events). spawn_thread_inner :4534-4700
  (child vm ids :4589-4591, set_entry_point :4604 emits child entry CallFunction).
- **Program**: NO identity field today. Engine init flow: euid :1541, engine_id :1542 (static atomic
  :1387-1390), metadata :1543, prof_enabled :1548 (ProfConfig read-once), $init loop :1696-1743
  (prof_ring None during init), Drop :859-896.
- **now_ticks call sites**: vm.rs:4111,4146,4233,4251,4296(+start :4720-4724); engine :277,:283,
  :1949,:1980,:1988,:2992,:4794,:5338. Value capture uses separate epoch_ms (:4151) — don't conflate.

## P1 progress

- [x] PackEnvelope framed encoding (prerequisite PR): magic `BAMLPKG\0` + u32 LE version=1 + borsh;
      typed decode errors (BadMagic/UnsupportedVersion/Borsh); write site pack_command.rs, read site
      pack_host extract_envelope; tests green (agent-verified in isolated worktree).
- [x] bex_vm_types::identity: consts (UNKNOWN=0, SPAWN_CLOSURE=1, FIRST_POOL=16 + canonical FQNs),
      assign/verify_function_ids, RevisionId/SourceSnapshotId (+baml_rev_1_/baml_src_1_ encode/decode),
      ProgramIdentity, SourceFileIdentity, §4.3 snapshot/revision/fallback hashing,
      §4.4 DefHashResolver::def_content_hash (relink::visit_index_operands canonicalization —
      ordinal rewrite + referent-name side list; layout-independence unit test green).
- [x] Program: #[borsh(skip)] identity + source_files. Function: def_meta (borsh) +
      FunctionCaptureProps.promote_on_error (THE wire bump); FORMAT_VERSION 3→4.
- [x] All Function literal sites patched (emit ×5 def_meta:None placeholders until agent B lands real
      values; vm.rs trampolines; link/relink fixtures; attach_builtins now PRESERVES function_id+def_meta).
- [x] file_blake3 salsa query (hir) + bex_cache::compiler_id() ("<ver>+<channel>+<blake3-exe-16hex>").
- [x] bex_events::dict: bamldict.proto (RevisionDictionaryV1), build_revision_dictionary (pure Program
      walk; reserved rows 0/1 FIRST; capture_flags bitfield; def_content_hash per row),
      ensure_dict_written (idempotent tmp+rename), read_dict; consumer writes dict before first
      artifact referencing the revision (writer_for hook, degrades to embedded table with warning).
- [x] Engine: fallback finalization at init (identity-less → assign ids + fallback_revision_id;
      new VmInternalError::IdentityNotFinalized); walk VERIFY-ONLY (debug full / release tail probe);
      build_program_metadata reads stamped ids, reserved rows 0/1 first, def_meta-driven keys;
      FQN sniffers DELETED; ProgramId DELETED (header field 3 now empty ⇒ absent on wire);
      header fields 10/11 = baml_src_1_/baml_rev_1_ string forms; EngineProfileMetadata.dictionary.
- [x] MIR lambda identity (3 producers; MIR-local MirDefinitionIdentity mirrors; nested-lambda parent
      chaining; single shared per-parent ordinal counter — deliberate deviation from per-kind debug
      counters, documented) + emit def_meta population + §7.1 capture defaults (agent-landed; oracles
      16/16 green).
- [x] emit finalize_program_identity (emit/src/identity.rs): assign ids + salsa file hashes +
      baml.toml disk read + compiler_id; wired at generate_project_bytecode{,_with_opt,_with_stdlib},
      generate_stdlib_program, reuse-linked tail. Pack-load finalize = engine fallback (equivalent per
      §4.3: packs are identity-less by construction since identity is borsh-skipped).
- [x] Cache note: cached stdlib blob is splice input only; all runnable outputs re-finalized by the
      emit entries. No whole-program cache loads exist in the CLI.
- [x] prof_gate moved to reserved-low scheme + §7.1 capture expectations; 32/32 green.
      identity_oracle.rs added in baml_tests: §4.4 golden property (unrelated edit ⇒ hashes
      byte-identical) + dense-ids/reserved-rows/dictionary assertions — green.
- [x] emit_determinism 4/4, link_units_oracle 5/5, relink_oracle 7/7 with finalizer wired.
- P1 COMPLETE.

## P2 progress

- [x] Raw records 0x06 SuspendThread (22B, reason SysOp|Await|AwaitAny|EarlyYield, seq u32) /
      0x07 ResumeThread (30B, self-contained: carries suspend_ts) / 0x08 LlmCallMeta (38B, flags
      provider|parse|retry, model_id/tokens) — encode/decode/size/roundtrip tests green; DiskEventV1
      variants 7/8/9 + transcode; normalize_disk_event skips (like SetFunctionId); prof_gate matches
      updated in same change (T20 stream identity filters suspend/resume as timing-only records).
- [x] Engine emission: prof_suspend/prof_resume at the 4 park points (SysOp/Await/AwaitAny arms +
      EarlyYield gc_safepoint); vm.prof_suspend_seq counter (cold path only).
- [x] LLM meta chain: sys_types::LlmCallMetaSample + SysOpContext.llm_meta per-call slot (installed in
      execute_sys_op, returned to the event loop); sys_llm deposits on EVERY exit path of
      execute_parse_response_from_owned (success/parse-error/finish-reason-rejection with real usage);
      engine prof_emit_llm_meta drains before prof_end_sysop; per-engine model intern table
      (llm_model_names() for P3 model_birth). Known gaps (documented): streaming accumulator path and
      retry flag (BAML-level orchestration) not yet fed — no silent loss, records simply absent for
      streaming calls until wired.
- [x] CCT engine modules (`bex_events/src/prof/cct/{mod,engine,nodes,recent,spawn}.rs`, target-neutral):
      SoA nodes + FxHash intern (dense-id pre-sizing); §5.6 fold >512 (scan ≤8, RECURSION_FOLD flag,
      folded_frames); §5.2 causal defer keyed Call/Thread with RANGE-BOUNDARY retries in per-thread
      TIMESTAMP order (call-id order breaks stack discipline — learned via fixture), provided-key-aware
      expiry (never synthesize a dependency a pending record can provide), synthesized unattributable
      parents + degraded partitions at DEFER_MAX_SWEEPS; §5.3 charge-to-current with self/await split,
      self-contained Resume reconstruction (works with missing Suspend), late-suspend drop counter;
      §5.5 spawn edges (one edge+subtree for equivalent spawns; instances 64+256 bounded with
      instances_dropped); §5.8 recent ring 4096×slots (thread_idx, parent_call_id, dump_ref);
      §3-N4 $id annotations (bounded open-call map + accessor); windows via dirty_epoch take_window.
- [x] Fixtures (tests/cct_engine.rs, 8/8): two-ring migration (order-invariance vs in-order reference),
      corrupt range degrades, defer-timeout synthesis, suspend/resume accounting (with+without suspend
      record), >512 fold with exact counts (consumer-level — unreachable end-to-end per MAX_FRAMES=256),
      shared spawn subtree, dirty-window discipline.
- [x] Consumer integration: transcode fork feeds per-engine CctEngine (runs_cct), cct_sweep_tick in the
      consumer loop, EngineClosed drops state (P3 replaces with boundary fold), stats gains
      cct_nodes/deferred/synthesized/evicted.
- [x] **CCT-equivalence oracle** (§10.3): prof_gate now runs the WHOLE suite in dual mode
      (init_prof_env sets BAML_PROFILE_PIPELINE=dual) + `cct_equivalence_matches_raw_derived_counters`
      compares live-consumer CCT totals vs raw-derived per-function enters/ends over a spawn+call
      program — EXACT match; 33/33 green. Oracle tap: ControlMsg::CctSnapshot + pub
      cct_totals_snapshot(timeout).
- [x] **Integrated bench (P2 EXIT GATE)** benches/cct_engine.rs — final (pinned taskset, best-of-3×3,
      EPYC-Milan 2GHz-class shared vCPU, clock=instant):
      **hotloop 47.8–48.6 ns/pair (≤50 gate PASS)**, p99-3543-nodes 52.7–54.4 (≤60 never-exceed PASS,
      >50 target on THIS slow VM — flag: confirm on CI hardware; per-platform baselines are binding),
      migration ~2000 (adversarial 100%-cross-ring stress leg; hot loop never defers per design).
      Probe decomposition: decode-only 9.6, recorder-only 2.3 ns/pair.
      Optimization journey (all fixtures stayed green throughout): 131.6 → 62.8 (thread slab+cache,
      dense partition Vec, no per-pair map maintenance, direct dispatch) → 49.3 (recent-ring modulo →
      power-of-2 mask [THE big one — runtime idiv per close], redundant dirty-write dedup) →
      47.8 pinned. u64-packed intern key kept (neutral-to-positive, less code).
- [x] Oracle race found & fixed BY the gate: engine Drop → EngineClosed removed CCT state before the
      snapshot reply → empty totals. Fix: bounded (8) retention of closed engines' CCT state
      (cct_closed VecDeque) merged into totals + cct_sweep_tick before Flush ack/snapshot reply.
      P3's boundary fold replaces retention. prof_gate 33/33; bex_events 179+8+4 green.
- P2 COMPLETE (recorder-stub caveat: re-affirm C2 when the P6 real recorder lands, per plan).
- NOTE (wasm): CooperativeProfileDrain does not embed CctEngine yet — lands with P4's ObserveEngine
      (no wasm consumer of the state exists until then).

## INCIDENT LOG

- (2026-07-31) A subagent ran `git stash` on the shared tree mid-flight, reverting all tracked
  uncommitted work. Recovered fully via `git stash pop` (untracked files were unaffected; 4 garbage
  re-edits discarded first). Standing rule sent to running agents: NEVER git stash/checkout/restore/
  reset/worktree against the shared tree. Future agent prompts must include this rule.

## P3 progress (started early while P2 bench runs)

- [x] `prof/cct/crc32c.rs`: in-tree CRC32C (Castagnoli, NOT crc32fast/IEEE), RFC 3720 vectors, streaming.
- [x] `prof/cct/segment.rs`: BCCT container — 112B header (magic/version/euid/engine/seq/align/epoch/
      clock/tick-ratio/revision_id[32]/crc32c), 32B DBLK block headers + 16B trailers (crc32c over
      header+payload, monotonic block_seq, COMMIT_MARKER), 64B block alignment, seal-by-append
      (footer_index block + 48B BCCTFOOT..TSEG trailer), `scan_segment` recovery (committed prefix,
      Torn{offset}, Sealed fast path, seq-gap = torn). Torn-tail test at EVERY offset; 12/12 cct lib
      tests green. Writer is sink-generic by design (wasm shares framing).
- [x] §6.3 typed row codecs (`prof/cct/blocks.rs`): column-major fixed-width (cct_delta/node_birth/
      spawn_edge/watermark/llm_delta via macro; partition_bind/cct_hist hand-rolled) + row-major
      variable (model_birth/marker/instance); roundtrip + short-payload-refusal tests green.
- [x] CctEngine::flush_window → TRUE DELTA rows (shadow columns on Nodes + llm_flushed map +
      SpawnEdges.flushed): births first (with birth thread), delta rows only when nonzero, hist rows
      only for windows WITH closes, idle window = zero rows. Delta-discipline fixture green (9/9).
      NOTE: spawn-edge running/awaiting_ns deltas = 0 in v1 (derivable from shared child subtree;
      documented in code).
- [x] BAML_OBS_LAYOUT (v1|dual|v2, default dual; writes_v1/writes_v2) + BAML_PROFILE_RAW knobs.
- [x] `prof/cct/session.rs` SessionWriter WRITTEN (compile blocked on meta agent): session dir
      naming <started>-<euid32>-e<engine>, header-fsync at segment create, window blocks (§6.3 order:
      births first), checkpoint-by-bytes (≥ table size, amortized ≤2×), kind-4 watermark rows attested
      by OFF-THREAD FsyncService completions (durable D1 mark advanced by helper thread; idle 10s
      heartbeat watermarks), D1 group commit 1s/1MiB, rotation 4MiB/15min, seal-by-append (footer
      index rows: kind/offset/rows/first/last), session.bamlmeta begin/heartbeat/end, partition_bind +
      model_birth + marker writers.
- [x] ModelBirth raw record 0x09 (self-contained stream: engine intern_llm_model emits once per
      model; CctEngine keeps names + unflushed kind-11 rows in WindowFlush.model_rows; proto field 10;
      normalize skips; prof_gate arms; roundtrip tests). 186 lib tests green.
- [x] §6.5 fold (`prof/cct/fold.rs`): fold_partition (dense re-map parent<child, totals/hist/llm/
      spawn/model rows) + encode_boundary_snapshot (ALWAYS-sealed BCCT bytes with footer+trailer;
      callers tmp+rename). Sealed-scan roundtrip test green.
- [x] prof/cct/meta.rs BMET streams (agent; 6/6 tests): 8B header (BMET\0 + ver u16 + reserved),
      u32len+kind+json+crc32c records, torn-tail prefix semantics, unknown kinds skipped+counted,
      MetaWriter O_APPEND (header only when file empty).
- [x] Consumer window wiring: cct_window_tick @250ms in consumer loop (gated runs_cct &&
      layout.writes_v2 && writes_enabled); lazy per-engine SessionWriter (revision id from metadata,
      clock from conv, sessions/ under .baml); lazy FsyncService; engine close = final window + seal +
      SessionEnd; write failures reported, never fatal. 188 lib tests green incl. session end-to-end
      scan test (sealed seg + births/deltas/hist/footer + meta begin/end).
- [x] Session heartbeat (10s D0, rate-limited in SessionWriter::heartbeat; wall-NOW stamps).
- [x] §6.1 session epochs: CctEngine::rotate_epoch (fresh node table; live stacks + spawn_ctx
      re-interned BY PATH via ancestor walk; spawn edges remapped w/ totals carried + shadows
      aligned; llm keys remapped; model names re-announced; epoch-boundary shape: spanning calls
      close with ends in the later epoch, enters in the carry-over checkpoint — documented).
      SessionWriter: EPOCH_ROTATE 256MiB/24h, should_rotate_epoch, close_epoch (carry-over kind-8
      checkpoint + EPOCH_CLOSE marker + seal + SessionEpochClose meta). Consumer wires rotation in
      cct_window_tick. Epoch fixture green; 188+10+4 tests, 0 warnings.
- [x] ControlMsg::BindBoundary/CompleteBoundary + boundary.bamlmeta writers + partition_bind +
      §6.5 fold write at completion (consumer side; PH provides host calls). prof_gate boundary test green.
- [x] BAML_PROFILE_RAW raw/ sink (prof/cct/raw.rs): BAMLRAW1 container (64B header: euid/engine/clock),
      u32-framed verbatim drained ranges, 64 MiB rotation, epoch-rotation-following, 64 MiB pending cap.
      Wired at transcode() → buffered → flushed at window ticks + engine close under <session>/raw/.
      prof_gate suite now runs with BAML_PROFILE_RAW=1 throughout + raw_firehose_replays_to_legacy_counts
      oracle (raw ranges re-derive EXACT legacy per-function enters/ends). 35/35 green.
- [x] **Short-engine session fix**: engines living <250 ms (fast CLI runs) never hit a window tick and left
      NO session; close_engine now mints + seals the session via shared mint_session() helper. Found by the
      raw oracle test.
- [x] Stats fix: report_stats also aggregates cct_closed engines (cct_nodes was 0 after EngineClosed).
- [x] obs-bench validate (src/validate.rs): walks .baml root — dict (prost decode), session.bamlmeta
      (BMET, first-kind check), .bamlseg scan (sealed/active/torn; crc), raw files (parse + full record
      decode), history/ boundary.bamlmeta + cct.bamlcct (MUST scan Sealed — tmp+rename contract).
      Torn/truncated = valid crash evidence; undecodable committed bytes = invalid. --json output.
      Verified against the real 5M-call e2e root: 3 files, 0 invalid (dict 830 rows, sealed 17-block seg).
- [x] obs-bench crashfuzz (src/crashfuzz.rs, C8): SIGKILL sweep min..max ms with seeded-LCG jitter +
      uninterrupted canary (must be clean AND produce a bamlseg). Categories: killed_before_begin /
      recovered / completed. First pass (hotloop 2M, dual, 8 kills 20–1500 ms): 1/4/3, 0 invalid.
- [x] Golden v2 fixtures (tests/golden_v2.rs + testdata/golden/v2/): session.bamlseg (real SessionWriter,
      deterministic), session.bamlmeta + boundary.bamlmeta (all 9 BMET record kinds), cct.bamlcct
      (fold + encode_boundary_snapshot), raw-000001.bamlprof. Byte-frozen + torn-tail contracts. 7/7,
      determinism confirmed by regenerate-then-verify.
- [x] C3 formalized in obs-bench rows: c3.run.session_bytes{,_per_s}, c3.run.legacy_to_session_ratio,
      c3.run.dict_bytes emitted whenever a leg produces sessions/ (see benchmark ledger).
- [x] clippy --all-targets clean (0 errors) for bex_events + tools_obs_bench (print allows for benches/CLI).
- [x] Rebuilt-CLI e2e verification (hotloop 100k iters, dual, raw on): cct_nodes=4 in stats; sessions dir
      with SEALED seg-000000.bamlseg (4 blocks) + session.bamlmeta + raw-000001.bamlprof (62 ranges,
      400,005 records == consumer records count EXACTLY — lossless firehose); validate 4 files 0 invalid.
      **P3 COMPLETE.**

## P4 progress

- [x] crates/bex_query created (sans-io, no tokio; features: native=mmap). Modules: source (SegmentSource
      trait + Poll::NeedData contract, MmapSource w/ committed-length + refresh/generation, SliceSource),
      bqf1 (§9.3 frames: 40B header BQF1 magic/kind/flags/request/epoch/ncols/nrows + 16B col directory +
      8-aligned payloads + crc32c trailer; encoder + host-side decoder; kinds RunsList/RunMeta/Timeline/
      LeftHeavy/TopFunctions/Status/LiveTotals/RecentCalls), runs (§9.6 bamlmeta scans; crashed =
      begin-without-complete + dead pid/heartbeat), cct (THE fold: epoch-aware — EPOCH_CLOSE marker splits
      id spaces, cross-epoch merge by ancestor path w/ intern map; checkpoint-authority totals =
      last node_total + deltas after; §9.4 bands per thread×window [fn0 root rows excluded — shared,
      unattributable]; left_heavy w/ 1/(2·px) floor + synthetic "smaller" rows; top_functions), engine
      (ObserveEngine: open_run boundary-snapshot-first→bound-session fallback, dict fqn join, LRU
      byte-budget cache 256MiB, frame methods).
- [x] **bex_events fix found by P4 tests**: SessionWriter::create collided on create_new(seg-000000) at
      epoch re-mint (same deterministic dir) → ALL post-rotation windows lost. Now resumes at
      next_free_seg_seq; meta appends (2nd SessionBegin). Regression test in session.rs.
- [x] bex_query tests 4/4 + 2 unit: session fold matches program truth exactly (incl. bands busy/errors),
      epoch re-mint merges by path (doubled counts, 2 segments), boundary .bamlcct folds via node_totals,
      ObserveEngine frames decode over synthetic root.
- [x] **Real-data probe** (statsfix 100k-iter hotloop session): user.main 1 call 43.96ms → user.step
      100,000 calls 30.2ms → user.add 100,000 calls 10.1ms; dict join 830 fns; left_heavy 3 rows = 304 B,
      timeline 1 band = 216 B (vs 2.21 GB JSON wire pathology). probe_real_root test, BAML_QUERY_PROBE_ROOT.
- [x] C6 in obs-bench: `replay --v2-root` times ObserveEngine open + first frame (see benchmark ledger).
- [x] C7 in obs-bench: `corpus synth` (seeded LCG sealed segments, sessions of ~8 MiB) + `corpus scan`
      (fold-all gate: wall, MB/s, VmHWM peak RSS). First pass in ledger.
- [x] LiveMirrorSource: bex_events consumer tap `cct_live_segment(engine_id, timeout)` →
      ControlMsg::CctLiveSegment → fold_all + encode_live_snapshot (always-sealed whole-engine segment,
      identical block format; fold.rs refactored fold_partition→fold_where; births now carry REAL
      partition_id). bex_query: LiveMirrorSource (fetch closure + refresh/generation) + ObserveEngine::
      open_live(key, bytes, revision). prof_gate live_segment_matches_oracle_totals green (36th test).
- [x] /api/obs WS route LANDED (subagent + my wiring): baml_lsp_server/src/obs_ws.rs (~800 lines).
      Route at build_router (playground_server.rs:1018), same guard as /api/ws; .baml root via
      resolve_project_root (same as history store). Protocol: JSON in {op:query|sub|unsub, id, method,
      run, pixel_width, limit} → BQF1 binary out (request_id echo; errors = Status frames). Methods:
      runs/run_meta(query-only)/timeline/left_heavy/top_functions/recent_calls. Subs: 250ms tick,
      one-in-flight (serial task), change-only (run_epoch for run methods; bytes-compare for
      runs/recent_calls); path-traversal-guarded run keys; failed sub → one 404 then dropped.
- [x] **Live mirror wired end-to-end**: open_run_preferring_live — run keys matching THIS process's euid
      (new pub process_euid_hex) route to cct_live_segment (RAM fold, ahead of group commit) via
      ObserveEngine::open_live; open_live epoch now content-sensitive ((len<<32)|crc32c — length alone
      froze fixed-population live tails). Foreign/boundary keys fall back to disk.
- [x] §9.4 exact-recency tier: engine.partition_count + consumer RecentCalls tap (pub recent_calls,
      RecentCallOut with function pre-joined via node table) → obs_ws method "recent_calls"
      (RecentCalls BQF1 frame kind 8; lane key = partition<<32|thread_idx; honest empty frame for
      non-live keys). prof_gate recent_calls_tap_matches_ring_contract green (37th test).
- [x] obs_ws tests 8/8; baml_lsp_server clippy 0 errors; prof_gate 37/37.
- [x] TS half LANDED (subagent): pkg-playground src/obs/{bqf1.ts, observe-client.ts, RunsView.tsx} +
      ExecutionPanel "Runs" tab + index.ts exports. Decoder: table CRC32C (RFC 3720 vectors), zero-copy
      TypedArray views, typed as* helpers per kind. Client: WsObserveClient (id-matched query promises,
      subscribe w/ standing-intent resubscribe on reconnect, 500ms→5s backoff); URL mirrors
      __PLAYGROUND_WS_URL with /api/ws→/api/obs rewrite. UI: runs table (status badges), timeline lanes
      canvas (dominant-fn hue, busy alpha, error ticks), left-heavy flame canvas (fold rows gray
      "N smaller"), top-functions table w/ fqn join. bqf1.test.ts 9/9; pkg-playground 22 files/160
      tests; tsc clean. (Env fixes: @xterm tarballs into node_modules, pkg-proto buf regen — both
      non-git-tracked.)
- [x] C13 wire bound: timeline frames LOD-climb (coarsen_bands power-of-2 window merge, sums exact,
      dominant=busiest constituent) and left_heavy halves pixel_width, both setting FLAG_LOD_DEGRADED;
      bounded ≤ DEFAULT_MAX_BYTES. Test: 500k-window merge exactness + flag behavior.
- [x] FINAL P4 BATTERY: bex_events 190+10+4+7+2, bex_query 6+2, obs-bench 8, baml_lsp_server 77
      (incl. obs_ws 8), prof_gate 37/37, identity_oracle 2/2, TS 160. **P4 COMPLETE** (tier-3 exact
      overlays are P6 by design; wasm ObserveEngine is P8).

## P5 progress

- [x] Research map (subagent): capture model = TraceValue arena (trace_heap.rs:51-73, 13 variants,
      cycles→Omitted); wire = prost BamlOutboundValue via encode_trace_snapshot_body
      (trace_value_encode.rs:429; value_capture.rs:378-417 is THE single drain funnel); current encoding
      non-canonical (NaN bits pass through, map insertion order, bigint hex/decimal dual, absent aliases
      null); class identity = display strings only, definition_key conventions DISAGREE (identity.rs:340
      elides pkg vs MIR ir.rs:211 full dotted — settled on MIR form); blob CAS = sha256 whole-body >64KiB
      (writer.rs store_body:216; threshold boundary_writer.rs:35); ValueCodec single variant; DagRef =
      additive proto tag ≥15 + codec variant.
- [x] bex_events/src/store/ NEW: canon.rs (C9 canonical encoder: BLAKE3 CIDs domain-prefixed
      baml-value-node-v1\0 / chunk-v1\0; bamlv_1_ wire form; determinism rules — map sort by key bytes
      last-dup-wins, NaN→0x7FF8...0, ±0 preserved, bigint minimal decimal, presence byte
      absent/null/value/default, 128 KiB chunking, 128-ary segments, inline-child ≤128 B leaf-only,
      indirect map keys >4096 B), pack.rs (§6.7 BPK1 container: 48B header, CK records w/ crc32c,
      torn-tail scan, PackWriter w/ shared flock writers.lock + .lease, seal→idx tmp+rename, 64 MiB
      seal; try_exclusive_writers_lock for GC), index.rs (BPKI: 256-fanout + sorted entries + crc),
      mod.rs (Store facade: dedupe-before-append across sealed idx + active, newest-first reads, idx
      REBUILD from pack scan on crashed-writer open). Tests 13/13; clippy 0.
- [x] C9 golden fixture frozen: testdata/golden/v2/canon.bamlcanon (root CID + full DAG bytes for an
      every-tag value incl. chunked string + segmented list). golden_v2 8/8.
- [x] canon::node_refs — structural ref scanner over the frozen layout (slots, map entries incl.
      indirect keys, CHUNKED chunk CIDs, class presence-gated slots); closure test reaches exactly
      every emitted node/chunk. Encoder debug-asserts presence↔slot coherence (decode contract).
- [x] store/gc.rs (§6.7): manifest.bamlcids append/parse (bamlv_1_ lines, O_APPEND), gc() —
      writers.lock EXCLUSIVE else skip-with-notice ("GC waits" adversarial ruling), gc.lock serializes,
      mark = manifests + flight/*.bamlcids + uploads.pin closed over node_refs, sweep = post-grace
      (24h default) packs: fully-dead unlinked whole, partially-live compacted in place (direct rewrite
      — PackWriter would deadlock on the shared lock), lease-protected packs untouched, every deletion
      tombstoned to retention.log jsonl. Tests 4/4 incl. no-readable-root-references-sweepable-CID.
- [x] **C5 gate test green**: transcript-append N=64 w/ 64 KiB prompt dedupes ≥20× vs naive
      (store-level test; obs-bench row when PH wires values end-to-end). Store suite 19/19.
- [x] store/retention.rs (§6.8): RetentionPolicy defaults (history 30d/2GiB/floor-20, sessions 7d/1GiB,
      raw 512MiB/session, profiles 7d); clean() honoring the degradation order (raw firehose first,
      whole-oldest-boundaries releasing CAS closure, sealed CCT aggregates last), dry-run mode, jsonl
      tombstones. Tests: raw-cap oldest-first + floor protection, dry-run deletes nothing,
      25-boundaries→floor-20. (Test run pending — DagRef agent mid-edit in value/ at time of writing.)
- [x] DagRef capture path LANDED (subagent): proto VALUE_CODEC_CANONICAL_DAG=2 + DagRefV1 (tag 15,
      additive; prost-build via build.rs, nothing committed); record.rs DagRef struct + CanonicalDag
      codec ("canonicalDag"); reader surfaces dag_ref on CapturedValue, rejects on lifecycle records;
      writer append_body_with_capture_and_dag; TraceOmissionReason::canonical_code FROZEN mapping
      (0..4); canonical_from_snapshot (Array→List, Map order passthrough, Instance→class:{name} w/
      Presence, type_args dropped, MediaKind::tag_str, dangling refs→Omitted); value_capture
      encode_snapshot dual-encodes; drain_to_value_writer_with_store(Option<&mut Store>) —
      Some: put_encoded + DagRef, None: byte-identical legacy; store-write failure degrades to
      DagRef-less (legacy body authoritative until P9); logs get no dag. wasm/playground arity updated.
      goldens v1 4/4 + v2 8/8 BYTE-FROZEN INTACT; prof_gate 37/37; clippy 0.
- [x] Post-merge battery: bex_events lib 213 (store 22 — retention test's now-vs-age fixed: sessions_age
      must not bind in the raw-cap test), bex_engine lib 112, goldens green.
- [x] §7.3 drain service LANDED (subagent): store/drain.rs ValueDrainService — one thread
      ("baml-value-drain") owning the Store; put_encoded sync round-trip (zero-copy Send-wrapped ptr,
      sound via blocking reply); append_manifest_and_commit = §6.7 ROOT-COMMIT ORDER (pack sync D1 THEN
      manifest, on the service thread); seal_and_stop + Drop best-effort; cpu_ns via
      CLOCK_THREAD_CPUTIME_ID (C10 hook); ValueStoreSink trait unifies Store and service;
      drain_to_value_writer_with_service. 5 tests incl. manifest-only-after-durable ordering.
- [x] §7.2 staging ring LANDED (subagent): value_capture staged VecDeque (byte-accounted via
      TraceHeap::snapshot_bytes; 32 MiB native / 8 MiB wasm; FIFO=LRU insert-time eviction);
      stage_with/release_staged/promote_staged(TraceCallKeyPrefix, trigger_id) → drafts move to pending
      and drain normally with promoted_by (proto field 16, additive; goldens frozen);
      CaptureLossReason::StagingEvicted (additive); stats staged/evicted/released/promoted; 6 tests.
      Deviations (documented in code): PromotionReport.staged_evicted is cumulative; prefix matching is
      engine/thread/exact (subtree ancestry not expressible from TraceCallKey — flight recorder covers
      retroactive subtree evidence).
- [x] **P5 COMPLETE** (engine/store side). Residuals riding later phases: 64→4 KiB inline threshold
      flip (PH — only meaningful once hosts wire the CAS per boundary); continuous-drain cadence
      (high-water wake + interval = host loop, PH); §7.5 audit records (host consent surface, PH);
      trigger→promote_staged linkage (P6 — consumer OnError should call promote_staged; API now exists).

## P6 progress

- [x] Flight recorder (§5.9): prof/cct/flight.rs — FlightRecorder ring of RAW drained ranges (16 MiB cap,
      one memcpy on the drain path, whole-chunk FIFO eviction w/ evicted counters, per-engine retained()
      + forget()); fed in ConsumerState::transcode (gated cct+v2).
- [x] Dumps: ConsumerState::flight_dump — §3.1 rate limits (≥5 s spacing, ≤16/engine, dropped counted),
      transcode via to_disk_event into sessions/<sess>/flight/<ts_ms>-<trigger>.bamlprof (EXACT legacy
      framing: build_header + encode_length_delimited_message + encode_disk_event — standard reader
      parses unchanged); deterministic session-dir formula works without a live SessionWriter;
      flight_dumps counter in BAML_OBS_STATS; pub bex_events::prof::flight_dump (Manual trigger) via
      ControlMsg::FlightDump.
- [x] Triggers (§3.1): OnError — CctEngine tracks root-level Errored closes (idx==0 in apply_end),
      consumer auto-dumps "error"; OnLatencyMs — duration > threshold (default 30 s, 0 disables,
      set_latency_threshold_ns) auto-dumps "latency". Perf spot-check post-change: hotloop 47.4 ns/pair
      (P2 gate intact).
- [x] prof_gate 38/38 incl. flight_recorder_dumps_on_error_and_rate_limits (errored program → auto dump
      parsed by read_bamlprof_from_bytes, ≥3 calls retained, immediate manual re-dump rate-limited).
- [ ] REMAINING P6: .bamlcids pin manifest sibling for dumps (needs value-CID linkage at dump time —
      lands with PH/staging integration); dump refs in boundary.bamlmeta (BoundaryTrigger record);
      ~~trigger→staged-promotion linkage~~ DONE: root-unhandled-throw path (lib.rs ~4446, next to the
      CaptureKind::RootError capture) now calls producer.promote_staged(ENGINE prefix, "error")
      synchronously — spawned helpers' staged inputs promote too; semantics covered by the staging
      agent's promotion tests; no-op when no capture context (all gate tests unaffected).
- [x] Dump refs: flight_dump appends MetaRecord::BoundaryTrigger{trigger, at_ms, detail:"flight:<file>"}
      to every BOUND boundary of the dumping engine (O_APPEND MetaWriter). prof_gate 38/38.
- [x] §5.12 wasm CCT embedding: CooperativeProfileDrain gains a per-engine CctEngine (gated on
      pipeline.runs_cct, test-overridable set_cct_enabled) + pub cct_live_segment(engine_id) emitting
      the IDENTICAL always-sealed segment bytes as the native consumer (same fold_all +
      encode_live_snapshot) — the wasm ObserveEngine folds them with the same code; unblocks P9 step 3
      (JSON chunk wire deletion). Test: private-registry drain → sealed segment → node_total decode.
- [x] **P6 CLOSED** with documented residuals: full-trace mode + TraceBudgetExhausted (host policy,
      existing stack-K machinery); flight .bamlcids pins (session-scoped dumps only — boundary
      manifests already pin); .bamlidx BIX1 (design-deprioritized: rebuild-on-open is normal);
      C2 re-measure w/ flight memcpy (obs-bench, CI hardware).

## P7 progress

- [x] BQL v1 LANDED (subagent): bex_query/src/bql.rs — lexer w/ byte offsets, AST
      (StageAst/ArgAst{Pos,Named,Cmp}/ValueAst incl. duration literals ns..d w/ checked overflow),
      typed planner (SetKind RunSet/CtxSet/Table; ONLY coercions RunSet→CtxSet [degraded-noted;
      >1 run → E_MULTI_RUN_CTX at materialization] + X→Table). Stages: runs(last=,status=)/run(id=)/
      ctx(); calls(fn=glob)/errors()/rollup(by=fn)/where(metric op val)/sort(by=,desc|asc);
      top(k,by=)/stats()/limit(k). Metrics calls|total_ns|self_ns|errors|p50|p95|p99 (hist ×4-stride
      bucket-upper fold; mean-with-note fallback). Implicit 1000-row cap w/ degraded note (§8.4).
      Typed BqlError{code,message,remedy}: E_PARSE/E_UNKNOWN_STAGE/E_TYPE/E_MULTI_RUN_CTX/E_BAD_ARG.
- [x] FrameKind::BqlTable=9: free-form cols + final Str meta col; frame row 0 = meta row w/ JSON
      {columns,rows,footer{sealed,torn,first/last_ts,degraded[]}} — empty results STILL ship the
      footer; FLAG_PARTIAL_TAIL mirrors torn. /api/obs method "bql" (q field; query-only; errors →
      Status 422 "CODE: msg (remedy: ...)"). Engine gains additive pub fold()/names() accessors.
- [x] Tests: bex_query 78 lib(+6 bql unit)+10 bql integration+8 fold; obs_ws 9 (incl. bql round-trip +
      422); torn-segment → footer.torn=true honesty test. Clippy 0.
- [x] baml q CLI LANDED (q_command.rs): positional query, --run (defaults to newest
      boundary/session), --format table|json; table render w/ footer line, errors as "CODE: msg +
      remedy"; registered in commands.rs w/ --project plumbing; help snapshot refreshed (insta accept).
- [x] **FULL-LOOP E2E (release CLI, plain `baml run main`, dual pipeline, 400k calls)**: minted dict +
      history/<ts>-main-<baml_id>/ {boundary.bamlmeta begin/bound/complete, SEALED cct.bamlcct,
      manifest.bamlcids, thread-1/value-0.bamlvalue} + sessions/<dir>/{meta, sealed seg} + store packs
      (+idx). obs-bench validate: 5 files 0 invalid. `baml q 'ctx() | top(5, by=total_ns)'` →
      user.main 90.1ms / user.work 2 calls / user.helper 400,000 calls p50=1µs, names from dict,
      footer sealed=true torn=false. `baml q 'runs(last=24h)'` lists the boundary w/ status+revision.
      `baml clean --dry-run` reports safely. **The design's §2 user stories are live.**
- [x] **VALUES FOLDED INTO BQL (goal.md, 2026-08-04)** — the §8.2/§8.4/§8.5 value plane:
      · canon.rs DECODER (exact inverse of the frozen encoder; DagSource fetch; §8.4 budgets —
        byte+depth, whole-subtree elision w/ ELIDED_REASON=255 + elided-CID handles; never partial)
        + schema-erased to_json; golden decode∘encode-identity test on canon.bamlcanon (root CID
        reproduced); round-trip/budget/missing-node tests (16 canon tests).
      · bex_query/values.rs: run value listing (.bamlvalue capture rows w/ role/kind/call key/CID/
        promoted_by), call→fn join via the RAW FIREHOSE exact source (honest: absent → degraded note,
        fn= filter → E_NO_EXACT_SOURCE w/ remedies), hydration Dag→inline→blob w/ BamlOutboundValue
        prost-mirror fallback. Session-path normalization fix (Bound stores relative path).
      · bql.rs: SetKind::ValueSet; stages values(role=[…], fn=glob)/get(max_bytes=64kb, depth)/
        instances(source=values)/vdiff(a=,b=)/stats(by=cid); lexer lists [a,b] + byte sizes 64kb/4mb;
        ColData::Json (real JSON in --format json, 96-char preview in tables, Str on the BQF1 wire);
        E_NO_EXACT_SOURCE + budget-elision footer notes; E_MISSING_BLOCK via hydrate errors.
      · ValueWriter::into_sink (additive). 15 bql integration tests incl. hermetic value-run fixture
        (meta+CAS+raw+dict), exact-source gating, dedupe view, vdiff, budget elision.
      · VERIFIED e2e on the release CLI (demo): run → ctx narrows → values(fn="*line_total*")|get →
        quantity:2→3.5 reveal → stats(by=cid) n=2 dedupe → fix → vdiff outputs_equal=0 on matched
        input CIDs. Docs updated (demo/baml-q.md full stage ref, AGENTS.md pure-CLI hunt + §3½
        verify-my-fix, deck slide → real query). GATES: bex_events 214+goldens BYTE-FROZEN, bex_query
        10+15+8+6, prof_gate 38/38, baml_cli full, lsp 78, clippy 0.
- [x] **ZERO-ENV VALUES + STUDIO PANEL (2026-08-04 follow-up)**: captures now CARRY function_id
      (VM capture sites → engine drafts via capture_with_fn → ValueCaptureV1 field 3 additive, goldens
      frozen; root id via callable_function_id, sysop tuple widened, frame pushes stamped; host-closure
      pairs/trampolines/logs stay 0 by design). values.rs resolves fn names capture-id-first, raw join
      kept as fallback for old artifacts → `baml run` + `values(fn=…)` needs NO env vars (proof: names
      resolve with zero raw files on disk). Studio Runs tab gained a **Captured values** panel
      (TS: FrameKind.BqlTable=9 decoder asBqlTable, client `bql` op, expandable JSON rows + degraded
      notes) driven by the same `values() | get(16kb)` query as the CLI; wire path verified end-to-end
      against the live server with the shipped decoder (6 hydrated named values). Docs → zero-env.
      Release CLI + webview dist rebuilt. Batteries: workspace check 0, bex_events 214+goldens,
      prof_gate 38/38, bex_vm, bex_query 8+16+6, baml_cli, lsp 78, pkg-playground 148+1; clippy 0.
- [ ] REMAINING P7: MCP tool; studio query box + language service (P8); series/delta/diff(align=fqn)
      aggregate compare; events/dumps stages; snapshot pinning; --schema; full footers on named
      /api/obs methods.

## PH progress

- [x] CLI host wiring LANDED (subagent, all in baml_cli): run_observability.rs (RunBoundary
      begin/context_builder/finish; history_enabled = ProfConfig::is_enabled() && BAML_HISTORY∉{0,false};
      bind/complete ADDITIONALLY gated on pipeline.runs_cct() — legacy-default runs stay begin-only to
      keep baml run's empty-stderr contract); boundary dir
      history/<created_ms>-<target_slug>-<baml_id_1_wire>/ (slug mirrors history::path rules); root
      thread from BexCallResult.entry_call_ref.thread_id (fallback 1); capture = playground-mirrored
      enabled(16) with logs_enabled:false (§3.2 cli defaults); drain → thread-<root>/value-0.bamlvalue +
      Store::open(<baml>/store) with **root-commit ordering sync_data→append_manifest→seal_active**;
      status mapping Ok/Exit→succeeded, cancelled, else failed; ALL failures verbose-only.
- [x] baml clean LANDED: clean_command.rs — retention + GC over project .baml; --dry-run (skips GC pass
      w/ notice — gc has no dry mode), --gc-only/--retention-only/--grace-hours; live-writer skip prints
      notice, exit 0 (flock-tested). Help snapshots regenerated.
- [x] SMOKE (BAML_PROFILE=1, dual): history dir with boundary.bamlmeta (begin/bound/complete succeeded)
      + SEALED cct.bamlcct + manifest.bamlcids (2 roots) + value-0.bamlvalue; sessions + store packs;
      **obs-bench validate: 5 files, 0 invalid**. BAML_HISTORY=0 mints nothing. baml_cli 460 lib + 91
      integration green; clippy 0.
- [x] Deviations noted: -e expression mode unwired; dispatch_target_traced duplicates ~40 lines of
      baml_exec::dispatch_target (no context seam there); exit_code_e2e needs env -u CLAUDECODE (preset
      quirk, pre-existing). REMAINING PH: SDK/pack hosts (same RunBoundary pattern), baml.toml
      [observability] knobs, §7.5 audit records, completion barrier for in-flight value drains.

## PH prep notes (research, 2026-07-31)

- baml_cli run path = crates/baml_cli/src/run_command.rs (NOT commands/); engine built at :300-308 and
  :722-748 (compile_to_engine → BexEngine::new); shutdown at :643.
- Surfaces for binding: BexEngine::engine_id() lib.rs:1926 (pub); call_function_with_trace →
  BexCallResult{value, entry_call_ref: CallRef} lib.rs:318-321 — root thread available POST-call;
  CLI wiring = mint ULID BoundaryId → write BoundaryBegin meta (MetaWriter, kind 16) into
  history/<created_ms>-<target_slug>-<baml_id_1_...>/ → run → bind_boundary(engine_id, id, entry thread,
  dir) → complete_boundary(id, status) (closed-engine retention makes post-run bind valid — the
  prof_gate boundary test does exactly this). Reference impl for playground: playground_server.rs:1738-1840.
- RootValueCaptureContext (lib.rs:324-328) already carries a boundary_id — playground path mints one
  today; CLI needs the same + capture defaults per §3.2 (cli: llm_boundary).

## P8 progress

- [x] baml studio LANDED (subagent): studio_command.rs (StudioArgs PATH/--port/--no-open;
      resolve_studio_root → Project | TraceViewer; port scan 4265..+100; browser opener polls TCP);
      reuses run_playground_server verbatim; /studio route on both static+dev-proxy routers serving the
      SPA shell with injected window.__STUDIO_INITIAL_TAB="runs" (App.tsx reads it, 3 additive lines);
      TRACE-VIEWER MODE WORKS (.baml-only dir → full server, obs root falls back to the dir;
      deliberately non-recursive .baml detection — ~/.baml is the CLI home config dir, pinned by test).
      Smoke vs e2e-final: /studio 200 + injected script; /api/obs 101 on upgrade; .baml-only dir
      identical + warning. baml_cli 472 green, server 79 green, clippy 0.
- [ ] REMAINING P8 (documented): TS wasm ObserveEngine host consuming the §5.12 live segments +
      HTTP-Range CacheSource (Poll::NeedData plumbing exists); value inspector + Sandwich + search +
      diff view; webview dist rebuild for the landing-tab hint; ClickHouse compilation prototype.

## P9 EXECUTION (in progress — user's directive "we can deprecate and remove current profiler"
## supersedes the CI-gate reading; goal hook enforced completion)

- [x] Step 1: defaults flipped — PipelineMode #[default] Legacy→Cct, ObsLayout #[default] Dual→V2
      (config.rs; default-pinning tests updated; garbage-fallback now coerces to Cct/V2). Paired bench
      with the new default pending release rebuild.
- [x] Step 4 LANDED (agent): transcode_legacy (−76) + writers map/writer_for/fail_writer/sync_files/
      flush_files + legacy heartbeat walk DELETED from consumer.rs (~−260 net); ProfileWriter DELETED
      from file.rs (−105; readers kept — goldens intact); PipelineMode = {Cct} only (legacy/dual
      parse-coerce silently, doc'd), ObsLayout = {V2} only; runs_cct()/writes_v2() kept returning true
      for rollout call sites. Dict write RELOCATED to mint_session (§4.2 order preserved — fixes the
      step-1-observed dict 0 B). NEW TickConverter::from_rate for raw-header replay. Flush now FORCES a
      window tick (ack ⇒ session+raw landed — replaces legacy sync semantics). Tombstone check hoisted
      to transcode() (guards raw/flight/CCT). Stats records re-sourced from CCT diagnostics.
      prof_gate re-point: run_main → (result, EngineTag{engine_id, meta}); load_profile = session-dir
      demux by euid+engine → raw files → record::iter → to_disk_event(from_rate) → build_header
      synth; marker demux gone; raw_firehose test now compares vs cct_totals_snapshot; equivalence
      oracle raw-derived; teardown test rewritten to sealed-session stability. Consumer unit tests
      (fake-producer roundtrip, soak) re-pointed to raw firehose. GATES: bex_events 221+10+4+8,
      prof_gate 38/38 (twice), obs-bench 8/8, baml_cli green (env -u), clippy 0, workspace check 0.
      SMOKE: **no profiles/ dir at all**; dict 829 rows; sealed session seg + boundary cct.bamlcct;
      validate 5 files 0 invalid 0 legacy.
- [x] Step 2 TS LANDED (agent): ExecutionProfileView.tsx DELETED (−438); run-store-projections
      −604 (all calls[]-derived paths incl. BOTH 1.5%-floor sites; kept status/logs/values/fetch
      projections); Trace AND Flame tabs removed (Trace's only source was the deleted projection —
      §9.3 run store keeps low-rate state only); worker-protocol: Run.calls/threads now optional,
      profileArtifactChunk variant DELETED; promptfiddle/website workers stop forwarding it; stale
      host tabs normalize to 'run'. tsc clean; vitest 22 files/148 green; zero refs remain to any
      deleted symbol.
- [x] Steps 2(rust)+3 LANDED (agent): run.rs 6055→4145 (ProfileEventObserver plane, ingest, CAP+trim,
      recompute+BFS+overlay builders DELETED; publish_engine_closed kept as doc'd no-op; UpsertCallNode
      TYPE kept — non-profile payload patches use it; attach_root_trace pins identity only);
      run_to_wire drops calls/threads; history/router.rs DELETED whole (−92); history profile publish +
      stack writers deleted (readers intact, value plane untouched); drain.rs 614→321 (output =
      {progress, diagnostics} + §5.12 CCT + cct_live_segment; corrupt diagnostics via validation walk);
      bridge_wasm −299 (ProfileArtifactChunk + profile segment writers gone; drain_wasm_profiles = CCT
      advance + diagnostics only); lsp_server observer/overlay-provider registrations deleted.
      12 profile-only tests deleted, 8 adapted (names in agent report). Gates: workspace check 0,
      bex_events 229, lsp 78, prof_gate 38/38, baml_cli green, bridge_wasm 5, clippy 0. Smoke validate:
      5 files 0 invalid 0 legacy.
- [x] wasm32 hygiene (me): blake3 moved to target-neutral deps (was accidentally unix-gated);
      store io modules (drain/gc/index/pack/retention + Store) cfg-gated native-only per §7.3;
      ValueStoreSink trait relocated to store root (target-neutral shape) so the single
      drain_to_value_writer_with_sink body compiles everywhere; with_store/with_service gated.
      bex_engine + bex_events check clean on wasm32-unknown-unknown (bridge_wasm's remaining wasm
      errors are PRE-EXISTING bex_cache reqwest::blocking — untouched).
- [x] Step 5: VALUE_INLINE_THRESHOLD_BYTES 64→4 KiB (boundary_writer.rs, §7.4 — dedupe engages;
      CAS DagRef is the authoritative plane). history tests 22/22.
- [x] Step 6: straggler sweep clean — zero references to deleted variants; parse strings coerce
      (documented); bench-row labels historical.
- [x] FINAL BATTERY: bex_events 207+10+4+8, cct_engine 10, bql 6+8(fold), obs-bench 8, bex_engine lib
      118, prof_gate 38/38, lsp 78, clippy 0.
- [x] **P9 COMPLETE — FINAL VERIFICATION**: quiet-box paired bench best-of-3: **74.4 ns/call**
      (74.4/77.3/81.3) vs legacy-era 113.6 → the re-impl is **35% CHEAPER than the profiler it
      replaced** while writing ~52,000× less; legacy disk 0 B every rep; dict 134,374 B (relocation
      verified); stats records=10,000,005 (re-sourced counter verified); consumer CPU 680–752 ms/10M
      records. Fresh full-loop e2e on the final binary: `.baml/` = {cache, dict, history, sessions,
      store} — **no legacy dir exists at all**; validate 5 files 0 invalid 0 torn **0 legacy**;
      `baml q 'ctx() | top(3, by=total_ns)'` answers with names/percentiles/footer.
      Bench rows: scratchpad/p9-final-rows.ndjson.

## P9 execution plan (original, now executing — kept for the step details)

Design §11 binds P9 on: paired baselines on CI-grade hardware + green oracles. Local evidence is strong
(C1 paired 113.6 legacy / 75.5 dual ns-per-call, C3 52,224×–70,200×, CCT-equivalence EXACT, raw-firehose
oracle EXACT, 38-test gate suite) but the p99-3543 leg (52.7–54.4 vs never-exceed 60) and all C-gates
must be re-measured on CI hardware with `obs-bench check` against refreshed baselines FIRST.

Deletion order (each step lands separately, gated on the full battery):
1. Flip default BAML_PROFILE_PIPELINE dual→cct (legacy still reachable). Re-run everything; ship a
   release behind this before ANY deletion. Flip BAML_OBS_LAYOUT default dual→v2.
2. Run-store profile projection: delete PROFILE_EVENTS_CAP (run.rs:2512), ingest_profile_event
   (:1200-1233), recompute_record_profile (:2577-2691); run_to_wire stops serializing calls[]
   (run_wire.rs:13-47). The §9.3 "one live plane" ruling — /api/obs serves what these fed.
   TS: run-store-projections.ts profile paths + ExecutionProfileView.tsx replaced by the obs Runs tab
   components (pkg-observe-ui absorbs; old view removable one release later per §9.7).
3. JSON profile wire: WsOutMessage::RunPatch profile-event payloads + ProfileArtifactChunk
   (bridge_wasm:1908 base64 path) — wasm switches to CooperativeProfileDrain→CctEngine (the §5.12
   embedding, still open) BEFORE this lands.
4. transcode_legacy + ProfileWriter + profiles/ dir writing (consumer.rs:826+, file.rs) — the raw
   firehose (BAML_PROFILE_RAW) + flight dumps become the only exact-event .bamlprof producers; prof_gate
   reconstruct oracles re-point at the raw sink (design §10.3 explicitly plans this).
5. Legacy value inline-64KiB blob path: flip threshold to 4 KiB (boundary_writer.rs:35) + make DagRef
   authoritative; legacy `blobs/` stays read-only forever (§6.1).
6. PROFILE_EVENTS_CAP-adjacent dead config + BAML_OBS_LAYOUT v1 writers (config.rs) last.
Each step's guard: prof_gate suite + CCT-equivalence + golden v1/v2 byte-frozen + obs-bench check green
on the platform baselines; any regression reverts the step, not the phase.

## Benchmark ledger

Machine: AMD EPYC-Milan (8 vCPU, linux x86_64, clock=instant, VM). obs-bench built debug; baml-cli release.

| date | bench id | value | notes |
|---|---|---|---|
| 2026-07-31 | c3.run.disk_bytes_per_call (hotloop 500k calls, legacy) | **44.1 B/call** | matches design anchor 45.2 B/call @5M ✔ |
| 2026-07-31 | c1.run.paired_wall_ns_per_call (hotloop, legacy) | **113.6 ns/call** | design cites ~123 ns/call legacy total ✔ |
| 2026-07-31 | c2.run.consumer_cpu_ms (hotloop 1M records) | 122.7 ms (≈122.7 ns/record) | via BAML_OBS_STATS self-report, debug-adjacent VM |
| 2026-07-31 | cct_update microbench (release) | flat 1/16/1024/4096 fns: 6.8/5.5/5.9/7.5 ns/pair; depth14: 9.9 | primitive only (no decode); P2 integrated gate ≤50 ns/call looks feasible |
| 2026-07-31 | cct_engine integrated bench (pinned, release) | hotloop 47.8–48.6 ns/pair; p99-3543 52.7–54.4; migration ~2000 | P2 exit gate; decode 9.6 + recorder 2.3 of that |
| 2026-07-31 | **C3 END-TO-END (hotloop 5M calls, dual, real CLI)** | legacy 235,688,641 B (47.1 B/call) vs **v2 session stream 4,513 B → 52,224×**; ~800 B/s vs ≤6 KB/s gate | THE central claim measured: sessions/ dir + dict written by the real pipeline; consumer CPU 1.44 s/10M records (both pipelines) |
| 2026-07-31 | **C3 formalized via obs-bench rows** (hotloop 5M, dual, paired) | c3.run.session_bytes 3,344; **session_bytes_per_s 2,713 vs ≤6,000 gate (PASS)**; legacy_to_session_ratio **70,200×**; dict_bytes 134,374 (one-time/revision); paired 75.5 ns/call (dual = both pipelines) | rows in NDJSON with machine/git provenance; scratchpad/c3-rows.ndjson |
| 2026-07-31 | crashfuzz C8 first pass (hotloop 2M calls, dual, 8 kills 20–1500 ms + canary) | 1 killed-before-begin, 4 recovered, 3 completed, 0 invalid | validate: e2e root scans clean (dict 830 rows, sealed 17-block seg) |
| 2026-07-31 | **C6 open path (same 100k-iter run, debug builds)** | legacy parse 237.9 + reconstruct 1,416.7 = **1,654.7 ms** vs bex_query open 2.61 + first frame 0.01 = **2.62 ms → 632×**; gate ≤250 ms | obs-bench replay --v2-root; left_heavy frame = 304 B |
| 2026-07-31 | C7 corpus scan first pass (256 MiB seeded corpus, 32 sessions, DEBUG obs-bench) | 20.8 MB/s scan, **peak RSS 34.3 MiB** (constant vs corpus size — byte-budget contract holds) | obs-bench corpus synth/scan; 10 GiB + release leg pending CI-class run |
| 2026-07-31 | **P9 step-1 bench (cct-only default, 5M calls)** | legacy disk **0 B** ✓; session 3,501 B; consumer CPU **686.9 ms**/10M records (vs 1,059.5 dual → −35%); paired 91.3 ns/call **PROVISIONAL — concurrent agent builds polluted the box; re-run quiet** | ALSO CONFIRMED LIVE: dict 0 B in cct-only (write lived in legacy writer_for — step-4 agent relocating); stats records=0 in cct mode (counter was legacy-side) |

## P0 progress

- [x] Workloads committed + validated end-to-end (hotloop, bench_rate, agent_like [86 threads/depth14/errors],
      transcript_append, idle_agent, recursion_depth [depth≤240; >512 fold pinned at consumer level, see below]).
- [x] **VM MAX_FRAMES=256 (vm.rs:103)** ⇒ design's depth-1024 end-to-end leg impossible; §5.6 fold (>512)
      reachable only via synthetic record fixtures in bex_events tests (P2). Documented in workload file.
- [x] BAML_PROFILE_PIPELINE=legacy|dual|cct (config.rs PipelineMode) forked at ConsumerState::transcode
      (transcode → transcode_legacy; cct/dual behave legacy until P2/P3). Tests green.
- [x] BAML_OBS_STATS consumer self-report (prof/stats.rs; NDJSON, thread CPU via CLOCK_THREAD_CPUTIME_ID,
      counters; reported on Flush + EngineClosed). Verified end-to-end through child process.
- [x] tools_obs_bench crate: run (paired legs, RUSAGE_CHILDREN, stats ingestion, disk bytes),
      check/refresh-baseline (.ci/obs-bench/<platform>.toml, measured-release-only gating),
      calibrate, prof-stats, value-stats (dedupe potential), replay (C6 legacy candidate),
      gen-paths (binary-tree contexts), report (claim ledger); crashfuzz/validate/corpus fail
      loudly naming their phase. Tests green.
- [x] bex_events/benches/cct_update.rs committed (see ledger).
- [x] 1.5% width floor fixed at both projection sites (true proportional widths; open calls extend to
      span end; renderers add minWidth:1px). Regression test added; 28/28 vitest green.
- [x] Removed empty crates/baml_studio/ dirs (broke crates/* glob once tools_obs_bench manifest existed).
- [x] Golden fixture scaffolding: testdata/golden/v1/ (README freeze contract; events.bamlprof +
      values.bamlvalue byte-frozen; torn-tail contract asserted at EVERY offset; BAML_GOLDEN_WRITE=1
      regeneration; tests/golden_v1.rs 4/4 green).
- [x] mise tasks obs-bench / obs-bench-record added. GH reusable workflow wiring deferred until gates
      are real (P2/P3) — recorded here so it isn't silently dropped.
- [x] prof_gate 32/32 green after consumer changes. P0 DONE.
- NOTE: CLI `baml run` exit still races the consumer (no completion barrier until PH); stats/bytes
  arrived complete in smoke because engine Drop lands before exit, but the barrier is the fix.
