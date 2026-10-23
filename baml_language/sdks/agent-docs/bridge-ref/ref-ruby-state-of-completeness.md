---
date: 2026-10-18
repository: baml4
---
# State of BAML↔Ruby completeness

The Ruby/Sorbet SDK generates typed functions, classes and enums for user code and supported standard-library symbols. It runs sync and async calls through the V1 C bridge, with call controls, generics and media. The tables below separate tested support from remaining gaps.

✅ means a named Ruby test proves the stated support, 🚧 means only part of the row is tested or supported, and ❌ means unsupported or not yet parity-verified. The Python column is copied from `ref-python-state-of-completeness.md` at commit `b74b83a3ab`, including its qualifications; it is not a claim about today's Python SDK. Links to `customizable/test_*.rb` identify real-engine tests; streaming tests use the keyless replay server. Fake-bridge tests are identified separately.

Validation: `cargo nextest run -p sdk_test_ruby_sorbet` passes all 15 harness tests, including minitest and Sorbet checks for all five shared fixtures, the fixture manifest and runtime checks. Individual Ruby tests still skip callable-subscript arity; a passing harness does not make those capabilities supported.

## Function-call forms

| Call form | Python | Ruby | Ruby surface and evidence |
| --- | --- | --- | --- |
| Free function (sync) | ✅ | ✅ | Generated namespace methods call the engine: [`test_main_hello_world_returns_literal`][function-main], [`test_main_single_required_arg_round_trips`][function-main]. |
| Free function (async) | ✅ | ✅ | `_async` siblings return `Baml::Call[T]`; `.wait` returns the result: [`test_cancellation_async_call_returns_none`][cancellation], [`test_four_call_forms_async`][invocation]. |
| Static method | ✅ | ✅ | Generated class methods and `_async` siblings: [`test_methods_on_classes_static_create_round_trips`][methods], [`test_methods_on_classes_static_create_async_round_trips`][methods]. |
| Instance method | ✅ | ✅ | Generated instance methods encode the receiver: [`test_methods_on_classes_instance_greet_with_arg_round_trips`][methods], [`test_methods_on_classes_instance_greet_async_with_arg_round_trips`][methods]. |
| Required args (positional) | ✅ | ✅ | Required parameters are positional: [`test_main_single_required_arg_round_trips`][function-main]. |
| Required args (keyword) | ✅ | n/a | Generated methods require positional arguments. |
| Optional args (omitted → default) | ✅ | ✅ | Keywords default to `Baml::UNSET`; omitted values let BAML evaluate defaults: [`test_optional_args_runtime_matrix`][optional-args]. |
| Optional args (supplied) | ✅ | ✅ | Explicit values and `nil` differ from omission: [`test_optional_args_python_unset_and_none_differ_in_one_call`][optional-args], [`test_optional_args_opt_box_method_matrix`][optional-args]. Nullable parameters without defaults remain required: [`test_optional_args_negative_runtime_cases_reject`][optional-args]. |
| Streaming | ✅ | ❌ | No host stream wrapper or generated stream companions. BAML-internal collection remains supported: [`test_streaming_e2e_stream_collect_in_baml`][streaming], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| `$build_request` companion | ✅ | ❌ | No function-spec wrapper or generated spec companions. |
| Generic function / method (inferred) | ✅ | ✅ | Engine inference from values and receivers: [`test_generic_inference_identity_infers_primitives`][generic-inference], [`test_generic_inference_genericbox_pair_with_unbound_receiver_recovers_class_var`][generic-inference]. |
| Generic function / method (`_types=` kwarg) | ✅ | ✅ | Named tokens, e.g. `_types: { T: Integer }`: [`test_generic_calls_identity_explicit`][generic-calls], [`test_generic_calls_identity_async_explicit`][generic-calls], [`test_generic_calls_genericbox_pair_with_explicit`][generic-calls]. |
| Generic function / method (subscript) | ✅ | ❌ | No callable-subscript API; use `_types:`. [`test_generic_inference_make_triple_partial_subscript_requires_full_arity`][generic-inference] explicitly skips this surface. |
| Host callback param | ✅ | ❌ | Declarations requiring function types are not generated. |

