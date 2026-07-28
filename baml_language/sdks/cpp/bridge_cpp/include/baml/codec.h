#ifndef BAML_CODEC_H_
#define BAML_CODEC_H_

// codec<T>: the typed boundary layer. `encode` fills an InboundValue
// message; `decode` converts a BamlOutboundValue into T, throwing error
// on arm mismatches. Generated code adds specializations for its
// classes/enums; this header owns the primitive and container instances.
//
// decode sees values through detail::unwrap (union metadata dropped) and
// widens literal values to their base scalar (Python parity).

#include <baml/box.h>
#include <baml/detail/host_value.h>
#include <baml/detail/proto.h>
#include <baml/errors.h>
#include <baml/lit.h>

#include <cstdint>
#include <cstdlib>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <type_traits>
#include <unordered_map>
#include <utility>
#include <variant>
#include <vector>

namespace baml {

template <typename T>
struct codec;  // primary template intentionally undefined

namespace detail {

template <typename Ret, typename ThrownU = void>
Ret call_handle_sync(uint64_t handle_key, args_encoder&& args);

[[noreturn]] inline void kind_mismatch(const char* expected,
                                       const pb::BamlOutboundValue& got) {
  throw error(std::string("BAML decode error: expected ") + expected +
              ", got " + arm_name(got.value_case()));
}

inline pb::BamlTy primitive_baml_ty(pb::BamlTyPrimitiveKind kind) {
  pb::BamlTy ty;
  ty.mutable_primitive()->set_kind(kind);
  return ty;
}

inline void annotate_selected_type(pb::InboundValue& value,
                                   const pb::BamlTy& selected_type) {
  if (value.has_value_type()) return;
  if (selected_type.ty_case() == pb::BamlTy::kUnion ||
      selected_type.ty_case() == pb::BamlTy::kOptional) {
    return;
  }
  value.mutable_value_type()->CopyFrom(selected_type);
}

inline bool selected_type_matches(const pb::BamlTy& expected,
                                  const pb::BamlTy& selected) {
  if (expected.SerializeAsString() == selected.SerializeAsString()) return true;
  return selected.ty_case() == pb::BamlTy::kOptional &&
         selected.optional().has_inner() &&
         selected_type_matches(expected, selected.optional().inner());
}

}  // namespace detail

namespace detail {

struct function_handle_state {
  explicit function_handle_state(uint64_t value) : key(value) {}
  ~function_handle_state() noexcept {
    if (key == 0) return;
    try {
      (void)api().handle_release(key);
    } catch (...) {
    }
  }

  uint64_t key;
};

}  // namespace detail

template <typename R, typename... Args>
struct codec<std::function<R(Args...)>> {
  static std::function<R(Args...)> decode(
      const detail::pb::BamlOutboundValue& raw) {
    const auto& value = detail::unwrap(raw);
    if (value.value_case() != detail::pb::BamlOutboundValue::kHandleValue ||
        value.handle_value().handle_type() != detail::pb::FUNCTION_REF ||
        value.handle_value().key() == 0) {
      detail::kind_mismatch("function", value);
    }
    auto state = std::make_shared<detail::function_handle_state>(
        value.handle_value().key());
    if (!value.handle_value().has_ty() ||
        value.handle_value().ty().ty_case() != detail::pb::BamlTy::kFunction) {
      throw error("BAML decode error: returned function has no function type");
    }
    const auto& function_ty = value.handle_value().ty().function();
    if (function_ty.params_size() != sizeof...(Args)) {
      throw error("BAML decode error: returned function arity mismatch");
    }
    auto names = std::make_shared<std::vector<std::string>>();
    names->reserve(function_ty.params_size());
    for (const auto& param : function_ty.params()) {
      if (!param.has_name() || param.name().empty()) {
        throw error(
            "BAML decode error: returned function parameter has no name");
      }
      names->push_back(param.name());
    }
    return [state, names](Args... args) -> R {
      detail::args_encoder encoded;
      std::size_t index = 0;
      (encoded.add_arg((*names)[index++],
                       [&args](detail::pb::InboundValue& target) {
                         codec<std::decay_t<Args>>::encode(target, args);
                       }),
       ...);
      if constexpr (std::is_void<R>::value) {
        detail::call_handle_sync<void>(state->key, std::move(encoded));
      } else {
        return detail::call_handle_sync<R>(state->key, std::move(encoded));
      }
    };
  }
};

template <>
struct codec<int64_t> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_INT);
  }
  static void encode(detail::pb::InboundValue& value_msg, int64_t v) {
    value_msg.set_int_value(v);
  }
  static int64_t decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kIntValue) {
      return v.int_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kIntValue) {
      return v.literal_value().int_value();
    }
    detail::kind_mismatch("int", v);
  }
};

