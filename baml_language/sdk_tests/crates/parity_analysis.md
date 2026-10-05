# SDK test coverage parity

This report inventories checked-in test declarations. It does not report whether tests passed.

Distinct exact test IDs: 1036. IDs with complete required parity: 316. Required gaps: 4949.


## Python-baselined parity

Parity is the share of the 368 shared test IDs declared in `python_pydantic2` that are also declared in each SDK environment. A Python test ID is shared unless a skip annotation waives every other SDK environment. SDK-only test IDs do not affect these percentages.

| SDK environment | Matching Python test IDs | Parity |
| --- | ---: | ---: |
| python_pydantic2 | 368 / 368 | 100.0% |
| typescript_node | 184 / 368 | 50.0% |
| typescript_web_chromium | 160 / 368 | 43.5% |
| typescript_web_cloudflare_workers | 160 / 368 | 43.5% |
| cpp | 131 / 368 | 35.6% |
| csharp | 0 / 368 | 0.0% |
| rust | 229 / 368 | 62.2% |
| go | 14 / 368 | 3.8% |
| java | 300 / 368 | 81.5% |
| swift | 185 / 368 | 50.3% |

| Test case | python_pydantic2 | typescript_node | typescript_web_chromium | typescript_web_cloudflare_workers | cpp | csharp | rust | go | java | swift | Required in | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| docstrings_etc/class_doc_summary_and_attributes | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/enum_doc_summary_and_members | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/enum_summary_only_omits_member_comments | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/go_codegen_function_doc_comment | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/imports | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/main_class_doc_summary_and_attributes_section | y | y | - | - | y | - | y | - | y | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| docstrings_etc/main_class_doc_summary_present | - | - | - | - | - | - | - | - | - | y | all |  |
| docstrings_etc/main_enum_and_variant_docs_attached | - | - | - | - | - | - | - | - | - | y | all |  |
| docstrings_etc/main_enum_doc_summary_and_members_section | y | y | - | - | y | - | y | - | y | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| docstrings_etc/main_enum_summary_only_omits_members_section | y | - | - | - | y | - | y | - | y | - | all |  |
| docstrings_etc/main_field_docs_attached | - | - | - | - | - | - | - | - | - | y | all |  |
| docstrings_etc/main_imports_symbols_reachable | y | y | y | y | - | - | y | - | y | y | all |  |
| docstrings_etc/main_multi_line_class_doc_preserved | - | - | - | - | - | - | - | - | - | y | all |  |
| docstrings_etc/main_no_inline_field_or_variant_doc_artifacts | y | y | - | - | y | - | y | - | y | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| docstrings_etc/main_undocumented_field_listed_as_bare_name_under_attributes | y | - | - | - | y | - | y | - | y | - | all |  |
| docstrings_etc/no_inline_field_or_variant_doc_artifacts | - | - | - | - | - | - | - | y | - | - | all |  |
| docstrings_etc/undocumented_field_has_no_doc_artifact | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/adt_media_generic_decodes_to_pyhandle | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/ambient_context_survives_await_rust_only | - | - | - | - | - | - | y | - | - | - | rust | Rust task scopes and lazy future cancellation are specific to Rust |
| function_calls/argument_of_another_kind_without_generated_class_is_a_type_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/argument_types_class_rejects_a_field_of_another_kind | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call |
| function_calls/argument_types_int_rejects_a_value_of_another_kind | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call |
| function_calls/argument_types_int_rejects_a_value_of_another_kind_async | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call |
| function_calls/argument_types_list_rejects_an_item_of_another_kind | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call |
| function_calls/argument_types_scalars_reject_a_value_of_another_kind | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a value of another Python type than the generated annotation; a statically typed SDK cannot write that call |
| function_calls/argument_types_values_that_the_boundary_converts_are_accepted | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a Python dict and a Python int where the generated annotations are a class and a float |
| function_calls/async_callback_can_use_originating_loop_resources_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/async_callback_can_use_originating_loop_resources_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/async_callback_inherits_application_context_across_suspension_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/async_callback_preserves_ambient_frame_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node async carrier, native signals, and entry-time snapshots are specific to TypeScript |
| function_calls/async_callback_reenters_async_baml_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python loop/ContextVar and Node AsyncLocalStorage assertions |
| function_calls/async_callback_reenters_sync_baml_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | sync callback re-entry differs between Python and Node |
| function_calls/async_entry_sync_callback_inherits_application_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/async_host_runs_in_calling_task_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python asyncio task and event-loop identity |
| function_calls/async_reentry_preserves_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/async_stream_schema_failure_is_parse_failed | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | exercises the Python asyncio streaming bridge |
| function_calls/audio_from_base64 | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/audio_from_file | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/audio_from_url | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/baml_closure_decodes_multiple_args_and_structured_return_values | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# covers this canonical behavior in its native integration harness |
| function_calls/baml_closure_is_a_native_callable_with_host_language_arguments | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# covers this canonical behavior in its native integration harness |
| function_calls/baml_closure_is_reusable_and_retains_mutable_captures | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# covers this canonical behavior in its native integration harness |
| function_calls/baml_error_carries_baml_trace | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_class_and_field_wire_names_are_exact | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_constructors_and_parsers_return_lossless_internal_values | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_go_constructed_values_round_trip_at_numeric_boundaries | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_malformed_values_fail_at_the_earliest_typed_boundary | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_nested_containers_and_baml_field_inspection | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_nullable_and_defaulted_positions | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_time_raw_class_transport_does_not_enforce_semantic_invariants | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_trace_is_embedded_in_go_error_string | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/baml_ty_int_returns_int | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_list_int_returns_typing_list_int | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_optional_string_returns_optional_str | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_string_returns_str | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_union_preserves_members | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_union_single_member_unwraps | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_union_with_unknown_member_keeps_any_arm | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/baml_ty_unknown_returns_typing_any | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/base_class_for_fqn_passes_non_generic_through | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/base_class_for_fqn_strips_parameterization | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/boolean_timeout_rejected_python_only | y | - | - | - | - | - | - | - | - | - | all |  |
| function_calls/call_by_name_returns_a_generated_class | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/call_by_name_returns_the_result | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/callable_entry_invokes_callback | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/callable_entry_waits_for_callback_completion | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/callable_parameter_aliases_map_positional_and_keyword_calls_to_wire_names | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/callable_returning_hostile_object_still_completes | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/callable_returning_unencodable_surfaces_as_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/callback_bound_method_adoption_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python bound method identity via its exact SDK function |
| function_calls/callback_captures_internal_baml_context | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/callback_exception_without_generated_class_is_the_original | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/callback_frame_and_reentry | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/callback_frame_is_installed_and_restored | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/callback_marker_adopts_once_during_recursion | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_adopts_sync_body | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_concurrent_reuse_and_later_direct_call | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_context_precedence | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_defaults_override_inherited_context | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_only_outer_wrapper_adopts | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_marker_sync_entry_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python synchronous BAML entry and inline callback dispatch |
| function_calls/callback_reenters_baml | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/callback_reuse_across_event_loop_turns_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/callback_reuse_after_previous_application_loop_is_closed_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/callback_suppresses_task_cancellation_and_returns_late_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python callback suppression of cancellation and late results |
| function_calls/callback_task_cancelled_before_first_execution_completes_call_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python task factory cancellation before callback execution |
| function_calls/callback_third_party_wrapper_is_not_adopted | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/callback_throws_caught_and_replaced_makes_the_function_infallible | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/callback_throws_caught_then_rethrown_value_is_the_replacement_union | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/callback_throws_rethrown_carries_the_effect_param_into_the_error_union | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/cancel_controls_with_cancelled_context_go_only | - | - | - | - | - | - | - | y | - | - | go | Go cancellation controls remain usable with a canceled context |
| function_calls/cancellation_async_call_returns_none | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/cancellation_async_cancel_skips_later_step | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | drives Python asyncio cancellation (task.cancel / wait_for) |
| function_calls/cancellation_async_cancel_via_asyncio_timeout | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/cancellation_async_cancel_via_call_context | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/cancellation_async_cancel_via_future_cancel | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/cancellation_async_cancel_via_task_cancel | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/cancellation_async_cancel_via_task_group_sibling | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/cancellation_async_cancel_via_timeout_race | - | - | - | - | - | - | - | - | - | y | all |  |
| function_calls/cancellation_can_pre_abort_async_baml_cancellation | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_cancels_an_async_generated_free_function | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_cancels_an_async_generated_instance_method | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_detaches_a_completed_call_before_aborting_the_remaining_call | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_future_destruction_detaches | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/cancellation_future_second_get_throws_future_error | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/cancellation_future_wait_for_times_out_while_in_flight | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/cancellation_future_wait_then_get | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/cancellation_immediately_cancels_every_call_attached_after_abort | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_pre_aborts_a_generated_synchronous_call | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_a_reused_call_context_as_abort_error_with_baml_reason | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_async_cancellation_as_abort_error_with_baml_reason | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_cancellation_through_promise_all | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_sync_pre_aborted_cancellation_as_abort_error | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_sync_call_returns_none | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/cancellation_sync_cancel_via_call_context | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/cancelled_waiter_does_not_end_host_execution | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancelled_waiter_keeps_host_context_through_cleanup_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python task unwinding and ContextVar semantics |
| function_calls/cancelled_waiter_keeps_host_resource_until_promise_exit_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node AsyncResource destruction and AsyncLocalStorage |
| function_calls/cancelled_waiter_preserves_callback_context | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancelled_waiter_stays_cancelled_when_callback_returns_late | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancelling_call_delivers_cancellation_and_allows_callback_cleanup_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python callback task and event loop ownership |
| function_calls/canonical_json_class_union_uses_declared_field_codecs | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_composes_through_containers_and_classes | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_defaults_and_callbacks | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_dynamic_union | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_rejects_extensions_before_dispatch | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_round_trips_at_top_level_and_through_alias | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/child_cancellation_does_not_cancel_input | y | y | y | y | - | - | y | - | - | - | all |  |
| function_calls/clean_exit_helper_process | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/clean_exit_terminates_process_with_code | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/closure_call_async_can_be_reused_after_creation_loop_closes_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python returned closure reuse across application loops |
| function_calls/closure_call_async_cancels_its_retained_callback_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python closure async entry cancellation delivery |
| function_calls/closure_call_async_preserves_binding_and_exception_identity_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python closure async entry argument and error semantics |
| function_calls/closure_call_async_uses_invoking_loop_and_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python explicit async closure entry and context capture |
| function_calls/coawait_co_await_cancelled_call_throws_cancelled | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_completed_future_fast_path | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_pending_future_resumes | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_throws_typed_into_the_coroutine | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_yields_the_decoded_value | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_uncaught_coroutine_exception_reaches_join | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/completed_call_can_reuse_callback | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/completed_call_does_not_pin_reused_callback_to_old_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/composite_token_observes_every_source | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/concurrent_calls_isolate_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/concurrent_calls_keep_callback_results_independent | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/concurrent_calls_share_callback_without_sharing_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/concurrent_host_invocations_isolate_context | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/concurrent_invocations_isolate_context | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/concurrent_reservation_attaches_once | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/configuration_snapshot_is_not_live | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cooperative_abort_allows_callback_cleanup_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/cooperative_abort_cleans_up_retained_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/coroutine_entry_uses_execution_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python coroutine creation versus first execution |
| function_calls/current_context_returns_detached_snapshot | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/deadline_reentry_does_not_reset_budget | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/decode_class_graceful_degradation_when_args_empty | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/decode_class_nested_generic | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/decode_class_parameterizes_with_generic_args | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/decode_failure_counts_repeated_keys_and_ignores_borrowed_host_keys | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/decode_failure_preserves_transferred_owner_and_releases_remaining | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/decode_failure_releases_untransferred_callable_and_media | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/decode_value_class_keeps_projected_handle_alias_as_model_field | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/decode_value_class_unregistered_fqn_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/decode_value_class_uses_typemap_get_class | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/decode_value_unknown_class_as_fields_keeps_a_generated_class | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/decode_value_unknown_class_as_fields_reaches_every_nested_class | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/decoded_pyhandle_releases_on_drop | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/define_function_preserves_generated_callable_metadata_and_generic_wrapping | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/discarded_args_do_not_release_borrowed_host_registry_keys | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/discarded_args_release_each_nested_wire_owner_once | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/dropping_started_future_cancels_rust_only | - | - | - | - | - | - | y | - | - | - | rust | Rust task scopes and lazy future cancellation are specific to Rust |
| function_calls/dynamic_application_map_preserves_control_like_keys | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/dynamic_call_accepts_controls | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/dynamic_call_async_accepts_controls | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/dynamic_name_and_handle_accept_controls | - | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/dynamic_name_and_handle_accept_controls_async | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/dynamic_type_bindings_async_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python dynamic type bindings accept Python classes and reflected BamlType handles |
| function_calls/empty_class_self_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/empty_controls | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/encode_error_releases_registered_callables | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/encode_failure_releases_every_cloned_capability_handle | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/encode_success_does_not_release | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/encoding_time_counts_against_deadline_python_only | y | - | - | - | - | - | - | - | - | - | all |  |
| function_calls/enum_strings_callback_receives_the_variant | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/enum_strings_plain_string_becomes_the_variant | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/enum_strings_plain_string_becomes_the_variant_async | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/enum_strings_string_stays_a_string_beside_a_string_member | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/enum_strings_string_that_names_no_variant_is_a_type_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/enum_strings_union_mismatch_names_types_as_written | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | passes a plain Python str where the generated annotation is an enum; a statically typed SDK cannot write that call |
| function_calls/error_call_cancellation_preserves_context_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/error_string_is_non_empty | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/errors_async_sibling_throws_typed | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_baml_error_carries_baml_trace | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_baml_trace_spliced_into_python_traceback | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_cancellation_surfaces_as_baml_panic | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_cancelled_async_call_stays_an_asyncio_cancellation | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_cancelled_sync_call_is_caught_as_baml_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_clean_exit_terminates_process_with_code | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/errors_clean_exit_terminates_process_with_code_0 | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/errors_clean_exit_terminates_process_with_code_7 | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/errors_every_baml_exception_class_is_a_baml_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_host_invalid_argument_wraps_baml_errors_invalid_argument | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_panic_is_caught_as_baml_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_rejected_argument_is_caught_as_baml_error_and_as_type_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_sdk_panic_is_caught_as_baml_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python exception class hierarchy |
| function_calls/errors_stdlib_error_surfaces_as_baml_error | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_stdlib_error_surfaces_typed | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_str_is_non_empty | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_typed_throw_is_still_a_baml_error | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_union_throws_preserves_class_name | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/errors_user_panic_surfaces_as_baml_panic | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_user_throw_surfaces_declared_instance | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/exit_wait_ends_on_ctrl_c | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/explicit_controls_are_applied | - | - | - | - | - | - | - | - | - | y | all |  |
| function_calls/explicit_controls_are_applied_async | - | - | - | - | - | - | - | - | - | y | all |  |
| function_calls/failed_admission_does_not_consume_reservation | - | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/four_call_forms | y | y | y | y | - | - | y | - | y | y | all |  |
| function_calls/four_call_forms_async | y | y | y | y | - | - | y | - | y | y | all |  |
| function_calls/from_lazy_entries_resolves_class_via_importlib | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/function_not_found_is_a_panic | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/function_not_found_without_generated_class_is_a_panic | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/function_ref_decodes_to_callable | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/function_spec_uses_canonical_method_fqns_and_wire_argument_names | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/generated_bytecode_version_skew_fails_before_deserialization | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/generic_callable_explicit_types_still_works | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/generic_callable_subscript_arity_mismatch_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/generic_callable_subscript_desugars_to_types_dict | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/generic_calls_choose_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_consume_int_wrapper_baseline | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/generic_calls_extract_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_generic_free_fn_requires_binding | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_generic_static_infers_binding | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_genericbox_get_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_genericbox_new_static_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_genericbox_pair_with_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_identity_async_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_identity_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_instance_method_unparameterized_receiver_raises | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_list_head_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_make_int_box_reified | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/generic_calls_make_int_container_reified | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/generic_calls_make_int_str_bool_triple_reified | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/generic_calls_make_nested_box_reified | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/generic_calls_make_triple_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_named_static_distinct_typevar_names | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_one_type_arg_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_parse_as_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_read_items_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_second_of_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_subscript_wrong_arity_raises | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_tag_or_value_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_two_type_args_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_calls_wrap_explicit | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/generic_classes_containers_and_concrete_outputs | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_identity_inference_and_explicit_types | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_inference_apply_closure_poisons_typevars_must_specify | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_apply_closure_typevars_specified_succeeds | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_body_only_var_still_requires_binding | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_choose_divergent_generic_instances_union | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_choose_infers_divergent_union | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_choose_infers_unified_typevar | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_choose_union_outside_container_is_sound | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_combine_invariant_class_arg_conflict_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_default_only_value_position | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python default-argument stubs and runtime TypeVar inference |
| function_calls/generic_inference_elem_type_heterogeneous_array_unifies | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_elem_type_homogeneous_array_is_single_type | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_elem_type_three_way_heterogeneous_array_unifies | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_extract_fully_unbound_nested_pair_recovers_all_vars | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_extract_infers_four_typevars_from_nesting | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_first_or_empty_list_round_trips_none | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_first_or_nonempty_infers_element | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_generic_static_infers_own_typevar | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_genericbox_get_infers_class_var_from_receiver | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_genericbox_pair_with_infers_method_typevar | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_genericbox_pair_with_unbound_receiver_recovers_class_var | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_glue_invariant_and_covariant_agree_binds | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_glue_invariant_vs_covariant_conflict_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_host_only_object_not_encodable_from_python | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_identity_async_infers | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_enum_round_trips | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_infers_generic_instance | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_infers_primitives | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_infers_user_class | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_nested_unbound_round_trips | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_identity_null_round_trips | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_identity_unbound_generic_instance_round_trips | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_list_head_infers_from_recursive_generic | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_list_head_unbound_recursive_recovers_t_from_fields | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_full_subscript_contradicted_by_actual_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_heterogeneous_list_element_unions | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_infers_multiple_typevars | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_make_triple_partial_explicit_then_infer | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_partial_subscript_requires_full_arity | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_subscript_fully_bound | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_make_triple_types_kwarg_contradicted_by_actual_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_maybe_id_null_round_trips | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_maybe_id_present_value_infers | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_merge_invariant_map_value_conflict_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_named_static_infers_distinct_typevars | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_one_type_arg_explicit_types_succeeds | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_pair_invariant_list_agree_binds | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_pair_invariant_list_conflict_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_parse_as_explicit_types_succeeds | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_read_items_infers_from_instance_wire_args | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_read_items_unbound_container_recovers_t_from_fields | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_return_only_var_still_requires_binding | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_second_of_infers_from_nested_generic | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_second_of_unbound_instance_recovers_field_type | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_tag_or_value_binds_generic_instance | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_triple_choose_join_includes_concrete_class | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_triple_choose_join_includes_enum_variant | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_triple_choose_three_covariant_join | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_two_typevar_union_is_uninferrable_rejects | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_union_concrete_sibling_absorbs_value_binds_rust_type | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_union_null_actual_binds_rust_type | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_union_with_concrete_sibling_infers_typevar | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_values_of_empty_map_round_trips_empty_list | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_values_of_nonempty_returns_values | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_inference_wrap_infers_and_returns_bound_generic | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/generic_inference_wrap_infers_and_returns_generic | y | - | - | - | - | - | - | - | y | y | all |  |
| function_calls/generic_instance_carries_sparse_value_type | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/generic_nullable_type_variables_preserve_every_pointer_boundary | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_over_union_round_trips | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/generic_receiver_and_static_helpers | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_return_only_type_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_union_input_and_engine_validation | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/get_class_unknown_fqn_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/get_class_unresolvable_module_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/get_enum_unknown_fqn_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/get_type_alias_unknown_fqn_raises | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/get_version | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/go_codegen_context_deadline | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_default_argument_serialization_error_names_argument | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_defaulted_argument_type_matrix | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_defaulted_void | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_option_name_collisions | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_optional_arg_last_value_wins | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_person_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/heap_handles_dedup_to_one_refcounted_key | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/hello_world_returns_literal | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/hidden_mode_does_not_inherit | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_baml_cancellation_remains_live_after_exit | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_callable_cancellation_while_dispatched | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_class_argument | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_closed_union_containers_and_nominal_arms | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_closed_union_literal_optional_and_selected_empty_container_arms | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_closed_union_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_declared_throw_is_catchable | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_late_and_uncaught_failures_release_native_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_media_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_nullable_optional_distinguishes_all_three_states | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_optional_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_optional_names_avoid_generated_and_projection_collisions | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_panic_does_not_cross_cgo_boundary | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_primitive_and_multiple_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_reentrant_call_does_not_deadlock | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_repeated_and_concurrent_reuse | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_structured_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callable_void_signatures | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/host_callables_adopts_a_custom_thenable_exactly_once | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_adopts_a_promise_from_a_separate_browser_realm | - | - | y | - | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_callables_async_callable_future_completing_exceptionally_round_trips_original | - | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/host_callables_async_callable_returning_future_is_awaited_by_bridge | - | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/host_callables_async_callable_runs_to_completion | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/host_callables_awaits_a_promise_returning_callback | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_call_repeatedly_invokes_callback_n_times | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_call_repeatedly_with_zero_n_returns_empty_list | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_call_with_throwing_in_baml_catches_host_callable_error | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_cancels_through_the_originating_runtime_after_runtime_replacement | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_class_callback_round_trips_class_value | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/host_callables_class_callback_round_trips_pydantic_model | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_completes_a_pending_host_call_after_runtime_replacement | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_completes_through_the_metadata_fallback_for_a_hostile_thrown_object | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_concurrent_throws_in_flight_rehydrate_to_their_own_object | - | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/host_callables_delivers_a_single_supplied_optional_by_name_defaulting_the_rest | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_delivers_both_supplied_optionals_in_one_opts_object | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_ignores_a_host_promise_settlement_after_its_outer_call_is_cancelled | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_int_return_callable_round_trip | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_lambda_round_trip | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/host_callables_multiple_callable_keys_are_distinct | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_multiple_throws_in_flight_do_not_collide_in_registry | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/host_callables_omits_both_optionals_so_the_callback_s_own_defaults_apply | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_optional_args_all_set_deliver_both | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/host_callables_optional_args_all_unset_apply_host_defaults | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/host_callables_optional_args_partially_set_deliver_by_name | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/host_callables_passes_a_generated_class_instance_into_the_callback | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_preserves_a_rejected_promise_reason_by_identity | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_preserves_an_error_whose_stack_is_not_a_string | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_preserves_arbitrary_thrown_js_values_without_hanging | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_preserves_number_kind_for_float_bigint_union | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | exercises JavaScript's distinct number and bigint host representations |
| function_calls/host_callables_preserves_same_realm_thrown_object_identity | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_rejects_callable_args_on_the_generated_sync_path_instead_of_hanging | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_release_fires_on_drop_of_callable | y | y | y | y | - | - | y | - | y | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, rust, java | callable release coverage depends on host weak-reference support and remains nondeterministic |
| function_calls/host_callables_resolves_integral_number_for_float_class_field | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | exercises the Node bridge's JavaScript number encoding in generated classes |
| function_calls/host_callables_resolves_integral_number_for_float_return | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | exercises the Node bridge's JavaScript number encoding for float host returns |
| function_calls/host_callables_returns_and_invokes_a_host_callable_nested_in_a_list | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_returns_and_invokes_a_nested_host_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_round_trips_a_typed_baml_error_through_typed_catch_and_propagation | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_round_trips_an_arrow_function_callback | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_simple_sync_callable_returns_string | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callables_surfaces_a_throwing_callback_as_a_baml_error | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_surfaces_a_wrong_callback_return_type_as_host_contract_violation | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_throwing_async_callable_round_trips_original_error | - | - | - | - | - | - | - | - | - | y | all |  |
| function_calls/host_callables_throwing_async_callable_round_trips_original_python_exception | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_throwing_callable_bamlerror_propagates_back_with_typed_fields | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_throwing_callable_custom_host_exception_round_trips_with_identity | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/host_callables_throwing_callable_custom_python_exception_round_trips_with_identity | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_throwing_callable_hostthrow_codegenned_class_is_caught_in_baml | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/host_callables_throwing_callable_hostthrow_propagates_back_with_typed_fields | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/host_callables_throwing_callable_keyerror_round_trips_with_identity | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_throwing_callable_out_of_range_round_trips_with_identity | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/host_callables_throwing_callable_round_trips_original_host_exception | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/host_callables_throwing_callable_round_trips_original_python_exception | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/host_callables_two_arg_callable_unpacks_positional_args | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/host_callback_cleanup_retains_context | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_callback_reentry_context | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_cancellation_waits_for_cleanup_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python task cancellation and asynchronous finally cleanup |
| function_calls/host_capture_avoids_getters_and_proxies_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | covers JS getters, Proxy traps, and custom thenables |
| function_calls/host_capture_preserves_application_values | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_child_outlives_parent | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_context_inherits_and_restores | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_current_exposes_generated_cancel_token | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_errors_preserve_identity_and_restore_context | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/host_preserves_call_shape_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | covers JS receivers, variadic arguments, and native Promise results |
| function_calls/host_rejects_unsupported_configuration_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers unsupported Python generator/decorator shapes |
| function_calls/host_rejects_unsupported_configuration_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | covers JS generator and marker configuration rejection |
| function_calls/host_result_encode_failure_releases_capability_clone | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/host_result_successful_encode_transfers_capability_clone_to_engine | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/host_supplied_json_supports_typed_narrowing | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests) |
| function_calls/host_thread_context_handoff_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python ContextVar handoff to a thread pool |
| function_calls/host_throw_encode_failure_releases_capability_clone | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/image_from_base64 | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/image_from_base64_with_mime | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/image_from_file | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/image_from_file_with_mime | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/image_from_url | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/image_from_url_with_mime | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/inbound_class_value_carries_base_fqn | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/initialize_runtime_from_source_reports_compile_errors | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/instance_method_cancellation_returns_exact_context_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_media_receiver_default_and_ownership_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_optional_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_throw_preserves_current_go_error_contract | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_methods_on_classes_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_never_method_has_error_only_signature_and_returns_panic | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instrument_decorator_forms_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python decorator forms |
| function_calls/instrument_preserves_sync_execution_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | covers Python signatures and positional argument binding |
| function_calls/invalid_controls_rejected | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/invalid_function_arguments_surface_baml_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invalid_response_preserves_wire_diagnostics | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | exercises Python BamlError and generated Pydantic payloads |
| function_calls/invalid_timeout_rejected | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/invocation_inheritance | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invocation_inheritance_multi_layer_context_patch_inherits_and_restores | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_lifecycle | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invocation_lifecycle_failed_admission_does_not_consume_reservation | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_lifecycle_pre_cancelled_call_does_not_enter_callback | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_lifecycle_reservation_is_single_use | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_lifecycle_retained_effective_token_observes_late_parent_cancellation | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_lifecycle_zero_timeout_does_not_enter_callback | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_options | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invocation_options_four_call_forms | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_options_four_call_forms_async | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_options_invalid_timeout_rejected | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/invocation_surfaces | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invocation_surfaces_returned_callable_accepts_controls | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/json_returned_from_host_callback_supports_typed_narrowing | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests) |
| function_calls/late_callback_keeps_async_local_storage_after_abort_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node callback Promises can outlive an aborted SDK waiter |
| function_calls/layered_callback_context_inheritance_and_restoration_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python sync/async re-entry and ContextVar propagation |
| function_calls/layered_callback_context_inheritance_and_restoration_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node async carrier, native signals, and entry-time snapshots are specific to TypeScript |
| function_calls/layered_callback_context_restores_after_returned_callable | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/legacy_controls_rejected | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/live_capability_methods_use_async_cancellation_decoder | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/live_handle_kinds_select_trusted_wrappers | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/live_token_cancels_after_admission | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_hello_world_returns_literal | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/main_returns_the_literal_async | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_returns_the_literal_sync | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_round_trips_a_single_positional_argument | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_round_trips_ints_bools_strings_and_floats | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_single_required_arg_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/media_decodes_and_reencodes_as_portable_payload | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's portable values |
| function_calls/method_generated_name_collisions_stay_callable | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/method_self_all_supported_positions_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/methods_accept_controls | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/methods_accept_controls_async | y | y | y | y | - | - | y | - | - | - | all |  |
| function_calls/methods_on_classes_create_constructs_a_greeter_async_plus_sync | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/methods_on_classes_exposes_sync_plus_async_bindings_for_both_flavors | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/methods_on_classes_greet_arg_echoes_a_non_self_argument_async_plus_sync | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/methods_on_classes_instance_greet_async_with_arg_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_instance_greet_with_arg_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_instance_who_async_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_instance_who_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_method_bindings_exist | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/methods_on_classes_static_create_async_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_static_create_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/methods_on_classes_who_returns_a_field_off_self_async_plus_sync | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/missing_argument_raises_invalid_argument | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/missing_argument_without_generated_class_keeps_its_message | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/multi_layer_context_patch_inherits_and_restores | - | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/native_closure_can_cross_sync_and_async_entries_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node sync calls support native closures whose captures need no host dispatch |
| function_calls/native_signal_cancels_without_cancelling_input_token_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node async carrier, native signals, and entry-time snapshots are specific to TypeScript |
| function_calls/nil_host_callable_fails_before_dispatch | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/non_generic_instance_value_type_has_no_type_args | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/null_controls_preserve_inherited_context | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/omitted_argument_is_not_null | y | y | y | y | - | - | y | - | y | y | all |  |
| function_calls/optional_args_async_samples | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/optional_args_covers_static_and_instance_optional_args | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/optional_args_covers_the_runtime_matrix | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/optional_args_negative_runtime_cases_reject | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/optional_args_opt_box_method_matrix | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/optional_args_python_unset_and_none_differ_in_one_call | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/optional_args_rejects_invalid_runtime_calls_that_bypass_types | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/optional_args_runtime_matrix | y | - | - | - | y | - | y | y | y | y | all |  |
| function_calls/optional_args_treats_undefined_as_omitted_and_keeps_null_distinct | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/optional_args_unset_and_null_differ_in_one_call | - | - | - | - | y | - | - | - | - | y | all |  |
| function_calls/options_snapshot_at_async_entry_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node async carrier, native signals, and entry-time snapshots are specific to TypeScript |
| function_calls/options_snapshot_when_coroutine_starts_python_only | y | - | - | - | - | - | - | - | - | - | all |  |
| function_calls/panic_value_of_unknown_class_reaches_the_caller_as_its_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/panic_without_generated_class_is_its_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/parameterize_applies_to_generic_type_alias | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/parameterize_falls_back_for_fully_bound_alias | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/parameterize_falls_back_for_non_generic_class | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/parameterize_no_args_returns_base | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/parameterize_single_arg_int | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/parse_json_successful_value_uses_generated_json_projection | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/pdf_from_base64 | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/pdf_from_file | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/pdf_from_url | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/pdf_from_url_with_mime | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/pre_aborted_call_does_not_dispatch_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/pre_aborted_native_signal_does_not_enter_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node async carrier, native signals, and entry-time snapshots are specific to TypeScript |
| function_calls/pre_cancelled_call_does_not_enter_callback | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/promise_all_failure_with_explicit_sibling_abort_preserves_error_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/promise_callback_preserves_async_local_storage_across_suspension_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/promise_callback_sync_reentry_rejects_host_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/prompt_wrapper_reencodes_repeatedly_without_a_handle | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's portable values |
| function_calls/provider_error_preserves_native_error_metadata | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | exercises Python BamlError and generated Pydantic payloads |
| function_calls/py_type_to_baml_type_returns_empty_for_unknown | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/py_type_to_baml_type_walks_mro | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/raises_async_sibling_also_has_raises | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/raises_imports | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/raises_imports_symbols_reachable | y | - | - | - | y | - | - | - | y | - | all |  |
| function_calls/raises_inferred_contract_without_clause_still_raises | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/raises_method_raises_block_in_pyi | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/raises_method_raises_blocks | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/raises_non_throwing_function_has_no_raises_block | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/raises_single_throws | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/raises_summary_precedes_raises_block | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/raises_union_throws_lists_all_names | y | - | - | - | y | - | y | - | y | - | all |  |
| function_calls/reflected_type_closed_dynamic_unions_and_callback | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/reflected_type_composes_through_optional_containers_and_classes | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/reflected_type_primitive_literal_and_nominal_descriptors | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/reflected_type_top_level_and_runtime_produced_values | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/rehydrate_host_value_reads_handle_of_undecoded_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/rehydrate_host_value_reads_projected_handle_alias | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/rejected_admission_does_not_consume_reservation | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/renamed_enum_member_encodes_raw_value_for_scalar_and_map_key | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/renamed_pydantic_field_populates_both_ways_and_encodes_raw_name | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/repeated_abort_does_not_interrupt_async_callback_cleanup_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python callback cancellation under an explicit controller |
| function_calls/repeated_cooperative_abort_does_not_interrupt_callback_cleanup_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/repeated_dispatches_each_start_with_entry_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/repeated_dispatches_each_start_with_entry_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python ContextVar isolation between dispatch tasks |
| function_calls/repeated_dispatches_invoke_callback_in_order | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/reservation_does_not_inherit | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/reservation_is_single_use | - | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/resolve_types_accepts_only_a_dict_for_generic | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/resolve_types_dict_maps_by_name_in_declaration_order | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/resolve_types_empty_params_rejects_types_kwarg | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/resolve_types_leaves_unnamed_params_for_the_engine_to_infer | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/resolve_types_rejects_unknown_keys | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/retained_callback_uses_invocation_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/retained_context_survives_parent_completion | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/retained_effective_token_observes_late_parent_cancellation | - | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/retained_effective_token_stays_live_after_callback | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/retained_host_callback_uses_later_invocations_application_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python callback task and event loop ownership |
| function_calls/retained_invocation_resolves_token_after_callback | y | - | - | - | - | - | - | - | - | - | all |  |
| function_calls/returned_callable_accepts_controls | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/returned_callable_accepts_controls_async | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/returned_callable_parameter_aliases_preserve_controls | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/returned_class_without_generated_class_is_an_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/returned_closure_preserves_callback_error_and_remains_reusable | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/returned_closure_retains_host_callback | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/returned_value_of_unknown_class_is_an_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/reused_callback_uses_current_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/runtime_executes_the_generated_sdk_in_a_browser | - | - | y | - | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_executes_the_generated_sdk_in_node | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_executes_the_generated_sdk_in_workerd | - | - | - | y | - | - | - | - | - | - | python_pydantic2, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_imports_the_generated_sdk_in_the_configured_runtime | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/same_callback_recurses_through_baml | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/same_callback_recurses_through_baml_without_reusing_an_entry_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | sync callback re-entry differs between Python and Node |
| function_calls/sdk_panic_wire_envelope_decodes_to_baml_panic | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | reads the Python bridge's host-callable registry and handle table |
| function_calls/set_unhandled_spawn_error_handler_returns_the_handler_it_replaces | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/shutdown_timeout_bounds_an_in_flight_call | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/shutdown_timeout_is_validated | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/single_required_arg_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/specialized_callable_rejects_type_bindings | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/static_method_errors_never_cancellation_and_collision_names | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/static_method_media_json_type_and_rust_type_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/static_method_required_default_and_structured_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points | y | y | - | - | y | - | y | - | y | y | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/stdlib_entrypoints_native_argv_callable_as_entry_point | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/stdlib_entrypoints_native_baml_sys_argv_is_callable_as_an_entry_point | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point | y | y | - | - | y | - | y | - | y | y | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/stdlib_error_surfaces_as_go_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/stdlib_error_without_generated_class_is_its_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/stdlib_reverse_overrides_seeded | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's type map |
| function_calls/stream_async_forwards_python_task_cancellation | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/stream_async_preserves_cancellation_when_native_cancel_fails | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/stream_companion_calls_its_exact_fqn | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/stream_exact_fqn_is_preserved_on_the_wire | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's identifier aliases |
| function_calls/stream_handle_kind_ignores_misleading_type_metadata | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/stream_schema_failure_is_parse_failed | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | exercises Python BamlError and generated Pydantic payloads |
| function_calls/stream_uses_canonical_method_fqns | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/successful_decode_transfers_ownership_to_callback | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's ownership of wire handles |
| function_calls/successful_encode_retains_clone_until_wire_owner_consumes_it | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's handle wrappers |
| function_calls/suspended_callback_survives_gc_until_cancellation_cleanup_finishes_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | asyncio tasks retain callback bodies until actual completion |
| function_calls/sync_call_returns_null | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/sync_callback_can_run_async_baml_with_its_own_application_loop_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Node cannot synchronously drive an application loop |
| function_calls/sync_callback_inherits_call_time_async_local_storage_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_callback_inherits_call_time_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_callback_of_async_entry_reenters_sync_baml_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | sync callback re-entry differs between Python and Node |
| function_calls/sync_callback_of_async_entry_runs_on_originating_loop_thread_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_callback_of_async_entry_runs_on_originating_loop_thread_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_callback_reenters_sync_baml_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | sync callback re-entry differs between Python and Node |
| function_calls/sync_callback_runs_on_sync_callers_thread_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_callback_sync_reentry_rejects_host_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_cancel_via_context | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/sync_entry_async_callback_copies_context_across_suspension_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python ContextVar semantics with a bridge-owned loop |
| function_calls/sync_entry_async_callback_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_entry_cancellation_reaches_async_callback_on_worker_loop_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python cancellation on a bridge-owned callback loop |
| function_calls/sync_entry_in_running_loop_with_self_contained_async_callback_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_entry_in_running_loop_with_sync_callback_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_entry_rejection_does_not_start_promise_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejection_preserves_application_context_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_before_dispatch_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_callback_before_async_reentry_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_callback_before_recursive_reentry_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_callback_before_sync_reentry_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_promise_callback_between_event_loop_turns_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_promise_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_retained_callback_and_allows_async_reuse_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node sync entries must reject callbacks hidden in native closure captures |
| function_calls/sync_entry_rejects_sync_callback_between_event_loop_turns_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_rejects_sync_callback_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node event loop, AsyncLocalStorage, or cooperative Promise cancellation |
| function_calls/sync_entry_sync_callback_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/sync_retained_closure_rejects_before_dispatch_typescript_only | - | y | - | - | - | - | - | - | - | - | typescript_node | Node cannot run JS callbacks while a sync native call blocks its event loop |
| function_calls/task_factory_failure_completes_dispatch_and_preserves_exception_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python asyncio task factory failures during dispatch |
| function_calls/taskgroup_failure_cancels_sibling_baml_call_and_preserves_error_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python callback task and event loop ownership |
| function_calls/thrown_class_without_generated_class_is_its_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | calls the Python bridge by function name, without the generated wrappers |
| function_calls/thrown_value_of_unknown_class_reaches_the_caller_as_its_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/timeout_upper_bound_accepted | y | y | y | y | - | - | y | - | y | - | all |  |
| function_calls/to_thread_sync_entry_inherits_copied_application_context_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/two_application_loops_on_two_threads_share_callback_without_rerouting_python_only | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python thread, event loop, and ContextVar semantics |
| function_calls/type_mismatch_without_generated_class_is_a_type_error | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's wire decoder |
| function_calls/unbound_generic_instance_carries_nominal_sparse_value_type | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's encoding of generic types |
| function_calls/unhandled_spawn_error_default_prints_a_cancelled_task_and_continues | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_handler_can_keep_the_default_for_some_errors | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_handler_is_told_when_the_task_was_cancelled | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_handler_none_restores_the_default | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_handler_replaces_the_default | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_handler_that_raises_is_reported_and_the_process_continues | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | observes process-wide state of the Python bridge |
| function_calls/unhandled_spawn_error_uses_host_default | y | - | - | - | y | - | - | y | y | y | python_pydantic2, cpp, go, java, swift | requires subprocess-level SDK harness support |
| function_calls/union_throws_preserves_concrete_class_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/unknown_control_rejected | y | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/unmarked_callback_explicit_context_and_reentry | y | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/unpolled_future_starts_nothing_rust_only | - | - | - | - | - | - | y | - | - | - | rust | Rust task scopes and lazy future cancellation are specific to Rust |
| function_calls/unset_and_none_differ_in_one_call | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/user_panic_surfaces_as_go_error_without_panicking | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/user_throw_surfaces_declared_class_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/video_from_base64 | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/video_from_file | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/video_from_url | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | unit test of the Python bridge's media constructors |
| function_calls/web_sysops_maps_fetch_failures_and_timeouts_into_declared_baml_errors | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_http_streaming_and_unrelated_sysops | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_sync_and_async_baml_fs_read_promptly | - | - | y | - | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_sync_http_before_dispatching_fetch | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_unsupported_filesystem_operations | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_supports_sync_and_async_baml_fs_read_through_node_fs_read_file_sync | - | - | - | y | - | - | - | - | - | - | python_pydantic2, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_trampolines_baml_http_fetch_to_global_fetch_and_buffers_the_response | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_trampolines_baml_http_send_with_method_headers_and_body | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/zero_timeout_does_not_enter_callback | y | y | y | y | - | - | y | - | y | - | all |  |
| host_reflect/compiled_package_returns_class_graph | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/generated_class_subclasses_resolve_to_declared_type | y | y | y | y | - | - | - | - | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | Python and TypeScript have generated-class subclass tokens; Go has no subclass construct |
| host_reflect/host_handles_expose_composition_only | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/known_type_tokens_compose_and_reject_unknowns | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/reflection_compile_errors_are_typed | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/runtime_class_definition_preserves_nested_metadata | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/runtime_enum_definition_decodes_alias | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/wire_occurrences_are_fresh_and_handles_reject_serialization | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| integration/baml_closures_execute_host_arguments_structured_returns_and_mutable_captures | - | - | - | - | - | y | - | - | - | - | csharp | C# canonical coverage executes through its native integration harness |
| integration/basic_calls_executes_sync_and_async | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/cancel_token_any_propagates_native_cancellation | - | - | - | - | - | y | - | - | - | - | csharp | isolates the flaky native cancellation propagation check |
| integration/canonical_documentation_consumer_compiles_and_executes | - | - | - | - | - | y | - | - | - | - | csharp | validates the C#-specific documentation consumer |
| integration/checked_in_union_runtime_source_matches_generator | - | - | - | - | - | y | - | - | - | - | csharp | validates C#-specific generated union runtime source |
| integration/dynamic_values_executes_native_dynamic_value_parity | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/failures_and_cancellation_executes_typed_failures_cancellation_and_exit | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/generated_baml_clients_are_not_tracked | - | - | - | - | - | y | - | - | - | - | csharp | validates C#-specific generated-client repository hygiene |
| integration/generics_executes_inferred_and_explicit_generics | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/generics_generated_surface_rejects_ambiguous_generic_calls | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific generated-surface compile coverage |
| integration/media_executes_media_in_both_directions | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/primitive_edges_executes_native_primitive_and_nullable_edges | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/stdlib_resources_executes_native_typed_resource_apis_lifetimes_and_state | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/stdlib_structurals_executes_native_stdlib_structural_roundtrips | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/streaming_executes_generated_native_stream_and_request_failure | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| integration/type_roundtrips_executes_nominals_collections_defaults_and_unions | - | - | - | - | - | y | - | - | - | - | csharp | exercises C#-specific native SDK integration coverage |
| llm_functions/constructs_a_live_spec_without_a_synthetic_fqn | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | validates TypeScript's live FunctionSpec metadata surface |
| llm_functions/dynamic_runtime_stream_identity_and_flat_projection_parity | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python BamlRuntimeValue identity through the generated sync stream surface |
| llm_functions/dynamic_runtime_stream_is_an_elegant_async_iterable | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python's generated async-iterable stream surface |
| llm_functions/flat_stream_calls_exact_companion_fqn | - | - | - | - | y | - | - | - | - | - | cpp | C++-only exact flat-stream companion dispatch check. |
| llm_functions/flat_stream_controls_are_typed_options | - | - | - | - | - | - | - | y | - | - | go | pins the Go generator's typed option surface for flat stream controls. |
| llm_functions/function_spec_helpers_accept_invocation_controls | y | - | - | - | - | - | - | - | - | - | all |  |
| llm_functions/function_spec_helpers_accept_invocation_controls_async | y | - | - | - | - | - | - | - | - | - | all |  |
| llm_functions/function_spec_parse_replaces_the_parse_companion | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python's generated FunctionSpec parse surface |
| llm_functions/function_spec_parse_returns_closed_enum | - | - | - | - | - | - | - | y | - | - | go | pins Go's closed-enum FunctionSpec decoder surface. |
| llm_functions/function_spec_parse_returns_runtime_error_for_invalid_output | - | - | - | - | - | - | - | y | - | - | go | pins Go's FunctionSpec parse-error translation. |
| llm_functions/function_spec_parse_returns_typed_class_and_fills_missing_nullable_field | - | - | - | - | - | - | - | y | - | - | go | pins Go's typed FunctionSpec class decoder surface. |
| llm_functions/function_spec_prompt_uses_generated_ai_prompt_type_and_is_reusable | - | - | - | - | - | - | - | y | - | - | go | pins the Go FunctionSpec prompt facade and reusable context behavior. |
| llm_functions/keeps_a_portable_prompt_reusable_across_engine_reentry | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | validates TypeScript's portable Prompt and Image wrappers |
| llm_functions/legacy_function_companion_wire_bindings_are_absent | - | - | - | - | - | - | - | y | - | - | go | pins the Go generator's emitted wire-binding names. |
| llm_functions/main_baml_sdk_lorem_and_baml_sdk_ipsum_are_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_classify_sentiment_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_extract_resume_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_extract_resume_operation_bindings | y | - | - | - | - | - | - | - | y | - | python_pydantic2, java | pins Python and Java flat operation binding spellings |
| llm_functions/main_extract_resume_spec_and_stream_bindings | - | - | - | - | - | - | y | - | - | - | rust | pins Rust generated spec and stream binding spellings |
| llm_functions/main_flat_stream_controls_live_on_the_stream_options | - | - | - | - | - | - | - | - | y | - | all |  |
| llm_functions/main_function_spec_prompt_is_portable_and_reusable | - | - | - | - | - | - | - | - | y | - | all |  |
| llm_functions/main_ipsum_classify_sentiment_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_ipsum_sentiment_enum_has_positive_negative_neutral_members | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_ipsum_sentiment_enum_shape | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/main_lorem_extract_resume_exposes_flat_spec_and_stream_bindings | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | pins TypeScript's dollar-preserving flat stream binding names |
| llm_functions/main_lorem_extract_resume_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_resume_class_shape | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_lorem_resume_is_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_streaming_doc_class_shape | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_lorem_streaming_doc_is_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_streaming_extract_exposes_flat_spec_and_stream_bindings | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | pins TypeScript's dollar-preserving flat streaming binding names |
| llm_functions/main_lorem_streaming_extract_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_namespaces_reachable_via_explicit_import | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_nullable_model_field_can_be_omitted | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | pins the Python generator's nullable-field `= None` default |
| llm_functions/main_replay_server_namespace_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_root_imports_cleanly | y | y | y | y | - | - | y | - | y | - | all |  |
| llm_functions/main_spec_replaces_prompt_parse_and_request_companions | - | - | - | - | - | - | y | - | - | - | rust | validates Rust's generated FunctionSpec prompt, parse, and request surface |
| llm_functions/main_streaming_extract_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_streaming_extract_operation_bindings | y | - | - | - | - | - | - | - | y | - | python_pydantic2, java | pins Python and Java flat streaming operation binding spellings |
| llm_functions/main_streaming_extract_spec_and_stream_bindings | - | - | - | - | - | - | y | - | - | - | rust | pins Rust generated streaming spec and stream binding spellings |
| llm_functions/main_types_and_bindings_reachable | - | - | - | - | - | - | - | - | - | y | all |  |
| llm_functions/on_event_plain_call_delivers_events | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | host on_event listener coverage lands Python-first; other SDKs port separately |
| llm_functions/on_event_plain_call_delivers_llm_call_model | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | host on_event listener coverage lands Python-first; other SDKs port separately |
| llm_functions/on_event_plain_call_raising_listener_does_not_fail | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | host on_event listener coverage lands Python-first; other SDKs port separately |
| llm_functions/on_event_stream_delivers_settle_events | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | host on_event listener coverage lands Python-first; other SDKs port separately |
| llm_functions/on_event_stream_raising_listener_does_not_fail | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | host on_event listener coverage lands Python-first; other SDKs port separately |
| llm_functions/projects_static_and_instance_llm_methods_without_invoking_a_provider | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | validates TypeScript static and instance operation member spellings |
| llm_functions/prompt_is_reusable_and_media_survives_request_preview | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python's portable Prompt and media wrapper surface |
| llm_functions/spec_projection_honors_cancellation | - | - | - | - | - | - | - | y | - | - | go | pins Go context cancellation through the Spec projection. |
| llm_functions/streaming_e2e_async_class_typed_next_async_yields_10_partials | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_async_next_async_yields_10_partials | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_baml_driven_collect_keeps_the_s_stream_finished_union_engine_side | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_baml_driven_collect_returns_the_final_doc | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_next_yields_10_doc_partials_final_is_a_typed_streaming_doc | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_next_yields_10_partials_and_drains_to_stream_finished | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| llm_functions/streaming_e2e_stream | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/streaming_e2e_stream_async | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/streaming_e2e_stream_collect_in_baml | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/streaming_e2e_stream_doc | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/streaming_e2e_stream_doc_async | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/streaming_e2e_stream_doc_collect_in_baml | y | - | - | - | - | - | y | - | y | y | all |  |
| package_edges/compile_cross_package_types_compile | - | - | - | - | - | - | - | y | - | - | all |  |
| package_edges/compile_llm_projection_default_overrides | - | - | - | - | - | - | - | y | - | - | go | pins Go generator options for authored Spec and Stream defaults. |
| type_shapes/alias_container_composition_and_defaults | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/alias_package_scope_collisions_compile_and_run | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/aliases_round_trip_alias_container | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/aliases_round_trip_maybe_rec | - | - | - | - | y | - | - | - | - | - | all |  |
| type_shapes/aliases_round_trip_rec_list | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/aliases_round_trip_string_list | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/bridge_handles_media_clones_ordinary_owners_and_wire_keys_independently | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_handles_media_clones_ownership_through_to_handle_and_from_handle | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | exercises TypeScript bridge media handle ownership APIs |
| type_shapes/bridge_handles_media_constructs_url_file_and_base64_descriptors | - | y | y | y | - | - | - | - | - | - | typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | exercises TypeScript bridge media descriptor APIs |
| type_shapes/bridge_handles_media_exposes_stable_seed_tags | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_handles_media_keeps_host_value_tags_outside_ordinary_clone_and_release | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_handles_media_normalizes_key_halves_losslessly_and_returns_defensive_key_objects | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_handles_media_rejects_an_invalid_ordinary_key_only_when_an_operation_resolves_it | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_surface_exports_the_same_runtime_values_in_node_browsers_and_workers | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/bridge_surface_preserves_the_public_constructor_names | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/class_refs_make_outer | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/class_refs_round_trip_inner | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/class_refs_round_trip_outer | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/closed_union_zero_value_returns_an_input_error | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/complex_models_round_trip_complex_profile_accepts_plain_object_literals_no_class_constructors | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/container_recursive_class_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/declared_enum_functions_and_class_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/deep_namespace_thing_reachable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/defaulted_enum_argument_and_invalid_values | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_accepts_natural_go_integers | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_candidates_round_trip_as_concrete_go_values | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_delegates_semantic_validation_to_baml | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_nested_containers_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_of_containers_uses_selected_type_metadata | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/dynamic_union_rejects_unserializable_go_values_in_bridge | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/enum_composition_matrix_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/enum_package_scope_collisions_compile_and_run | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/enums_pick_positive | y | y | y | y | y | - | y | - | y | - | all |  |
| type_shapes/enums_pick_sentiment | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/enums_round_trip_enums | y | y | y | y | y | - | y | - | y | - | all |  |
| type_shapes/enums_round_trip_sentiment | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/enums_round_trip_sentiment_positive | y | y | y | y | y | - | y | - | y | - | all |  |
| type_shapes/every_supported_leaf_through_lists_and_maps | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/forward_refs_round_trip_g_node_int | y | y | y | y | - | - | y | - | y | - | all |  |
| type_shapes/forward_refs_round_trip_node_symbol_exists | - | - | - | - | y | - | - | - | - | y | all |  |
| type_shapes/forward_refs_round_trip_other | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/forward_refs_round_trip_rec_list | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/forward_refs_round_trip_rec_list_with_other | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/generic_generic | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generic_generic_wrapper_get_value | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/generic_wrapper_get_value | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/generics_round_trip_box_int | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generics_round_trip_differing_instantiation | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generics_round_trip_generic_binary_tree_int | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generics_round_trip_generic_linked_list_int | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generics_round_trip_nested_generics | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/generics_round_trip_wrapper_int | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/handles_baml_fs_open_returns_a_typed_file_handle | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| type_shapes/handles_file_cursor_state_persists_across_calls | y | y | - | - | - | - | y | - | y | y | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| type_shapes/handles_http_get_response_fields_and_methods | y | y | - | - | - | - | y | - | y | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| type_shapes/handles_image_from_base64_roundtrips_payload | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/handles_open_file_returns_file_handle | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/host_created_media_round_trips_through_optional_list | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python host-created media wrapper encoding |
| type_shapes/lists_round_trip_empty_list | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/lists_round_trip_ints | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/lists_round_trip_list_container | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/lists_round_trip_optional_strings | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/lists_round_trip_union_list | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_literal_values_convert_implicitly | - | - | - | - | y | - | - | - | - | - | all |  |
| type_shapes/literals_return_literals | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_flag_mixed_literal_union | y | - | - | - | y | - | - | - | - | - | all |  |
| type_shapes/literals_round_trip_literal42 | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_literal_draft | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_literal_escaped | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_literal_false | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_literal_true | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/literals_round_trip_literals | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/lorem_resume_reachable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/main_all_namespaces_reachable | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/main_deep_namespace_thing_reachable | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/main_generated_models_ignore_extra_fields | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python Pydantic extra-field compatibility |
| type_shapes/main_lorem_resume_reachable | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/main_root_foo_reachable | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/main_root_imports_cleanly | y | y | y | y | - | - | y | - | y | - | all |  |
| type_shapes/main_runtime_owned_builtin_leaves_expose_their_public_names | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/make_foo | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/make_outer | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/maps_round_trip_enum_keyed_map | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/maps_round_trip_list_valued_map | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/maps_round_trip_map_container | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/maps_round_trip_resume | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/maps_round_trip_sentiment | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/maps_round_trip_simple_map | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/media_constructors_and_accessors | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/media_nested_optional_and_containers | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/media_return_and_round_trip_all_kinds | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/media_return_audio | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_return_audio_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_return_image | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_return_image_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_return_pdf | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_return_pdf_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_return_video | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_return_video_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_round_trip_audio | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_round_trip_audio_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_round_trip_image | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_round_trip_image_can_reuse_the_same_wrapper | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_round_trip_media | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_round_trip_media_preserves_all_four_media_fields | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_round_trip_pdf | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_round_trip_pdf_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_round_trip_video | y | - | - | - | - | - | y | - | y | y | all |  |
| type_shapes/media_round_trip_video_preserves_url_and_mime | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/media_uses_the_runtime_owned_wrapper_constructors | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/nested_class_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/no_op | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/nullable_class_fields_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/nullable_container_boundaries_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/nullable_container_fields_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/nullable_recursive_classes_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/opaque_rust_type_default_and_host_callback_positions | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/opaque_rust_type_nested_containers_classes_and_null | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/opaque_rust_type_round_trips_and_remains_reusable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/optional_round_trip_optional_container | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/optional_round_trip_optional_int | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/optional_round_trip_optional_resume | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/optional_round_trip_optional_union | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/optional_round_trip_resume | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/optional_top_level_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/overlapping_list_arms_preserve_exact_kind_for_empty_and_nonempty_values | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/overlapping_literal_arms_round_trip_with_exact_kind | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/overlapping_map_arms_preserve_exact_kind_for_empty_and_nonempty_values | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/pick_positive | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/pick_sentiment | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/primitive_class_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/primitive_literal_union_constructors_round_trip_with_exact_kind_and_value | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/primitives_return_bigint | y | - | - | - | y | - | - | - | y | - | all |  |
| type_shapes/primitives_return_bool | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_float | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_int | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_null | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_string | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_bigint | y | - | - | - | y | - | - | - | y | - | all |  |
| type_shapes/primitives_round_trip_bigint_async | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python async entry of the bigint round trip |
| type_shapes/primitives_round_trip_bigint_in_i64_range | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | Python has one int type; a value in the i64 range rides the int channel and the engine widens it to bigint |
| type_shapes/primitives_round_trip_bool | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_float | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_float_accepts_int | y | - | - | - | y | - | y | - | y | - | all |  |
| type_shapes/primitives_round_trip_int | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_int_async | - | - | - | - | - | - | - | - | - | y | all |  |
| type_shapes/primitives_round_trip_null | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_primitives | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_primitives_float_field_accepts_int | y | - | - | - | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_string | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_uint8_array | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/recursion_round_trip_int_binary_tree | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/recursion_round_trip_mutual_recursion | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/recursion_round_trip_scc_t1_t2_t3 | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/recursion_round_trip_scc_t4_t5_t6 | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/required_container_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_bool | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_float | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_literals | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_null | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/return_string | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/root_foo_reachable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/root_imports_cleanly | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_bool | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_box_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_dedup | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_deep | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_deep_thing_from_a | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_deep_thing_from_lorem | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_differing_instantiation | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_empty_list | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_empty_list_preserves_selected_arm | - | - | - | - | - | - | - | - | - | y | all |  |
| type_shapes/round_trip_enums | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_fizz_buzz_foo_bar | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_fizz_foo_bar | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_float | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_float_accepts_int_constant | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_foo | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_foo_bar | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_forward_ref_g_node_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_forward_ref_other | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_generic_binary_tree_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_generic_linked_list_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_inner | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_int_binary_tree | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_ints | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_ipsum | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_list_container | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_list_valued_map | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literal42 | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literal_draft | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literal_escaped | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literal_false | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literal_true | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_literals | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_lorem_resume | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_lorem_resume_from_ipsum | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_map_resume | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_map_sentiment | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_mutual_recursion | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_nested_generics | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_null | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_null_to_end | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_container | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_plus_null | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_resume | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_strings | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_optional_union | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_outer | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_primitives | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_primitives_float_field_accepts_int_constant | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_required_resume | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_resume_or_http_response | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_root_foo_from_ab | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_root_foo_from_lorem | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_scct1_t2_t3 | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_scct4_t5_t6 | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_sentiment | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_sentiment_positive | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_simple_map | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_singleton_unwrap | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_string | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_string_list | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_thing_from_ab | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_uint8_array | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_union_container | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_union_list | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_union_t | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_wrapper_int | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/routing_make_foo | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_deep_thing_from_a | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_deep_thing_from_lorem | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_foo | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_lorem_resume_from_ipsum | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_resume | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_resume_or_http_response | y | y | y | y | - | - | - | - | y | y | all |  |
| type_shapes/routing_round_trip_root_foo | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_root_foo_from_ab | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_thing_from_ab | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/supported_namespaces_reachable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/symbol_collisions_round_trip_deep | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_fizz_buzz_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_fizz_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_ipsum | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/transparent_alias_functions_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/type_alias_declared_before_classes_is_importable | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python-specific generated type alias ordering |
| type_shapes/typemap_is_installed_during_root_module_evaluation | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/typemap_preserves_user_enum_and_generic_mappings | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/typemap_resolves_every_runtime_owned_base_to_one_bridge_constructor_identity | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/union_aliases_are_transparent_and_flatten_before_thresholding | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/unions_consumption_surfaces | - | - | - | - | - | - | - | - | - | y | all |  |
| type_shapes/unions_match_dispatches_by_type | - | - | - | - | y | - | - | - | - | - | all |  |
| type_shapes/unions_round_trip_dedup | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_round_trip_null_to_end | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_round_trip_optional_plus_null | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_round_trip_singleton_unwrap | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_round_trip_str_or_int_list | y | - | - | - | y | - | y | - | y | - | all |  |
| type_shapes/unions_round_trip_t | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_round_trip_union_container | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/unions_union_is_a_plain_std_variant | - | - | - | - | y | - | - | - | - | - | all |  |
| type_shapes/void_no_op | y | y | y | y | y | - | y | - | y | y | all |  |
| unsupported_only/compile_unsupported_only_package_compiles | - | - | - | - | - | - | - | y | - | - | all |  |

## Baseline comparison

No required coverage pairs changed.

