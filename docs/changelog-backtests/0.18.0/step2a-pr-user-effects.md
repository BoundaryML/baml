# User effects by PR

Range: `baml-language-0.17.0..baml-language-0.18.0` (lower tag excluded; upper tag included). Sources were inspected at the upper tag, not current canary.

A PR can contain several independently classified effects. Intermediate API names are replaced with the names present in 0.18.0.

## [#4453](https://github.com/BoundaryML/baml/pull/4453)

- **C02 · FEATURE:** Runtime package compilation reuses a precompiled standard library. The PR’s five-sample cold-path median fell from approximately 4–5 seconds to 20.6 milliseconds for a small package. These measurements describe that workload and host.

## [#4458](https://github.com/BoundaryML/baml/pull/4458)

- **C03 · FEATURE:** PR #4458 reports an empty-project median of 530.2 → 489.1 ms and a full-test-project median of 2.333 → 2.120 s. PR #4463 reports 497.5 → 485.6 ms and 2.180 → 2.133 s against its own immediate baseline. These are separate A/B measurements; their percentages must not be added.

## [#4460](https://github.com/BoundaryML/baml/pull/4460)

- **C33 · BUGFIX:** Indirect calls that need unsupported runtime type checks now produce E0010. Runtime-created types no longer leak hidden synthetic names into source diagnostics.

## [#4463](https://github.com/BoundaryML/baml/pull/4463)

- **C03 · FEATURE:** PR #4458 reports an empty-project median of 530.2 → 489.1 ms and a full-test-project median of 2.333 → 2.120 s. PR #4463 reports 497.5 → 485.6 ms and 2.180 → 2.133 s against its own immediate baseline. These are separate A/B measurements; their percentages must not be added.

## [#4408](https://github.com/BoundaryML/baml/pull/4408)

- **C32 · BUGFIX:** CLI shutdown reports active futures and supports cancellation. Leaked-future cleanup no longer leaves completed tests waiting indefinitely.

## [#4409](https://github.com/BoundaryML/baml/pull/4409)

- **C31 · BUGFIX:** `baml run` surfaces `log.*` events and respects `--log` and `BAML_LOG`. The test command accepts the unified log option. This does not establish that packed-binary logging or `--log-file` is fixed.

## [#4466](https://github.com/BoundaryML/baml/pull/4466)

- **C34 · BUGFIX:** Calling a nonexistent member on an `unknown` receiver produces E0007 before execution.

## [#4478](https://github.com/BoundaryML/baml/pull/4478)

- **C35 · BUGFIX:** Literal patterns distinguish `1`, `1.0`, and `1n`. Dense integer matches no longer crash on a float. Literal aliases match their value instead of every value of the base type.

## [#4489](https://github.com/BoundaryML/baml/pull/4489)

- **C36 · BUGFIX:** `baml fmt` removes unnecessary parentheses in binary chains, call arguments, postfix receivers, and unary operands. It keeps parentheses needed for meaning or comments.

## [#4467](https://github.com/BoundaryML/baml/pull/4467)

- **C37 · BUGFIX:** A map created before a loop retains changes made by callees in successive iterations.

## [#4470](https://github.com/BoundaryML/baml/pull/4470)

- **C38 · BUGFIX:** Non-data output schemas produce structured errors rather than crashes or silently incomplete schemas. Realized generic output planning is finite and preserves supported schemas.

## [#4473](https://github.com/BoundaryML/baml/pull/4473)

- **C39 · BUGFIX:** Extracting an unspecialized generic function reports a diagnostic instead of returning null silently. Generic specialization is not available in the final release.

## [#4491](https://github.com/BoundaryML/baml/pull/4491)

- **C10 · FEATURE:** Narrow an opaque class value to `reflect.AnyClass`. Read fields and inspect their metadata without knowing the class at compile time. The API is read-only. Bound field handles expose `value<T>()`.
- **C40 · BUGFIX:** A `let ... else` branch must diverge. Invalid branches produce E0113, including loops that can break back into the surrounding function. Invalid primitive companion types also receive source diagnostics.

## [#4493](https://github.com/BoundaryML/baml/pull/4493)

