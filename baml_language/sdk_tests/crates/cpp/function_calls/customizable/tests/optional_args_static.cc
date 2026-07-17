// Static shape checks for the optional-args surface.
// Port of function_calls/customizable/optional_args_static.py, which is a
// pyright-only file (never executed) documenting the call shapes the type
// checker must reject. The C++ analog asserts the same shapes at compile
// time: each suppressed pyright error corresponds to a static_assert that
// the shape does not compile here.
#include <baml_sdk.h>

#include <cstdint>
#include <optional>
#include <type_traits>
#include <utility>

namespace {

// Expression-SFINAE probe that calls optional_args_probe by NAME so the
// defaulted opts parameter participates (a function pointer would lose it).
template <typename... Args>
constexpr auto ProbeInvocable(int)
    -> decltype(baml_sdk::optional_args_probe(std::declval<Args>()...),
                bool()) {
  return true;
}
template <typename...>
constexpr bool ProbeInvocable(...) {
  return false;
}

template <typename T, typename = void>
struct has_set_opt1 : std::false_type {};
template <typename T>
struct has_set_opt1<
    T, std::void_t<decltype(std::declval<T>().set_opt1(int64_t{1}))>>
    : std::true_type {};

template <typename T, typename = void>
struct has_set_opt3 : std::false_type {};
template <typename T>
struct has_set_opt3<
    T, std::void_t<decltype(std::declval<T>().set_opt3(int64_t{1}))>>
    : std::true_type {};

// Positive control: the documented good shapes compile.
static_assert(
    ProbeInvocable<int64_t>(0),
    "optional_args_probe(x) must be callable with the required arg alone");
static_assert(ProbeInvocable<int64_t, baml_sdk::optional_args_probe_opts>(0),
              "optional_args_probe(x, opts) must be callable");
static_assert(has_set_opt1<baml_sdk::optional_args_probe_opts>::value,
              "opts must expose set_opt1");

// optional_args_probe()  -- missing required arg.
static_assert(
    !ProbeInvocable<>(0),
    "optional_args_probe() without the required arg must not compile");

// optional_args_probe(1, 8)  -- optionals are not positional.
static_assert(!ProbeInvocable<int64_t, int>(0),
              "a positional value must not convert to the opts struct");

// optional_args_probe(1, opt3=1)  -- unknown optional arg.
static_assert(!has_set_opt3<baml_sdk::optional_args_probe_opts>::value,
              "opts must not expose a setter for an undeclared optional");

// optional_args_probe("x")  -- wrong type for the required arg.
static_assert(!ProbeInvocable<const char*>(0),
              "a string must not convert to the int-typed required arg");

// optional_args_probe(1, opt1="x")  -- wrong type for an optional arg.
static_assert(!std::is_constructible<::baml::arg<std::optional<int64_t>>,
                                     const char*>::value,
              "a string must not convert to an int-typed optional arg");

}  // namespace
