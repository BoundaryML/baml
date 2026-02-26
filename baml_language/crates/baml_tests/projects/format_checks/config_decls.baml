// Testing all the formatter cases for declarations that are just a config block
// as well as any special cases inside the config block

// ===== Basic client declaration =====
client<llm> BasicClient {
    provider openai,
    options {
        model "gpt-4",
    }
}

// ===== Client with many options =====
client<llm> FullClient {
    provider openai
    retry_policy MyRetryPolicy
    options {
        model: "gpt-4"
        max_tokens 100
        temperature 0.7,
        top_p 0.9
    }
}

// ===== Client with string provider =====
client<llm> StringProviderClient {
    provider "openai-generic"
    options {
        base_url "https://openrouter.ai/api/v1"
        model "mistralai/mistral-small-3.1-24b-instruct"
    }
}

// ===== Client with nested config =====
client<llm> NestedConfigClient {
    provider openai
    options {
        model "gpt-4"
        headers {
            "X-API-Key" "my-key"
            "Authorization" "Bearer token"
        }
    }
}

// ===== Client with array in config =====
client<llm> ArrayConfigClient {
    provider openai
    options {
        model "gpt-4",
        stop_sequences: ["END", "STOP", "DONE"]
    }
}

// ===== Client with long option values pushing past line limit =====
client<llm> LongOptionClient {
    provider openai
    options {
        model "some-extremely-long-model-name-that-goes-on-and-on-and-should-push-well-past-the-line-width-limit"
        base_url "https://some-really-long-domain-name-for-testing.example.com/api/v1/chat/completions"
    }
}

// ===== Client with boolean and number values =====
client<llm> TypedValuesClient {
    provider openai
    options {
        model "gpt-4"
        temperature: 0.5
        max_tokens 2048
        stream true
    }
}

// ===== Client with env access in config =====
client<llm> EnvClient {
    provider: openai
    options {
        api_key env.OPENAI_API_KEY
        model "gpt-4"
    }
}

// ===== Client with array of objects in config =====
client<llm> ArrayOfObjectsClient {
    provider openai
    options: {
        model "gpt-4",
        tools [
            {
                server_label "web_search"
                server_url "https://example.com"
                type "mcp"
            }
            {
                server_label "calculator"
                server_url "https://calc.example.com"
            }
        ]
    }
}

// ===== Client with raw string in config =====
client<llm> RawStringConfigClient {
    provider openai,
    options: {
        model: "gpt-4"
        system_prompt #"
            You are a helpful assistant.
            Always be concise.
        "#
    },
}

// ===== Client with mixed value types =====
client<llm> MixedValuesClient {
    provider openai
    options {
        model "gpt-4"
        max_tokens 100
        temperature 0.7
        stream true
        stop ["END"]
        inline_string #"inline raw"#
        "string key" "value with spaces"
    }
}

// ===== Trivia on client declaration (line comments) =====
// 1 client keyword leading
client // 2 client keyword trailing
// 3 type angle leading
< // 4 open angle trailing
// 5 type leading
llm // 6 type trailing
// 7 close angle leading
> // 8 close angle trailing
// 9 name leading
TriviaClient // 10 name trailing
// 11 open brace leading
{ // 12 open brace trailing
    // 13 provider key leading
    provider: // 14 provider key trailing
    // 15 provider value leading
    openai // 16 provider value trailing
    // 17 options key leading
    options // 18 options key trailing
    // 19 options brace leading
    { // 20 options open brace trailing
        // 21 first option leading
        model // 22 model key trailing
        // 23 model value leading
        "gpt-4" // 24 model value trailing
    // 25 options close brace leading
    } // 26 options close brace trailing
// 27 close brace leading
} // 28 close brace trailing

///////////////////