- **C10 · FEATURE:** Narrow an opaque class value to `reflect.AnyClass`. Read fields and inspect their metadata without knowing the class at compile time. The API is read-only. Bound field handles expose `value<T>()`.
- **C40 · BUGFIX:** A `let ... else` branch must diverge. Invalid branches produce E0113, including loops that can break back into the surrounding function. Invalid primitive companion types also receive source diagnostics.

## [#4490](https://github.com/BoundaryML/baml/pull/4490)

- **C41 · BUGFIX:** Loops over joined map results and `Iterable`-bounded generics no longer abort compilation.

## [#4495](https://github.com/BoundaryML/baml/pull/4495)

- **C42 · BUGFIX:** `x?.method<T>()` carries its type arguments to the method and no longer fails inside the VM.

## [#4501](https://github.com/BoundaryML/baml/pull/4501)

- **C43 · BUGFIX:** Runtime class and enum definitions survive interface dispatch and nested reflection views. Reflected declarations preserve field metadata, docstrings, and declaration order.

## [#4498](https://github.com/BoundaryML/baml/pull/4498)

- **C12 · FEATURE:** Conditions and logical operators accept truthy and falsy values. Null, false, numeric zero, empty strings, empty containers, and empty bytes are falsy. Logical operators still return bool. A truthy nullable-string condition narrows the value to string. Strings also gain `is_empty()`.

## [#4518](https://github.com/BoundaryML/baml/pull/4518)

- **C45 · BUGFIX:** Inline runtime type arguments that escape through constructed result types, thrown types, or optional chains produce E0168 with guidance to name the type first.

## [#4516](https://github.com/BoundaryML/baml/pull/4516)

- **C44 · BUGFIX:** Runtime-created and runtime-compiled types keep their identity inside interface implementations. Equality checks and maps keyed by these types remain consistent.

## [#4529](https://github.com/BoundaryML/baml/pull/4529)

- **C46 · BUGFIX:** Methods on top-level `let` and `client` bindings receive the correct receiver. Session-bound calls keep their structured errors instead of replacing them with VM failures.

## [#4531](https://github.com/BoundaryML/baml/pull/4531)

- **C47 · BUGFIX:** Assigning to a Session binding must match its declared type. Invalid assignments fail during compilation. Redeclaring a binding can establish a new type.

## [#4530](https://github.com/BoundaryML/baml/pull/4530)

- **C45 · BUGFIX:** Inline runtime type arguments that escape through constructed result types, thrown types, or optional chains produce E0168 with guidance to name the type first.

## [#4441](https://github.com/BoundaryML/baml/pull/4441)

- **C15 · FEATURE:** `UnknownError.from<T>()` preserves known errors and avoids wrapping an error twice. `with_message<T>()` adds context while retaining the original cause and stack trace.

## [#4502](https://github.com/BoundaryML/baml/pull/4502)

- **C30 · BUGFIX:** The x86_64 and aarch64 musl Node addons now link against musl and load on Alpine. The PR verified both architectures with fixed and unfixed builds. This user-visible fix is implemented entirely in a GitHub workflow.

## [#4535](https://github.com/BoundaryML/baml/pull/4535)

- **C28 · BREAKING_CHANGE:** With no `output_dir`, generators write `baml_sdk` next to `baml.toml`. C# now uses the same `baml_sdk` directory name. Update build includes and scripts. An explicit manifest `output_dir` still names the parent directory.

## [#4459](https://github.com/BoundaryML/baml/pull/4459)

- **C07 · FEATURE:** Build a provider request for inspection without making an LLM call or requiring API credentials. Preview authentication uses placeholders. A preview that would require file reads or URL fetches reports `PreviewUnsupported`.
- **C08 · FEATURE:** OpenAI, Anthropic, and Vertex clients support total-request and time-to-first-token deadlines on the native runtime. Configure schema repair attempts on `ai.Agent`. Browser SSE deadlines are not enforced by this change.
- **C09 · FEATURE:** Generated Python models default nullable class fields to None and ignore unknown fields. Nullable function parameters still require an argument unless the BAML function declares a default.
- **C38 · BUGFIX:** Non-data output schemas produce structured errors rather than crashes or silently incomplete schemas. Realized generic output planning is finite and preserves supported schemas.
- **C66 · BUGFIX:** Generated Python models preserve concrete union values and stream cancellation. Provider errors retain typed failure information, including `FinishReasonError`. Streaming retry, fallback, and round-robin clients can recover before the first text delta.

