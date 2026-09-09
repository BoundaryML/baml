# Aggregated and classified user effects

Range: `baml-language-0.17.0..baml-language-0.18.0` (lower tag excluded; upper tag included). Sources were inspected at the upper tag, not current canary.

Each entry has exactly one classification and at most three source PRs. Compatibility changes that repair invalid behavior are classified as BUGFIX; their required user actions remain explicit. There is one headline: local execution queries. Performance improvements are FEATURE and retain the PR measurements.

## C01: Query local execution traces

**HEADLINE_CHANGE** · [#4548](https://github.com/BoundaryML/baml/pull/4548), [#4563](https://github.com/BoundaryML/baml/pull/4563), [#4578](https://github.com/BoundaryML/baml/pull/4578)

Use `baml query` to explore locally recorded executions with SQL. The playground’s Telemetry tab shows executions, call paths, retained spans, errors, and captured values. Profiling data lives in `.baml/profiles-v1`.

## C02: Compile runtime packages faster

**FEATURE** · [#4453](https://github.com/BoundaryML/baml/pull/4453)

Runtime package compilation reuses a precompiled standard library. The PR’s five-sample cold-path median fell from approximately 4–5 seconds to 20.6 milliseconds for a small package. These measurements describe that workload and host.

## C03: Reduce compiler inference work

**FEATURE** · [#4458](https://github.com/BoundaryML/baml/pull/4458), [#4463](https://github.com/BoundaryML/baml/pull/4463)

PR #4458 reports an empty-project median of 530.2 → 489.1 ms and a full-test-project median of 2.333 → 2.120 s. PR #4463 reports 497.5 → 485.6 ms and 2.180 → 2.133 s against its own immediate baseline. These are separate A/B measurements; their percentages must not be added.

## C04: Observe LLM calls with on_event

**FEATURE** · [#4570](https://github.com/BoundaryML/baml/pull/4570)

Direct calls, explicit agents, and streams accept the same optional event listener. Events expose usage and request/response information. Errors thrown by a listener do not fail the LLM run. Streams report usage when they settle.

## C05: Iterate over Python streams

**FEATURE** · [#4604](https://github.com/BoundaryML/baml/pull/4604)

Python streams support `async for`. Each iteration yields a non-null partial result. Read the completed value with `final_async()`.

## C06: Parse streamed backlogs in batches

**FEATURE** · [#4604](https://github.com/BoundaryML/baml/pull/4604)

The PR’s debug-profile benchmark measured 27.2 seconds of per-delta parsing for approximately 99 KB across 2,550 deltas. Parsing 32-delta backlogs took 0.9 seconds; 128-delta backlogs took 0.23 seconds. At approximately 33 KB, the corresponding measurements were 2.9 seconds, 393 ms, and 40 ms. This is parser work under backlogs, not end-to-end provider latency.

## C07: Preview requests without credentials

**FEATURE** · [#4459](https://github.com/BoundaryML/baml/pull/4459)

Build a provider request for inspection without making an LLM call or requiring API credentials. Preview authentication uses placeholders. A preview that would require file reads or URL fetches reports `PreviewUnsupported`.

## C08: Configure native request deadlines and schema attempts

**FEATURE** · [#4459](https://github.com/BoundaryML/baml/pull/4459)

OpenAI, Anthropic, and Vertex clients support total-request and time-to-first-token deadlines on the native runtime. Configure schema repair attempts on `ai.Agent`. Browser SSE deadlines are not enforced by this change.

## C09: Default nullable Python model fields to None

**FEATURE** · [#4459](https://github.com/BoundaryML/baml/pull/4459)

Generated Python models default nullable class fields to None and ignore unknown fields. Nullable function parameters still require an argument unless the BAML function declares a default.

## C10: Inspect class values through reflection

**FEATURE** · [#4491](https://github.com/BoundaryML/baml/pull/4491), [#4493](https://github.com/BoundaryML/baml/pull/4493)

Narrow an opaque class value to `reflect.AnyClass`. Read fields and inspect their metadata without knowing the class at compile time. The API is read-only. Bound field handles expose `value<T>()`.

## C11: Use unreflect in nested type expressions

**FEATURE** · [#4574](https://github.com/BoundaryML/baml/pull/4574)

`unreflect(expr)` can appear inside local type expressions. Item signatures still reject it because they have no body scope to own the runtime type. Name the runtime type first when it would escape through a result or thrown value.

## C12: Use truthiness in conditions

**FEATURE** · [#4498](https://github.com/BoundaryML/baml/pull/4498)

Conditions and logical operators accept truthy and falsy values. Null, false, numeric zero, empty strings, empty containers, and empty bytes are falsy. Logical operators still return bool. A truthy nullable-string condition narrows the value to string. Strings also gain `is_empty()`.

## C13: Call interface members explicitly

**FEATURE** · [#4500](https://github.com/BoundaryML/baml/pull/4500)

Use `(Type as Interface).member` to select an implementation. `Interface.instance_method(instance)` infers the implementing type.

## C14: Infer stored lambda parameters from later calls

**FEATURE** · [#4599](https://github.com/BoundaryML/baml/pull/4599)

A stored lambda can infer an omitted parameter type from its uses.

## C15: Wrap unknown errors while retaining their cause

**FEATURE** · [#4441](https://github.com/BoundaryML/baml/pull/4441)

`UnknownError.from<T>()` preserves known errors and avoids wrapping an error twice. `with_message<T>()` adds context while retaining the original cause and stack trace.

## C16: Limit and skip iterator elements

**FEATURE** · [#4510](https://github.com/BoundaryML/baml/pull/4510)

Iterators gain `take`, `skip`, `take_while`, and `skip_while`. These adapters are lazy. `take` can bound an infinite iterator before collection.

## C17: Choose the random generator for primitive values

**FEATURE** · [#4135](https://github.com/BoundaryML/baml/pull/4135)

`int.random()`, `float.random()`, and `bigint.random()` accept an optional `rng`. `bool.random()` is new and accepts the same option.

## C18: Share I/O code across readers and writers

**FEATURE** · [#4606](https://github.com/BoundaryML/baml/pull/4606)

Files, TCP streams, and process pipes implement `baml.io.Read` and `baml.io.Write`. Readers share `bytes()` and `text()`. Writers share `write()`, which retries partial writes until all bytes are accepted.

## C19: Use throwing expressions in prompts

**FEATURE** · [#4604](https://github.com/BoundaryML/baml/pull/4604)

Prompt interpolation can call functions that throw. Rendering failures surface as `ai.errors.PromptRenderError`.

## C20: Remove output types from Agent and Runner

**BREAKING_CHANGE** · [#4570](https://github.com/BoundaryML/baml/pull/4570), [#4604](https://github.com/BoundaryML/baml/pull/4604)

`ai.Agent` is no longer generic. `ai.Runner` now puts `Out` on its `run` method and returns `ai.RunResult<Out>`. Custom runner implementations must move their output type parameter to the method and declare their associated `Error`.

## C21: Handle batches from TurnStream.next()

**BREAKING_CHANGE** · [#4604](https://github.com/BoundaryML/baml/pull/4604)

Low-level `ai.stream.TurnStream.next()` returns `string[] | ai.stream.Done`. Iterate the returned array to process individual deltas. High-level streams continue to return parsed partial values.

## C22: Use the reflect root package

**BREAKING_CHANGE** · [#4543](https://github.com/BoundaryML/baml/pull/4543), [#4580](https://github.com/BoundaryML/baml/pull/4580)

Replace `baml.reflect.*` with `reflect.*`. Replace the bare runtime `type` value type with `reflect.Type`. `AnyClass` and `AnyFunction` also move from `baml` into `reflect`.

## C23: Convert reflection views explicitly

**BREAKING_CHANGE** · [#4580](https://github.com/BoundaryML/baml/pull/4580)

Reflection kind views are wrappers. They are no longer subtypes of `reflect.Type`. Use `as_type()` when an API needs the underlying type.

## C24: Remove generic arguments from JSON serialization

**BREAKING_CHANGE** · [#4601](https://github.com/BoundaryML/baml/pull/4601)

`to_string` and `to_json` now inspect the runtime value, including values typed as `unknown`. Remove explicit type arguments. Replace `encode` with `to_string`. `from_string<T>` remains generic.

## C25: Replace hash string literals

**BREAKING_CHANGE** · [#4565](https://github.com/BoundaryML/baml/pull/4565)

Hash-delimited strings are rejected throughout BAML. Use quoted strings for plain text or backticks for interpolation. Rewrite Jinja expressions as BAML expressions.

## C26: Replace legacy declarative test blocks

**BREAKING_CHANGE** · [#4602](https://github.com/BoundaryML/baml/pull/4602)

Write tests as expression bodies. Call the function and assert on its result inside the test.

## C27: Call ctx.output_format()

**BREAKING_CHANGE** · [#4567](https://github.com/BoundaryML/baml/pull/4567)

Use `ctx.output_format()` in prompts. It also accepts output-format options such as prefixes, class hoisting, enum formatting, map style, and the null spelling.

## C28: Update generated SDK paths

**BREAKING_CHANGE** · [#4522](https://github.com/BoundaryML/baml/pull/4522), [#4535](https://github.com/BoundaryML/baml/pull/4535)

With no `output_dir`, generators write `baml_sdk` next to `baml.toml`. C# now uses the same `baml_sdk` directory name. Update build includes and scripts. An explicit manifest `output_dir` still names the parent directory.

## C29: Set the test timeout explicitly for long tests

**BREAKING_CHANGE** · [#4541](https://github.com/BoundaryML/baml/pull/4541)

Tests now time out after 300,000 ms by default. Raise `BAML_TEST_TIMEOUT_MS` for tests that intentionally take longer.

## C30: Load the Node bridge on Alpine Linux

**BUGFIX** · [#4502](https://github.com/BoundaryML/baml/pull/4502)

The x86_64 and aarch64 musl Node addons now link against musl and load on Alpine. The PR verified both architectures with fixed and unfixed builds. This user-visible fix is implemented entirely in a GitHub workflow.

## C31: Show BAML logs during CLI execution

**BUGFIX** · [#4409](https://github.com/BoundaryML/baml/pull/4409)

`baml run` surfaces `log.*` events and respects `--log` and `BAML_LOG`. The test command accepts the unified log option. This does not establish that packed-binary logging or `--log-file` is fixed.

## C32: Explain waits during shutdown

**BUGFIX** · [#4408](https://github.com/BoundaryML/baml/pull/4408), [#4541](https://github.com/BoundaryML/baml/pull/4541)

CLI shutdown reports active futures and supports cancellation. Leaked-future cleanup no longer leaves completed tests waiting indefinitely.

## C33: Reject invalid runtime compilation boundaries

**BUGFIX** · [#4460](https://github.com/BoundaryML/baml/pull/4460)

Indirect calls that need unsupported runtime type checks now produce E0010. Runtime-created types no longer leak hidden synthetic names into source diagnostics.

## C34: Diagnose unknown member access

**BUGFIX** · [#4466](https://github.com/BoundaryML/baml/pull/4466)

Calling a nonexistent member on an `unknown` receiver produces E0007 before execution.

## C35: Match numeric literal patterns by membership

**BUGFIX** · [#4478](https://github.com/BoundaryML/baml/pull/4478)

Literal patterns distinguish `1`, `1.0`, and `1n`. Dense integer matches no longer crash on a float. Literal aliases match their value instead of every value of the base type.

## C36: Remove redundant formatter parentheses

**BUGFIX** · [#4489](https://github.com/BoundaryML/baml/pull/4489), [#4541](https://github.com/BoundaryML/baml/pull/4541)

`baml fmt` removes unnecessary parentheses in binary chains, call arguments, postfix receivers, and unary operands. It keeps parentheses needed for meaning or comments.

## C37: Preserve map mutations across loop calls

**BUGFIX** · [#4467](https://github.com/BoundaryML/baml/pull/4467)

A map created before a loop retains changes made by callees in successive iterations.

## C38: Report unsupported LLM output schemas

**BUGFIX** · [#4470](https://github.com/BoundaryML/baml/pull/4470), [#4459](https://github.com/BoundaryML/baml/pull/4459)

Non-data output schemas produce structured errors rather than crashes or silently incomplete schemas. Realized generic output planning is finite and preserves supported schemas.

## C39: Diagnose invalid reflected generic extraction

**BUGFIX** · [#4473](https://github.com/BoundaryML/baml/pull/4473)

Extracting an unspecialized generic function reports a diagnostic instead of returning null silently. Generic specialization is not available in the final release.

## C40: Reject falling-through let-else branches

**BUGFIX** · [#4491](https://github.com/BoundaryML/baml/pull/4491), [#4493](https://github.com/BoundaryML/baml/pull/4493)

A `let ... else` branch must diverge. Invalid branches produce E0113, including loops that can break back into the surrounding function. Invalid primitive companion types also receive source diagnostics.

## C41: Compile loops over inferred iterator results

**BUGFIX** · [#4490](https://github.com/BoundaryML/baml/pull/4490)

Loops over joined map results and `Iterable`-bounded generics no longer abort compilation.

## C42: Retain generic arguments in optional calls

**BUGFIX** · [#4495](https://github.com/BoundaryML/baml/pull/4495)

`x?.method<T>()` carries its type arguments to the method and no longer fails inside the VM.

## C43: Preserve runtime definitions and reflected metadata

**BUGFIX** · [#4501](https://github.com/BoundaryML/baml/pull/4501), [#4583](https://github.com/BoundaryML/baml/pull/4583)

Runtime class and enum definitions survive interface dispatch and nested reflection views. Reflected declarations preserve field metadata, docstrings, and declaration order.

## C44: Preserve runtime type identity through dispatch

**BUGFIX** · [#4516](https://github.com/BoundaryML/baml/pull/4516), [#4536](https://github.com/BoundaryML/baml/pull/4536)

Runtime-created and runtime-compiled types keep their identity inside interface implementations. Equality checks and maps keyed by these types remain consistent.

## C45: Prevent runtime types from escaping unnamed

**BUGFIX** · [#4518](https://github.com/BoundaryML/baml/pull/4518), [#4530](https://github.com/BoundaryML/baml/pull/4530)

Inline runtime type arguments that escape through constructed result types, thrown types, or optional chains produce E0168 with guidance to name the type first.

## C46: Call methods on top-level bindings

**BUGFIX** · [#4529](https://github.com/BoundaryML/baml/pull/4529)

Methods on top-level `let` and `client` bindings receive the correct receiver. Session-bound calls keep their structured errors instead of replacing them with VM failures.

## C47: Type-check Session assignments

**BUGFIX** · [#4531](https://github.com/BoundaryML/baml/pull/4531)

Assigning to a Session binding must match its declared type. Invalid assignments fail during compilation. Redeclaring a binding can establish a new type.

## C48: Validate generator naming conventions

**BUGFIX** · [#4526](https://github.com/BoundaryML/baml/pull/4526)

Unsupported generator naming conventions produce E0019 at `baml.toml`. Go and C# require `language`; other targets require `preserve-case`.

## C49: Preserve reassigned values across branches and loops

**BUGFIX** · [#4508](https://github.com/BoundaryML/baml/pull/4508), [#4544](https://github.com/BoundaryML/baml/pull/4544)

Short-circuit expressions and branch-assigned locals retain their values without leaking operands or hanging later loop iterations.

## C50: Honor array annotations in match coverage

**BUGFIX** · [#4547](https://github.com/BoundaryML/baml/pull/4547)

Match arms annotated with `int[]` or `string[]` correctly refine a union of arrays.

## C51: Avoid compiler failures in local runtime-type calls

**BUGFIX** · [#4541](https://github.com/BoundaryML/baml/pull/4541)

Generic calls involving local runtime type bindings lower without the previous internal compiler failure. Context-typed lambda signatures also infer correctly. Malformed diagnostic fragments fall back to plain text.

## C52: Locate unresolved-type diagnostics correctly

**BUGFIX** · [#4566](https://github.com/BoundaryML/baml/pull/4566)

Undefined types produce diagnostics anchored to the owning signature or body. Diagnostic rendering no longer panics or points at an unrelated expression.

## C53: Truncate overflowing integer left shifts

**BUGFIX** · [#4135](https://github.com/BoundaryML/baml/pull/4135)

Integer left shifts truncate at the 63-bit integer width instead of panicking on overflow.

## C54: Diagnose untyped empty containers

**BUGFIX** · [#4573](https://github.com/BoundaryML/baml/pull/4573)

Empty arrays and maps with no inferable element type produce E0155. Add a type annotation or supply contextual typing.

## C55: Keep editor results tied to the current document

**BUGFIX** · [#4581](https://github.com/BoundaryML/baml/pull/4581)

The language server rejects stale background results and uses unsaved editor buffers during project discovery. File and workspace changes update project ownership and diagnostics.

## C56: Reject incompatible compiler artifacts early

**BUGFIX** · [#4568](https://github.com/BoundaryML/baml/pull/4568)

Versioned bytecode and package artifacts validate their format, compiler fingerprint, and checksum before decoding. Canary or development builds with mismatched fingerprints report an actionable error. Regenerate the SDK and use a matching bridge.

## C57: Avoid Session helper-name collisions

**BUGFIX** · [#4571](https://github.com/BoundaryML/baml/pull/4571)

Generated Session evaluation helpers no longer collide with user-defined binding names.

## C58: Keep reflected container and schema identities intact

**BUGFIX** · [#4577](https://github.com/BoundaryML/baml/pull/4577)

Empty runtime containers preserve their original type identity. Standalone rendered schemas include required static dependencies. Conflicting same-name declarations produce an error; equivalent ones fold together.

## C59: Retain runtime evaluation errors

**BUGFIX** · [#4583](https://github.com/BoundaryML/baml/pull/4583)

Session evaluation and runtime compilation preserve original diagnostics and structured causes.

## C60: Validate reflect.call_any results

**BUGFIX** · [#4600](https://github.com/BoundaryML/baml/pull/4600)

`reflect.call_any` checks the returned value against the requested result type. A mismatch raises `reflect.InvalidArgumentError` at the call boundary.

## C61: Preserve host media and prompt rendering

**BUGFIX** · [#4604](https://github.com/BoundaryML/baml/pull/4604)

Optional media arrays cross the host boundary correctly. Host-provided media appears in rendered requests. Output-format layout and role-edge whitespace match the migration parity fixtures.

## C62: Preserve valid optional class responses

**BUGFIX** · [#4612](https://github.com/BoundaryML/baml/pull/4612)

Omitting an optional nested class field no longer causes a valid containing optional object to become null. Regression coverage exercises both OpenAI Chat and Responses ingestion.

## C63: Require values for required class fields

**BUGFIX** · [#4619](https://github.com/BoundaryML/baml/pull/4619)

Class constructors report E0001 for omitted required fields. Supply every required value or make the field nullable. An `unknown` field still requires an explicit value.

## C64: Specialize generic function values before storing them

**BUGFIX** · [#4621](https://github.com/BoundaryML/baml/pull/4621)

A stored function value needs a concrete signature. Use `identity<string>` or annotate the binding with a concrete function type. Direct generic calls can still infer arguments independently.

## C65: Reject imprecise throws unknown declarations

**BUGFIX** · [#4593](https://github.com/BoundaryML/baml/pull/4593)

Functions that do not throw, or only throw a narrower type, report E0097 for `throws unknown`. Remove the clause or give a precise bound. Real unknown error boundaries remain supported.

## C66: Preserve Python values, cancellation, and typed provider errors

**BUGFIX** · [#4459](https://github.com/BoundaryML/baml/pull/4459)

Generated Python models preserve concrete union values and stream cancellation. Provider errors retain typed failure information, including `FinishReasonError`. Streaming retry, fallback, and round-robin clients can recover before the first text delta.

## C67: Restore HTTP in CLI and playground execution

**BUGFIX** · [#4609](https://github.com/BoundaryML/baml/pull/4609)

HTTP operations work again in baml run and the playground because their runtime includes the HTTP implementation.

## C68: Migrate file, socket, and process-pipe I/O

**BREAKING_CHANGE** · [#4606](https://github.com/BoundaryML/baml/pull/4606)

Reads now take a byte limit and return null at EOF. Replace file.read_bytes with file.read and file.write_bytes with file.write. Process stdin is a WritePipe: replace write_stdin and close_stdin with stdin.write and stdin.close. TCP read/write no longer accept per-call timeout parameters; use baml.future.with_timeout.

## C69: Configure shutdown waits for background work

**BREAKING_CHANGE** · [#4541](https://github.com/BoundaryML/baml/pull/4541)

CLI shutdown now allows 15 seconds for remaining background futures before cancelling and abandoning them. Set BAML_SHUTDOWN_GRACE_MS=0 to restore an unlimited wait. Explicitly await or clean up background work when it must complete.