`_baml: Baml::Options.new(timeout_ms:, cancel:)` supplies per-call controls to free functions and methods: [`test_four_call_forms`][invocation], [`test_methods_accept_controls`][invocation]. Trace options are unsupported: only `trace: nil` is accepted, and [`test_invalid_timeout_rejected`][invocation] checks rejection of a non-nil trace. Watcher declarations are not generated.

Sorbet erases `GenericBox[Integer]` at runtime. Use `Baml.bind_types(instance, Integer)` when a host-built instance must carry concrete type arguments; decoded instances retain them automatically. Explicit method bindings require a bound receiver: [`test_generic_calls_genericbox_pair_with_explicit`][generic-calls], [`test_generic_calls_instance_method_unparameterized_receiver_raises`][generic-calls]. `T::Struct#with` makes a new instance and drops the binding; bind the copy again when needed.

## Runtime behaviors

These rows describe what happens when a call returns, throws, or terminates.

| Runtime behavior | Python | Ruby | Ruby outcome and evidence |
| --- | --- | --- | --- |
| Normal return | ✅ | ✅ | Decodes the `ok` value: [`test_main_hello_world_returns_literal`][function-main], [`test_class_refs_round_trip_inner`][class-refs], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| BAML error | ✅ (docs-only) | ✅ | Raises `Baml::Error < StandardError` with decoded `value`, `type_name`, `message` and `baml_trace`: [`test_errors_union_throws_preserves_class_name`][errors], [`test_errors_baml_error_carries_baml_trace`][errors]. BAML frames appear in Ruby's backtrace: [`test_errors_baml_trace_spliced_into_python_traceback`][errors]. |
| BAML panic | ✅ | ✅ | Raises `Baml::Panic < Exception`, not `StandardError`, with a decoded payload: [`test_errors_user_panic_surfaces_as_baml_panic`][errors]. Ordinary `rescue => e` does not catch it. |
| Host-callback error | ✅ | ❌ | No host-callback dispatch or exception round trip. |
| Cancellation | ✅ (async only) | ✅ | Tokens from `BamlSdk::Baml::Spawn::CancelToken.baml_new` cancel sync and async calls; `Call#cancel` cancels an async call. Cancellation raises `Baml::CancelledError < Baml::Panic`: [`test_cancellation_sync_cancel_via_call_context`][cancellation], [`test_cancellation_async_cancel_via_call_context`][cancellation], [`test_cancellation_async_cancel_via_task_cancel`][cancellation]. A `wait(timeout:)` timeout raises `Baml::WaitTimeout` without cancelling; `timeout_ms` cancels the engine call: [`test_cancellation_async_cancel_via_asyncio_timeout`][cancellation]. |
| OS exit | ✅ | ✅ | `baml.sys.exit` terminates the process without Ruby `at_exit` hooks: [`test_errors_clean_exit_terminates_process_with_code_0`][errors], [`test_errors_clean_exit_terminates_process_with_code_7`][errors]. |

The fake-bridge test [`test_call_waiters_cache_outcomes_and_survive_interrupt_and_cancel`][bridge-call] checks multiple waiters, cached results and errors, an interrupted waiter, foreign-thread completion and a cancellation race.

Generated packages initialize on their first call or through `initialize!`. Loading a different program in the same process and using an inherited runtime after fork raise errors: fixture-backed [`test_different_program`][loader], [`test_pid_check`][loader].

Gem packaging, clean-app installation and native-library cache lookup are unsupported. Supply the native bridge through `BAML_BRIDGE_PATH`. Publishing and automatic bridge download remain out of scope.

## Static typing

The [shared Ruby harness][static-gate] runs `srb tc` on each of the five generated packages and its tests with Sorbet **0.6.13506**, pinned in a separate [test bundle][static-gemfile]. Every generated Ruby file, including bytecode and model declarations, uses `# typed: strict`. No SDK functions or models are recreated in test RBIs.

Strict consumers in the fixture overlays cover sync and async returns, stdlib methods, optional arguments, call controls, generics, bigint, bytes, media, unions, aliases, classes and enums. One negative consumer requires exactly one wrong-argument diagnostic (`7002`), so a regression to `T.untyped` fails the gate.

The [boundary RBI][static-bridge] describes the runtime-facing API, including call and generic helpers; it does not recreate generated SDK declarations. The bridge's decoded return remains dynamic, so this gate proves generated declarations and consumer checking, not the runtime's adherence to BAML return types. Real-engine and replay tests cover runtime behavior separately, including recursion, forward references and symbol collisions.

