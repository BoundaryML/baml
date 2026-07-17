#ifndef BAML_LIT_H_
#define BAML_LIT_H_

// baml::Lit<Vs...>: BAML literal types as distinct C++ types (C++17).
//
// BAML literal types are singleton value types: `"draft"`, `42`, `true`,
// and enum-variant types like `Sentiment.Positive`. Each distinct value is
// a distinct C++ type, so literal unions dispatch and exhaustively match
// at compile time exactly like any other baml::Union:
//
//   // status "draft" | "sent" | "paid"
//   baml::match(invoice.status,
//     [](BAML_LIT("draft")) { ... },
//     [](BAML_LIT("sent"))  { ... },
//     [](BAML_LIT("paid"))  { ... });
//
// One macro spells them all -- overloaded constexpr helpers classify the
// argument and normalize its value at compile time:
//   BAML_LIT("draft")              string  -> Lit<'d','r','a','f','t'>
//   BAML_LIT(42)                   int     -> Lit<int64_t{42}>
//   BAML_LIT(true)                 bool    -> Lit<true>
//   BAML_LIT(Sentiment::Positive)  enum    -> Lit<Sentiment::Positive>
//
// C++17 cannot pass a string literal as a template argument (class-type
// NTTPs are C++20), so BAML_LIT explodes the literal into a char pack via
// constant-expression indexing (the Boost.Metaparse technique), capped at
// 64 characters. Generated code never uses the macro: the emitter spells
// the char packs directly.
//
// A bare integer must NOT be spelled Lit<1>: template identity includes
// the parameter's TYPE, and `1` deduces `int`, minting a type distinct
// from the canonical Lit<int64_t{1}>. The shape check rejects it with a
// pointer to BAML_LIT(1), whose value normalization canonicalizes the
// type. (baml::IntLit<1> / baml::BoolLit<true> are macro-free alternates
// with the same normalization.)
//
// Every Lit carries its value statically: `decltype(x)::value` (also
// implicitly convertible), so a catch-all match arm can recover the
// runtime value: `match(u, [](auto l) { return decltype(l)::value; })`.

#include <cstddef>
#include <cstdint>
#include <string_view>
#include <type_traits>
#include <utility>

