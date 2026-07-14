#ifndef BAML_TY_H_
#define BAML_TY_H_

// Ty<T>: compile-time reflection of a C++ SDK type into its BamlTy wire form
// (baml_type.proto). Explicit generic bindings ride CallFunctionArgs.type_args
// as (TypeVar name, BamlTy) pairs; generated code adds specializations for
// its classes and enums. Each Encode() writes a BamlTy message BODY.

#include <baml/box.h>
#include <baml/detail/wire.h>

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <variant>
#include <vector>

namespace baml {

template <typename T>
struct Ty;  // primary template intentionally undefined

namespace detail {

// BamlTy.primitive = 1 { kind = 1 } with BamlTyPrimitiveKind values.
inline void WritePrimitiveTy(wire::Writer& ty_msg, uint64_t kind) {
  wire::Writer primitive;
  primitive.Uint64Field(1, kind);
  ty_msg.MessageField(1, primitive);
}

}  // namespace detail

template <>
struct Ty<std::string> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 1);
  }
};
template <>
struct Ty<int64_t> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 2);
  }
};
template <>
struct Ty<double> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 3);
  }
};
template <>
struct Ty<bool> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 4);
  }
};
template <>
struct Ty<std::monostate> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 5);
  }
};
template <>
struct Ty<std::vector<uint8_t>> {
  static void Encode(detail::wire::Writer& m) {
    detail::WritePrimitiveTy(m, 6);
  }
};

template <typename T>
struct Ty<std::vector<T>> {
  static void Encode(detail::wire::Writer& m) {
    detail::wire::Writer list;  // BamlTyList { item = 1 }
    detail::wire::Writer item;
    Ty<T>::Encode(item);
    list.MessageField(1, item);
    m.MessageField(4, list);  // BamlTy.list
  }
};

template <typename T>
struct Ty<std::map<std::string, T>> {
  static void Encode(detail::wire::Writer& m) {
    detail::wire::Writer map_ty;  // BamlTyMap { key = 1, value = 2 }
    detail::wire::Writer key;
    Ty<std::string>::Encode(key);
    map_ty.MessageField(1, key);
    detail::wire::Writer value;
    Ty<T>::Encode(value);
    map_ty.MessageField(2, value);
    m.MessageField(5, map_ty);  // BamlTy.map
  }
};

template <typename T>
struct Ty<std::optional<T>> {
  static void Encode(detail::wire::Writer& m) {
    detail::wire::Writer opt;  // BamlTyOptional { inner = 1 }
    detail::wire::Writer inner;
    Ty<T>::Encode(inner);
    opt.MessageField(1, inner);
    m.MessageField(6, opt);  // BamlTy.optional
  }
};

template <typename... Ts>
struct Ty<std::variant<Ts...>> {
  static void Encode(detail::wire::Writer& m) {
    detail::wire::Writer union_ty;  // BamlTyUnion { options = 1 }
    (EncodeOption<Ts>(union_ty), ...);
    m.MessageField(7, union_ty);  // BamlTy.union
  }

 private:
  template <typename T>
  static void EncodeOption(detail::wire::Writer& union_ty) {
    detail::wire::Writer option;
    Ty<T>::Encode(option);
    union_ty.MessageField(1, option);
  }
};

// Boxes are a C++ recursion artifact, invisible at the type level.
template <typename T>
struct Ty<Box<T>> {
  static void Encode(detail::wire::Writer& m) { Ty<T>::Encode(m); }
};

template <typename T>
struct Ty<OptionalBox<T>> {
  static void Encode(detail::wire::Writer& m) {
    detail::wire::Writer opt;  // BamlTyOptional { inner = 1 }
    detail::wire::Writer inner;
    Ty<T>::Encode(inner);
    opt.MessageField(1, inner);
    m.MessageField(6, opt);  // BamlTy.optional
  }
};

}  // namespace baml

#endif  // BAML_TY_H_
