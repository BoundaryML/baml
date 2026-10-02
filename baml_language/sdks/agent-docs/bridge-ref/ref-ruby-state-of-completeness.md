---
date: 2026-09-28
repository: baml4
---
# State of BAML↔Ruby completeness

This change adds generated Ruby/Sorbet packages, synchronous calls through the V1 C bridge, typed values, and keyless LLM calls against the replay server. Later work adds ordinary-call breadth, async calls and cancellation, generics, streams/media/client selection, and gem packaging.

✅ means tested support, 🚧 means partial support, and ❌ means unsupported or not yet parity-verified. The Python column is copied from `ref-python-state-of-completeness.md` at commit `b74b83a3ab`, including its qualifications; it is not a claim about today's Python SDK. Ruby test links distinguish real-engine tests from protocol-only tests.

## Function-call forms

| Call form | Python | Ruby | Ruby surface and evidence |
| --- | --- | --- | --- |
| Free function (sync) | ✅ | ✅ | Generated namespace methods call the engine: [`test_main_hello_world_returns_literal`][function-main], [`test_main_single_required_arg_round_trips`][function-main]. |
| Free function (async) | ✅ | ❌ | No async siblings. |
| Static method | ✅ | ❌ | Class methods are not generated. |
| Instance method | ✅ | ❌ | Instance methods are not generated, including `DocLoader.load`. |
| Required args (positional) | ✅ | ✅ | Required parameters are positional: [`test_main_single_required_arg_round_trips`][function-main]; multi-argument calls in [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming] via the replay server's two-argument `replay_serve_until_shutdown`. |
| Required args (keyword) | ✅ | n/a | Required arguments are generated as positional parameters. |
| Optional args (omitted → default) | ✅ | ❌ | Generated defaults raise `UnsupportedTypeError`; omission is not implemented. |
| Optional args (supplied) | ✅ | ❌ | Nilable arguments are generated as `nil`-default keywords, but no call test exercises them yet; defaulted arguments raise. |
| Streaming | ✅ | ❌ | No Ruby stream wrapper. The collect tests drain streams inside BAML and return ordinary values. |
| `$build_request` companion | ✅ | ❌ | No generated companion. |
| Generic function / method (inferred) | ✅ | ❌ | Generic callables are not generated. |
| Generic function / method (`_types=` kwarg) | ✅ | ❌ | No explicit type-argument API. |
| Generic function / method (subscript) | ✅ | ❌ | No subscript call API. |
| Host callback param | ✅ | ❌ | No host-callable registration or dispatch. |

Injected `on_event` callbacks are ignored (the engine applies its default); host-driven streaming remains unsupported.

## Runtime behaviors

These rows describe what happens when a call returns, throws, or terminates.

| Runtime behavior | Python | Ruby | Ruby outcome and evidence |
| --- | --- | --- | --- |
| Normal return | ✅ | ✅ | Decodes the `ok` value: [`test_main_hello_world_returns_literal`][function-main], [`test_person_round_trip`][function-main], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| BAML error | ✅ (docs-only) | 🚧 | Raises `Baml::Error` with `type_name` and `message` only; payload and BAML trace are not decoded. `test_errors_union_throws_preserves_class_name` compares single and union throws, not decoded error instances. |
| BAML panic | ✅ | 🚧 | Raises the separate `Baml::PanicError` with `type_name` and `message` only; payload and BAML trace are not decoded. `test_errors_user_panic_surfaces_as_baml_panic` checks the type name rather than a decoded `UserPanic`. |
| Host-callback error | ✅ | ❌ | No host callbacks or original-exception recovery. |
| Cancellation | ✅ (async only) | ❌ | No cancellation API. |
| OS exit | ✅ | ❌ | Exit panics raise `UnsupportedTypeError`; they do not terminate the Ruby process. |

The partial error assertions live in [`test_errors.rb`][errors]. Python's replay namespace test also checks async siblings; Ruby covers only sync bindings under the Ruby-only `test_replay_server_sync_namespace_bindings` name.

## Static typing

The nextest `sdk_test_ruby_sorbet::bridge_tests::static_consumer` gate checks the actual app-local generated SDK source with Sorbet **0.6.13506**, pinned in a separate [test bundle][static-gemfile]. Every generated Ruby file, including bytecode and model declarations, uses `# typed: strict` and is included in the check. No SDK functions or models are recreated in test RBIs.

The positive consumers check string arguments and returns, `Person` construction and field reads, class round trips, arrays, nullable fields and enums. A fresh generator fixture also checks self-references, cross-namespace references to later classes/enums, and maps. Four separate negative consumers require exact source locations and codes: wrong function argument and struct-field input (`7002`), wrong return assignment and field-result assignment (`7007`). Clean baselines run first; missing or extra diagnostics fail the gate, including when those signatures become `T.untyped`.

The [boundary RBI][static-bridge] describes only the dynamic bridge entry points, error readers and Sorbet's runtime signature helper. The bridge's decoded return remains dynamic; this gate proves generated declarations and consumer checking, not the bridge implementation or the runtime's adherence to BAML return types. The existing real-engine and replay tests cover runtime behavior separately. Recursive class loading and exact type/field registration have a separate runtime regression test; this does not add cyclic-object encoding support.

