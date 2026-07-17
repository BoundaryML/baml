# C++ style

This SDK follows the [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html).
Headers use `.h`, sources use `.cc`, include guards use the Google
`BAML_..._H_` form, and formatting is clang-format with
`BasedOnStyle: Google` (see `.clang-format`).

## Carve-outs

Functions are PascalCase except the following, all sanctioned by the Google
guide itself:

1. Accessors returning stored state keep snake_case: `message()`,
   `class_name()`, `baml_trace()`, `payload()`, `bytes()`, `code()`, and
   `detail::OwnedBuffer`'s `empty()`/`to_string()`. Generated opts
   setters are `set_<param>` -- the guide's own mutator convention.
2. The vocabulary rule (Abseil's practice: structure is Google-cased,
   vocabulary is std-cased). Types stay PascalCase (`Union`, `Box`,
   `Arg`, `Unset`; lowercase `union` is a C++ keyword regardless), but
   API that a user's muscle memory or generic code treats as
   optional/variant-shaped keeps std spelling, exactly as
   `absl::optional::has_value` / `absl::visit` / `absl::nullopt` do:
   `baml::match` (mirrors `std::visit`), `baml::unset` (mirrors
   `std::nullopt`), `Arg::is_set`/`is_unset`/`value`,
   `Box`/`OptionalBox::has_value`, `BamlError::is<T>`/`get<T>` (mirror
   `std::holds_alternative`/`std::get`).
3. Type traits keep std spelling: `is_std_optional`, `has_set_opt1`,
   `has_set_opt3`.
4. `extern "C"` symbols keep snake_case (C ABI): `baml_cpp_result_trampoline`.
5. Generated identifiers derived from BAML source names are unchanged,
   including derived names: the opts struct for function `probe` is
   `probeOpts` (source casing preserved, never re-cased)
   (functions, classes, fields, params in generated `baml_sdk` code).

## Deliberate deviations

- Exceptions are used deliberately: the `BamlError` contract is the bridge's
  error surface. This is a knowing deviation from the Google guide's
  no-exceptions rule.
- Generated code approximates Google style (2-space indent) but is not
  clang-formatted.