template <>
struct codec<double> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_FLOAT);
  }
  static void encode(detail::pb::InboundValue& value_msg, double v) {
    value_msg.set_float_value(v);
  }
  static double decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    switch (v.value_case()) {
      case detail::pb::BamlOutboundValue::kFloatValue:
        return v.float_value();
      // Engine ints coerce to float when the declared type is float.
      case detail::pb::BamlOutboundValue::kIntValue:
        return static_cast<double>(v.int_value());
      case detail::pb::BamlOutboundValue::kLiteralValue:
        // Float literals ride as source text; malformed text surfaces as
        // a error, never a bare std:: exception.
        if (v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kFloatValue) {
          try {
            return std::stod(v.literal_value().float_value());
          } catch (const std::exception&) {
            throw error("BAML decode error: malformed float literal '" +
                        v.literal_value().float_value() + "'");
          }
        }
        break;
      default:
        break;
    }
    detail::kind_mismatch("float", v);
  }
};

template <>
struct codec<bool> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_BOOL);
  }
  static void encode(detail::pb::InboundValue& value_msg, bool v) {
    value_msg.set_bool_value(v);
  }
  static bool decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kBoolValue) {
      return v.bool_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kBoolValue) {
      return v.literal_value().bool_value();
    }
    detail::kind_mismatch("bool", v);
  }
};

template <>
struct codec<std::string> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_STRING);
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const std::string& v) {
    value_msg.set_string_value(v);
  }
  static std::string decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kStringValue) {
      return v.string_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kStringValue) {
      return v.literal_value().string_value();
    }
    detail::kind_mismatch("string", v);
  }
};

template <>
struct codec<std::vector<uint8_t>> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_BYTES);
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const std::vector<uint8_t>& v) {
    value_msg.set_uint8array_value(
        std::string(reinterpret_cast<const char*>(v.data()), v.size()));
  }
  static std::vector<uint8_t> decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kUint8ArrayValue) {
      detail::kind_mismatch("uint8array", v);
    }
    const std::string& bytes = v.uint8array_value();
    return std::vector<uint8_t>(bytes.begin(), bytes.end());
  }
};

template <>
struct codec<std::monostate> {
  static detail::pb::BamlTy baml_ty() {
    return detail::primitive_baml_ty(detail::pb::BAML_TY_PRIMITIVE_NULL);
  }
  static void encode(detail::pb::InboundValue&, std::monostate) {
    // BAML null = absent InboundValue oneof: set nothing.
  }
  static std::monostate decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kNullValue &&
        v.value_case() != detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      detail::kind_mismatch("null", v);
    }
    return std::monostate{};
  }
};

// Boxes are transparent on the wire: the box exists only to break C++
// type-recursion cycles.
template <typename T>
struct codec<box<T>> {
  static detail::pb::BamlTy baml_ty() { return codec<T>::baml_ty(); }
  static void encode(detail::pb::InboundValue& value_msg, const box<T>& v) {
    codec<T>::encode(value_msg, *v);
  }
  static box<T> decode(const detail::pb::BamlOutboundValue& v) {
    return box<T>(codec<T>::decode(v));
  }
};

