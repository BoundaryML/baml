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
// 256 characters. Generated code never uses the macro: the emitter spells
// the char packs directly, at any length.
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
//
// Possible future representation: identity as an FNV-1a-64 hash NTTP
// (Lit<0x...>) with an emitter-generated hash->string side-table. That
// would delete the char-pack macro machinery and the length cap, at the
// cost of unreadable literal types in compiler diagnostics and no ::value
// for off-schema spellings. Same user-facing API either way; revisit if
// the char packs become a problem in practice.

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
      "BAML_LIT(\"...\") for strings; BAML_LIT(n) / baml::IntLit<n> for ints "
      "(a bare Lit<1> deduces `int`, not int64_t, which would mint a second "
      "type); BAML_LIT(b) / baml::BoolLit<b> for bools; BAML_LIT(E::V) / "
      "baml::Lit<E::V> for enum variants");
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
                "string literal exceeds BAML_LIT's 256-character cap (name "
                "the type via decltype of the generated function instead)");
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
// BAML_LIT(42) and BAML_LIT(int64_t{42}) are the same type. An unsigned
// value above INT64_MAX must not wrap into an aliased identity
// (BAML_LIT(UINT64_MAX) == BAML_LIT(-1)); reaching the throw during
// constant evaluation makes the call non-constant, so the out-of-range
// spelling fails to compile with this message in the diagnostic.
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
  return std::is_signed<T>::value ||
                 static_cast<uint64_t>(v) <= static_cast<uint64_t>(INT64_MAX)
             ? static_cast<int64_t>(v)
             : throw "BAML int literals must be representable in int64_t";
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
#define BAML_DETAIL_LIT_CH8(s, i)                                            \
      BAML_DETAIL_LIT_CH(s, (i) + 0), BAML_DETAIL_LIT_CH(s, (i) + 1),        \
      BAML_DETAIL_LIT_CH(s, (i) + 2), BAML_DETAIL_LIT_CH(s, (i) + 3),        \
      BAML_DETAIL_LIT_CH(s, (i) + 4), BAML_DETAIL_LIT_CH(s, (i) + 5),        \
      BAML_DETAIL_LIT_CH(s, (i) + 6), BAML_DETAIL_LIT_CH(s, (i) + 7)

#define BAML_DETAIL_LIT_CH64(s, i)                                           \
      BAML_DETAIL_LIT_CH8(s, (i) + 0),  BAML_DETAIL_LIT_CH8(s, (i) + 8),     \
      BAML_DETAIL_LIT_CH8(s, (i) + 16), BAML_DETAIL_LIT_CH8(s, (i) + 24),    \
      BAML_DETAIL_LIT_CH8(s, (i) + 32), BAML_DETAIL_LIT_CH8(s, (i) + 40),    \
      BAML_DETAIL_LIT_CH8(s, (i) + 48), BAML_DETAIL_LIT_CH8(s, (i) + 56)

// The 256-character cap applies to this macro only, never to generated
// code (the emitter spells char packs at any length). Realistically it
// should never be an issue: literal types are short tag strings, and a
// 256+ character literal type is a pathological schema. If one ever
// exists, decltype(the_generated_function()) names its type without the
// macro, and a `const auto&` match arm catches it.
#define BAML_LIT(s)                                                          \
  ::baml::detail::LitSelect<::baml::detail::LitArgOf(s),                     \
      ::baml::detail::LitValueOf(s), sizeof(s),                              \
      BAML_DETAIL_LIT_CH64(s, 0),   BAML_DETAIL_LIT_CH64(s, 64),             \
      BAML_DETAIL_LIT_CH64(s, 128), BAML_DETAIL_LIT_CH64(s, 192)>::type
// clang-format on

#endif  // BAML_LIT_H_
