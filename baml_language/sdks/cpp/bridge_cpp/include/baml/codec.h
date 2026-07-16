#ifndef BAML_CODEC_H_
#define BAML_CODEC_H_

// Codec<T>: the typed boundary layer. `Encode` writes T into an InboundValue
// message body; `Decode` converts a parsed OutboundValue into T, throwing
// BamlError on kind mismatches. Generated code adds specializations for its
// classes/enums; this header owns the primitive and container instances.

#include <baml/box.h>
#include <baml/detail/proto.h>
#include <baml/detail/wire.h>
#include <baml/errors.h>
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

// Decodes a BamlOutboundResult envelope into T, throwing BamlError /
// BamlPanic / BamlCancelled for the non-ok arms.
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