Typing still has limits: literal values use their Ruby primitive types, recursive alias back-edges and JSON use `T.untyped`, and `_types:` token values are dynamic. Declarations requiring function types are not generated. Method signatures retain Sorbet's lazy validation.

The pinned checker runs on x86_64/arm64 macOS and x86_64/aarch64 glibc Linux. Windows, 32-bit Linux and musl hosts explicitly ignore the static test because no compatible binary is supplied; their runtime tests still run. Setup installs the checker under `target/ruby-sorbet-static-bundle` only on supported hosts, and installation failures there fail setup.

## Value kinds

Encode means Ruby → BAML; decode means BAML → Ruby. The generated-package tests make real calls.

| Value kind | Python | Ruby | Ruby value and evidence |
| --- | --- | --- | --- |
| Null | ✅ | ✅ | `nil`: [`test_primitives_round_trip_null`][primitives]. Void returns also become `nil`: [`test_void_no_op`][void]. |
| Bool | ✅ | ✅ | `true` / `false`: [`test_primitives_round_trip_bool`][primitives]. |
| Int (i64) | ✅ | ✅ | `Integer` within i64: [`test_primitives_round_trip_int`][primitives]. |
| Bigint (outside i64) | ✅ | ✅ | Arbitrary-precision `Integer`: [`test_primitives_round_trip_bigint`][primitives] covers positive and negative 2⁸⁰ and the 2⁶⁴ boundary; [`test_primitives_return_bigint`][primitives] checks decode. |
| Float | ✅ | ✅ | `Float`: [`test_primitives_round_trip_float`][primitives]. Integer inputs widen to float: [`test_primitives_round_trip_float_accepts_int`][primitives]. |
| String | ✅ | ✅ | `String`: [`test_primitives_round_trip_string`][primitives]. |
| Bytes | ✅ | ✅ | Binary `String`, encoded using the declared bytes hint. When a union allows both string and bytes, binary encoding selects bytes: [`test_primitives_round_trip_uint8_array`][primitives] covers a real round trip and protocol selection through nested containers and aliases. |
| List | ✅ | ✅ | `Array`, including empty and mixed-element lists: [`test_lists_round_trip_ints`][lists], [`test_lists_round_trip_empty_list`][lists], [`test_lists_round_trip_union_list`][lists]. |
| Map | ✅ | ✅ | `Hash`: [`test_maps_round_trip_simple_map`][maps], [`test_maps_round_trip_list_valued_map`][maps]. |
| Enum | ✅ | ✅ | Generated `T::Enum`: [`test_enums_round_trip_sentiment`][enums], [`test_enums_round_trip_sentiment_positive`][enums]. Dynamic enums are unsupported. |
| Class | ✅ | ✅ | Generated `T::Struct`, registered by BAML type name: [`test_class_refs_round_trip_inner`][class-refs], [`test_streaming_e2e_stream_doc_collect_in_baml`][streaming]. |
| Generic explicitly reified by BAML-known type | ✅ | ✅ | `Baml.bind_types` attaches runtime type arguments to generated generic structs: [`test_generic_calls_identity_explicit`][generic-calls]. Decoded type arguments are retained, including nested ones: [`test_generic_calls_make_int_box_reified`][generic-calls], [`test_generic_calls_make_nested_box_reified`][generic-calls]. |
| Generic implicitly reified by BAML-known type | ✅ | ✅ | The engine infers from unbound instance fields: [`test_generic_inference_identity_unbound_generic_instance_round_trips`][generic-inference], [`test_generic_inference_extract_fully_unbound_nested_pair_recovers_all_vars`][generic-inference]. |
| Generic reified by host-only type | ❌ (need more rules for serializing Python-only types) | ❌ | Unregistered Ruby objects cannot be encoded: [`test_generic_inference_host_only_object_not_encodable_from_python`][generic-inference]. |
| Union | ✅ (union metadata dropped) | ✅ | Decodes to the Ruby variant value; metadata is dropped. Generated signatures use `T.any` / `T.nilable`: [`test_unions_round_trip_null_to_end`][unions], [`test_unions_round_trip_optional_plus_null`][unions], [`test_unions_round_trip_union_container`][unions]. |
| BAML interface | ❌ | ❌ | No interface value support; affected declarations are not generated. |
| Media | ✅ | ✅ | Frozen value objects `Baml::Image`, `Audio`, `Video` and `Pdf`, not engine handles. URL round trips: [`test_media_round_trip_media`][media]. Base64, file contents, names, MIME types and local-file loading: [`test_handles_image_from_base64_roundtrips_payload`][handles]. |
| Stream | ✅ | ❌ | No host stream wrapper; declarations requiring stream values are not generated. |
| Host callable | ✅ | ❌ | No host-value registry or dispatch; declarations requiring function types are not generated. |
| Host callable (async) | ✅ (works, but returned coroutine is driven to completion on new asyncio event loop) | ❌ | No host-callback dispatch. |
| BAML closure (function reference) | ❌ | ❌ | No callable closure wrapper; declarations requiring function types are not generated. |
| BAML closure (closure / bound method / generic function / function) | ❌ | ❌ | No callable closure wrapper; declarations requiring function types are not generated. |
| BAML type reference values | ❌ | ❌ | No first-class BAML type-value encoding or decoding. `Baml::Types` tokens are call metadata, not ordinary argument values. |
| BAML type definition values | ❌ | ❌ | No type-definition values. |
| BAML `$rust_type` values: files, sockets, etc. | ✅ | 🚧 | Generated resource structs retain `Baml::Handle` fields. Files preserve cursor state: [`test_handles_open_file_returns_file_handle`][handles], [`test_handles_file_cursor_state_persists_across_calls`][handles]. HTTP response fields and methods: [`test_handles_http_get_response_fields_and_methods`][handles]. Socket values are not parity-verified. |
| Native exception thrown by a host callback | ✅ | ❌ | No opaque host-value registry or exception round trip. |
| Unused SDK heap handles (`Handle`) | 🚧 | 🚧 | Handles work inside tested resources: [`test_handles_file_cursor_state_persists_across_calls`][handles]. Standalone unused heap-handle ownership and GC release are not parity-verified. |
| Unused SDK collector values (`Collector`) | 🚧 | ❌ | No collector wrapper or verified collector-value path. |
| Unused SDK prompt values (`PromptAst`) | 🚧 | ❌ | No prompt-value support. |
| BAML builtin type (`Future` / `UnscheduledFuture`) | ❌ | ❌ | No first-class engine future values. `Baml::Call` is the host call promise, not a BAML future value. |
| Arbitrary unsupported host object | ❌ | ❌ | Encoding raises `TypeError`: [`test_generic_inference_host_only_object_not_encodable_from_python`][generic-inference]. |
| Cyclic / self-referential objects | ❌ | ❌ | No cyclic-object encoding or identity preservation. Recursive type declarations and acyclic recursive values do not provide cycle handling. |

