#ifndef BAML_CODEC_H_
#define BAML_CODEC_H_

// Codec<T>: the typed boundary layer. `Encode` writes T into an InboundValue
// message body; `Decode` converts a parsed OutboundValue into T, throwing
// BamlError on kind mismatches. Generated code adds specializations for its
// classes/enums; this header owns the primitive and container instances.

#include <baml/bigint.h>
#include <baml/box.h>
#include <baml/detail/host_value.h>
#include <baml/detail/proto.h>
#include <baml/detail/wire.h>
#include <baml/errors.h>
#include <baml/handle.h>
#include <baml_cffi.h>

#include <cstdint>
#include <cstdlib>
#include <map>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

namespace baml {

template <typename T>
struct Codec;  // primary template intentionally undefined

namespace detail {

[[noreturn]] inline void KindMismatch(const char* expected,
                                      const OutboundValue& got) {
  throw BamlError(std::string("BAML decode error: expected ") + expected +
                  ", got " + got.KindName());
}

// dependent_t<X, Deps...> is X, made formally dependent on Deps. Generated
// template bodies reference Codec<X> for concrete X through this alias so
// the lookup defers to instantiation time -- Codec specializations are
// defined after the classes whose inline template methods mention them.
template <typename X, typename...>
struct dependent {
  using type = X;
};
template <typename X, typename... Deps>
using dependent_t = typename dependent<X, Deps...>::type;

}  // namespace detail

template <>
struct Codec<int64_t> {
  static void Encode(detail::wire::Writer& value_msg, int64_t v) {
    value_msg.Int64Field(3, v);  // InboundValue.int_value
  }
  static int64_t Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Int) {
      detail::KindMismatch("int", v);
    }
    return v.int_v;
  }
};

template <>
struct Codec<double> {
  static void Encode(detail::wire::Writer& value_msg, double v) {
    value_msg.DoubleField(4, v);  // InboundValue.float_value
  }
  static double Decode(const detail::OutboundValue& v) {
    // Engine ints coerce to float when the declared type is float.
    if (v.kind == detail::OutboundValue::Kind::Int) {
      return static_cast<double>(v.int_v);
    }
    if (v.kind != detail::OutboundValue::Kind::Float) {
      detail::KindMismatch("float", v);
    }
    return v.float_v;
  }
};

template <>
struct Codec<bool> {
  static void Encode(detail::wire::Writer& value_msg, bool v) {
    value_msg.BoolField(5, v);  // InboundValue.bool_value
  }
  static bool Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Bool) {
      detail::KindMismatch("bool", v);
    }
    return v.bool_v;
  }
};

template <>
struct Codec<std::string> {
  static void Encode(detail::wire::Writer& value_msg, const std::string& v) {
    value_msg.StringField(2, v);  // InboundValue.string_value
  }
  static std::string Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::String) {
      detail::KindMismatch("string", v);
    }
    return v.string_v;
  }
};

template <>
struct Codec<std::vector<uint8_t>> {
  static void Encode(detail::wire::Writer& value_msg,
                     const std::vector<uint8_t>& v) {
    value_msg.BytesField(11, v.data(),
                         v.size());  // InboundValue.uint8array_value
  }
  static std::vector<uint8_t> Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Bytes) {
      detail::KindMismatch("uint8array", v);
    }
    return v.bytes_v;
  }
};

template <>
struct Codec<std::monostate> {
  static void Encode(detail::wire::Writer&, std::monostate) {
    // BAML null = absent InboundValue oneof: write nothing.
  }
  static std::monostate Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Null) {
      detail::KindMismatch("null", v);
    }
    return std::monostate{};
  }
};

template <>
struct Codec<BigInt> {
  static void Encode(detail::wire::Writer& value_msg, const BigInt& v) {
    value_msg.StringField(12, v.hex());  // InboundValue.bigint_value
  }
  static BigInt Decode(const detail::OutboundValue& v) {
    // Engine ints widen to bigint when the declared type is bigint.
    if (v.kind == detail::OutboundValue::Kind::Int) {
      return BigInt(v.int_v);
    }
    if (v.kind != detail::OutboundValue::Kind::BigInt) {
      detail::KindMismatch("bigint", v);
    }
    return BigInt::FromHex(v.string_v);
  }
};

template <>
struct Codec<Handle> {
  static void Encode(detail::wire::Writer& value_msg, const Handle& v) {
    detail::wire::Writer handle;  // BamlHandle
    handle.Uint64Field(1, v.CloneKeyForWire());
    handle.Int64Field(2, v.handle_type());
    value_msg.MessageField(10, handle);  // InboundValue.handle
  }
  static Handle Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Handle) {
      detail::KindMismatch("handle", v);
    }
    return Handle(v.handle_key, v.handle_type);
  }
};

