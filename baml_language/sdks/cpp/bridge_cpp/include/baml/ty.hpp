#ifndef BAML_TY_HPP
#define BAML_TY_HPP

// ty<T>: compile-time reflection of a C++ SDK type into its BamlTy wire form
// (baml_type.proto). Explicit generic bindings ride CallFunctionArgs.type_args
// as (TypeVar name, BamlTy) pairs; generated code adds specializations for
// its classes and enums. Each encode() writes a BamlTy message BODY.

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <variant>
#include <vector>

#include <baml/box.hpp>
#include <baml/detail/wire.hpp>

namespace baml {

template <typename T>
struct ty;  // primary template intentionally undefined

namespace detail {

// BamlTy.primitive = 1 { kind = 1 } with BamlTyPrimitiveKind values.
inline void write_primitive_ty(wire::Writer& ty_msg, uint64_t kind) {
    wire::Writer primitive;
    primitive.uint64_field(1, kind);
    ty_msg.message_field(1, primitive);
}

}  // namespace detail

template <>
struct ty<std::string> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 1); }
};
template <>
struct ty<int64_t> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 2); }
};
template <>
struct ty<double> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 3); }
};
template <>
struct ty<bool> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 4); }
};
template <>
struct ty<std::monostate> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 5); }
};
template <>
struct ty<std::vector<uint8_t>> {
    static void encode(detail::wire::Writer& m) { detail::write_primitive_ty(m, 6); }
};

template <typename T>
struct ty<std::vector<T>> {
    static void encode(detail::wire::Writer& m) {
        detail::wire::Writer list;  // BamlTyList { item = 1 }
        detail::wire::Writer item;
        ty<T>::encode(item);
        list.message_field(1, item);
        m.message_field(4, list);  // BamlTy.list
    }
};

template <typename T>
struct ty<std::map<std::string, T>> {
    static void encode(detail::wire::Writer& m) {
        detail::wire::Writer map_ty;  // BamlTyMap { key = 1, value = 2 }
        detail::wire::Writer key;
        ty<std::string>::encode(key);
        map_ty.message_field(1, key);
        detail::wire::Writer value;
        ty<T>::encode(value);
        map_ty.message_field(2, value);
        m.message_field(5, map_ty);  // BamlTy.map
    }
};

template <typename T>
struct ty<std::optional<T>> {
    static void encode(detail::wire::Writer& m) {
        detail::wire::Writer opt;  // BamlTyOptional { inner = 1 }
        detail::wire::Writer inner;
        ty<T>::encode(inner);
        opt.message_field(1, inner);
        m.message_field(6, opt);  // BamlTy.optional
    }
};

template <typename... Ts>
struct ty<std::variant<Ts...>> {
    static void encode(detail::wire::Writer& m) {
        detail::wire::Writer union_ty;  // BamlTyUnion { options = 1 }
        (encode_option<Ts>(union_ty), ...);
        m.message_field(7, union_ty);  // BamlTy.union
    }

private:
    template <typename T>
    static void encode_option(detail::wire::Writer& union_ty) {
        detail::wire::Writer option;
        ty<T>::encode(option);
        union_ty.message_field(1, option);
    }
};

// Boxes are a C++ recursion artifact, invisible at the type level.
template <typename T>
struct ty<Box<T>> {
    static void encode(detail::wire::Writer& m) { ty<T>::encode(m); }
};

template <typename T>
struct ty<OptionalBox<T>> {
    static void encode(detail::wire::Writer& m) {
        detail::wire::Writer opt;  // BamlTyOptional { inner = 1 }
        detail::wire::Writer inner;
        ty<T>::encode(inner);
        opt.message_field(1, inner);
        m.message_field(6, opt);  // BamlTy.optional
    }
};

}  // namespace baml

#endif  // BAML_TY_HPP