// ===== Trivia on client declaration (block comments) =====
/* 1 client keyword leading */
client /* 2 client keyword trailing */
/* 3 type angle leading */
< /* 4 open angle trailing */
/* 5 type leading */
llm /* 6 type trailing */
/* 7 close angle leading */
> /* 8 close angle trailing */
/* 9 name leading */
TriviaClientBlock /* 10 name trailing */
/* 11 open brace leading */
{ /* 12 open brace trailing */
    /* 13 provider key leading */
    provider /* 14 provider key trailing */
    /* 15 provider value leading */
    openai /* 16 provider value trailing */
    /* 17 options key leading */
    options: /* 18 options key trailing */
    /* 19 options brace leading */
    { /* 20 options open brace trailing */
        /* 21 first option leading */
        model /* 22 model key trailing */
        /* 23 model value leading */
        "gpt-4" /* 24 model value trailing */
    /* 25 options close brace leading */
    } /* 26 options close brace trailing */
/* 27 close brace leading */
} /* 28 close brace trailing */

///////////////////

// ===== Client with very long name exceeding limit =====
client<llm> ThisIsAnExtremelyLongClientNameThatDefinitelyExceedsTheDefaultLineLimitOfOneHundredColumns {
    provider openai
    options {
        model "gpt-4"
    }
}

// ===== Client with deeply nested config (3 levels) =====
client<llm> DeeplyNestedClient {
    provider openai
    options {
        model "gpt-4"
        headers {
            "X-API-Key" "my-key"
            "Authorization" "Bearer token"
            custom_config {
                retry_on_failure true
                timeout_ms 30000
                metadata {
                    team "platform"
                    environment "production"
                }
            }
        }
    }
}

// ===== Client with very long nested values exceeding limit =====
client<llm> LongNestedValuesClient {
    provider openai
    options {
        model "anthropic/claude-3.5-sonnet-20250101-with-some-extremely-long-suffix-that-pushes-past-the-line-limit-easily"
        base_url "https://some-extremely-long-api-gateway-subdomain.internal.production.cluster.example.com/api/v2/chat/completions/streaming"
        headers {
            "Authorization" "Bearer some-extremely-long-api-token-value-that-is-used-for-authentication-and-exceeds-the-line-limit-easily-and-comfortably"
            "X-Custom-Header" "another-very-long-header-value-that-also-pushes-past-one-hundred-columns-for-testing-purposes"
        }
    }
}

// ===== Client with many options pushing config length =====
client<llm> ManyOptionsClient {
    provider openai
    options {
        model "gpt-4"
        max_tokens 4096
        temperature 0.7
        top_p 0.95
        frequency_penalty 0.5
        presence_penalty 0.3
        stream true
        stop_sequences ["END", "STOP", "DONE", "COMPLETE", "FINISHED"]
        api_key env.OPENAI_API_KEY
        base_url "https://api.openai.com/v1"
    }
}

// ===== Client with long array values exceeding limit =====
client<llm> LongArrayClient {
    provider openai
    options {
        model "gpt-4"
        stop_sequences ["STOP_GENERATION_NOW", "END_OF_RESPONSE_MARKER", "COMPLETION_FINISHED_SIGNAL", "TERMINATE_OUTPUT_HERE", "FINAL_ANSWER_DELIMITER"]
    }
}

// ===== Client with array of objects with long values =====
client<llm> LongArrayOfObjectsClient {
    provider openai
    options {
        model "gpt-4"
        tools [
            {
                server_label "this_is_a_very_long_server_label_that_exceeds_the_line_limit_when_combined_with_indentation"
                server_url "https://some-extremely-long-domain-name-for-the-mcp-server.internal.production.example.com/api/v2"
                type "mcp"
                description "A very long description of the tool server that explains what it does and how it should be used by the model"
            }
            {
                server_label "another_long_server_label_for_testing_line_wrapping_behavior_in_nested_array_objects"
                server_url "https://another-long-domain.internal.staging.example.com/tools/v1"
            }
        ]
    }
}

