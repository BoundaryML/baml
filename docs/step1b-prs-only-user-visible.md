# BAML 0.24.0 release audit

Prepared on 2026-11-05 (Pacific). These immutable boundaries apply to the inventory, effects, release post, and follow-up checklist.

- Lower: `baml-language-0.23.0`, `f0f0bb5135bba599c11a32cc4d631f2f66f1f96f`.
- Upper: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Inventory: 84 commits/PRs across the entire repository, in chronological order.
- Scope follows the user's just-published-release request: use the published tag rather than later `origin/canary` commits. At preparation, `origin/canary` was `853072242cf951cca227759a130b9cb91d60e9a0`; its 11 later commits are excluded, including `package_name` (#5532/#5533), BTELCAS v6 (#5543), and the Homebrew publication fix (#5546).
- GitHub release: https://github.com/BoundaryML/baml/releases/tag/baml-language-0.24.0, published 2026-11-05T22:23:21Z. Paired wrapper: 0.3.0.
- Publication is incomplete at audit time: https://github.com/BoundaryML/baml/actions/runs/42111122075 failed at Homebrew publishing. Go mirror publishing, manifest promotion, developer docs, and final artifact verification were skipped. The canary manifest still reports 0.23.0. This does not change the tagged source boundary. Recheck the checklist's publication gates before merging the public post or sending announcements.
- `git fetch origin --tags` refreshed canary but reported two unrelated pre-existing v0 tag conflicts (`0.226.3`, `0.226.3-engine`); neither was overwritten. Both v1 release tags were resolved and checked against GitHub release metadata.

## User-visible PRs

