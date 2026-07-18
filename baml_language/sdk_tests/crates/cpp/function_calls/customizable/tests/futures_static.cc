// Static shape checks for the baml::future surface (no python analog;
// documents the compile-time contract the way unions_static.cc does for
// baml::variant).
#include <baml_sdk.h>

#include <cstdint>
#include <string>
#include <type_traits>

namespace {

using int_future = baml::future<int64_t, baml::variant<std::string, int64_t>>;

// std::future semantics: single consumer, move-only.
static_assert(std::is_move_constructible<int_future>::value,
              "baml::future must be movable");
static_assert(std::is_move_assignable<int_future>::value,
              "baml::future must be move-assignable");
static_assert(!std::is_copy_constructible<int_future>::value,
              "baml::future must not be copyable");
static_assert(!std::is_copy_assignable<int_future>::value,
              "baml::future must not be copy-assignable");

// The ThrownU parameter rides baml::variant canonicalization: a reordered
// spelling of the throws set cannot mint a second Future type.
static_assert(
    std::is_same<
        baml::future<int64_t, baml::variant<std::string, int64_t>>,
        baml::future<int64_t, baml::variant<int64_t, std::string>>>::value,
    "Future's throws union must be order-canonical");

}  // namespace