## [#4526](https://github.com/BoundaryML/baml/pull/4526)

- **C48 · BUGFIX:** Unsupported generator naming conventions produce E0019 at `baml.toml`. Go and C# require `language`; other targets require `preserve-case`.

## [#4510](https://github.com/BoundaryML/baml/pull/4510)

- **C16 · FEATURE:** Iterators gain `take`, `skip`, `take_while`, and `skip_while`. These adapters are lazy. `take` can bound an infinite iterator before collection.

## [#4536](https://github.com/BoundaryML/baml/pull/4536)

- **C44 · BUGFIX:** Runtime-created and runtime-compiled types keep their identity inside interface implementations. Equality checks and maps keyed by these types remain consistent.

## [#4500](https://github.com/BoundaryML/baml/pull/4500)

- **C13 · FEATURE:** Use `(Type as Interface).member` to select an implementation. `Interface.instance_method(instance)` infers the implementing type.

## [#4508](https://github.com/BoundaryML/baml/pull/4508)

- **C49 · BUGFIX:** Short-circuit expressions and branch-assigned locals retain their values without leaking operands or hanging later loop iterations.

## [#4544](https://github.com/BoundaryML/baml/pull/4544)

- **C49 · BUGFIX:** Short-circuit expressions and branch-assigned locals retain their values without leaking operands or hanging later loop iterations.

## [#4522](https://github.com/BoundaryML/baml/pull/4522)

- **C28 · BREAKING_CHANGE:** With no `output_dir`, generators write `baml_sdk` next to `baml.toml`. C# now uses the same `baml_sdk` directory name. Update build includes and scripts. An explicit manifest `output_dir` still names the parent directory.

## [#4547](https://github.com/BoundaryML/baml/pull/4547)

- **C50 · BUGFIX:** Match arms annotated with `int[]` or `string[]` correctly refine a union of arrays.

## [#4548](https://github.com/BoundaryML/baml/pull/4548)

- **C01 · HEADLINE_CHANGE:** Use `baml query` to explore locally recorded executions with SQL. The playground’s Telemetry tab shows executions, call paths, retained spans, errors, and captured values. Profiling data lives in `.baml/profiles-v1`.

## [#4541](https://github.com/BoundaryML/baml/pull/4541)

- **C29 · BREAKING_CHANGE:** Tests now time out after 300,000 ms by default. Raise `BAML_TEST_TIMEOUT_MS` for tests that intentionally take longer.
- **C32 · BUGFIX:** CLI shutdown reports active futures and supports cancellation. Leaked-future cleanup no longer leaves completed tests waiting indefinitely.
- **C36 · BUGFIX:** `baml fmt` removes unnecessary parentheses in binary chains, call arguments, postfix receivers, and unary operands. It keeps parentheses needed for meaning or comments.
- **C51 · BUGFIX:** Generic calls involving local runtime type bindings lower without the previous internal compiler failure. Context-typed lambda signatures also infer correctly. Malformed diagnostic fragments fall back to plain text.
- **C69 · BREAKING_CHANGE:** CLI shutdown now allows 15 seconds for remaining background futures before cancelling and abandoning them. Set BAML_SHUTDOWN_GRACE_MS=0 to restore an unlimited wait. Explicitly await or clean up background work when it must complete.

## [#4543](https://github.com/BoundaryML/baml/pull/4543)

- **C22 · BREAKING_CHANGE:** Replace `baml.reflect.*` with `reflect.*`. Replace the bare runtime `type` value type with `reflect.Type`. `AnyClass` and `AnyFunction` also move from `baml` into `reflect`.

## [#4565](https://github.com/BoundaryML/baml/pull/4565)

- **C25 · BREAKING_CHANGE:** Hash-delimited strings are rejected throughout BAML. Use quoted strings for plain text or backticks for interpolation. Rewrite Jinja expressions as BAML expressions.

## [#4566](https://github.com/BoundaryML/baml/pull/4566)