The pinned checker runs on x86_64/arm64 macOS and x86_64/aarch64 glibc Linux. Windows, 32-bit Linux and musl hosts explicitly ignore the static test because no compatible binary is supplied; their runtime tests still run. Setup installs the checker under `target/ruby-sorbet-static-bundle` only on supported hosts, and installation failures there fail setup. The runtime Gemfile and its platform lock entries are unchanged.

## Value kinds

Encode means Ruby → BAML; decode means BAML → Ruby. Protocol tests exercise protobuf encoding and decoding without the engine. The generated-package tests make real calls.

| Value kind | Python | Ruby | Ruby value and evidence |
| --- | --- | --- | --- |
| Null | ✅ | ✅ | `nil`: [`test_nil_encoding_and_decoding`][bridge-call] covers both directions; [`test_primitive_round_trips`][function-main] checks a real void return. |
| Bool | ✅ | ✅ | `true` / `false`: [`test_primitive_round_trips`][function-main]. |
| Int (i64) | ✅ | ✅ | `Integer` within i64: [`test_primitive_round_trips`][function-main]. |
| Bigint (outside i64) | ✅ | ❌ | No bigint wire support. |
| Float | ✅ | ✅ | `Float`: [`test_primitive_round_trips`][function-main]. |
| String | ✅ | ✅ | `String`, including Unicode and NUL: [`test_primitive_round_trips`][function-main]. |
| Bytes | ✅ | ❌ | No `uint8array_value` support. |
| List | ✅ | ✅ | `Array`: protocol tests [`test_nested_class_list_map_and_enum_encode`][protocol], [`test_nested_class_list_map_optional_and_enum_decode`][protocol], and [`test_empty_containers`][protocol]; real-engine decode in [`test_streaming_e2e_stream_collect_in_baml`][streaming]. |
| Map | ✅ | ✅ | `Hash` with string, int, bool, or enum keys: protocol tests [`test_map_key_types_in_both_directions`][protocol] and [`test_empty_containers`][protocol]. |
| Enum | ✅ | ✅ | Generated `T::Enum`: [`test_main_ipsum_sentiment_enum_shape`][llm-main] checks the shape; [`test_sentiment_wire_values_use_the_generated_enum`][llm-main] checks protocol encoding and decoding. |
| Class | ✅ | ✅ | Generated `T::Struct`, registered by BAML type name: [`test_person_round_trip`][function-main], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| Generic explicitly reified by BAML-known type | ✅ | ❌ | No generic class generation or type-argument encoding. |
| Generic implicitly reified by BAML-known type | ✅ | ❌ | No generic class generation or inference API. |
| Generic reified by host-only type | ❌ (need more rules for serializing Python-only types) | ❌ | No host-only generic types. |
| Union | ✅ (union metadata dropped) | 🚧 | Decode unwraps the variant. No generated union types; nilable fields are supported. |
| BAML interface | ❌ | ❌ | No interface value support. |
| Media | ✅ | ❌ | No media wrappers or handles. |
| Stream | ✅ | ❌ | No stream handles cross the Ruby boundary. Engine-side collection does not provide host streaming. |
| Host callable | ✅ | ❌ | No host-callable encoding or dispatch. |
| Host callable (async) | ✅ (works, but returned coroutine is driven to completion on new asyncio event loop) | ❌ | No async host callbacks. |
| BAML closure (function reference) | ❌ | ❌ | No function-reference handles. |
| BAML closure (closure / bound method / generic function / function) | ❌ | ❌ | No callable value support. |
| BAML type reference values | ❌ | ❌ | No type-value encoding or decoding. |
| BAML type definition values | ❌ | ❌ | No type-definition values. |
| BAML `$rust_type` values: files, sockets, etc. | ✅ | ❌ | No resource handles. |
| Native exception thrown by a host callback | ✅ | ❌ | No opaque host-value handles or exception recovery. |
| Unused SDK heap handles (`Handle`) | 🚧 | ❌ | No heap-handle support. |
| Unused SDK collector values (`Collector`) | 🚧 | ❌ | No collector handles. |
| Unused SDK prompt values (`PromptAst`) | 🚧 | ❌ | No prompt handles. |
| BAML builtin type (`Future` / `UnscheduledFuture`) | ❌ | ❌ | No future values. |
| Arbitrary unsupported host object | ❌ | ❌ | Encoding raises `UnsupportedTypeError`. |
| Cyclic / self-referential objects | ❌ | ❌ | No cycle handling. |

[function-main]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_main.rb
[errors]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_errors.rb
[llm-main]: ../../../sdk_tests/crates/ruby_sorbet/llm_functions/customizable/test_main.rb
[streaming]: ../../../sdk_tests/crates/ruby_sorbet/llm_functions/customizable/test_streaming_e2e.rb
[protocol]: ../../../sdk_tests/crates/ruby_sorbet/test/protocol_test.rb
[bridge-call]: ../../../sdk_tests/crates/ruby_sorbet/test/bridge_call_test.rb
[static-gemfile]: ../../../sdk_tests/crates/ruby_sorbet/test/static/Gemfile
[static-bridge]: ../../../sdk_tests/crates/ruby_sorbet/test/static/bridge.rbi
