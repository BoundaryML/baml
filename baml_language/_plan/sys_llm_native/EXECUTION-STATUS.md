# Execution status — sys_llm → native BAML migration

Updated: 2026-08-13 (mid Stage B). Everything uncommitted on `canary`.

## Done

- **Media diff from ~/projects/baml2 applied + verified** (PromptMessage.parts, ai.wire.resolve_media, media lowering in 3 clients). Baseline: 1503 baml_tests + bex_engine green; ONE stale snapshot (`bytecode_display_formats`) → regen in Stage C.
- **Rename** `openai.OpenAiClient` → `openai.ResponsesClient` everywhere (0 source refs left; ~36 snapshot files + LSP expectations regen in Stage C).
- **Shorthand map** (`lower_cst.rs`): + openai-chat, openai-images, azure, ollama, openrouter, vertex, bedrock, ai-gateway-images. Diagnostic now suggests `openai.GenericClient`.
- **Packages**: new `aws`, `vercel` registered (hir package.rs, emit lib.rs, builtins2 ALL); 16 stub files pre-created + registered.
- **Stage A agents (all landed, cargo-checked where Rust)**:
  - A2: `ai.content.Media` (+`Media.new(value, provider_id=, revised_prompt=)` — MUST use constructor), `ModelTurn.media()`, `PromptMessage.metadata: map<string, baml.json.json>`, runner media branch porting sys_llm coercion contract byte-identical error strings; `_parses` signature now `(self, turn, candidate)`.
  - A4: `ai.wire.merge_request_body(base, overrides)` (null deletes key, maps merge, arrays replace, LAST step); `ai.wire.sanitize_for_client(journal, client_id)` (foreign reasoning dropped, empty turns dropped, orphaned tool calls → synthesized ToolFailed "No result provided").
  - A7: `crates/sys_auth` (926 LOC, 18 unit tests green); sysops in `ai.internal._*` (`ai/ns_internal/auth.baml`; moved from baml.auth per user directive, codegen extended for ai-package $rust_io_function scan); public wrappers `google.internal.{gcp_access_token,gcp_project_id,gcp_quota_project_id}` and `aws.internal.{AwsSignOptions, sign_request, resolve_region}`. sys_ops + sys_native wired; cargo check/clippy clean.
  - A8: `ns_llm_mock` test kit (mock_json_serve/mock_sse_serve/MockRequest/MockResponse/sse_data/sse_event + README), baml.toml `[test]` profiles (default=offline excludes ::live::; live includes only), `ns_llm_live_smoke` verified LIVE via infisical (openai/anthropic/google PASS).
- **Cross-agent fix applied by orchestrator**: `_wire_blocks` in wire.baml got the `let media: root.content.Media => null` arm.

## Stage B: DONE (6/6, ~1.8M tokens). Full reports in tasks/wtvo6rqth.output.

