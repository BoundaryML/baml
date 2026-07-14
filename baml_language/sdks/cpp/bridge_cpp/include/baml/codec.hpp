#ifndef BAML_CODEC_HPP
#define BAML_CODEC_HPP

// codec<T>: the typed boundary layer. `encode` writes T into an InboundValue
// message body; `decode` converts a parsed OutboundValue into T, throwing
// BamlError on kind mismatches. Generated code adds specializations for its
// classes/enums; this header owns the primitive and container instances.

#include <cstdint>
#include <cstdlib>
#include <map>
#include <optional>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include <baml_cffi.h>

#include <baml/detail/proto.hpp>
#include <baml/detail/wire.hpp>
#include <baml/errors.hpp>

namespace baml {

template <typename T>
struct codec;  // primary template intentionally undefined

namespace detail {

[[noreturn]] inline void kind_mismatch(const char* expected, const OutboundValue& got) {
    throw BamlError(std::string("BAML decode error: expected ") + expected + ", got " +
                    got.kind_name());
}

}  // namespace detail

template <>
struct codec<int64_t> {
    static void encode(detail::wire::Writer& value_msg, int64_t v) {
        value_msg.int64_field(3, v);  // InboundValue.int_value
    }
    static int64_t decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::Int) {
            detail::kind_mismatch("int", v);
        }
        return v.int_v;
    }
};

template <>
struct codec<double> {
    static void encode(detail::wire::Writer& value_msg, double v) {
        value_msg.double_field(4, v);  // InboundValue.float_value
    }
    static double decode(const detail::OutboundValue& v) {
        // Engine ints coerce to float when the declared type is float.
        if (v.kind == detail::OutboundValue::Kind::Int) {
            return static_cast<double>(v.int_v);
        }
        if (v.kind != detail::OutboundValue::Kind::Float) {
            detail::kind_mismatch("float", v);
        }
        return v.float_v;
    }
};

template <>
struct codec<bool> {
    static void encode(detail::wire::Writer& value_msg, bool v) {
        value_msg.bool_field(5, v);  // InboundValue.bool_value
    }
    static bool decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::Bool) {
            detail::kind_mismatch("bool", v);
        }
        return v.bool_v;
    }
};

template <>
struct codec<std::string> {
    static void encode(detail::wire::Writer& value_msg, const std::string& v) {
        value_msg.string_field(2, v);  // InboundValue.string_value
    }
    static std::string decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::String) {
            detail::kind_mismatch("string", v);
        }
        return v.string_v;
    }
};

template <>
struct codec<std::vector<uint8_t>> {
    static void encode(detail::wire::Writer& value_msg, const std::vector<uint8_t>& v) {
        value_msg.bytes_field(11, v.data(), v.size());  // InboundValue.uint8array_value
    }
    static std::vector<uint8_t> decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::Bytes) {
            detail::kind_mismatch("uint8array", v);
        }
        return v.bytes_v;
    }
};

template <>
struct codec<std::monostate> {
    static void encode(detail::wire::Writer&, std::monostate) {
        // BAML null = absent InboundValue oneof: write nothing.
    }
    static std::monostate decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::Null) {
            detail::kind_mismatch("null", v);
        }
        return std::monostate{};
    }
};

template <typename T>
struct codec<std::optional<T>> {
    static void encode(detail::wire::Writer& value_msg, const std::optional<T>& v) {
        if (v.has_value()) {
            codec<T>::encode(value_msg, *v);
        }
        // nullopt = BAML null = absent oneof: write nothing.
    }
    static std::optional<T> decode(const detail::OutboundValue& v) {
        if (v.kind == detail::OutboundValue::Kind::Null) {
            return std::nullopt;
        }
        return codec<T>::decode(v);
    }
};

template <typename T>
struct codec<std::vector<T>> {
    static void encode(detail::wire::Writer& value_msg, const std::vector<T>& v) {
        detail::wire::Writer list_msg;  // InboundListValue
        for (const T& item : v) {
            detail::wire::Writer item_msg;
            codec<T>::encode(item_msg, item);
            list_msg.message_field(1, item_msg);
        }
        value_msg.message_field(6, list_msg);  // InboundValue.list_value
    }
    static std::vector<T> decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::List) {
            detail::kind_mismatch("list", v);
        }
        std::vector<T> out;
        out.reserve(v.items.size());
        for (const detail::OutboundValue& item : v.items) {
            out.push_back(codec<T>::decode(item));
        }
        return out;
    }
};

template <typename T>
struct codec<std::map<std::string, T>> {
    static void encode(detail::wire::Writer& value_msg, const std::map<std::string, T>& v) {
        detail::wire::Writer map_msg;  // InboundMapValue
        for (const auto& entry : v) {
            detail::wire::Writer entry_msg;  // InboundMapEntry
            entry_msg.string_field(1, entry.first);
            detail::wire::Writer item_msg;
            codec<T>::encode(item_msg, entry.second);
            entry_msg.message_field(6, item_msg);
            map_msg.message_field(1, entry_msg);
        }
        value_msg.message_field(7, map_msg);  // InboundValue.map_value
    }
    static std::map<std::string, T> decode(const detail::OutboundValue& v) {
        if (v.kind != detail::OutboundValue::Kind::Map) {
            detail::kind_mismatch("map", v);
        }
        std::map<std::string, T> out;
        for (const auto& entry : v.fields) {
            out.emplace(entry.first, codec<T>::decode(entry.second));
        }
        return out;
    }
};

namespace detail {

// Extracts a human-readable message from a thrown BAML value: the `message`
// field of an error class when present, else the class FQN, else the value's
// kind.
inline std::string error_message_of(const OutboundValue& v) {
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
    return v.kind_name();
}

inline std::string join_trace(const std::vector<std::string>& trace) {
    std::string out;
    for (const std::string& line : trace) {
        if (!out.empty()) {
            out += "\n";
        }
        out += line;
    }
    return out;
}

[[noreturn]] inline void throw_from_result(OutboundResult&& result) {
    const std::string class_name =
        result.value.kind == OutboundValue::Kind::Class ? result.value.name : std::string();
    std::string message = error_message_of(result.value);
    std::string trace = join_trace(result.trace);

    if (result.arm == OutboundResult::Arm::Panic) {
        if (result.is_exit_panic) {
            // Clean baml.sys.exit: hard process exit, not a catchable panic
            // (parity with Python's os._exit path).
            flush_events();
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

// Declared in future.hpp; the codec provides the definition.
template <typename T>
T decode_result(const std::vector<uint8_t>& envelope) {
    OutboundResult result = parse_outbound_result(envelope);
    if (result.arm != OutboundResult::Arm::Ok) {
        throw_from_result(std::move(result));
    }
    return codec<T>::decode(result.value);
}

template <>
inline void decode_result<void>(const std::vector<uint8_t>& envelope) {
    OutboundResult result = parse_outbound_result(envelope);
    if (result.arm != OutboundResult::Arm::Ok) {
        throw_from_result(std::move(result));
    }
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_CODEC_HPP
