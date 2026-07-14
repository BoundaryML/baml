# C++ style

This SDK follows the [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html).
Headers use `.h`, sources use `.cc`, include guards use the Google
`BAML_..._H_` form, and formatting is clang-format with
`BasedOnStyle: Google` (see `.clang-format`).

## Carve-outs

Functions are PascalCase except the following, all sanctioned by the Google
guide itself:

1. Accessors returning stored state keep snake_case: `key()`,
   `handle_type()`, `hex()`, `message()`, `class_name()`, `baml_trace()`,
   `payload()`, `call_id()`, `bytes()`.
2. std-mimicking vocabulary API keeps std spelling: `Arg` (`is_set`,
   `is_unset`, `value`), `Box`/`OptionalBox` (`has_value`, `operator*`,
   `operator->`), `Future` (`get`, `wait`, `wait_for`), `OwnedBuffer`
   (`data`, `size`, `empty`, `to_string`), `Handle` (`empty`, `swap`),
   `BamlError` (`is<T>`, `get<T>`, `what`), `unset_t`/`unset`, `Null`.
3. Type traits keep std spelling: `is_std_optional`, `is_arg`, `arg_inner`,
   `dependent`, `dependent_t`, `has_set_opt1`, `has_set_opt3`.
4. `extern "C"` symbols keep snake_case (C ABI): `baml_cpp_result_trampoline`,
   `baml_cpp_host_dispatch_trampoline`, `baml_cpp_host_release_trampoline`.
5. Generated identifiers derived from BAML source names are unchanged
   (functions, classes, fields, params in generated `baml_sdk` code).

## Deliberate deviations

- Exceptions are used deliberately: the `BamlError` contract is the bridge's
  error surface. This is a knowing deviation from the Google guide's
  no-exceptions rule.
- Generated code approximates Google style (2-space indent) but is not
  clang-formatted.