// ===== Trivia on deeply nested config (line comments) =====
// 1 client leading
client // 2 client trailing
< // 3 angle trailing
llm // 4 type trailing
> // 5 close angle trailing
// 6 name leading
TriviaDeeplyNestedClient // 7 name trailing
// 8 open brace leading
{ // 9 open brace trailing
    // 10 provider leading
    provider // 11 provider trailing
    // 12 value leading
    openai // 13 value trailing
    // 14 options leading
    options // 15 options trailing
    // 16 options brace leading
    { // 17 options brace trailing
        // 18 model leading
        model // 19 model trailing
        // 20 model value leading
        "gpt-4" // 21 model value trailing
        // 22 headers leading
        headers // 23 headers trailing
        // 24 headers brace leading
        { // 25 headers brace trailing
            // 26 first header leading
            "X-API-Key" // 27 first header trailing
            // 28 first value leading
            "my-key" // 29 first value trailing
            // 30 second header leading
            "Authorization" // 31 second header trailing
            // 32 second value leading
            "Bearer token" // 33 second value trailing
        // 34 headers close leading
        } // 35 headers close trailing
    // 36 options close leading
    } // 37 options close trailing
// 38 close brace leading
} // 39 close brace trailing

///////////////////

// ===== Trivia on deeply nested config (block comments) =====
/* 1 client leading */
client /* 2 client trailing */
< /* 3 angle trailing */
llm /* 4 type trailing */
> /* 5 close angle trailing */
/* 6 name leading */
TriviaDeeplyNestedClientBlock /* 7 name trailing */
/* 8 open brace leading */
{ /* 9 open brace trailing */
    /* 10 provider leading */
    provider /* 11 provider trailing */
    /* 12 value leading */
    openai /* 13 value trailing */
    /* 14 options leading */
    options /* 15 options trailing */
    /* 16 options brace leading */
    { /* 17 options brace trailing */
        /* 18 model leading */
        model /* 19 model trailing */
        /* 20 model value leading */
        "gpt-4" /* 21 model value trailing */
        /* 22 headers leading */
        headers /* 23 headers trailing */
        /* 24 headers brace leading */
        { /* 25 headers brace trailing */
            /* 26 first header leading */
            "X-API-Key" /* 27 first header trailing */
            /* 28 first value leading */
            "my-key" /* 29 first value trailing */
            /* 30 second header leading */
            "Authorization" /* 31 second header trailing */
            /* 32 second value leading */
            "Bearer token" /* 33 second value trailing */
        /* 34 headers close leading */
        } /* 35 headers close trailing */
    /* 36 options close leading */
    } /* 37 options close trailing */
    /* 38 comma leading */
    , /* 39 comma trailing */
/* 40 close brace leading */
} /* 41 close brace trailing */

///////////////////

// ===== Trivia on config item with long value (line comments) =====
client<llm> TriviaLongValueClient {
    provider openai
    options {
        // 1 key leading
        model // 2 key trailing
        // 3 value leading
        "some-extremely-long-model-name-that-goes-on-and-on-and-should-push-well-past-the-line-width-limit" // 4 value trailing
        // 5 next key leading
        base_url // 6 next key trailing
        // 7 next value leading
        "https://some-really-long-domain-name-for-testing.example.com/api/v1/chat/completions" // 8 next value trailing
    }
}

// ===== Trivia on config item with long value (block comments) =====
client<llm> TriviaLongValueClientBlock {
    provider openai
    options {
        /* 1 key leading */
        model /* 2 key trailing */
        /* 3 value leading */
        "some-extremely-long-model-name-that-goes-on-and-on-and-should-push-well-past-the-line-width-limit" /* 4 value trailing */
        /* 5 next key leading */
        base_url /* 6 next key trailing */
        /* 7 next value leading */
        "https://some-really-long-domain-name-for-testing.example.com/api/v1/chat/completions" /* 8 next value trailing */
    }
}

// ===== Retry policy =====
retry_policy BasicRetry {
    max_retries 3
}

// ===== Retry policy with many settings =====
retry_policy FullRetry {
    max_retries 5
    backoff_multiplier 2.0
    initial_delay_ms 1000
}

