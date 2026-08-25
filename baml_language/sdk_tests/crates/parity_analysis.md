# SDK test coverage parity

This report inventories checked-in test declarations. It does not report whether tests passed.

Distinct exact test IDs: 684. IDs with complete required parity: 39. Required gaps: 4457.

Baseline ratchet: UNCHANGED. Required gaps: 4457 (baseline: 4457). Present declarations: 2069 (baseline: 2069). Newly missing required pairs: 0. Resolved baseline gaps: 0. Weakened requirements: 0.

## Python-baselined parity

Parity is the share of the 303 test IDs declared in `python_pydantic2` that are also declared in each SDK environment. SDK-only test IDs do not affect these percentages.

| SDK environment | Matching Python test IDs | Parity |
| --- | ---: | ---: |
| python_pydantic2 | 303 / 303 | 100.0% |
| typescript_node | 124 / 303 | 40.9% |
| typescript_web_chromium | 117 / 303 | 38.6% |
| typescript_web_cloudflare_workers | 117 / 303 | 38.6% |
| cpp | 128 / 303 | 42.2% |
| csharp | 0 / 303 | 0.0% |
| rust | 224 / 303 | 73.9% |
| go | 13 / 303 | 4.3% |
| java | 293 / 303 | 96.7% |
| swift | 185 / 303 | 61.1% |

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
| function_calls/callback_throws_caught_and_replaced_makes_the_function_infallible | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/callback_throws_caught_then_rethrown_value_is_the_replacement_union | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/callback_throws_rethrown_carries_the_effect_param_into_the_error_union | - | - | - | - | - | - | y | - | - | - | rust | validates Rust-specific inferred callback error unions |
| function_calls/cancellation_accepts_the_native_decimal_uint64_boundary_spellings | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_async_call_returns_none | y | - | - | - | y | - | y | - | y | y | all |  |
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
| function_calls/cancellation_rejects_malformed_and_overflowing_ids | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_a_reused_call_context_as_abort_error_with_baml_reason | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_async_cancellation_as_abort_error_with_baml_reason | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_cancellation_through_promise_all | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_surfaces_sync_pre_aborted_cancellation_as_abort_error | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/cancellation_sync_call_returns_none | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/cancellation_sync_cancel_via_call_context | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/canonical_json_class_union_uses_declared_field_codecs | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_composes_through_containers_and_classes | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_defaults_and_callbacks | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_dynamic_union | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_rejects_extensions_before_dispatch | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/canonical_json_round_trips_at_top_level_and_through_alias | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/clean_exit_helper_process | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/clean_exit_terminates_process_with_code | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/coawait_co_await_cancelled_call_throws_cancelled | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_completed_future_fast_path | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_pending_future_resumes | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_throws_typed_into_the_coroutine | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_co_await_yields_the_decoded_value | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/coawait_uncaught_coroutine_exception_reaches_join | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/empty_class_self_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/error_call_cancellation_preserves_context_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/error_string_is_non_empty | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/errors_async_sibling_throws_typed | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_baml_error_carries_baml_trace | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_baml_trace_spliced_into_python_traceback | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_cancellation_surfaces_as_baml_panic | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_clean_exit_terminates_process_with_code | - | - | - | - | - | - | y | - | - | - | all |  |
| function_calls/errors_clean_exit_terminates_process_with_code_0 | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/errors_clean_exit_terminates_process_with_code_7 | y | - | - | - | - | - | - | - | y | - | all |  |
| function_calls/errors_host_invalid_argument_wraps_baml_errors_invalid_argument | y | - | - | - | - | - | y | - | y | - | all |  |
| function_calls/errors_stdlib_error_surfaces_as_baml_error | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_stdlib_error_surfaces_typed | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_str_is_non_empty | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_typed_throw_is_still_a_baml_error | - | - | - | - | y | - | - | - | - | - | all |  |
| function_calls/errors_union_throws_preserves_class_name | y | y | y | y | y | - | y | - | y | y | all |  |
| function_calls/errors_user_panic_surfaces_as_baml_panic | y | - | - | - | - | - | y | - | y | y | all |  |
| function_calls/errors_user_throw_surfaces_declared_instance | y | - | - | - | y | - | y | - | y | y | all |  |
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
| function_calls/generic_nullable_type_variables_preserve_every_pointer_boundary | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_receiver_and_static_helpers | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_return_only_type_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/generic_union_input_and_engine_validation | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_context_deadline | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_default_argument_serialization_error_names_argument | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_defaulted_argument_type_matrix | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_defaulted_void | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_option_name_collisions | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_optional_arg_last_value_wins | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/go_codegen_person_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/hello_world_returns_literal | - | - | - | - | - | - | - | y | - | - | all |  |
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
| function_calls/host_callables_preserves_same_realm_thrown_object_identity | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_rejects_callable_args_on_the_generated_sync_path_instead_of_hanging | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/host_callables_release_fires_on_drop_of_callable | y | y | y | y | - | - | y | - | y | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, rust, java | callable release coverage depends on host weak-reference support and remains nondeterministic |
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
| function_calls/host_supplied_json_supports_typed_narrowing | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests) |
| function_calls/instance_method_cancellation_returns_exact_context_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_media_receiver_default_and_ownership_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_optional_arguments | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_method_throw_preserves_current_go_error_contract | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_methods_on_classes_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/instance_never_method_has_error_only_signature_and_returns_panic | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/invalid_function_arguments_surface_baml_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/json_returned_from_host_callback_supports_typed_narrowing | y | y | y | y | y | - | y | y | y | y | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, rust, go, java, swift | C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests) |
| function_calls/main_hello_world_returns_literal | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/main_returns_the_literal_async | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_returns_the_literal_sync | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_round_trips_a_single_positional_argument | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_round_trips_ints_bools_strings_and_floats | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/main_single_required_arg_round_trips | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/method_generated_name_collisions_stay_callable | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/method_self_all_supported_positions_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
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
| function_calls/nil_host_callable_fails_before_dispatch | - | - | - | - | - | - | - | y | - | - | all |  |
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
| function_calls/parse_json_successful_value_uses_generated_json_projection | - | - | - | - | - | - | - | y | - | - | all |  |
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
| function_calls/runtime_executes_the_generated_sdk_in_a_browser | - | - | y | - | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_executes_the_generated_sdk_in_node | - | y | - | - | - | - | - | - | - | - | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_executes_the_generated_sdk_in_workerd | - | - | - | y | - | - | - | - | - | - | python_pydantic2, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/runtime_imports_the_generated_sdk_in_the_configured_runtime | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/single_required_arg_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/static_method_errors_never_cancellation_and_collision_names | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/static_method_media_json_type_and_rust_type_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/static_method_required_default_and_structured_round_trips | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points | y | y | - | - | y | - | y | - | y | y | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/stdlib_entrypoints_native_argv_callable_as_entry_point | y | - | - | - | y | - | y | - | y | y | all |  |
| function_calls/stdlib_entrypoints_native_baml_sys_argv_is_callable_as_an_entry_point | - | y | y | y | - | - | - | - | - | - | all |  |
| function_calls/stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point | y | y | - | - | y | - | y | - | y | y | python_pydantic2, typescript_node, cpp, csharp, rust, go, java, swift |  |
| function_calls/stdlib_error_surfaces_as_go_error | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/sync_call_returns_null | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/sync_cancel_via_context | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/unhandled_spawn_error_uses_host_default | - | - | - | - | y | - | - | y | y | y | cpp, go, java, swift | requires subprocess-level SDK harness support |
| function_calls/union_throws_preserves_concrete_class_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/unset_and_none_differ_in_one_call | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/user_panic_surfaces_as_go_error_without_panicking | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/user_throw_surfaces_declared_class_identity | - | - | - | - | - | - | - | y | - | - | all |  |
| function_calls/web_sysops_maps_fetch_failures_and_timeouts_into_declared_baml_errors | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_http_streaming_and_unrelated_sysops | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_sync_and_async_baml_fs_read_promptly | - | - | y | - | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_sync_http_before_dispatching_fetch | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_rejects_unsupported_filesystem_operations | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_supports_sync_and_async_baml_fs_read_through_node_fs_read_file_sync | - | - | - | y | - | - | - | - | - | - | python_pydantic2, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_trampolines_baml_http_fetch_to_global_fetch_and_buffers_the_response | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| function_calls/web_sysops_trampolines_baml_http_send_with_method_headers_and_body | - | - | y | y | - | - | - | - | - | - | python_pydantic2, typescript_web_chromium, typescript_web_cloudflare_workers, cpp, csharp, rust, go, java, swift |  |
| host_reflect/compiled_package_returns_class_graph | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/generated_class_subclasses_resolve_to_declared_type | y | y | y | y | - | - | - | - | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers | Python and TypeScript have generated-class subclass tokens; Go has no subclass construct |
| host_reflect/host_handles_expose_composition_only | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/known_type_tokens_compose_and_reject_unknowns | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/reflection_compile_errors_are_typed | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/runtime_class_definition_preserves_nested_metadata | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/runtime_enum_definition_decodes_alias | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| host_reflect/wire_occurrences_are_fresh_and_handles_reject_serialization | y | y | y | y | - | - | - | y | - | - | python_pydantic2, typescript_node, typescript_web_chromium, typescript_web_cloudflare_workers, go | BEP-066 host reflection is currently exposed only by Python, TypeScript, and Go |
| integration/baml_closure_decodes_multiple_args_and_structured_return_values | - | - | - | - | - | y | - | - | - | - | csharp | C# canonical coverage executes through its native integration harness |
| integration/baml_closure_is_a_native_callable_with_host_language_arguments | - | - | - | - | - | y | - | - | - | - | csharp | C# canonical coverage executes through its native integration harness |
| integration/baml_closure_is_reusable_and_retains_mutable_captures | - | - | - | - | - | y | - | - | - | - | csharp | C# canonical coverage executes through its native integration harness |
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
| llm_functions/main_baml_sdk_lorem_and_baml_sdk_ipsum_are_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_classify_sentiment_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_extract_resume_companion_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_extract_resume_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_ipsum_classify_sentiment_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_ipsum_sentiment_enum_has_positive_negative_neutral_members | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_ipsum_sentiment_enum_shape | y | - | - | - | - | - | y | - | y | y | all |  |
| llm_functions/main_lorem_exposes_the_stream_companion_classes_beside_their_base_type | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_extract_resume_companion_bindings_exist | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_extract_resume_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_resume_class_shape | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_lorem_resume_is_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_streaming_doc_class_shape | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_lorem_streaming_doc_is_reachable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_streaming_extract_companion_bindings_exist | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_lorem_streaming_extract_sync_plus_async_factories_are_callable | - | y | y | y | - | - | - | - | - | - | all |  |
| llm_functions/main_namespaces_reachable_via_explicit_import | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_replay_server_namespace_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_root_imports_cleanly | y | y | y | y | - | - | y | - | y | - | all |  |
| llm_functions/main_stream_types_lorem_leaf_present | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_streaming_extract_companion_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_streaming_extract_factory_bindings | y | - | - | - | - | - | y | - | y | - | all |  |
| llm_functions/main_types_and_bindings_reachable | - | - | - | - | - | - | - | - | - | y | all |  |
| llm_functions/parse_companion_honors_cancellation | - | - | - | - | - | - | - | y | - | - | all |  |
| llm_functions/parse_companion_returns_closed_enum | - | - | - | - | - | - | - | y | - | - | all |  |
| llm_functions/parse_companion_returns_runtime_error_for_invalid_output | - | - | - | - | - | - | - | y | - | - | all |  |
| llm_functions/parse_companion_returns_typed_class_and_fills_missing_nullable_field | - | - | - | - | - | - | - | y | - | - | all |  |
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
| type_shapes/primitives_return_bigint | y | - | - | - | - | - | - | - | y | - | all |  |
| type_shapes/primitives_return_bool | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_float | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_int | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_null | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_return_string | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/primitives_round_trip_bigint | y | - | - | - | - | - | - | - | y | - | all |  |
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
| type_shapes/round_trip_box_of_resume_stream | - | - | - | - | - | - | - | y | - | - | all |  |
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
| type_shapes/round_trip_resume_or_resume_stream | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_resume_stream | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_root_foo_from_ab | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_root_foo_from_lorem | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/round_trip_root_foo_stream | - | - | - | - | - | - | - | y | - | - | all |  |
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
| type_shapes/routing_round_trip_root_foo | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_root_foo_from_ab | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/routing_round_trip_thing_from_ab | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/streams_round_trip_box_of_resume_stream | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/streams_round_trip_resume_or_http_response | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/streams_round_trip_resume_or_resume_stream | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/streams_round_trip_resume_stream | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/streams_round_trip_root_foo_stream | y | y | y | y | - | - | y | - | y | y | all |  |
| type_shapes/supported_namespaces_reachable | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/symbol_collisions_round_trip_deep | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_fizz_buzz_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_fizz_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_foo_bar | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/symbol_collisions_round_trip_ipsum | y | y | y | y | y | - | y | - | y | y | all |  |
| type_shapes/transparent_alias_functions_round_trip | - | - | - | - | - | - | - | y | - | - | all |  |
| type_shapes/type_alias_declared_before_classes_is_importable | y | - | - | - | - | - | - | - | - | - | python_pydantic2 | validates Python-specific generated type alias ordering |
| type_shapes/typemap_is_installed_during_root_module_evaluation | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/typemap_keeps_generated_stream_companions_distinct_from_runtime_owned_bases | - | y | y | y | - | - | - | - | - | - | all |  |
| type_shapes/typemap_preserves_user_enum_generic_and_companion_mappings | - | y | y | y | - | - | - | - | - | - | all |  |
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
