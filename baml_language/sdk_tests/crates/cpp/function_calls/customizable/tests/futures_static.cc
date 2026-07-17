// Static shape checks for the baml::Future surface (no python analog;
// documents the compile-time contract the way unions_static.cc does for
// baml::Union).
#include <baml_sdk.h>

#include <cstdint>
#include <string>
#include <type_traits>

namespace {

using IntFuture = baml::Future<int64_t, baml::Union<std::string, int64_t>>;

// std::future semantics: single consumer, move-only.
static_assert(std::is_move_constructible<IntFuture>::value,
              "baml::Future must be movable");
static_assert(std::is_move_assignable<IntFuture>::value,
              "baml::Future must be move-assignable");
static_assert(!std::is_copy_constructible<IntFuture>::value,
              "baml::Future must not be copyable");
static_assert(!std::is_copy_assignable<IntFuture>::value,
              "baml::Future must not be copy-assignable");

// The ThrownU parameter rides baml::Union canonicalization: a reordered
// spelling of the throws set cannot mint a second Future type.
static_assert(
    std::is_same<
        baml::Future<int64_t, baml::Union<std::string, int64_t>>,
        baml::Future<int64_t, baml::Union<int64_t, std::string>>>::value,
    "Future's throws union must be order-canonical");

}  // namespace