// ===== Retry policy with long name exceeding limit =====
retry_policy ThisIsAnExtremelyLongRetryPolicyNameThatDefinitelyExceedsTheDefaultLineLimitOfOneHundredCols {
    max_retries 10
    backoff_multiplier 3.0
    initial_delay_ms 5000
}

// ===== Trivia on retry policy (line comments) =====
// 1 keyword leading
retry_policy // 2 keyword trailing
// 3 name leading
TriviaRetry // 4 name trailing
// 5 open brace leading
{ // 6 open brace trailing
    // 7 first item leading
    max_retries // 8 key trailing
    // 9 value leading
    3 // 10 value trailing
// 11 close brace leading
} // 12 close brace trailing

// ===== Trivia on retry policy (block comments) =====
/* 1 keyword leading */
retry_policy /* 2 keyword trailing */
/* 3 name leading */
TriviaRetryBlock /* 4 name trailing */
/* 5 open brace leading */
{ /* 6 open brace trailing */
    /* 7 first item leading */
    max_retries /* 8 key trailing */
    /* 9 value leading */
    3 /* 10 value trailing */
/* 11 close brace leading */
} /* 12 close brace trailing */

// ===== Test declaration =====
function TestTarget() -> string {
    client BasicClient
    prompt #"Hello"#
}

test BasicTest {
    functions [TestTarget]
    args {

    }
}

// ===== Test with type builder =====
test TypeBuilderTest {
    functions [TestTarget]
    type_builder {
        class InnerClass {
            b int
        }
        dynamic class DynamicInner {
            c string
        }
        enum InnerEnum {
            X
            Y
        }
        dynamic enum DynamicInnerEnum {
            P
            Q
        }
        type InnerAlias = string | int
    },
    args {

    }
}

// ===== Trivia on test declaration (line comments) =====
// 1 test keyword leading
test // 2 test keyword trailing
// 3 name leading
TriviaTest // 4 name trailing
// 5 open brace leading
{ // 6 open brace trailing
    // 7 functions key leading
    functions: // 8 functions key trailing
    // 9 functions value leading
    [TestTarget] // 10 functions value trailing
    // 11 args key leading
    args // 12 args key trailing
    // 13 args block leading
    { // 14 args open brace trailing
    // 15 args close brace leading
    } // 16 args close brace trailing
// 17 close brace leading
} // 18 close brace trailing

// ===== Trivia on test declaration (block comments) =====
/* 1 test keyword leading */
test /* 2 test keyword trailing */
/* 3 name leading */
TriviaTestBlock /* 4 name trailing */
/* 5 open brace leading */
{ /* 6 open brace trailing */
    /* 7 functions key leading */
    functions /* 8 functions key trailing */
    /* 9 functions value leading */
    [TestTarget] /* 10 functions value trailing */
    /* 11 args key leading */
    args /* 12 args key trailing */
    /* 13 args block leading */
    { /* 14 args open brace trailing */
    /* 15 args close brace leading */
    } /* 16 args close brace trailing */
/* 17 close brace leading */
} /* 18 close brace trailing */

// ===== Test with type builder containing long/complex types =====
test LongTypeBuilderTest {
    functions [TestTarget]
    type_builder {
        class VeryLongClassNameThatExceedsTheLineLimitWhenCombinedWithTheTypeBuilderIndentation {
            extremely_long_field_name_for_testing_wrapping int
            another_very_long_field_name string
        }
        enum VeryLongEnumNameThatAlsoExceedsTheLineLimitWhenCombinedWithTheTypeBuilderIndentation {
            VeryLongVariantNameThatExceedsLimit
            AnotherVeryLongVariantName
        }
        type VeryLongTypeAliasNameThatExceedsLimit = int | string | bool | float | null | int[] | string[]
    }
    args {

    }
}