namespace detail {

// True for types whose Decode widens an integer wire value (double and
// BigInt accept Kind::Int), transitively through the containers a union
// alternative can be built from. The variant codec tries these LAST so a
// union's int arm always wins a wire int regardless of alternative order
// (the emitter sorts alternatives canonically; order must not matter).
template <typename T>
struct is_widening_decoder : std::false_type {};
template <>
struct is_widening_decoder<double> : std::true_type {};
template <>
struct is_widening_decoder<BigInt> : std::true_type {};
template <typename T>
struct is_widening_decoder<std::vector<T>> : is_widening_decoder<T> {};
template <typename T>
struct is_widening_decoder<std::optional<T>> : is_widening_decoder<T> {};
template <typename T>
struct is_widening_decoder<std::map<std::string, T>> : is_widening_decoder<T> {
};
template <typename T>
struct is_widening_decoder<Box<T>> : is_widening_decoder<T> {};
template <typename T>
struct is_widening_decoder<OptionalBox<T>> : is_widening_decoder<T> {};

}  // namespace detail

// Union values arrive as their inner value (union metadata is dropped on the
// wire, Python parity). Alternatives are canonically sorted by the emitter
// (BAML unions are sets), so decode must be order-independent: an exact-kind
// pass runs before the widening decoders (double/BigInt, which accept wire
// ints). Generated class codecs check the wire FQN, making class
// alternatives dispatch precisely. Encode writes the active alternative's
// inner value (no union wrapper inbound).
template <typename... Ts>
struct Codec<std::variant<Ts...>> {
  static void Encode(detail::wire::Writer& value_msg,
                     const std::variant<Ts...>& v) {
    std::visit(
        [&value_msg](const auto& alt) {
          Codec<std::decay_t<decltype(alt)>>::Encode(value_msg, alt);
        },
        v);
  }
  static std::variant<Ts...> Decode(const detail::OutboundValue& v) {
    std::optional<std::variant<Ts...>> out;
    ((!detail::is_widening_decoder<Ts>::value && TryAlternative<Ts>(v, out)) ||
     ...);
    if (!out.has_value()) {
      ((detail::is_widening_decoder<Ts>::value && TryAlternative<Ts>(v, out)) ||
       ...);
    }
    if (!out.has_value()) {
      detail::KindMismatch("a union alternative", v);
    }
    return std::move(*out);
  }

 private:
  template <typename T>
  static bool TryAlternative(const detail::OutboundValue& v,
                             std::optional<std::variant<Ts...>>& out) {
    try {
      out.emplace(std::in_place_type<T>, Codec<T>::Decode(v));
      return true;
    } catch (const BamlError&) {
      return false;
    }
  }
};

// Boxes are transparent on the wire: the box exists only to break C++
// type-recursion cycles.
template <typename T>
struct Codec<Box<T>> {
  static void Encode(detail::wire::Writer& value_msg, const Box<T>& v) {
    Codec<T>::Encode(value_msg, *v);
  }
  static Box<T> Decode(const detail::OutboundValue& v) {
    return Box<T>(Codec<T>::Decode(v));
  }
};

template <typename T>
struct Codec<OptionalBox<T>> {
  static void Encode(detail::wire::Writer& value_msg, const OptionalBox<T>& v) {
    if (v.has_value()) {
      Codec<T>::Encode(value_msg, *v);
    }
    // empty = BAML null = absent oneof: write nothing.
  }
  static OptionalBox<T> Decode(const detail::OutboundValue& v) {
    if (v.kind == detail::OutboundValue::Kind::Null) {
      return OptionalBox<T>();
    }
    return OptionalBox<T>(Codec<T>::Decode(v));
  }
};

template <typename T>
struct Codec<std::optional<T>> {
  static void Encode(detail::wire::Writer& value_msg,
                     const std::optional<T>& v) {
    if (v.has_value()) {
      Codec<T>::Encode(value_msg, *v);
    }
    // nullopt = BAML null = absent oneof: write nothing.
  }
  static std::optional<T> Decode(const detail::OutboundValue& v) {
    if (v.kind == detail::OutboundValue::Kind::Null) {
      return std::nullopt;
    }
    return Codec<T>::Decode(v);
  }
};

template <typename T>
struct Codec<std::vector<T>> {
  static void Encode(detail::wire::Writer& value_msg, const std::vector<T>& v) {
    detail::wire::Writer list_msg;  // InboundListValue
    for (const T& item : v) {
      detail::wire::Writer item_msg;
      Codec<T>::Encode(item_msg, item);
      list_msg.MessageField(1, item_msg);
    }
    value_msg.MessageField(6, list_msg);  // InboundValue.list_value
  }
  static std::vector<T> Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::List) {
      detail::KindMismatch("list", v);
    }
    std::vector<T> out;
    out.reserve(v.items.size());
    for (const detail::OutboundValue& item : v.items) {
      out.push_back(Codec<T>::Decode(item));
    }
    return out;
  }
};