template <typename T>
struct codec<optional_box<T>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_optional()->mutable_inner()->CopyFrom(codec<T>::baml_ty());
    return ty;
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const optional_box<T>& v) {
    if (v.has_value()) {
      codec<T>::encode(value_msg, *v);
    }
    // empty = BAML null = absent oneof: set nothing.
  }
  static optional_box<T> decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kNullValue ||
        v.value_case() == detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      return optional_box<T>();
    }
    return optional_box<T>(codec<T>::decode(raw));
  }
};

template <typename T>
struct codec<std::optional<T>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_optional()->mutable_inner()->CopyFrom(codec<T>::baml_ty());
    return ty;
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const std::optional<T>& v) {
    if (v.has_value()) {
      codec<T>::encode(value_msg, *v);
    }
    // nullopt = BAML null = absent oneof: set nothing.
  }
  static std::optional<T> decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kNullValue ||
        v.value_case() == detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      return std::nullopt;
    }
    return codec<T>::decode(raw);
  }
};

template <typename T>
struct codec<std::vector<T>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_list()->mutable_item()->CopyFrom(codec<T>::baml_ty());
    return ty;
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const std::vector<T>& v) {
    detail::pb::InboundListValue* list = value_msg.mutable_list_value();
    for (const T& item : v) {
      codec<T>::encode(*list->add_values(), item);
    }
    detail::annotate_selected_type(value_msg, baml_ty());
  }
  static std::vector<T> decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kListValue) {
      detail::kind_mismatch("list", v);
    }
    std::vector<T> out;
    out.reserve(static_cast<size_t>(v.list_value().items_size()));
    for (const auto& item : v.list_value().items()) {
      out.push_back(codec<T>::decode(item));
    }
    return out;
  }
};

template <typename T>
struct codec<std::unordered_map<std::string, T>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    auto* map = ty.mutable_map();
    map->mutable_key()->CopyFrom(codec<std::string>::baml_ty());
    map->mutable_value()->CopyFrom(codec<T>::baml_ty());
    return ty;
  }
  static void encode(detail::pb::InboundValue& value_msg,
                     const std::unordered_map<std::string, T>& v) {
    detail::pb::InboundMapValue* map = value_msg.mutable_map_value();
    for (const auto& entry : v) {
      detail::pb::InboundMapEntry* e = map->add_entries();
      e->set_string_key(entry.first);
      codec<T>::encode(*e->mutable_value(), entry.second);
    }
    detail::annotate_selected_type(value_msg, baml_ty());
  }
  static std::unordered_map<std::string, T> decode(
      const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kMapValue) {
      detail::kind_mismatch("map", v);
    }
    std::unordered_map<std::string, T> out;
    for (const auto& entry : v.map_value().entries()) {
      out.emplace(entry.key(), codec<T>::decode(entry.value()));
    }
    return out;
  }
};

