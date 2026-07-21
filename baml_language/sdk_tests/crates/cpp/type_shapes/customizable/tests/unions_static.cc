// Static shape checks for the baml::variant surface (no python analog;
// documents the compile-time contract the way optional_args_static.cc does
// for opts structs). Each "must not compile" shape is pinned as a negative
// trait assertion rather than a commented-out snippet.
#include <baml_sdk.h>

#include <string>
#include <type_traits>
#include <variant>

namespace {

using IntOrString = baml::variant<int64_t, std::string>;

// Canonicalization is total: every permutation of an alternative set is
// one instantiation, including through nesting in std containers.
static_assert(std::is_same<baml::variant<int64_t, std::string>,
                           baml::variant<std::string, int64_t>>::value,
              "baml::variant must be order-canonical");
static_assert(
    std::is_same<std::optional<baml::variant<int64_t, std::string>>,
                 std::optional<baml::variant<std::string, int64_t>>>::value,
    "canonicalization must hold under std::optional");
static_assert(
    std::is_same<std::vector<baml::variant<int64_t, std::string>>,
                 std::vector<baml::variant<std::string, int64_t>>>::value,
    "canonicalization must hold under std::vector");

// Literal types are ordinary alternatives: the BAML_LIT macro family hits
// the canonical char-pack / normalized-scalar instantiations, and Lit
// unions canonicalize like any other.
static_assert(
    std::is_same<BAML_LIT("draft"), baml::lit<'d', 'r', 'a', 'f', 't'>>::value,
    "BAML_LIT must produce the canonical char pack");
static_assert(std::is_same<BAML_LIT(1), baml::lit<int64_t{1}>>::value,
              "BAML_LIT must normalize ints to int64_t");
static_assert(std::is_same<BAML_LIT(1), baml::int_lit<1>>::value,
              "int_lit must agree with BAML_LIT");
static_assert(std::is_same<baml::variant<BAML_LIT("a"), BAML_LIT("b")>,
                           baml::variant<BAML_LIT("b"), BAML_LIT("a")>>::value,
              "Lit unions must be order-canonical");

// Alternatives are a set, not a list: duplicates collapse, so type-based
// construction and access are never ambiguous.
static_assert(std::is_same<baml::variant<int64_t, int64_t>,
                           baml::variant<int64_t>>::value,
              "baml::variant must deduplicate repeated alternatives");
static_assert(std::is_same<baml::variant<int64_t, std::string, int64_t>,
                           baml::variant<std::string, int64_t>>::value,
              "deduplication must compose with order canonicalization");

// A baml::variant IS a std::variant instantiation (alias, not a wrapper).
static_assert(
    std::is_same<
        IntOrString,
        std::variant<typename std::variant_alternative<0, IntOrString>::type,
                     typename std::variant_alternative<1, IntOrString>::type>>::
        value,
    "baml::variant must be a plain std::variant instantiation");

// match exhaustiveness: an arm set missing an alternative is not invocable
// with that alternative, which is exactly what makes baml::match (std::visit
// underneath) reject it at compile time.
const auto int_only = [](int64_t) { return 0; };
using IntOnlyArms = baml::detail::overloaded<std::decay_t<decltype(int_only)>>;
static_assert(std::is_invocable<IntOnlyArms, int64_t>::value,
              "sanity: the int arm must accept int64_t");
static_assert(!std::is_invocable<IntOnlyArms, const std::string&>::value,
              "a match missing the string arm must not compile");

// The const auto& catch-all restores invocability for every alternative.
const auto catch_all = [](const auto&) { return 0; };
using WithCatchAll =
    baml::detail::overloaded<std::decay_t<decltype(int_only)>,
                             std::decay_t<decltype(catch_all)>>;
static_assert(std::is_invocable<WithCatchAll, const std::string&>::value,
              "a const auto& arm must catch remaining alternatives");

}  // namespace