- B1: ChatCompat core (1352L ns_internal/chat.baml) + ChatClient/GenericClient/AzureClient/OllamaClient/OpenRouterClient; 25 offline+live tests in ns_llm_openai_chat. NOTE: uses local `_chat_send` instead of ai.wire.send_as because `function` is an illegal BAML field name (tool_calls wire shape uses `fn` + ToJson/FromJson overrides; decode via baml.json.to<T>).
- B2: ResponsesClient typed rewrite (971L); image_generation tool via `ai._media_shape`; input_audio NESTED (docs-verified); streaming fixes (failed→throw, classified SSE errors, function_call_arguments deltas). 15 tests in ns_llm_openai_responses. Live image test double-gated on OPENAI_LIVE_IMAGE.
- B3: Anthropic typed rewrite (1140L); 6 bugs fixed; cache_control from PromptMessage.metadata onto last block; AnthropicBodyParams = Vertex rawPredict reuse seam; max_tokens 8192. 24 tests in ns_llm_anthropic. FOUND COMPILER BUG: `x = <null-expr> ?? x` assigns null (worked around in-file; repo scanned — no other instances).
- B4: Gemini typed rewrite + VertexClient (OAuth via gcp_access_token; project/location chains; express mode; global default location). invoke renamed gemini_invoke/vertex_invoke. Claude-on-Vertex DEFERRED w/ typed refusal — ~20-line wiring via anthropic.internal.anthropic_messages_body IF cross-package google→anthropic ref compiles (try at integration). 17 tests in ns_llm_google. Scratch-verified 16/16 PASS.
- B5: BedrockClient (Converse, non-streaming; sign_request LAST; LABEL_SET encoder in BAML; toolConfig net-new; cachePoint via cache_system_prompt/cache_tools opts). 34 tests ns_llm_bedrock (33/34 scratch-pass; 1 needs real signer). Live: amazon.nova-micro-v1:0, gated on AWS_PROFILE. FOUND: MIR panic hoisting generic mock fn to root ns (kept in subnamespace).
- B6: openai.ImageClient (docs-pinned vs openai-openapi@master) + vercel.AiGatewayImageClient (sys_llm-verbatim headers; png-mime bug fixed); llm_image_outputs fixture rewritten to openai-images shorthand. 18 tests ns_llm_images. FOUND COMPILER BUG: `string | image` union value matches NEITHER arm (mixed:image runner shape unusable from user code).

