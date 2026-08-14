# C++ style

Layout is Google; naming is the C++ standard library's.

- **Formatting**: clang-format with `BasedOnStyle: Google` (see
  `.clang-format`). Headers use `.h`, sources use `.cc`, include guards
  use the `BAML_..._H_` form.
- **Naming**: std-library convention, per the C++ Core Guidelines
  ([NL.10](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines#nl10-prefer-underscore_style-names):
  prefer `underscore_style`, consistent with the ISO standard library) --
  the same convention Boost uses. Enforced by clang-tidy
  `readability-identifier-naming` (see `.clang-tidy`).

## The naming rules

| Entity | Case | Examples |
|---|---|---|
| types | `snake_case` | `baml::variant`, `baml::future`, `baml::lit`, `baml::error`, `baml::thrown<U>`, `detail::call_state` |
| functions and methods | `snake_case` | `baml::match`, `future::cancel`, `codec<T>::encode`, `detail::call_sync` |
| constants | `snake_case` | `baml::unset` (type `baml::unset_t`, the `nullopt`/`nullopt_t` pattern) |
| enumerators | `snake_case` | `lit_shape::integer` (`int`/`bool`/`enum` are keywords; spell them out) |
| template parameters | `CamelCase` | `T`, `Ret`, `ThrownU`, `WriteValue` |
| macros | `BAML_UPPER` | `BAML_LIT`, `BAML_TEST` |
| private members | trailing `_` | `state_`, `engine_call_id_` |

`baml::variant` is the one vocabulary drift from BAML's own terminology:
lowercase `union` is a C++ keyword, and `variant` is the std name for the
same shape.

## Exceptions to the rules

1. `extern "C"` symbols and everything in the C ABI header
   (`baml_cffi.h`: `BamlApiV1`, `BamlBuffer`, ...) keep their contract
   spellings.
2. Generated protobuf code (`pb/`) keeps protoc's conventions.
3. Generated identifiers derived from BAML source names are spelled
   exactly as the BAML author wrote them, never re-cased (`SleepMs` stays
   `SleepMs`). Suffixes the generator adds are snake and follow the other
   bridges: the async sibling of `SleepMs` is `SleepMs_async` (python
   parity), the opts struct for `probe` is `probe_opts`, setters are
   `set_<param>`.

## Deliberate deviations from the Google guide

- Exceptions are used deliberately: the `baml::error` contract is the
  bridge's error surface. This is a knowing deviation from the Google
  guide's no-exceptions rule.
- Generated code approximates the layout (2-space indent) but is not
  clang-formatted.