namespace baml {
namespace detail {

enum class LitShape { kInvalid, kString, kInt, kBool, kEnum };

// A plain alias (template <class T, class...> using Head = T) does not
// survive pack expansion into its fixed parameter on clang; a struct
// specialization does.
template <class... Ts>
struct LitHead;
template <class T, class... Rest>
struct LitHead<T, Rest...> {
  using type = T;
};

template <class... Ts>
constexpr LitShape LitShapeOf() {
  if constexpr ((std::is_same<Ts, char>::value && ...)) {
    // Includes the empty pack: BAML_LIT("") is the empty-string type.
    return LitShape::kString;
  } else if constexpr (sizeof...(Ts) == 1) {
    using T = typename LitHead<Ts...>::type;
    if (std::is_same<T, int64_t>::value) return LitShape::kInt;
    if (std::is_same<T, bool>::value) return LitShape::kBool;
    if (std::is_enum<T>::value) return LitShape::kEnum;
    return LitShape::kInvalid;
  } else {
    return LitShape::kInvalid;
  }
}

template <LitShape S, auto... Vs>
struct LitBase {
  static_assert(
      S != LitShape::kInvalid,
      "unsupported baml::Lit shape. The four literal spellings: "
      "BAML_LIT(\"...\") for strings; BAML_LIT_INT(n) / baml::IntLit<n> for "
      "ints (a bare integer deduces `int`, not int64_t, which would mint a "
      "second type); BAML_LIT_BOOL(b) / baml::Lit<true> for bools; "
      "BAML_LIT_ENUM(E::V) / baml::Lit<E::V> for enum variants");
};

template <auto... Cs>
struct LitBase<LitShape::kString, Cs...> {
  static constexpr char chars[] = {static_cast<char>(Cs)..., '\0'};
  static constexpr std::string_view value{chars, sizeof...(Cs)};
  constexpr operator std::string_view() const { return value; }
};

template <auto V>
struct LitBase<LitShape::kInt, V> {
  static constexpr int64_t value = V;
  constexpr operator int64_t() const { return value; }
};

template <auto V>
struct LitBase<LitShape::kBool, V> {
  static constexpr bool value = V;
  constexpr operator bool() const { return value; }
};

template <auto V>
struct LitBase<LitShape::kEnum, V> {
  static constexpr decltype(V) value = V;
  constexpr operator decltype(V)() const { return value; }
};

}  // namespace detail

template <auto... Vs>
struct Lit : detail::LitBase<detail::LitShapeOf<decltype(Vs)...>(), Vs...> {
  // Unit type: two values of the same Lit are always equal (memberwise
  // equality of generated structs and variant equality both rely on this).
  friend constexpr bool operator==(Lit, Lit) { return true; }
  friend constexpr bool operator!=(Lit, Lit) { return false; }
};

template <int64_t V>
using IntLit = Lit<V>;

template <bool V>
using BoolLit = Lit<V>;

namespace detail {

template <class T>
struct IsLit : std::false_type {};
template <auto... Vs>
struct IsLit<Lit<Vs...>> : std::true_type {};

// Strips the '\0' padding BAML_LIT's fixed-arity expansion appends, so
// every spelling of the same string lands on the same Lit instantiation.
// N is sizeof(literal) for the cap check; BAML strings never contain
// embedded NULs, so the first '\0' is the end.
template <std::size_t N, char... Cs>
struct TrimNulls {
  static_assert(N <= sizeof...(Cs) + 1,
                "string literal exceeds BAML_LIT's 64-character cap");
  static constexpr char arr[] = {Cs...};
  static constexpr std::size_t Length() {
    std::size_t n = 0;
    while (n < sizeof...(Cs) && arr[n] != '\0') ++n;
    return n;
  }
  template <std::size_t... Is>
  static Lit<arr[Is]...> Pick(std::index_sequence<Is...>);
  using type = decltype(Pick(std::make_index_sequence<Length()>{}));
};

// BAML_LIT(x) argument classification: overload resolution IS the
// dispatch. Every helper is called in template-argument position, so x
// must be a literal / constant expression. Bare `char` is deliberately
// not a literal shape (BAML has no char type; spell the one-char string).

enum class LitArg { kString, kScalar };

template <class T>
constexpr bool kLitScalarArg =
    (std::is_integral<T>::value && !std::is_same<T, bool>::value &&
     !std::is_same<T, char>::value) ||
    std::is_enum<T>::value;

template <std::size_t N>
constexpr LitArg LitArgOf(const char (&)[N]) {
  return LitArg::kString;
}
constexpr LitArg LitArgOf(bool) { return LitArg::kScalar; }
template <class T, std::enable_if_t<kLitScalarArg<T>, int> = 0>
constexpr LitArg LitArgOf(T) {
  return LitArg::kScalar;
}

// The scalar value, normalized: every integral type lands on int64_t so
// BAML_LIT(42) and BAML_LIT(int64_t{42}) are the same type.
template <std::size_t N>
constexpr int64_t LitValueOf(const char (&)[N]) {
  return 0;  // placeholder; the string path never reads it
}
constexpr bool LitValueOf(bool v) { return v; }
template <class T, std::enable_if_t<std::is_integral<T>::value &&
                                        !std::is_same<T, bool>::value &&
                                        !std::is_same<T, char>::value,
                                    int> = 0>
constexpr int64_t LitValueOf(T v) {
  return static_cast<int64_t>(v);
}
template <class T, std::enable_if_t<std::is_enum<T>::value, int> = 0>
constexpr T LitValueOf(T v) {
  return v;
}

template <std::size_t N>
constexpr char LitCharAt(const char (&s)[N], std::size_t i) {
  return i < N ? s[i] : '\0';
}
template <class T, std::enable_if_t<!std::is_array<T>::value, int> = 0>
constexpr char LitCharAt(const T&, std::size_t) {
  return '\0';
}

template <LitArg K, auto V, std::size_t N, char... Cs>
struct LitSelect;
template <auto V, std::size_t N, char... Cs>
struct LitSelect<LitArg::kString, V, N, Cs...> : TrimNulls<N, Cs...> {};
template <auto V, std::size_t N, char... Cs>
struct LitSelect<LitArg::kScalar, V, N, Cs...> {
  using type = Lit<V>;
};

}  // namespace detail
}  // namespace baml