template <typename T>
struct Codec<std::map<std::string, T>> {
  static void Encode(detail::wire::Writer& value_msg,
                     const std::map<std::string, T>& v) {
    detail::wire::Writer map_msg;  // InboundMapValue
    for (const auto& entry : v) {
      detail::wire::Writer entry_msg;  // InboundMapEntry
      entry_msg.StringField(1, entry.first);
      detail::wire::Writer item_msg;
      Codec<T>::Encode(item_msg, entry.second);
      entry_msg.MessageField(6, item_msg);
      map_msg.MessageField(1, entry_msg);
    }
    value_msg.MessageField(7, map_msg);  // InboundValue.map_value
  }
  static std::map<std::string, T> Decode(const detail::OutboundValue& v) {
    if (v.kind != detail::OutboundValue::Kind::Map) {
      detail::KindMismatch("map", v);
    }
    std::map<std::string, T> out;
    for (const auto& entry : v.fields) {
      out.emplace(entry.first, Codec<T>::Decode(entry.second));
    }
    return out;
  }
};

namespace detail {

// Extracts a human-readable message from a thrown BAML value: the `message`
// field of an error class when present, else the class FQN, else the value's
// kind.
inline std::string ErrorMessageOf(const OutboundValue& v) {
  if (v.kind == OutboundValue::Kind::Class) {
    for (const auto& field : v.fields) {
      if (field.first == "message" &&
          field.second.kind == OutboundValue::Kind::String) {
        return v.name + ": " + field.second.string_v;
      }
    }
    return v.name;
  }
  if (v.kind == OutboundValue::Kind::String) {
    return v.string_v;
  }
  return v.KindName();
}

inline std::string JoinTrace(const std::vector<std::string>& trace) {
  std::string out;
  for (const std::string& line : trace) {
    if (!out.empty()) {
      out += "\n";
    }
    out += line;
  }
  return out;
}

[[noreturn]] inline void ThrowFromResult(OutboundResult&& result) {
  const std::string class_name = result.value.kind == OutboundValue::Kind::Class
                                     ? result.value.name
                                     : std::string();

  // A baml.errors.HostCallable wrapping a native host exception carries a
  // _handle into this process's registry: rethrow the original exception
  // object instead of a flattened BamlError (Python-identity parity).
  if (result.arm == OutboundResult::Arm::Error &&
      class_name == "baml.errors.HostCallable") {
    for (const auto& field : result.value.fields) {
      if (field.first == "_handle" &&
          field.second.kind == OutboundValue::Kind::Handle &&
          field.second.handle_type == kHandleHostValueOpaque) {
        if (std::exception_ptr original =
                HostValueRegistry::Instance().FindException(
                    field.second.handle_key)) {
          std::rethrow_exception(original);
        }
      }
    }
  }

  std::string message = ErrorMessageOf(result.value);
  std::string trace = JoinTrace(result.trace);

  if (result.arm == OutboundResult::Arm::Panic) {
    if (result.is_exit_panic) {
      // Clean baml.sys.exit: hard process exit, not a catchable panic
      // (parity with Python's os._exit path). The legacy flush_events()
      // call is gone: it is a documented no-op and lives outside the v1
      // ABI table.
      std::_Exit(static_cast<int>(result.exit_code));
    }
    if (class_name == "baml.panics.Cancelled") {
      throw BamlCancelled(std::move(message), class_name, std::move(trace),
                          std::move(result.raw_value));
    }
    throw BamlPanic(std::move(message), class_name, std::move(trace),
                    std::move(result.raw_value));
  }
  throw BamlError(std::move(message), class_name, std::move(trace),
                  std::move(result.raw_value));
}

// Declared in future.h; the codec provides the definition.
template <typename T>
T DecodeResult(const std::vector<uint8_t>& envelope) {
  OutboundResult result = ParseOutboundResult(envelope);
  if (result.arm != OutboundResult::Arm::Ok) {
    ThrowFromResult(std::move(result));
  }
  return Codec<T>::Decode(result.value);
}

template <>
inline void DecodeResult<void>(const std::vector<uint8_t>& envelope) {
  OutboundResult result = ParseOutboundResult(envelope);
  if (result.arm != OutboundResult::Arm::Ok) {
    ThrowFromResult(std::move(result));
  }
}

}  // namespace detail

// Typed access to a thrown BAML value: re-decode the raw error payload the
// envelope carried. Declared in errors.h; defined here because decoding
// needs the codec.
template <typename T>
T BamlError::get() const {
  detail::wire::Reader r(payload().data(), payload().size());
  return Codec<T>::Decode(detail::ParseOutboundValue(r));
}

template <typename T>
bool BamlError::is() const {
  try {
    (void)get<T>();
    return true;
  } catch (const BamlError&) {
    return false;
  }
}

}  // namespace baml

#endif  // BAML_CODEC_H_
