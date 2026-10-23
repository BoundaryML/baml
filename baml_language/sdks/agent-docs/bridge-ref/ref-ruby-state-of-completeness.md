---
date: 2026-10-18
repository: baml4
---
# State of BAML↔Ruby completeness

The Ruby/Sorbet SDK generates typed functions, classes and enums for user code and supported standard-library symbols. It runs sync and async calls through the V1 C bridge. The tables below separate tested support from remaining gaps.

✅ means a named Ruby test proves the stated support, 🚧 means only part of the row is tested or supported, and ❌ means unsupported or not yet parity-verified. The Python column is copied from `ref-python-state-of-completeness.md` at commit `b74b83a3ab`, including its qualifications; it is not a claim about today's Python SDK. Links to `customizable/test_*.rb` identify real-engine tests; streaming tests use the keyless replay server. Fake-bridge tests are identified separately.

Validation: `cargo nextest run -p sdk_test_ruby_sorbet` passes all 11 enabled harness tests, including minitest and Sorbet checks for three shared fixtures, the fixture manifest and runtime checks. The `docstrings_etc` and `unsupported_only` fixtures' minitest and Sorbet gates are ignored because they have no test files.

## Function-call forms

| Call form | Python | Ruby | Ruby surface and evidence |
| --- | --- | --- | --- |
| Free function (sync) | ✅ | ✅ | Generated namespace methods call the engine: [`test_main_hello_world_returns_literal`][function-main], [`test_main_single_required_arg_round_trips`][function-main]. |
| Free function (async) | ✅ | ✅ | `_async` siblings return `Baml::Call[T]`; `.wait` returns the result: [`test_methods_on_classes_static_create_async_round_trips`][methods]. |
| Static method | ✅ | ✅ | Generated class methods and `_async` siblings: [`test_methods_on_classes_static_create_round_trips`][methods], [`test_methods_on_classes_static_create_async_round_trips`][methods]. |
| Instance method | ✅ | ✅ | Generated instance methods encode the receiver: [`test_methods_on_classes_instance_greet_with_arg_round_trips`][methods], [`test_methods_on_classes_instance_greet_async_with_arg_round_trips`][methods]. |
| Required args (positional) | ✅ | ✅ | Required parameters are positional: [`test_main_single_required_arg_round_trips`][function-main]. |
| Required args (keyword) | ✅ | n/a | Generated methods require positional arguments. |
| Optional args (omitted → default) | ✅ | ❌ | Callables with defaulted arguments are not generated. |
| Optional args (supplied) | ✅ | ❌ | Callables with defaulted arguments are not generated. Nullable parameters without defaults remain required. |
| Streaming | ✅ | ❌ | No host stream wrapper or generated stream companions. BAML-internal collection remains supported: [`test_streaming_e2e_stream_collect_in_baml`][streaming], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| `$build_request` companion | ✅ | ❌ | No function-spec wrapper or generated spec companions. |
| Generic function / method (inferred) | ✅ | ❌ | Generic declarations are not generated. |
| Generic function / method (`_types=` kwarg) | ✅ | ❌ | No explicit type-binding API; generic declarations are not generated. |
| Generic function / method (subscript) | ✅ | ❌ | No callable-subscript API. |
| Host callback param | ✅ | ❌ | Declarations requiring function types are not generated. |

Per-call timeout, cancel-token and trace options are unsupported. No `_baml:` keyword is generated. Watcher declarations are not generated.

## Runtime behaviors

These rows describe what happens when a call returns, throws, or terminates.

| Runtime behavior | Python | Ruby | Ruby outcome and evidence |
| --- | --- | --- | --- |
| Normal return | ✅ | ✅ | Decodes the `ok` value: [`test_main_hello_world_returns_literal`][function-main], [`test_methods_on_classes_static_create_round_trips`][methods], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| BAML error | ✅ (docs-only) | 🚧 | Raises `Baml::Error < StandardError` with a message and `type_name`. The fake-bridge [`test_call_waiters_cache_outcomes_and_survive_interrupt_and_cancel`][bridge-call] checks a cached error. Decoded payloads and BAML traces are unsupported. |
| BAML panic | ✅ | 🚧 | Raises `Baml::Panic < Exception`, not `StandardError`. Ordinary `rescue => e` does not catch it. Decoded payloads and BAML traces are unsupported. |
| Host-callback error | ✅ | ❌ | No host-callback dispatch or exception round trip. |
| Cancellation | ✅ (async only) | 🚧 | `Call#cancel` requests cancellation; a `wait(timeout:)` timeout raises `Baml::WaitTimeout` without cancelling. The fake-bridge [`test_call_waiters_cache_outcomes_and_survive_interrupt_and_cancel`][bridge-call] checks a cancellation race. Cancel-token options, invocation timeouts and a dedicated cancellation exception are unsupported. |
| OS exit | ✅ | ❌ | Exit panics do not terminate the Ruby process. |

