//! Opt-in golden coverage. Adding a corpus file never opts it into IR or
//! formatter snapshots. Keep examples small and state the regression each
//! phase protects; the complete corpus is still checked, emitted and formatted.

pub(super) struct Example {
    pub path: &'static str,
    /// MIR: source-level names. Bytecode: fully qualified emitted names.
    /// PPIR/formatter select the whole source file, so this is empty.
    pub functions: &'static [&'static str],
    pub reason: &'static str,
}

pub(super) const PPIR: &[Example] = &[
    Example {
        path: "ns_fixtures/ns_namespaces_nested/main.baml",
        functions: &[],
        reason: "Cross-namespace name resolution and item expansion.",
    },
    Example {
        path: "ns_fixtures/ns_llm_spec_mode/main.baml",
        functions: &[],
        reason: "LLM spec and tool companion desugaring.",
    },
    Example {
        path: "ns_fixtures/ns_testset_dynamic/main.baml",
        functions: &[],
        reason: "Dynamic testset registry expansion.",
    },
    Example {
        path: "ns_fixtures/ns_optional_function_parameters/main.baml",
        functions: &[],
        reason: "Default-argument parameter lowering.",
    },
    Example {
        path: "ns_fixtures/ns_method_explicit_type_args/method_explicit_type_args.baml",
        functions: &[],
        reason: "Explicit generic method arguments.",
    },
    Example {
        path: "ns_fixtures/ns_numeric_literal_method_call/numeric_literal_method_call.baml",
        functions: &[],
        reason: "Numeric literal receivers remain values, not malformed member bases.",
    },
];

pub(super) const MIR: &[Example] = &[
    Example {
        path: "ns_fixtures/ns_short_circuit_locals/short_circuit_locals.baml",
        functions: &[
            "chained_and",
            "conditional_short_circuit_in_loop",
            "mixed_join",
        ],
        reason: "Short-circuit joins and loop-carried locals.",
    },
    Example {
        path: "ns_fixtures/ns_lambda_basic/lambda_basic.baml",
        functions: &["test_capture", "test_nested"],
        reason: "Captured values and nested lambda lowering.",
    },
    Example {
        path: "ns_fixtures/ns_catch_throw/catch_throw.baml",
        functions: &["SingleCatchWildcard", "CatchWithFallback"],
        reason: "Catch handlers and chained error dispatch.",
    },
    Example {
        path: "ns_fixtures/ns_json_to_from_string_generic_forwarding/json_to_from_string_generic_forwarding.baml",
        functions: &["fetch_as"],
        reason: "Forwarded runtime type argument remains TypeArgRef(0).",
    },
    Example {
        path: "ns_fixtures/ns_json_to_from_string_concrete/json_to_from_string_concrete.baml",
        functions: &["serialize_user", "deserialize_user"],
        reason: "Concrete type load for decoding, no type argument for encoding.",
    },
    Example {
        path: "ns_fixtures/ns_patterns_new/patterns_new.baml",
        functions: &[
            "or_with_repeated_typed_binding",
            "chain_narrow_then_wildcard",
        ],
        reason: "Or-pattern bindings and narrowed match fallthrough.",
    },
    Example {
        path: "ns_fixtures/ns_parser_statements/while_loop.baml",
        functions: &["simple_while_loop"],
        reason: "Loop backedge and mutable local lowering.",
    },
    Example {
        path: "ns_fixtures/ns_llm_quoted_prompt/main.baml",
        functions: &["QuotedPrompt", "BacktickPrompt"],
        reason: "Both prompt syntaxes emit real executable bodies.",
    },
];

