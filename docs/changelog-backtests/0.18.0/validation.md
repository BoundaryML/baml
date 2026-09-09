# Validation evidence

Validation used the published macOS ARM64 BAML 0.18.0 artifact, not the worktree’s current compiler. Its version command reports `baml-cli 0.18.0`. The downloaded archive’s SHA-256 matched the release checksum: `dad86121702f9f6c10ade1b5cd834379848d16910613f96fe398c875556dc67e`.

Release asset: [baml-language-0.18.0-aarch64-apple-darwin.tar.gz](https://github.com/BoundaryML/baml/releases/download/baml-language-0.18.0/baml-language-0.18.0-aarch64-apple-darwin.tar.gz). The matching `.sha256` release asset was checked before extraction. Source inspection used `git show baml-language-0.18.0:<path>` and immutable merge diffs, so later canary changes could not change the API examples. Temporary test projects and downloaded artifacts were kept outside the repository.

## Examples

All 22 positive BAML snippets compile with the released CLI. Each was checked in its own project with `[package] name = "root"` and a single `baml_src/main.baml`. Historical “before” snippets are migration illustrations and are not expected to compile under 0.18.0.

| Entry | Example | `baml check` | Pure execution |
| --- | --- | --- | --- |
| C04 | Observe LLM calls with on_event | Pass | Compile only |
| C07 | Preview requests without credentials | Pass | Compile only |
| C08 | Configure native request deadlines and schema attempts | Pass | Compile only |
| C10 | Inspect class values through reflection | Pass | Pass |
| C11 | Use unreflect in nested type expressions | Pass | Pass |
| C12 | Use truthiness in conditions | Pass | Pass |
| C13 | Call interface members explicitly | Pass | Pass |
| C14 | Infer stored lambda parameters from later calls | Pass | Pass |
| C15 | Wrap unknown errors while retaining their cause | Pass | Compile only |
| C16 | Limit and skip iterator elements | Pass | Pass |
| C17 | Choose the random generator for primitive values | Pass | Pass |
| C18 | Share I/O code across readers and writers | Pass | Compile only |
| C19 | Use throwing expressions in prompts | Pass | Compile only |
| C20 | Remove output types from Agent and Runner | Pass | Compile only |
| C21 | Handle batches from TurnStream.next() | Pass | Compile only |
| C22 | Use the reflect root package | Pass | Compile only |
| C23 | Convert reflection views explicitly | Pass | Pass |
| C24 | Remove generic arguments from JSON serialization | Pass | Pass |
| C25 | Replace hash string literals | Pass | Compile only |
| C26 | Replace legacy declarative test blocks | Pass | Compile only |
| C27 | Call ctx.output_format() | Pass | Compile only |
| C68 | Migrate file, socket, and process-pipe I/O | Pass | Compile only |

Nine pure examples were also executed successfully: class field reflection, nested runtime types, truthiness, interface calls, stored lambdas, iterator adapters, random-generator selection, type-view conversion, and JSON serialization. LLM examples were not sent to providers. Source-level compilation validates their BAML surface, not provider integration.

Both Python snippets pass Python syntax parsing. The 0.18.0 generator was also run on a `Record { label: string? }` schema. Its Python model and stub contain `label: typing.Optional[str] = None`, and the model declares `extra="ignore"`. The async-stream example’s exported function name exists in that generated SDK. Python bridge iteration was checked against the final-tag `_stream.py` implementation; it was not executed against a live provider.

The explicit `output_dir = ".."` example uses the release manifest’s `[generator.python]` table with `naming_convention = "preserve-case"`. The generator’s default path was observed when generating the model fixture: it created `<project>/baml_sdk`.

The query example was executed after recording a local `main` run with `BAML_PROFILE=1`. `SELECT * FROM threads WHERE parent_thread_id IS NULL LIMIT 10` returned one execution root with `status=succeeded` and terminal `result=complete`. `baml query --schema` also succeeded. The catalog has no `executions` table; the draft uses the actual `threads` relation.

## Targeted historical checks

| Claim / source | Released 0.18.0 result | Consequence |
| --- | --- | --- |
| `reflect.function.Type.specialize` from #4519 | E0007: no member `specialize` | Exclude the removed feature. The deletion is visible in #4560. |
| #4429 CLI logging | `baml run --log error main` prints the expected error-level marker; exit 0 | Draft a partial issue update. |
| #4588 lost runtime errors | A closed localhost endpoint produces `ai.errors.NetworkFailure` with traceback; exit 1 | Confirm that specific failure path. Other scenarios rely on the reporter’s nightly confirmation, not a fresh replay here. |
| #4506 inferred generic missing method | `baml check` exits 0; running the function produces the reported internal compiler error; exit 1 | Do not mark the issue fixed. The repro omits its old `throws unknown` declaration so the new precision diagnostic cannot mask the defect. |

## Document checks

The artifact checks cover all 114 PR decisions, at most three source PRs per aggregated effect, one classification per effect, example or benchmark evidence for every feature/headline, unique notification destinations, source-quote lengths, balanced code fences, and relative links. They also verify `isPublished: false` and that the historical blog post has no diff. Whitespace and applicable repository hooks are checked before committing.

## Limits

This is a documentation backtest, not a full release certification. It does not rerun Rust/SDK CI for the historical toolchain, reproduce author benchmarks, exercise provider calls, or validate every migration against the old binary. Performance tables retain their PR-specific baselines and build-profile qualifications. Current GitHub/Linear source records can include edits and comments made after release day. All direct non-v0 PRs were checked for notification candidates; unlinked Discord conversations and the external Shortcut story were not searched. No private issue bodies, credentials, or downloaded binaries are committed.