// BAML literal types (baml::lit). encode writes the static value as its
// plain scalar/enum arm (the engine types the parameter). decode reuses
// the base codec for arm handling (plain and literal wire arms alike) and
// then requires the exact value: a mismatch throws, which inside a union
// rejects this alternative and lets the next one probe.
template <auto... Vs>
struct codec<lit<Vs...>> {
  using L = lit<Vs...>;
  static constexpr detail::lit_shape shape =
      detail::lit_shape_of<decltype(Vs)...>();

  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    auto* literal = ty.mutable_literal();
    if constexpr (shape == detail::lit_shape::string) {
      literal->set_string_value(std::string(L::value));
    } else if constexpr (shape == detail::lit_shape::integer) {
      literal->set_int_value(L::value);
    } else if constexpr (shape == detail::lit_shape::boolean) {
      literal->set_bool_value(L::value);
    } else {
      return codec<std::decay_t<decltype(L::value)>>::baml_ty();
    }
    return ty;
  }

  static void encode(detail::pb::InboundValue& value_msg, const L&) {
    if constexpr (shape == detail::lit_shape::string) {
      value_msg.set_string_value(std::string(L::value));
    } else if constexpr (shape == detail::lit_shape::integer) {
      value_msg.set_int_value(L::value);
    } else if constexpr (shape == detail::lit_shape::boolean) {
      value_msg.set_bool_value(L::value);
    } else {
      codec<std::decay_t<decltype(L::value)>>::encode(value_msg, L::value);
    }
  }

  static L decode(const detail::pb::BamlOutboundValue& raw) {
    if constexpr (shape == detail::lit_shape::string) {
      if (codec<std::string>::decode(raw) != L::value) {
        mismatch("literal \"" + std::string(L::value) + "\"");
      }
    } else if constexpr (shape == detail::lit_shape::integer) {
      if (codec<int64_t>::decode(raw) != L::value) {
        mismatch("literal " + std::to_string(L::value));
      }
    } else if constexpr (shape == detail::lit_shape::boolean) {
      if (codec<bool>::decode(raw) != L::value) {
        mismatch(L::value ? "literal true" : "literal false");
      }
    } else {
      using E = std::decay_t<decltype(L::value)>;
      if (codec<E>::decode(raw) != L::value) {
        mismatch("enum-variant literal");
      }
    }
    return L{};
  }

 private:
  [[noreturn]] static void mismatch(const std::string& expected) {
    throw error("BAML decode error: expected " + expected);
  }
};

// BAML unions (baml::variant<Ts...> = order-canonical std::variant). encode
// writes the ACTIVE alternative's value with no union wrapper (the engine
// types the member; Python parity). decode receives the union-unwrapped
// inner value and must pick an alternative from the concrete wire arm
// alone -- alternatives are in canonical (sorted) order, so selection is
// order-independent by construction:
//
//   pass 0 (literal): lit alternatives claim their exact values first, so
//     lit<"auto"> beats a std::string sibling for the string "auto";
//   pass 1 (strict): each non-lit alternative decodes only its exact wire
//     arms (int never satisfies a double alternative);
//   pass 2 (lenient): the engine's int->float coercion is admitted, for
//     unions with a float arm but no int arm.
//
// Class/enum alternatives dispatch precisely by wire FQN inside their own
// codecs; structurally ambiguous alternatives (two lists) resolve by
// probing elements.
template <class... Ts>
struct codec<std::variant<Ts...>> {
  using V = std::variant<Ts...>;

  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    auto* union_ty = ty.mutable_union_();
    (union_ty->add_options()->CopyFrom(codec<Ts>::baml_ty()), ...);
    return ty;
  }

  static void encode(detail::pb::InboundValue& value_msg, const V& v) {
    std::visit(
        [&value_msg](const auto& alt) {
          using T = std::decay_t<decltype(alt)>;
          codec<T>::encode(value_msg, alt);
          detail::annotate_selected_type(value_msg, codec<T>::baml_ty());
        },
        v);
  }

  static V decode(const detail::pb::BamlOutboundValue& raw) {
    if (raw.value_case() == detail::pb::BamlOutboundValue::kUnionVariantValue) {
      const auto& selected = raw.union_variant_value();
      if (selected.has_selected_option_index()) {
        if (!selected.has_self_type() ||
            selected.self_type().ty_case() != detail::pb::BamlTy::kUnion) {
          throw error(
              "BAML decode error: indexed union has no union self_type");
        }
        const auto& options = selected.self_type().union_().options();
        const auto index = selected.selected_option_index();
        if (index >= static_cast<uint32_t>(options.size())) {
          throw error(
              "BAML decode error: union selected option index is out of range");
        }
        return decode_selected(selected.value(), options.Get(index));
      }
    }
    const auto& v = detail::unwrap(raw);
    std::optional<V> out;
    const bool lit = (try_arm<Ts>(v, out, pass::literal) || ...);
    if (!lit) {
      const bool strict = (try_arm<Ts>(v, out, pass::strict) || ...);
      if (!strict) {
        (try_arm<Ts>(v, out, pass::lenient) || ...);
      }
    }
    if (!out.has_value()) {
      detail::kind_mismatch("union", v);
    }
    return std::move(*out);
  }

 private:
  enum class pass { literal, strict, lenient };

  template <std::size_t I = 0>
  static V decode_selected(const detail::pb::BamlOutboundValue& value,
                           const detail::pb::BamlTy& selected_type) {
    if constexpr (I < std::variant_size_v<V>) {
      using T = std::variant_alternative_t<I, V>;
      if (detail::selected_type_matches(codec<T>::baml_ty(), selected_type)) {
        return V(std::in_place_index<I>, codec<T>::decode(value));
      }
      return decode_selected<I + 1>(value, selected_type);
    }
    throw error(
        "BAML decode error: selected union type is not representable by the "
        "generated C++ union");
  }

  static bool is_int_arm(const detail::pb::BamlOutboundValue& v) {
    if (v.value_case() == detail::pb::BamlOutboundValue::kIntValue) {
      return true;
    }
    return v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
           v.literal_value().literal_case() ==
               detail::pb::BamlLiteralValue::kIntValue;
  }

  template <class T>
  static bool try_arm(const detail::pb::BamlOutboundValue& v,
                      std::optional<V>& out, pass pass) {
    if ((pass == pass::literal) != detail::is_lit<T>::value) {
      return false;
    }
    if (pass == pass::strict && std::is_same<T, double>::value &&
        is_int_arm(v)) {
      return false;
    }
    try {
      out.emplace(std::in_place_type<T>, codec<T>::decode(v));
      return true;
    } catch (const error&) {
      return false;
    }
  }
};

