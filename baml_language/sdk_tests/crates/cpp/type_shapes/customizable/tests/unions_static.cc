// Static shape checks for the baml::Union surface (no python analog;
// documents the compile-time contract the way optional_args_static.cc does
// for opts structs). Each "must not compile" shape is pinned as a negative
// trait assertion rather than a commented-out snippet.
#include <baml_sdk.h>

#include <string>
#include <type_traits>
#include <variant>

namespace {

using IntOrString = baml::Union<int64_t, std::string>;

// Canonicalization is total: every permutation of an alternative set is
// one instantiation, including through nesting in std containers.
static_assert(std::is_same<baml::Union<int64_t, std::string>,
                           baml::Union<std::string, int64_t>>::value,
              "Union must be order-canonical");
static_assert(
    std::is_same<std::optional<baml::Union<int64_t, std::string>>,
                 std::optional<baml::Union<std::string, int64_t>>>::value,
    "canonicalization must hold under std::optional");
static_assert(
    std::is_same<std::vector<baml::Union<int64_t, std::string>>,
                 std::vector<baml::Union<std::string, int64_t>>>::value,
    "canonicalization must hold under std::vector");

// A Union IS a std::variant instantiation (alias, not a wrapper).
static_assert(
    std::is_same<
        IntOrString,
        std::variant<typename std::variant_alternative<0, IntOrString>::type,
                     typename std::variant_alternative<1, IntOrString>::type>>::
        value,
    "Union must be a plain std::variant instantiation");

// match exhaustiveness: an arm set missing an alternative is not invocable
// with that alternative, which is exactly what makes baml::Match (std::visit
// underneath) reject it at compile time.
const auto int_only = [](int64_t) { return 0; };
using IntOnlyArms = baml::detail::Overloaded<std::decay_t<decltype(int_only)>>;
static_assert(std::is_invocable<IntOnlyArms, int64_t>::value,
              "sanity: the int arm must accept int64_t");
static_assert(!std::is_invocable<IntOnlyArms, const std::string&>::value,
              "a Match missing the string arm must not compile");

// The const auto& catch-all restores invocability for every alternative.
const auto catch_all = [](const auto&) { return 0; };
using WithCatchAll =
    baml::detail::Overloaded<std::decay_t<decltype(int_only)>,
                             std::decay_t<decltype(catch_all)>>;
static_assert(std::is_invocable<WithCatchAll, const std::string&>::value,
              "a const auto& arm must catch remaining alternatives");

}  // namespace