- [#5264](https://github.com/BoundaryML/baml/pull/5264) — feat(compiler): compile BAML MIR to Rust.
- [#5417](https://github.com/BoundaryML/baml/pull/5417) — Make the LSP deadlock watchdog opt-in.
- [#5418](https://github.com/BoundaryML/baml/pull/5418) — Cache interface impl resolution across calls.
- [#5414](https://github.com/BoundaryML/baml/pull/5414) — Let a type choose what a model writes for it.
- [#5415](https://github.com/BoundaryML/baml/pull/5415) — Build class instances and enum values from reflection.
- [#5420](https://github.com/BoundaryML/baml/pull/5420) — Fix the flaky all_settled cancellation test and the race it guards.
- [#5385](https://github.com/BoundaryML/baml/pull/5385) — fix(emit): materialize bound methods in projection stores.
- [#5429](https://github.com/BoundaryML/baml/pull/5429) — Take no locks on the allocation and write-barrier hot path, and scan only finalizable objects.
- [#5430](https://github.com/BoundaryML/baml/pull/5430) — Warn when the Go generator skips a symbol, and generate generic functions with defaulted or callback parameters.
- [#5431](https://github.com/BoundaryML/baml/pull/5431) — Use FxHashMap for the VM's per-op caches.
- [#5432](https://github.com/BoundaryML/baml/pull/5432) — Skip the generic to_string path for string and primitive interpolation holes.
- [#5433](https://github.com/BoundaryML/baml/pull/5433) — Implement join natively.
- [#5359](https://github.com/BoundaryML/baml/pull/5359) — Bridge coercion rework.
- [#5439](https://github.com/BoundaryML/baml/pull/5439) — fix(parser): better distinguish subtraction from negation.
- [#5424](https://github.com/BoundaryML/baml/pull/5424) — Add `baml codequality` command for code complexity metrics.
- [#5446](https://github.com/BoundaryML/baml/pull/5446) — Compile-check every BAML example in the agent skills.
- [#5449](https://github.com/BoundaryML/baml/pull/5449) — Put numbers first in the `baml codequality` list views.
- [#5448](https://github.com/BoundaryML/baml/pull/5448) — skill: reorganize baml-core and add the style guidelines.
- [#5452](https://github.com/BoundaryML/baml/pull/5452) — skill: check baml codequality as you go.
- [#5453](https://github.com/BoundaryML/baml/pull/5453) — Count NPath past u64 in `baml codequality`.
- [#5455](https://github.com/BoundaryML/baml/pull/5455) — baml fmt --check.
- [#5466](https://github.com/BoundaryML/baml/pull/5466) — Stop rejecting multi-tick backtick strings whose content looks like code.
- [#5467](https://github.com/BoundaryML/baml/pull/5467) — Normalize CRLF source and keep a file's line ending when formatting.
- [#5470](https://github.com/BoundaryML/baml/pull/5470) — Use aws-lc instead of ring in the pack host.
- [#5471](https://github.com/BoundaryML/baml/pull/5471) — Client adapters now provide streaming backlog in one batch.
- [#5477](https://github.com/BoundaryML/baml/pull/5477) — Build a chain of string + in one step.
- [#5479](https://github.com/BoundaryML/baml/pull/5479) — perf(pack-host): stop linking stdlib source for package-name lookups.
- [#5481](https://github.com/BoundaryML/baml/pull/5481) — "language" naming convention for rust and python.
- [#5480](https://github.com/BoundaryML/baml/pull/5480) — Hide --agent-skill-check and make the stale skill warning more urgent.
- [#5482](https://github.com/BoundaryML/baml/pull/5482) — fix(python): make pydantic a runtime dependency of baml_bridge.
- [#5486](https://github.com/BoundaryML/baml/pull/5486) — Reject ai.Role interpolated into untagged strings.
- [#5485](https://github.com/BoundaryML/baml/pull/5485) — Record bridge traces in the cwd project so baml query --local finds them.
- [#5426](https://github.com/BoundaryML/baml/pull/5426) — fix: teach the rust-based baml wrapper to install-on-demand.
- [#5456](https://github.com/BoundaryML/baml/pull/5456) — fix(stdlib): improve subprocess management API, for the baml wrapper.
- [#5457](https://github.com/BoundaryML/baml/pull/5457) — fix(stdlib): augment fs, path, host, archive, toml APIs.
- [#5463](https://github.com/BoundaryML/baml/pull/5463) — feat: allow baml pack to use a locally built host binary.
- [#5422](https://github.com/BoundaryML/baml/pull/5422) — fix: implement a baml-native wrapper.
- [#5496](https://github.com/BoundaryML/baml/pull/5496) — Look up only a runtime class's own witnesses.
- [#5502](https://github.com/BoundaryML/baml/pull/5502) — stdlib docs: qualify static calls with baml. (TASK-198).
- [#5497](https://github.com/BoundaryML/baml/pull/5497) — Stop waking the idle-GC coordinator twice per call.
- [#5500](https://github.com/BoundaryML/baml/pull/5500) — Run cleanup on Ctrl-C / SIGTERM.
- [#5494](https://github.com/BoundaryML/baml/pull/5494) — Redact credentials in google + aws errors.
- [#5509](https://github.com/BoundaryML/baml/pull/5509) — Stop the Rust bridge from turning on serde_json's arbitrary_precision and preserve_order.
- [#5491](https://github.com/BoundaryML/baml/pull/5491) — feat: point baml-in-baml bcs telemetry at prod.
- [#5511](https://github.com/BoundaryML/baml/pull/5511) — Decode class-union fields to their Go structs in generic decoding.
- [#5508](https://github.com/BoundaryML/baml/pull/5508) — Resolve async Python calls on their event loop in batches.
- [#5518](https://github.com/BoundaryML/baml/pull/5518) — Stop printing a napi leak warning when a Node process exits early.
- [#5521](https://github.com/BoundaryML/baml/pull/5521) — Use log crate in rust bridge instead of printing to stderr.
- [#5516](https://github.com/BoundaryML/baml/pull/5516) — fix(installer): fall back to plain `toolchain use` when the help probe fails.
- [#5519](https://github.com/BoundaryML/baml/pull/5519) — Generate async functions by default, with _sync twins.
- [#5498](https://github.com/BoundaryML/baml/pull/5498) — Escape analysis: both backends skip locks on task-local objects.
- [#5503](https://github.com/BoundaryML/baml/pull/5503) — Bytecode: compile `for` over an array as an index walk.
- [#5506](https://github.com/BoundaryML/baml/pull/5506) — Native: keep the compiled-runtime hot helpers inlinable.
- [#5524](https://github.com/BoundaryML/baml/pull/5524) — Keep known compound words like OAuth whole in language naming.
- [#5526](https://github.com/BoundaryML/baml/pull/5526) — Make functions BAML returns async, with sync twins.
- [#5527](https://github.com/BoundaryML/baml/pull/5527) — Generate a new constructor for Rust structs that defaults optional fields to None.
- [#5531](https://github.com/BoundaryML/baml/pull/5531) — fix(escape analysis): share objects a finalizer can reach.
- [#5537](https://github.com/BoundaryML/baml/pull/5537) — Move setup-baml and baml-action into the monorepo.
- [#5483](https://github.com/BoundaryML/baml/pull/5483) — feat: release baml-in-baml to prod: 0.3.0 wrapper, 0.24.0 toolchain.

All other PR dispositions are recorded in [the full inventory](step1a-release-inventory.md). Inclusion requires a shipped effect, including SDK metadata, installer compatibility, bundled docs, and public action tags outside baml_language. Notification scope remains broader and includes all 81 v1 PRs.