The fake-bridge test [`test_call_waiters_cache_outcomes_and_survive_interrupt_and_cancel`][bridge-call] checks multiple waiters, cached results and errors, an interrupted waiter, foreign-thread completion and a cancellation race.

Generated packages initialize on their first call or through `initialize!`. Loading a different program in the same process and using an inherited runtime after fork raise errors: fixture-backed [`test_different_program`][loader], [`test_pid_check`][loader].

Gem packaging, clean-app installation and native-library cache lookup are unsupported. Supply the native bridge through `BAML_BRIDGE_PATH`. Publishing and automatic bridge download remain out of scope.

## Static typing

The [shared Ruby harness][static-gate] runs `srb tc` on each of the three enabled generated packages and its tests with Sorbet **0.6.13506**, pinned in a separate [test bundle][static-gemfile]. Every generated Ruby file, including bytecode and model declarations, uses `# typed: strict`. No SDK functions or models are recreated in test RBIs.

Strict consumers in the fixture overlays cover sync and async returns, stdlib methods, optionals, classes and enums. One negative consumer requires exactly one wrong-argument diagnostic (`7002`), so a regression to `T.untyped` fails the gate.

The [boundary RBI][static-bridge] describes the runtime-facing API, including call helpers; it does not recreate generated SDK declarations. The bridge's decoded return remains dynamic, so this gate proves generated declarations and consumer checking, not the runtime's adherence to BAML return types. Real-engine and replay tests cover runtime behavior separately.

Typing still has limits: single literal values use their Ruby primitive types, as checked by [`test_literals_return_literals`][literals]. Type aliases, literal unions and general unions are unsupported; declarations requiring them are not generated. Generated docstrings and `Raises` sections are unsupported. JSON has no special `T.untyped` mapping. Declarations requiring function types are not generated. Method signatures retain Sorbet's lazy validation.

The pinned checker runs on x86_64/arm64 macOS and x86_64/aarch64 glibc Linux. Windows, 32-bit Linux and musl hosts explicitly ignore the static test because no compatible binary is supplied; their runtime tests still run. Setup installs the checker under `target/ruby-sorbet-static-bundle` only on supported hosts, and installation failures there fail setup.

## Value kinds

Encode means Ruby → BAML; decode means BAML → Ruby. The generated-package tests make real calls.