namespace detail {

// Extracts a human-readable message from a thrown BAML value: the `message`
// field of an error class when present, else the class FQN, else the
// value's arm name.
inline std::string error_message_of(const pb::BamlOutboundValue& raw) {
  const pb::BamlOutboundValue& v = unwrap(raw);
  if (v.value_case() == pb::BamlOutboundValue::kClassValue) {
    const pb::BamlValueClass& cls = v.class_value();
    for (const auto& field : cls.fields()) {
      if (field.key() == "message" &&
          field.value().value_case() == pb::BamlOutboundValue::kStringValue) {
        return cls.name() + ": " + field.value().string_value();
      }
    }
    return cls.name();
  }
  if (v.value_case() == pb::BamlOutboundValue::kStringValue) {
    return v.string_value();
  }
  return arm_name(v.value_case());
}

inline std::string join_trace(
    const google::protobuf::RepeatedPtrField<std::string>& trace) {
  std::string out;
  for (const std::string& line : trace) {
    if (!out.empty()) {
      out += "\n";
    }
    out += line;
  }
  return out;
}

// The FQN of a thrown class value, for class_name() routing.
inline std::string thrown_class_name(const pb::BamlOutboundValue& raw) {
  const pb::BamlOutboundValue& v = unwrap(raw);
  return v.value_case() == pb::BamlOutboundValue::kClassValue
             ? v.class_value().name()
             : std::string();
}

// The re-encoded thrown value, kept on the exception so error::get<T>()
// can decode it as a typed value later.
inline std::vector<uint8_t> raw_payload(const pb::BamlOutboundValue& v) {
  const std::string bytes = v.SerializeAsString();
  return std::vector<uint8_t>(bytes.begin(), bytes.end());
}

[[noreturn]] inline void throw_from_result(
    const pb::BamlOutboundResult& result) {
  const bool is_panic = result.result_case() == pb::BamlOutboundResult::kPanic;
  const pb::BamlOutboundValue& value =
      is_panic ? result.panic().value() : result.error().value();
  std::string class_name = thrown_class_name(value);
  std::string message = error_message_of(value);

  // A baml.errors.HostCallable wrapping a native host exception carries a
  // _handle into this process's registry: rethrow the original exception
  // object instead of a flattened baml::error (Python-identity parity).
  if (!is_panic && class_name == "baml.errors.HostCallable") {
    const pb::BamlOutboundValue& v = unwrap(value);
    if (v.value_case() == pb::BamlOutboundValue::kClassValue) {
      for (const auto& field : v.class_value().fields()) {
        if (field.key() == "_handle" &&
            field.value().value_case() == pb::BamlOutboundValue::kHandleValue &&
            field.value().handle_value().handle_type() ==
                pb::HOST_VALUE_OPAQUE) {
          if (std::exception_ptr original =
                  host_value_registry::instance().find_exception(
                      field.value().handle_value().key())) {
            std::rethrow_exception(original);
          }
        }
      }
    }
  }

  if (is_panic) {
    if (result.panic().is_exit_panic()) {
      // Clean baml.sys.exit: hard process exit, not a catchable panic
      // (parity with Python's os._exit path).
      std::_Exit(static_cast<int>(result.panic().exit_code()));
    }
    std::string trace = join_trace(result.panic().trace());
    if (class_name == "baml.panics.Cancelled") {
      throw cancelled(std::move(message), class_name, std::move(trace),
                      raw_payload(value));
    }
    throw panic(std::move(message), class_name, std::move(trace),
                raw_payload(value));
  }
  throw error(std::move(message), class_name,
              join_trace(result.error().trace()), raw_payload(value));
}

inline pb::BamlOutboundResult parse_result_envelope(
    const std::vector<uint8_t>& envelope) {
  pb::BamlOutboundResult result;
  if (!result.ParseFromArray(envelope.data(),
                             static_cast<int>(envelope.size()))) {
    throw error("BAML decode error: malformed result envelope");
  }
  if (result.result_case() == pb::BamlOutboundResult::RESULT_NOT_SET) {
    throw error("BAML decode error: result envelope has no arm");
  }
  return result;
}

// The typed error path: when the error arm's value decodes into the
// function's declared `throws` union, throw thrown<ThrownU> carrying
// the typed payload. Anything else (undeclared throw, panic, exit) falls
// through to the untyped throw_from_result.
template <typename ThrownU>
[[noreturn]] void throw_from_result_typed(
    const pb::BamlOutboundResult& result) {
  if (result.result_case() == pb::BamlOutboundResult::kError) {
    const pb::BamlOutboundValue& value = result.error().value();
    std::optional<ThrownU> decoded;
    try {
      decoded.emplace(codec<ThrownU>::decode(value));
    } catch (const error&) {
      // Not in the declared set: untyped fallback below.
    }
    if (decoded.has_value()) {
      throw thrown<ThrownU>(std::move(*decoded), error_message_of(value),
                            thrown_class_name(value),
                            join_trace(result.error().trace()),
                            raw_payload(value));
    }
  }
  throw_from_result(result);
}

// Decodes a BamlOutboundResult envelope into T. Non-ok arms throw:
// thrown<ThrownU> for declared throws (when ThrownU is not void),
// else error / panic / cancelled.
template <typename T, typename ThrownU = void>
T decode_result(const std::vector<uint8_t>& envelope) {
  pb::BamlOutboundResult result = parse_result_envelope(envelope);
  if (result.result_case() != pb::BamlOutboundResult::kOk) {
    if constexpr (std::is_void<ThrownU>::value) {
      throw_from_result(result);
    } else {
      throw_from_result_typed<ThrownU>(result);
    }
  }
  if constexpr (!std::is_void<T>::value) {
    return codec<T>::decode(result.ok());
  }
}

}  // namespace detail

// Typed access to a thrown BAML value: re-decode the raw error payload the
// envelope carried. Declared in errors.h; defined here because decoding
// needs the codec.
template <typename T>
T error::get() const {
  detail::pb::BamlOutboundValue value;
  if (!value.ParseFromArray(payload().data(),
                            static_cast<int>(payload().size()))) {
    throw error("BAML decode error: malformed error payload");
  }
  return codec<T>::decode(value);
}

template <typename T>
bool error::is() const {
  try {
    (void)get<T>();
    return true;
  } catch (const error&) {
    return false;
  }
}

}  // namespace baml

#endif  // BAML_CODEC_H_