// ===== Trivia on test with type builder (line comments) =====
// 1 test leading
test // 2 test trailing
// 3 name leading
TriviaTypeBuilderTest // 4 name trailing
// 5 open brace leading
{ // 6 open brace trailing
    // 7 functions leading
    functions // 8 functions trailing
    // 9 value leading
    [TestTarget] // 10 value trailing
    // 11 type_builder leading
    type_builder // 12 type_builder trailing
    // 13 tb open brace leading
    { // 14 tb open brace trailing
        // 15 class leading
        class // 16 class trailing
        // 17 class name leading
        TbInnerClass // 18 class name trailing
        // 19 class brace leading
        { // 20 class brace trailing
            // 21 field leading
            field_name: // 22 field name trailing
            // 23 type leading
            int // 24 type trailing
        // 25 class close leading
        } // 26 class close trailing
        // 27 enum leading
        enum // 28 enum trailing
        // 29 enum name leading
        TbInnerEnum // 30 enum name trailing
        // 31 enum brace leading
        { // 32 enum brace trailing
            // 33 variant leading
            X // 34 variant trailing
        // 35 enum close leading
        } // 36 enum close trailing
        // 37 type alias leading
        type // 38 type trailing
        // 39 alias name leading
        TbAlias // 40 alias name trailing
        // 41 equals leading
        = // 42 equals trailing
        // 43 rhs leading
        string // 44 rhs trailing
    // 45 tb close leading
    } // 46 tb close trailing
    // 47 args leading
    args // 48 args trailing
    // 49 args brace leading
    { // 50 args brace trailing
    // 51 args close leading
    } // 52 args close trailing
// 53 close leading
} // 54 close trailing

///////////////////

// ===== Trivia on test with type builder (block comments) =====
/* 1 test leading */
test /* 2 test trailing */
/* 3 name leading */
TriviaTypeBuilderTestBlock /* 4 name trailing */
/* 5 open brace leading */
{ /* 6 open brace trailing */
    /* 7 functions leading */
    functions /* 8 functions trailing */
    /* 9 value leading */
    [TestTarget] /* 10 value trailing */
    /* 11 type_builder leading */
    type_builder /* 12 type_builder trailing */
    /* 13 tb open brace leading */
    { /* 14 tb open brace trailing */
        /* 15 class leading */
        class /* 16 class trailing */
        /* 17 class name leading */
        TbInnerClassBlock /* 18 class name trailing */
        /* 19 class brace leading */
        { /* 20 class brace trailing */
            /* 21 field leading */
            field_name :/* 22 field name trailing */
            /* 23 type leading */
            int /* 24 type trailing */
        /* 25 class close leading */
        } /* 26 class close trailing */
        /* 27 enum leading */
        enum /* 28 enum trailing */
        /* 29 enum name leading */
        TbInnerEnumBlock /* 30 enum name trailing */
        /* 31 enum brace leading */
        { /* 32 enum brace trailing */
            /* 33 variant leading */
            X /* 34 variant trailing */
        /* 35 enum close leading */
        } /* 36 enum close trailing */
        /* 37 type alias leading */
        type /* 38 type trailing */
        /* 39 alias name leading */
        TbAliasBlock /* 40 alias name trailing */
        /* 41 equals leading */
        = /* 42 equals trailing */
        /* 43 rhs leading */
        string /* 44 rhs trailing */
    /* 45 tb close leading */
    } /* 46 tb close trailing */
    /* 47 args leading */
    args /* 48 args trailing */
    /* 49 args brace leading */
    { /* 50 args brace trailing */
    /* 51 args close leading */
    } /* 52 args close trailing */
/* 53 close leading */
} /* 54 close trailing */

///////////////////

// ===== Short names =====
client<llm> C {
    provider openai
    options {
        model "gpt-4"
    }
}

retry_policy R {
    max_retries 1
}

// ===== Extraneous whitespace =====
client<llm>    ExtraWhitespaceClient    {
    provider    openai
    options    {
        model    "gpt-4"
    }
}

retry_policy    ExtraWhitespaceRetry    {
    max_retries    3
}

// ===== Config with commas between items =====
client<llm> CommaClient {
    provider openai,
    options {
        model "gpt-4",
        max_tokens 100,
    }
}

// ===== Config with items on same line =====
client<llm> SameLineClient {
    provider openai
    options { model "gpt-4" }
}
