#ifndef BAML_UNION_H_
#define BAML_UNION_H_

// baml::Union<Ts...>: BAML's union type as an order-canonical
// std::variant.
//
// BAML unions are sets (`string | int` == `int | string`), but
// std::variant<A, B> and std::variant<B, A> are distinct C++ types.
// baml::Union sorts its alternatives at compile time (by a per-type
// constexpr name), so every spelling of the same alternative set resolves
// to the SAME std::variant instantiation. It is an alias, not a wrapper
// class: a baml::Union value is a plain std::variant and works with
// std::get, std::holds_alternative, std::visit, and every other variant
// API. (Lowercase `union` is a C++ keyword; Union follows the guide's
// type-alias casing, while match mirrors std::visit per the vocabulary
// rule in STYLE.md.)
//
// baml::match(u, arms...) is the reading companion: one callable per
// alternative, dispatched by TYPE (never by index, which is meaningless
// under canonical ordering). std::visit enforces exhaustiveness at compile
// time; a `[](const auto&) { ... }` arm is the explicit catch-all.
//
// Nullability is NOT spelled with Union: `T | null` is std::optional<T>
// and `A | B | null` is std::optional<Union<A, B>> (see codec.h).

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
constexpr std::string_view TypeName() {
#if defined(_MSC_VER)
  return __FUNCSIG__;
#else
  return __PRETTY_FUNCTION__;
#endif
}

// Indices 0..N-1 sorted by type name (insertion sort: constexpr std::sort
// is C++20 and this is C++17).
template <class... Ts>
constexpr std::array<std::size_t, sizeof...(Ts)> SortedIndices() {
  std::array<std::string_view, sizeof...(Ts)> names{TypeName<Ts>()...};
  std::array<std::size_t, sizeof...(Ts)> idx{};
  for (std::size_t i = 0; i < idx.size(); ++i) idx[i] = i;
  for (std::size_t i = 1; i < idx.size(); ++i)
    for (std::size_t j = i; j > 0 && names[idx[j]] < names[idx[j - 1]]; --j) {
      std::size_t tmp = idx[j];
      idx[j] = idx[j - 1];
      idx[j - 1] = tmp;
    }
  return idx;
}

// Reorders the pack into canonical order via the sorted indices.
template <class... Ts>
struct CanonSort {
  static constexpr std::array<std::size_t, sizeof...(Ts)> idx =
      SortedIndices<Ts...>();
  template <std::size_t... Is>
  static std::variant<std::tuple_element_t<idx[Is], std::tuple<Ts...>>...>
      Helper(std::index_sequence<Is...>);
  using type = decltype(Helper(std::make_index_sequence<sizeof...(Ts)>{}));
};

}  // namespace detail

// Alias, not a class: every alternative-set spelling resolves to the same
// std::variant instantiation.
template <class... Ts>
using Union = typename detail::CanonSort<Ts...>::type;

namespace detail {

template <class... Fs>
struct Overloaded : Fs... {
  using Fs::operator()...;
};
template <class... Fs>
Overloaded(Fs...) -> Overloaded<Fs...>;

}  // namespace detail

// Pattern matching over a baml::Union (or any std::variant): one callable per
// alternative, dispatched by type. A missing arm is a compile error;
// `[](const auto&) { ... }` is the explicit catch-all (spell the parameter
// `const auto&`, not `auto&&`, or it out-competes the exact arms).
template <class V, class... Fs>
decltype(auto) match(V&& v, Fs&&... fs) {
  return std::visit(detail::Overloaded{std::forward<Fs>(fs)...},
                    std::forward<V>(v));
}

}  // namespace baml

#endif  // BAML_UNION_H_