pub(super) const BYTECODE: &[Example] = &[
    Example {
        path: "ns_fixtures/ns_function_call/function_call.baml",
        functions: &[
            "user.fixtures.function_call.Bar",
            "user.fixtures.function_call.Foo",
            "user.fixtures.function_call.GreetBaz",
        ],
        reason: "Direct calls, arithmetic and method dispatch.",
    },
    Example {
        path: "ns_fixtures/ns_short_circuit_locals/short_circuit_locals.baml",
        functions: &[
            "user.fixtures.short_circuit_locals.chained_and",
            "user.fixtures.short_circuit_locals.conditional_short_circuit",
            "user.fixtures.short_circuit_locals.conditional_short_circuit_in_loop",
            "user.fixtures.short_circuit_locals.initialized_local_in_branch_loop",
            "user.fixtures.short_circuit_locals.mixed_join",
        ],
        reason: "All five predecessor-coverage regressions: stack-carried values versus local slots.",
    },
    Example {
        path: "ns_fixtures/ns_lambda_basic/lambda_basic.baml",
        functions: &[
            "user.fixtures.lambda_basic.test_capture",
            "user.fixtures.lambda_basic.test_nested",
        ],
        reason: "Closure construction, capture and indirect calls.",
    },
    Example {
        path: "ns_fixtures/ns_patterns_new/patterns_new.baml",
        functions: &[
            "user.fixtures.patterns_new.or_with_repeated_typed_binding",
            "user.fixtures.patterns_new.chain_narrow_then_wildcard",
        ],
        reason: "Runtime discrimination and branch targets.",
    },
    Example {
        path: "ns_fixtures/ns_catch_throw/catch_throw.baml",
        functions: &[
            "user.fixtures.catch_throw.SingleCatchWildcard",
            "user.fixtures.catch_throw.CatchWithFallback",
        ],
        reason: "Exception dispatch and handler bytecode.",
    },
    Example {
        path: "ns_fixtures/ns_json_to_from_string_generic_forwarding/json_to_from_string_generic_forwarding.baml",
        functions: &["user.fixtures.json_to_from_string_generic_forwarding.fetch_as"],
        reason: "Runtime type arguments passed through a generic call.",
    },
    Example {
        path: "ns_fixtures/ns_host_callable_call/main.baml",
        functions: &["user.fixtures.host_callable_call.call_with_callback"],
        reason: "Host callbacks use CallIndirect without a synthesized wrapper.",
    },
    Example {
        path: "ns_spawn_semantics/spawn_semantics.baml",
        functions: &["user.spawn_semantics.multiple_awaits_on_same_future_return_cached_value_fn"],
        reason: "Spawn and repeated awaits on one future.",
    },
    Example {
        path: "ns_fixtures/ns_json_parse_stringify_intrinsics/json_parse_stringify_intrinsics.baml",
        functions: &["user.fixtures.json_parse_stringify_intrinsics.parse_and_roundtrip"],
        reason: "JSON intrinsic call lowering.",
    },
];

pub(super) const FORMATTER: &[Example] = &[
    Example {
        path: "ns_fixtures/ns_parser_expressions/precedence.baml",
        functions: &[],
        reason: "Expression precedence and parentheses.",
    },
    Example {
        path: "ns_fixtures/ns_parser_statements/if_statements.baml",
        functions: &[],
        reason: "Conditional block layout.",
    },
    Example {
        path: "ns_fixtures/ns_parser_statements/for_loops.baml",
        functions: &[],
        reason: "Loop formatting.",
    },
    Example {
        path: "ns_fixtures/ns_backtick_dedent/main.baml",
        functions: &[],
        reason: "Multiline prompt dedenting.",
    },
    Example {
        path: "ns_fixtures/ns_backtick_strings/main.baml",
        functions: &[],
        reason: "Backtick interpolation and escapes.",
    },
    Example {
        path: "ns_fixtures/ns_top_level_header_comment/repro.baml",
        functions: &[],
        reason: "Top-level comment attachment.",
    },
    Example {
        path: "ns_fixtures/ns_comment_in_type/comment_in_type.baml",
        functions: &[],
        reason: "Comments inside types.",
    },
    Example {
        path: "ns_fixtures/ns_type_annotation/type_positions.baml",
        functions: &[],
        reason: "Type annotations in declaration positions.",
    },
    Example {
        path: "ns_fixtures/ns_lambda_fat_arrow/lambda_fat_arrow.baml",
        functions: &[],
        reason: "Lambda syntax normalization.",
    },
    Example {
        path: "ns_fixtures/ns_patterns_new/patterns_new.baml",
        functions: &[],
        reason: "Match patterns and guards.",
    },
    Example {
        path: "ns_fixtures/ns_testset_nested/main.baml",
        functions: &[],
        reason: "Nested test syntax.",
    },
    Example {
        path: "ns_fixtures/ns_method_explicit_type_args/method_explicit_type_args.baml",
        functions: &[],
        reason: "Generic method arguments.",
    },
];