#define BAML_DETAIL_LIT_CH(s, i) (::baml::detail::LitCharAt((s), (i)))

// clang-format off
#define BAML_LIT(s)                                                          \
  ::baml::detail::LitSelect<::baml::detail::LitArgOf(s),                     \
      ::baml::detail::LitValueOf(s), sizeof(s),                              \
      BAML_DETAIL_LIT_CH(s, 0),  BAML_DETAIL_LIT_CH(s, 1),                   \
      BAML_DETAIL_LIT_CH(s, 2),  BAML_DETAIL_LIT_CH(s, 3),                   \
      BAML_DETAIL_LIT_CH(s, 4),  BAML_DETAIL_LIT_CH(s, 5),                   \
      BAML_DETAIL_LIT_CH(s, 6),  BAML_DETAIL_LIT_CH(s, 7),                   \
      BAML_DETAIL_LIT_CH(s, 8),  BAML_DETAIL_LIT_CH(s, 9),                   \
      BAML_DETAIL_LIT_CH(s, 10), BAML_DETAIL_LIT_CH(s, 11),                  \
      BAML_DETAIL_LIT_CH(s, 12), BAML_DETAIL_LIT_CH(s, 13),                  \
      BAML_DETAIL_LIT_CH(s, 14), BAML_DETAIL_LIT_CH(s, 15),                  \
      BAML_DETAIL_LIT_CH(s, 16), BAML_DETAIL_LIT_CH(s, 17),                  \
      BAML_DETAIL_LIT_CH(s, 18), BAML_DETAIL_LIT_CH(s, 19),                  \
      BAML_DETAIL_LIT_CH(s, 20), BAML_DETAIL_LIT_CH(s, 21),                  \
      BAML_DETAIL_LIT_CH(s, 22), BAML_DETAIL_LIT_CH(s, 23),                  \
      BAML_DETAIL_LIT_CH(s, 24), BAML_DETAIL_LIT_CH(s, 25),                  \
      BAML_DETAIL_LIT_CH(s, 26), BAML_DETAIL_LIT_CH(s, 27),                  \
      BAML_DETAIL_LIT_CH(s, 28), BAML_DETAIL_LIT_CH(s, 29),                  \
      BAML_DETAIL_LIT_CH(s, 30), BAML_DETAIL_LIT_CH(s, 31),                  \
      BAML_DETAIL_LIT_CH(s, 32), BAML_DETAIL_LIT_CH(s, 33),                  \
      BAML_DETAIL_LIT_CH(s, 34), BAML_DETAIL_LIT_CH(s, 35),                  \
      BAML_DETAIL_LIT_CH(s, 36), BAML_DETAIL_LIT_CH(s, 37),                  \
      BAML_DETAIL_LIT_CH(s, 38), BAML_DETAIL_LIT_CH(s, 39),                  \
      BAML_DETAIL_LIT_CH(s, 40), BAML_DETAIL_LIT_CH(s, 41),                  \
      BAML_DETAIL_LIT_CH(s, 42), BAML_DETAIL_LIT_CH(s, 43),                  \
      BAML_DETAIL_LIT_CH(s, 44), BAML_DETAIL_LIT_CH(s, 45),                  \
      BAML_DETAIL_LIT_CH(s, 46), BAML_DETAIL_LIT_CH(s, 47),                  \
      BAML_DETAIL_LIT_CH(s, 48), BAML_DETAIL_LIT_CH(s, 49),                  \
      BAML_DETAIL_LIT_CH(s, 50), BAML_DETAIL_LIT_CH(s, 51),                  \
      BAML_DETAIL_LIT_CH(s, 52), BAML_DETAIL_LIT_CH(s, 53),                  \
      BAML_DETAIL_LIT_CH(s, 54), BAML_DETAIL_LIT_CH(s, 55),                  \
      BAML_DETAIL_LIT_CH(s, 56), BAML_DETAIL_LIT_CH(s, 57),                  \
      BAML_DETAIL_LIT_CH(s, 58), BAML_DETAIL_LIT_CH(s, 59),                  \
      BAML_DETAIL_LIT_CH(s, 60), BAML_DETAIL_LIT_CH(s, 61),                  \
      BAML_DETAIL_LIT_CH(s, 62), BAML_DETAIL_LIT_CH(s, 63)>::type
// clang-format on

#endif  // BAML_LIT_H_