Shared-file requests triage: shorthands/diagnostic ALREADY DONE pre-B; structured_prompt_requests.rs input_audio nesting FIXED by orchestrator; deferred to Stage D/post: ai.wire.drop_nulls promotion (5 local copies exist, tested), Reasoning.signature+redacted, StopReason Other/raw, Usage.total_tokens/cache_creation_input_tokens, TurnMeta model/cache fields, StreamEvent tool/reasoning delta variants, fetch_sse structured status, percent-encoder helper, make_role metadata surface (role() can't set metadata — inline Role literal only), ai.content.Media.metadata slot.

## Stage C progress

- First rebuild: Rust clean. Only integration break: `aws`/`vercel` missing from USER-package dep list in `baml_compiler2_hir/src/package.rs` (~40 E0003) — fixed. The 3 `RuntimeTy Unknown` panics were bytecode-lowering of those unresolved names with the stale binary; gone after rebuild.
- `baml-cli check` on corpus: CLEAN (warnings only, all pre-existing). Warm check 1.5s.
- Offline corpus run: **2793 passed, 0 failed** including all **135** new llm_* provider tests.
- cargo check across 7 touched crates: clean.
- IN FLIGHT (task b5rkf216i): INSTA_UPDATE regen (full baml_tests) → UPDATE_EXPECT LSP regen → clean verify matrix (baml_tests, bex_engine, lsp2_actions, sys_auth, baml_cli).
- Snapshot regen DONE (baml_tests full incl. rename fallout + __ai_std__/__baml_std__; LSP 468/468 via UPDATE_EXPECT; baml_cli render_builtin_package_listing). Clean verify: bex_engine/lsp/sys_auth green.
- OPEN + delegated: (1) RuntimeTy-Unknown panic — ONLY in the Rust harness path (`cargo test -p baml_tests --test baml_src`; compile_multi_file with emit_test_cases:false + OptLevel::One); bisected to ns_llm_bedrock/bedrock.baml; CLI check/test on same corpus is clean and 34/34 pass — debugger agent on it. (2) USER DIRECTIVE: move `baml.auth` sysops → `ai/ns_internal/auth.baml` (`ai.internal._gcp_access_token` etc.) — requires extending baml_builtins2_codegen to scan the ai package; mover agent on it (updates google/aws wrappers, deregisters baml/ns_auth, snapshot regen).
- **STAGE C COMPLETE.** RuntimeTy panic ROOT-CAUSED + FIXED as a real 3-file compiler fix (not a workaround): (1) TIR `collect_throw_facts_from_expr` was a drifted partial copy of `throws_analysis` — missing the to_string/to_json/from_json sugar-fallback guards, so `f().to_string() catch_all(e)` silently typed e as `X | Unknown`; now shared via `sugar_fallback_call_throws()`. (2) MIR's three sugar-fallback guards checked Unknown only at top level next to a recursive typevar check — now recursive `contains_error_recovery` with erasure to BuiltinUnknown (ntypeargs=0 drop traps the VM — pre-existing comment was false). Harness 2793/2793 (both paths), cold-cache clean, stdlib bytecode byte-identical, mir/tir/emit unit tests green, snapshots regen (9 new llm_* + _root + prompt_tag_runtime). The "harness-only" theory was a warm-cache artifact. The old vertex_rejects_claude hang was a PRE-EXISTING flake in the deleted test (A/B confirmed); its replacement passes 3/3. OPEN compiler bug (non-fatal, documented + repro in _plan/sys_llm_native/compiler-bugs/runtime-ty-unknown/): throws analysis leaves cross-package calls that delegate to directly-recursive fns (ai.wire.merge_request_body→_merge_json) unaccounted → phantom `unknown` in catch_all type unions.
- DONE: (a) Claude-on-Vertex wiring — add `anthropic` to google's package deps in package.rs provider arm + B4's ~20-line sketch in google/ns_internal/vertex.baml (branch on claude* model prefix → publishers/anthropic :rawPredict/:streamRawPredict, body via anthropic.internal.anthropic_messages_body + AnthropicBodyParams, merge {"anthropic_version":"vertex-2023-10-16"}, keep model in body); (b) Stage D fable verifiers; (c) Stage E live.

## Stage C runbook (after Stage B lands)

1. Read all 6 Stage B reports; apply any "shared-file requests" myself; reconcile.
2. `cargo build -p baml_cli` (first stdlib rebuild with everything).
3. Stdlib compile check: `target/debug/baml-cli check` in `crates/baml_tests/baml_src` — stdlib diagnostics surface here. Batch-fix per package (spawn fix agents with the full error list per package; opus).
4. Rust: `cargo check -p baml_builtins2 -p bex_vm -p sys_native -p sys_ops -p baml_compiler2_ast -p baml_compiler2_hir -p baml_compiler2_emit`.
5. Snapshots: `INSTA_UPDATE=always cargo test -p baml_tests` (covers __ai_std__, __baml_std__ (new baml.auth ns), bytecode_display_formats, llm_image_outputs, rename fallout ~36 snaps, new ns_llm_* bytecode snaps). Then `UPDATE_EXPECT=1 cargo test -p baml_lsp2_actions_tests` (rename + new stdlib surface).
6. `cargo test -p baml_tests` clean run (no update env) + `-p bex_engine` + `-p baml_lsp2_actions_tests` + `cargo test -p sys_auth`.
7. Native suites: `target/debug/baml-cli test` in crates/baml_tests/baml_src (offline profile) — all new ns_llm_* mock tests.
8. `cargo test -p baml_cli` (test_profiles_e2e still green with new [test] table).

## Stage D RESULTS

- 6 fable verifiers: **51 findings (3 critical / 25 major / 23 minor)**, all evidence-backed (doc citations + executed probes); per-area JSON in _plan/sys_llm_native/stage-d-findings/.
- Shared fixes LANDED (workflow wf_9ddb264d-bf6): **S1** stream termination contract — `TurnStream.from_sse(..., require_terminal: string? = null)`; strict + no TurnDone at close → typed NetworkFailure; `classify_http` grew optional `headers` (Retry-After→retry_after_ms, ms-header wins; send_as passes resp.headers); 13 new tests in ns_llm_stream_contract; corpus 2823/0. **S2** REAL COMPILER FIX for media narrowing — `PullSink::is_type` in baml_compiler2_emit/src/emit.rs had no `TyTemplate::Media` arm so `v is image` compiled to constant false (last match arm swallowed unions); routed to structural matcher; 17 tests in ns_media_union_narrowing; images union test upgraded to natural narrowing. FOLLOW-UP FILED: same defect class for `RealizedTy::Type`/opaque leaves (`v is type` constant false) — one-arm fix each, not applied (out of scope).
- Area fixes IN FLIGHT (workflow wf_f1c9b331-07a): F1..F6 fixing all criticals+majors per findings JSON + wiring require_terminal per decoder + tests. Policy decisions made by orchestrator: Azure api_version default → GA 2024-10-21; enterprise switch stays dropped (doc pointer to VertexClient); claude-on-vertex body drops `model` (merge {"model": null}); dall-e url-mode → fetch-and-inline.
- AFTER area fixes: orchestrator final sweep (INSTA_UPDATE full baml_tests incl. __ai_std__ (4 currently-stale snaps), UPDATE_EXPECT LSP, clean verify matrix, full corpus) → Stage E live.

## Stage D design: 6 fable verifiers (per provider) — docs-vs-typed-classes field audit + parity matrix audit + hostile inputs; loop until dry. Known deferred items are in PLAN §6; A2/A4/A7/A8 "uncertainties" lists feed the verifiers.

## Stage E: `infisical run -- target/debug/baml-cli test --profile live` in crates/baml_tests/baml_src. AWS keys in Infisical are EMPTY — Bedrock live needs local `AWS_PROFILE=boundaryml-dev` SSO session, else skip cleanly.

## Phase 2 (after green): move types/output_format.rs + types/sap.rs out of sys_llm → sys_ops (its only caller; bex_engine dev-dep is stale); delete crates/sys_llm + forks/aws-bedrock; keep forks/{google-cloud-auth,aws-sigv4,aws-config} as sys_auth deps; delete baml/ns_prompt/sys_llm_types.baml (registered in builtins2 ALL — deregister!), fix stale `<Fn>$build_request` in baml/ns_env/env.baml:21, remove empty baml_std/baml/ns_ai/ dirs; grep-audit.

## Known follow-ups / filed-by-agents (not this migration)

- Compiler bugs found: (1) primitive media union narrowing broken at runtime (`image|audio` match picks wrong arm; `is` false); (2) `baml.json.from_json<union-of-media>` ignores envelope kind; (3) parser: bare string literal containing "client"/"prompt"/"tools" at fn-body top level → E0010 misclassification (parser.rs:4060-4135).
- `ai.errors.ParseFailed` should grow `detail: string?`.
- `ai.content.Reasoning` lacks a signature field (Anthropic thinking replay w/ tools) — B3 will report; deferred per PLAN §6 unless trivial.
- `ai.internal._*` appears in `baml describe` (no privacy mechanism).
- AWS no-creds error surfaces as Io not AccessError (IMDS probe).

## FINAL (2026-08-14)

Migration COMPLETE through all phases. Commits on canary: 0dd1cff77 (phase 1),
c50bf98f4 (origin/canary merge, -X theirs), 8b09b2855 (post-merge + Stage D
fixes), 340ba4e2b (phase 2 deletion + phase 3 compiler fixes).

- sys_llm (20,453 L) + forks/aws-bedrock DELETED; output_format+sap live in sys_ops.
- 5 compiler bugs fixed at root (?? self-assign, E0010 string prescan, media
  json kind, `is type` tag, shorthand pattern binders); 3 fixed upstream by
  hir_ty; both workarounds removed. POST-MERGE-STATUS.md is the ledger.
- Final green: corpus 2950/0, LSP 468/0, full crate matrix (one known
  load-sensitive perf test passes isolated), 32 snapshots regenerated.
- Live: 12/20 pass (anthropic incl. tools+stream, gemini, vertex OAuth,
  openrouter). BLOCKED ON USER: OpenAI org spend limit (7 tests), AWS SSO
  login for bedrock (1 test).
- Leftovers (cosmetic): architecture.svg regen needs graphviz; ~75 provenance
  comments citing deleted sys_llm paths kept deliberately; ARCHITECTURE.md
  pre-existing drift; `.agents/` untracked (not ours).