[function-main]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_main.rb
[errors]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_errors.rb
[streaming]: ../../../sdk_tests/crates/ruby_sorbet/llm_functions/customizable/test_streaming_e2e.rb
[class-refs]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_class_refs.rb
[void]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_void.rb
[bridge-call]: ../../../sdk_tests/crates/ruby_sorbet/test/bridge_call_test.rb
[static-gemfile]: ../../../sdk_tests/crates/ruby_sorbet/test/static/Gemfile
[static-bridge]: ../../ruby/bridge_ruby/rbi/baml.rbi
[static-gate]: ../../../sdk_tests/harness_runner/src/ruby_sorbet.rs
[methods]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_methods_on_classes.rb
[optional-args]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_optional_args.rb
[cancellation]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_cancellation.rb
[invocation]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_invocation.rb
[generic-calls]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_generic_calls.rb
[generic-inference]: ../../../sdk_tests/crates/ruby_sorbet/function_calls/customizable/test_generic_inference.rb
[primitives]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_primitives.rb
[lists]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_lists.rb
[maps]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_maps.rb
[enums]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_enums.rb
[unions]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_unions.rb
[media]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_media.rb
[handles]: ../../../sdk_tests/crates/ruby_sorbet/type_shapes/customizable/test_handles.rb
[loader]: ../../../sdk_tests/crates/ruby_sorbet/test/bridge_loader_test.rb
