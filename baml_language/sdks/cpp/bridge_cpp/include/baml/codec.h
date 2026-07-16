#ifndef BAML_CODEC_H_
#define BAML_CODEC_H_

// Codec<T>: the typed boundary layer. `Encode` fills an InboundValue
// message; `Decode` converts a BamlOutboundValue into T, throwing BamlError
// on arm mismatches. Generated code adds specializations for its
// classes/enums; this header owns the primitive and container instances.
//
// Decode sees values through detail::Unwrap (union metadata dropped) and
// widens literal values to their base scalar (Python parity).

#include <baml/box.h>
#include <baml/detail/proto.h>
#include <baml/errors.h>

#include <cstdint>
#include <cstdlib>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <variant>
#include <vector>

namespace baml {

template <typename T>
struct Codec;  // primary template intentionally undefined

namespace detail {

[[noreturn]] inline void KindMismatch(const char* expected,
                                      const pb::BamlOutboundValue& got) {
  throw BamlError(std::string("BAML decode error: expected ") + expected +
                  ", got " + ArmName(got.value_case()));
}

}  // namespace detail

template <>
struct Codec<int64_t> {
  static void Encode(detail::pb::InboundValue& value_msg, int64_t v) {
    value_msg.set_int_value(v);
  }
  static int64_t Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kIntValue) {
      return v.int_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kIntValue) {
      return v.literal_value().int_value();
    }
    detail::KindMismatch("int", v);
  }
};

template <>
struct Codec<double> {
  static void Encode(detail::pb::InboundValue& value_msg, double v) {
    value_msg.set_float_value(v);
  }
  static double Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    switch (v.value_case()) {
      case detail::pb::BamlOutboundValue::kFloatValue:
        return v.float_value();
      // Engine ints coerce to float when the declared type is float.
      case detail::pb::BamlOutboundValue::kIntValue:
        return static_cast<double>(v.int_value());
      case detail::pb::BamlOutboundValue::kLiteralValue:
        // Float literals ride as source text.
        if (v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kFloatValue) {
          return std::stod(v.literal_value().float_value());
        }
        break;
      default:
        break;
    }
    detail::KindMismatch("float", v);
  }
};

template <>
struct Codec<bool> {
  static void Encode(detail::pb::InboundValue& value_msg, bool v) {
    value_msg.set_bool_value(v);
  }
  static bool Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kBoolValue) {
      return v.bool_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kBoolValue) {
      return v.literal_value().bool_value();
    }
    detail::KindMismatch("bool", v);
  }
};

template <>
struct Codec<std::string> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const std::string& v) {
    value_msg.set_string_value(v);
  }
  static std::string Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kStringValue) {
      return v.string_value();
    }
    if (v.value_case() == detail::pb::BamlOutboundValue::kLiteralValue &&
        v.literal_value().literal_case() ==
            detail::pb::BamlLiteralValue::kStringValue) {
      return v.literal_value().string_value();
    }
    detail::KindMismatch("string", v);
  }
};

template <>
struct Codec<std::vector<uint8_t>> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const std::vector<uint8_t>& v) {
    value_msg.set_uint8array_value(
        std::string(reinterpret_cast<const char*>(v.data()), v.size()));
  }
  static std::vector<uint8_t> Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kUint8ArrayValue) {
      detail::KindMismatch("uint8array", v);
    }
    const std::string& bytes = v.uint8array_value();
    return std::vector<uint8_t>(bytes.begin(), bytes.end());
  }
};

template <>
struct Codec<std::monostate> {
  static void Encode(detail::pb::InboundValue&, std::monostate) {
    // BAML null = absent InboundValue oneof: set nothing.
  }
  static std::monostate Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kNullValue &&
        v.value_case() != detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      detail::KindMismatch("null", v);
    }
    return std::monostate{};
  }
};

// Boxes are transparent on the wire: the box exists only to break C++
// type-recursion cycles.
template <typename T>
struct Codec<Box<T>> {
  static void Encode(detail::pb::InboundValue& value_msg, const Box<T>& v) {
    Codec<T>::Encode(value_msg, *v);
  }
  static Box<T> Decode(const detail::pb::BamlOutboundValue& v) {
    return Box<T>(Codec<T>::Decode(v));
  }
};

template <typename T>
struct Codec<OptionalBox<T>> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const OptionalBox<T>& v) {
    if (v.has_value()) {
      Codec<T>::Encode(value_msg, *v);
    }
    // empty = BAML null = absent oneof: set nothing.
  }
  static OptionalBox<T> Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kNullValue ||
        v.value_case() == detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      return OptionalBox<T>();
    }
    return OptionalBox<T>(Codec<T>::Decode(v));
  }
};

template <typename T>
struct Codec<std::optional<T>> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const std::optional<T>& v) {
    if (v.has_value()) {
      Codec<T>::Encode(value_msg, *v);
    }
    // nullopt = BAML null = absent oneof: set nothing.
  }
  static std::optional<T> Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() == detail::pb::BamlOutboundValue::kNullValue ||
        v.value_case() == detail::pb::BamlOutboundValue::VALUE_NOT_SET) {
      return std::nullopt;
    }
    return Codec<T>::Decode(v);
  }
};

