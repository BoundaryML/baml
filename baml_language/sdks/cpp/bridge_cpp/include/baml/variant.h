#ifndef BAML_VARIANT_H_
#define BAML_VARIANT_H_

// baml::variant<Ts...>: BAML's union type as an order-canonical
// std::variant.
//
// BAML unions are sets (`string | int` == `int | string`, `int | int` ==
// `int`), but std::variant<A, B> and std::variant<B, A> are distinct C++
// types. baml::variant sorts its alternatives at compile time (by a per-type
// constexpr name) and drops duplicates, so every spelling of the same
// alternative set resolves to the SAME std::variant instantiation. It is an
// alias, not a wrapper class: a baml::variant value is a plain std::variant and
// works with std::get, std::holds_alternative, std::visit, and every other
// variant API. (Lowercase `union` is a C++ keyword; variant follows the guide's
// type-alias casing, while match mirrors std::visit per the vocabulary
// rule in STYLE.md.)
//
// baml::match(u, arms...) is the reading companion: one callable per
// alternative, dispatched by TYPE (never by index, which is meaningless
// under canonical ordering). std::visit enforces exhaustiveness at compile
// time; a `[](const auto&) { ... }` arm is the explicit catch-all.
//
// Nullability is NOT spelled with variant: `T | null` is std::optional<T>
// and `A | B | null` is std::optional<variant<A, B>> (see codec.h).

#include <array>
#include <cstddef>
#include <string_view>
#include <tuple>
#include <utility>
#include <variant>

namespace baml {
namespace detail {

// Unique compile-time name per type (the canonical-sort key). The pretty
// function name embeds the fully-qualified type; uniqueness and a stable
// order per compiler are all that matters, not the exact spelling.
template <class T>
constexpr std::string_view type_name() {
#if defined(_MSC_VER)
  return __FUNCSIG__;
#else
  return __PRETTY_FUNCTION__;
#endif
}

// Indices 0..N-1 sorted by type name and then deduplicated (equal names
// are adjacent after the sort, and a name is unique per type). Surviving
// indices are packed at the front with `count` saying how many; a constexpr
// function cannot return a runtime-sized array. Insertion sort because
// constexpr std::sort is C++20 and this is C++17.
template <std::size_t N>
struct canon_indices {
  std::array<std::size_t, N> idx;
  std::size_t count;
};

template <class... Ts>
constexpr canon_indices<sizeof...(Ts)> sorted_unique_indices() {
  std::array<std::string_view, sizeof...(Ts)> names{type_name<Ts>()...};
  std::array<std::size_t, sizeof...(Ts)> idx{};
  for (std::size_t i = 0; i < idx.size(); ++i) idx[i] = i;
  for (std::size_t i = 1; i < idx.size(); ++i)
    for (std::size_t j = i; j > 0 && names[idx[j]] < names[idx[j - 1]]; --j) {
      std::size_t tmp = idx[j];
      idx[j] = idx[j - 1];
      idx[j - 1] = tmp;
    }
  std::size_t count = 0;
  for (std::size_t i = 0; i < idx.size(); ++i)
    if (i == 0 || names[idx[i]] != names[idx[i - 1]]) idx[count++] = idx[i];
  return {idx, count};
}

// Reorders the pack into canonical order via the sorted, deduplicated
// indices.
template <class... Ts>
struct canon_sort {
  static constexpr canon_indices<sizeof...(Ts)> canon =
      sorted_unique_indices<Ts...>();
  using source_types = std::tuple<Ts...>;
  template <std::size_t I>
  using canonical_type = std::tuple_element_t<canon.idx[I], source_types>;
  template <std::size_t... Is>
  static std::variant<canonical_type<Is>...> helper(std::index_sequence<Is...>);
  using type = decltype(helper(std::make_index_sequence<canon.count>{}));
};

}  // namespace detail

// Alias, not a class: every alternative-set spelling resolves to the same
// std::variant instantiation.
template <class... Ts>
using variant = typename detail::canon_sort<Ts...>::type;

namespace detail {

template <class... Fs>
struct overloaded : Fs... {
  using Fs::operator()...;
};
template <class... Fs>
overloaded(Fs...) -> overloaded<Fs...>;

}  // namespace detail

// Pattern matching over a baml::variant (or any std::variant): one callable per
// alternative, dispatched by type. A missing arm is a compile error;
// `[](const auto&) { ... }` is the explicit catch-all (spell the parameter
// `const auto&`, not `auto&&`, or it out-competes the exact arms).
template <class V, class... Fs>
decltype(auto) match(V&& v, Fs&&... fs) {
  return std::visit(detail::overloaded{std::forward<Fs>(fs)...},
                    std::forward<V>(v));
}

}  // namespace baml

#endif  // BAML_VARIANT_H_
