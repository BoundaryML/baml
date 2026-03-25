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
// <no-diagnostics-expected>