- **C52 · BUGFIX:** Undefined types produce diagnostics anchored to the owning signature or body. Diagnostic rendering no longer panics or points at an unrelated expression.

## [#4135](https://github.com/BoundaryML/baml/pull/4135)

- **C17 · FEATURE:** `int.random()`, `float.random()`, and `bigint.random()` accept an optional `rng`. `bool.random()` is new and accepts the same option.
- **C53 · BUGFIX:** Integer left shifts truncate at the 63-bit integer width instead of panicking on overflow.

## [#4580](https://github.com/BoundaryML/baml/pull/4580)

- **C22 · BREAKING_CHANGE:** Replace `baml.reflect.*` with `reflect.*`. Replace the bare runtime `type` value type with `reflect.Type`. `AnyClass` and `AnyFunction` also move from `baml` into `reflect`.
- **C23 · BREAKING_CHANGE:** Reflection kind views are wrappers. They are no longer subtypes of `reflect.Type`. Use `as_type()` when an API needs the underlying type.

## [#4570](https://github.com/BoundaryML/baml/pull/4570)

- **C04 · FEATURE:** Direct calls, explicit agents, and streams accept the same optional event listener. Events expose usage and request/response information. Errors thrown by a listener do not fail the LLM run. Streams report usage when they settle.
- **C20 · BREAKING_CHANGE:** `ai.Agent` is no longer generic. `ai.Runner` now puts `Out` on its `run` method and returns `ai.RunResult<Out>`. Custom runner implementations must move their output type parameter to the method and declare their associated `Error`.

## [#4563](https://github.com/BoundaryML/baml/pull/4563)

- **C01 · HEADLINE_CHANGE:** Use `baml query` to explore locally recorded executions with SQL. The playground’s Telemetry tab shows executions, call paths, retained spans, errors, and captured values. Profiling data lives in `.baml/profiles-v1`.

## [#4578](https://github.com/BoundaryML/baml/pull/4578)

- **C01 · HEADLINE_CHANGE:** Use `baml query` to explore locally recorded executions with SQL. The playground’s Telemetry tab shows executions, call paths, retained spans, errors, and captured values. Profiling data lives in `.baml/profiles-v1`.

## [#4573](https://github.com/BoundaryML/baml/pull/4573)

- **C54 · BUGFIX:** Empty arrays and maps with no inferable element type produce E0155. Add a type annotation or supply contextual typing.

## [#4567](https://github.com/BoundaryML/baml/pull/4567)

- **C27 · BREAKING_CHANGE:** Use `ctx.output_format()` in prompts. It also accepts output-format options such as prefixes, class hoisting, enum formatting, map style, and the null spelling.

## [#4581](https://github.com/BoundaryML/baml/pull/4581)

- **C55 · BUGFIX:** The language server rejects stale background results and uses unsaved editor buffers during project discovery. File and workspace changes update project ownership and diagnostics.

## [#4568](https://github.com/BoundaryML/baml/pull/4568)

- **C56 · BUGFIX:** Versioned bytecode and package artifacts validate their format, compiler fingerprint, and checksum before decoding. Canary or development builds with mismatched fingerprints report an actionable error. Regenerate the SDK and use a matching bridge.

## [#4571](https://github.com/BoundaryML/baml/pull/4571)

- **C57 · BUGFIX:** Generated Session evaluation helpers no longer collide with user-defined binding names.

## [#4577](https://github.com/BoundaryML/baml/pull/4577)

- **C58 · BUGFIX:** Empty runtime containers preserve their original type identity. Standalone rendered schemas include required static dependencies. Conflicting same-name declarations produce an error; equivalent ones fold together.

## [#4574](https://github.com/BoundaryML/baml/pull/4574)

- **C11 · FEATURE:** `unreflect(expr)` can appear inside local type expressions. Item signatures still reject it because they have no body scope to own the runtime type. Name the runtime type first when it would escape through a result or thrown value.

## [#4583](https://github.com/BoundaryML/baml/pull/4583)

- **C43 · BUGFIX:** Runtime class and enum definitions survive interface dispatch and nested reflection views. Reflected declarations preserve field metadata, docstrings, and declaration order.
- **C59 · BUGFIX:** Session evaluation and runtime compilation preserve original diagnostics and structured causes.

