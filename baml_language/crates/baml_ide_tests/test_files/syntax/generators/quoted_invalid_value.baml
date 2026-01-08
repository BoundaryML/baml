// Test that invalid quoted values are properly reported as errors
// Previously, using value_word() to get the span would return None for string literals,
// so errors were silently ignored.

generator quoted_invalid {
  output_type "invalid_output"
  default_client_mode "invalid_mode"
  module_format "invalid_format"
}

//----
//- diagnostics
// Error: Invalid value `invalid_output` for property `output_type` in generator `quoted_invalid`. Valid values: python/pydantic, python/pydantic/v1, typescript, typescript/react, ruby/sorbet, go, rest/openapi, boundary-cloud
//    ╭─[ generators_quoted_invalid_value.baml:6:14 ]
//    │
//  6 │   output_type "invalid_output"
//    │              ────────┬────────  
//    │                      ╰────────── Invalid value `invalid_output` for property `output_type` in generator `quoted_invalid`. Valid values: python/pydantic, python/pydantic/v1, typescript, typescript/react, ruby/sorbet, go, rest/openapi, boundary-cloud
//    │ 
//    │ Note: Error code: E0019
// ───╯
// Error: Invalid value `invalid_mode` for property `default_client_mode` in generator `quoted_invalid`. Valid values: sync, async. Use "sync" or "async"
//    ╭─[ generators_quoted_invalid_value.baml:7:22 ]
//    │
//  7 │   default_client_mode "invalid_mode"
//    │                      ───────┬───────  
//    │                             ╰───────── Invalid value `invalid_mode` for property `default_client_mode` in generator `quoted_invalid`. Valid values: sync, async. Use "sync" or "async"
//    │ 
//    │ Note: Error code: E0019
// ───╯
// Error: Invalid value `invalid_format` for property `module_format` in generator `quoted_invalid`. Valid values: cjs, esm. Use "cjs" or "esm"
//    ╭─[ generators_quoted_invalid_value.baml:8:16 ]
//    │
//  8 │   module_format "invalid_format"
//    │                ────────┬────────  
//    │                        ╰────────── Invalid value `invalid_format` for property `module_format` in generator `quoted_invalid`. Valid values: cjs, esm. Use "cjs" or "esm"
//    │ 
//    │ Note: Error code: E0019
// ───╯