| Value kind | Python | Ruby | Ruby value and evidence |
| --- | --- | --- | --- |
| Null | ✅ | ✅ | `nil`: [`test_void_no_op`][void]. The compiler represents both null and void/unit as the same type. |
| Bool | ✅ | 🚧 | `true` / `false`: [`test_stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point`][stdlib] checks decode. Primitive round-trip parity is not verified. |
| Int (i64) | ✅ | 🚧 | `Integer` within i64: [`test_primitives_return_int`][primitives] checks decode. Primitive round-trip parity is not verified. |
| Bigint (outside i64) | ✅ | ❌ | No bigint encoding, decoding or generated type mapping. |
| Float | ✅ | 🚧 | `Float`; primitive round-trip parity is not verified. Generated inputs require `Float`, not `Integer`. |
| String | ✅ | ✅ | `String`: [`test_main_single_required_arg_round_trips`][function-main]. |
| Bytes | ✅ | ❌ | No bytes codec or generated type mapping. |
| List | ✅ | 🚧 | `Array`: [`test_streaming_e2e_stream_collect_in_baml`][streaming] checks decoded string lists. List round-trip parity is not verified. |
| Map | ✅ | 🚧 | String-keyed `Hash` values are supported but not parity-verified. Non-string keys are unsupported. |
| Enum | ✅ | 🚧 | Generated `T::Enum`: [`test_main_ipsum_sentiment_enum_shape`][llm-main] checks its shape. Enum-variant types and dynamic enums are unsupported. Enum round-trip parity is not verified. |
| Class | ✅ | ✅ | Generated `T::Struct`, registered by BAML type name: [`test_methods_on_classes_static_create_round_trips`][methods], [`test_methods_on_classes_instance_greet_with_arg_round_trips`][methods], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| Generic explicitly reified by BAML-known type | ✅ | ❌ | Generic classes and declarations requiring them are not generated. |
| Generic implicitly reified by BAML-known type | ✅ | ❌ | Generic classes and declarations requiring them are not generated. |
| Generic reified by host-only type | ❌ (need more rules for serializing Python-only types) | ❌ | No generic value support. |
| Union | ✅ (union metadata dropped) | ❌ | General unions are unsupported. Optionals remain supported as `T.nilable`, checked by [`test_main_lorem_streaming_doc_class_shape`][llm-main]. |
| BAML interface | ❌ | ❌ | No interface value support; affected declarations are not generated. |
| Media | ✅ | ❌ | No media values, codec or generated type mapping. |
| Stream | ✅ | ❌ | No host stream wrapper; declarations requiring stream values are not generated. |
| Host callable | ✅ | ❌ | No host-value registry or dispatch; declarations requiring function types are not generated. |
| Host callable (async) | ✅ (works, but returned coroutine is driven to completion on new asyncio event loop) | ❌ | No host-callback dispatch. |
| BAML closure (function reference) | ❌ | ❌ | No callable closure wrapper; declarations requiring function types are not generated. |
| BAML closure (closure / bound method / generic function / function) | ❌ | ❌ | No callable closure wrapper; declarations requiring function types are not generated. |
| BAML type reference values | ❌ | ❌ | No first-class BAML type-value encoding or decoding. |
| BAML type definition values | ❌ | ❌ | No type-definition values. |
| BAML `$rust_type` values: files, sockets, etc. | ✅ | 🚧 | Generated resource structs retain `Baml::Handle` fields. File, HTTP and socket values are not parity-verified. |
| Native exception thrown by a host callback | ✅ | ❌ | No opaque host-value registry or exception round trip. |
| Unused SDK heap handles (`Handle`) | 🚧 | 🚧 | The fake-bridge [`test_result_handles_are_owned_before_wait_and_after_partial_decode`][bridge-call] checks result ownership and GC release. Real-engine handle ownership is not parity-verified. |
| Unused SDK collector values (`Collector`) | 🚧 | ❌ | No collector wrapper or verified collector-value path. |
| Unused SDK prompt values (`PromptAst`) | 🚧 | ❌ | No prompt-value support. |
| BAML builtin type (`Future` / `UnscheduledFuture`) | ❌ | ❌ | No first-class engine future values. `Baml::Call` is the host call promise, not a BAML future value. |
| Arbitrary unsupported host object | ❌ | ❌ | Encoding raises `TypeError`. |
| Cyclic / self-referential objects | ❌ | ❌ | No cyclic-object encoding or identity preservation. Recursive type declarations and acyclic recursive values do not provide cycle handling. |

[function-main]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_main.rb
[streaming]: ../../../sdk_tests/crates/ruby_sorbet/llm_functions/customizable/test_streaming_e2e.rb
[void]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_void.rb
[bridge-call]: ../../../sdk_tests/crates/ruby_sorbet/test/bridge_call_test.rb
[static-gemfile]: ../../../sdk_tests/crates/ruby_sorbet/test/static/Gemfile
[static-bridge]: ../../ruby/bridge_ruby/rbi/baml.rbi
[static-gate]: ../../../sdk_tests/harness_runner/src/ruby_sorbet.rs
[methods]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_methods_on_classes.rb
[primitives]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_primitives.rb
[literals]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_literals.rb
[llm-main]: ../../../sdk_tests/crates/ruby_sorbet/llm_functions/customizable/test_main.rb
[stdlib]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_stdlib_entrypoints.rb
[loader]: ../../../sdk_tests/crates/ruby_sorbet/test/bridge_loader_test.rb