## [#4601](https://github.com/BoundaryML/baml/pull/4601)

- **C24 · BREAKING_CHANGE:** `to_string` and `to_json` now inspect the runtime value, including values typed as `unknown`. Remove explicit type arguments. Replace `encode` with `to_string`. `from_string<T>` remains generic.

## [#4600](https://github.com/BoundaryML/baml/pull/4600)

- **C60 · BUGFIX:** `reflect.call_any` checks the returned value against the requested result type. A mismatch raises `reflect.InvalidArgumentError` at the call boundary.

## [#4602](https://github.com/BoundaryML/baml/pull/4602)

- **C26 · BREAKING_CHANGE:** Write tests as expression bodies. Call the function and assert on its result inside the test.

## [#4604](https://github.com/BoundaryML/baml/pull/4604)

- **C05 · FEATURE:** Python streams support `async for`. Each iteration yields a non-null partial result. Read the completed value with `final_async()`.
- **C06 · FEATURE:** The PR’s debug-profile benchmark measured 27.2 seconds of per-delta parsing for approximately 99 KB across 2,550 deltas. Parsing 32-delta backlogs took 0.9 seconds; 128-delta backlogs took 0.23 seconds. At approximately 33 KB, the corresponding measurements were 2.9 seconds, 393 ms, and 40 ms. This is parser work under backlogs, not end-to-end provider latency.
- **C19 · FEATURE:** Prompt interpolation can call functions that throw. Rendering failures surface as `ai.errors.PromptRenderError`.
- **C20 · BREAKING_CHANGE:** `ai.Agent` is no longer generic. `ai.Runner` now puts `Out` on its `run` method and returns `ai.RunResult<Out>`. Custom runner implementations must move their output type parameter to the method and declare their associated `Error`.
- **C21 · BREAKING_CHANGE:** Low-level `ai.stream.TurnStream.next()` returns `string[] | ai.stream.Done`. Iterate the returned array to process individual deltas. High-level streams continue to return parsed partial values.
- **C61 · BUGFIX:** Optional media arrays cross the host boundary correctly. Host-provided media appears in rendered requests. Output-format layout and role-edge whitespace match the migration parity fixtures.

## [#4606](https://github.com/BoundaryML/baml/pull/4606)

- **C18 · FEATURE:** Files, TCP streams, and process pipes implement `baml.io.Read` and `baml.io.Write`. Readers share `bytes()` and `text()`. Writers share `write()`, which retries partial writes until all bytes are accepted.
- **C68 · BREAKING_CHANGE:** Reads now take a byte limit and return null at EOF. Replace file.read_bytes with file.read and file.write_bytes with file.write. Process stdin is a WritePipe: replace write_stdin and close_stdin with stdin.write and stdin.close. TCP read/write no longer accept per-call timeout parameters; use baml.future.with_timeout.

## [#4599](https://github.com/BoundaryML/baml/pull/4599)

- **C14 · FEATURE:** A stored lambda can infer an omitted parameter type from its uses.

## [#4609](https://github.com/BoundaryML/baml/pull/4609)

- **C67 · BUGFIX:** HTTP operations work again in baml run and the playground because their runtime includes the HTTP implementation.

## [#4612](https://github.com/BoundaryML/baml/pull/4612)

- **C62 · BUGFIX:** Omitting an optional nested class field no longer causes a valid containing optional object to become null. Regression coverage exercises both OpenAI Chat and Responses ingestion.

## [#4619](https://github.com/BoundaryML/baml/pull/4619)

- **C63 · BUGFIX:** Class constructors report E0001 for omitted required fields. Supply every required value or make the field nullable. An `unknown` field still requires an explicit value.

## [#4621](https://github.com/BoundaryML/baml/pull/4621)

- **C64 · BUGFIX:** A stored function value needs a concrete signature. Use `identity<string>` or annotate the binding with a concrete function type. Direct generic calls can still infer arguments independently.

## [#4593](https://github.com/BoundaryML/baml/pull/4593)

- **C65 · BUGFIX:** Functions that do not throw, or only throw a narrower type, report E0097 for `throws unknown`. Remove the clause or give a precise bound. Real unknown error boundaries remain supported.