template <typename T>
struct Codec<std::vector<T>> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const std::vector<T>& v) {
    detail::pb::InboundListValue* list = value_msg.mutable_list_value();
    for (const T& item : v) {
      Codec<T>::Encode(*list->add_values(), item);
    }
  }
  static std::vector<T> Decode(const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kListValue) {
      detail::KindMismatch("list", v);
    }
    std::vector<T> out;
    out.reserve(static_cast<size_t>(v.list_value().items_size()));
    for (const auto& item : v.list_value().items()) {
      out.push_back(Codec<T>::Decode(item));
    }
    return out;
  }
};

template <typename T>
struct Codec<std::unordered_map<std::string, T>> {
  static void Encode(detail::pb::InboundValue& value_msg,
                     const std::unordered_map<std::string, T>& v) {
    detail::pb::InboundMapValue* map = value_msg.mutable_map_value();
    for (const auto& entry : v) {
      detail::pb::InboundMapEntry* e = map->add_entries();
      e->set_string_key(entry.first);
      Codec<T>::Encode(*e->mutable_value(), entry.second);
    }
  }
  static std::unordered_map<std::string, T> Decode(
      const detail::pb::BamlOutboundValue& raw) {
    const auto& v = detail::Unwrap(raw);
    if (v.value_case() != detail::pb::BamlOutboundValue::kMapValue) {
      detail::KindMismatch("map", v);
    }
    std::unordered_map<std::string, T> out;
    for (const auto& entry : v.map_value().entries()) {
      out.emplace(entry.key(), Codec<T>::Decode(entry.value()));
    }
    return out;
  }
};

namespace detail {

// Extracts a human-readable message from a thrown BAML value: the `message`
// field of an error class when present, else the class FQN, else the
// value's arm name.
inline std::string ErrorMessageOf(const pb::BamlOutboundValue& raw) {
  const pb::BamlOutboundValue& v = Unwrap(raw);
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
  return ArmName(v.value_case());
}

inline std::string JoinTrace(
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
inline std::string ThrownClassName(const pb::BamlOutboundValue& raw) {
  const pb::BamlOutboundValue& v = Unwrap(raw);
  return v.value_case() == pb::BamlOutboundValue::kClassValue
             ? v.class_value().name()
             : std::string();
}

// The re-encoded thrown value, kept on the exception so BamlError::get<T>()
// can decode it as a typed value later.
inline std::vector<uint8_t> RawPayload(const pb::BamlOutboundValue& v) {
  const std::string bytes = v.SerializeAsString();
  return std::vector<uint8_t>(bytes.begin(), bytes.end());
}

[[noreturn]] inline void ThrowFromResult(const pb::BamlOutboundResult& result) {
  const bool is_panic = result.result_case() == pb::BamlOutboundResult::kPanic;
  const pb::BamlOutboundValue& value =
      is_panic ? result.panic().value() : result.error().value();
  std::string class_name = ThrownClassName(value);
  std::string message = ErrorMessageOf(value);

  if (is_panic) {
    if (result.panic().is_exit_panic()) {
      // Clean baml.sys.exit: hard process exit, not a catchable panic
      // (parity with Python's os._exit path).
      std::_Exit(static_cast<int>(result.panic().exit_code()));
    }
    std::string trace = JoinTrace(result.panic().trace());
    if (class_name == "baml.panics.Cancelled") {
      throw BamlCancelled(std::move(message), class_name, std::move(trace),
                          RawPayload(value));
    }
    throw BamlPanic(std::move(message), class_name, std::move(trace),
                    RawPayload(value));
  }
  throw BamlError(std::move(message), class_name,
                  JoinTrace(result.error().trace()), RawPayload(value));
}

inline pb::BamlOutboundResult ParseResultEnvelope(
    const std::vector<uint8_t>& envelope) {
  pb::BamlOutboundResult result;
  if (!result.ParseFromArray(envelope.data(),
                             static_cast<int>(envelope.size()))) {
    throw BamlError("BAML decode error: malformed result envelope");
  }
  if (result.result_case() == pb::BamlOutboundResult::RESULT_NOT_SET) {
    throw BamlError("BAML decode error: result envelope has no arm");
  }
  return result;
}

// Decodes a BamlOutboundResult envelope into T, throwing BamlError /
// BamlPanic / BamlCancelled for the non-ok arms.
template <typename T>
T DecodeResult(const std::vector<uint8_t>& envelope) {
  pb::BamlOutboundResult result = ParseResultEnvelope(envelope);
  if (result.result_case() != pb::BamlOutboundResult::kOk) {
    ThrowFromResult(result);
  }
  return Codec<T>::Decode(result.ok());
}

template <>
inline void DecodeResult<void>(const std::vector<uint8_t>& envelope) {
  pb::BamlOutboundResult result = ParseResultEnvelope(envelope);
  if (result.result_case() != pb::BamlOutboundResult::kOk) {
    ThrowFromResult(result);
  }
}

}  // namespace detail

// Typed access to a thrown BAML value: re-decode the raw error payload the
// envelope carried. Declared in errors.h; defined here because decoding
// needs the codec.
template <typename T>
T BamlError::get() const {
  detail::pb::BamlOutboundValue value;
  if (!value.ParseFromArray(payload().data(),
                            static_cast<int>(payload().size()))) {
    throw BamlError("BAML decode error: malformed error payload");
  }
  return Codec<T>::Decode(value);
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
