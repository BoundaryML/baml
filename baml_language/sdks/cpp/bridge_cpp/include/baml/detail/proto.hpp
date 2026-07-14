#ifndef BAML_DETAIL_PROTO_HPP
#define BAML_DETAIL_PROTO_HPP

// Encoders/decoders for the bridge_ctypes CFFI protobuf schemas
// (baml_inbound.proto / baml_outbound.proto / baml_handle.proto), built on
// the hand-rolled wire layer. The outbound side parses into a small DOM
// (OutboundValue) that codec<T> then converts to typed values; copies are
// deliberate and visible (contract: coarse-grained boundary, measurable
// copies) and can be optimized later without changing the codec API.

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <baml/detail/wire.hpp>

namespace baml {
namespace detail {

// ---------------------------------------------------------------------------
// Outbound DOM
// ---------------------------------------------------------------------------

struct OutboundValue {
    enum class Kind {
        Null,
        String,
        Int,
        Float,
        Bool,
        Class,
        Enum,
        List,
        Map,
        Handle,
        Media,
        Bytes,
        BigInt,
    };

    Kind kind = Kind::Null;

    int64_t int_v = 0;
    double float_v = 0.0;
    bool bool_v = false;
    std::string string_v;              // String / BigInt / Enum variant name
    std::vector<uint8_t> bytes_v;      // Bytes
    std::string name;                  // Class / Enum FQN
    std::vector<std::pair<std::string, OutboundValue>> fields;  // Class / Map
    std::vector<OutboundValue> items;  // List

    uint64_t handle_key = 0;
    int32_t handle_type = 0;

    // Media: which source variant is set is encoded in media_source.
    enum class MediaSource { None, Url, Base64, File };
    int32_t media_kind = 0;
    std::string media_mime;
    MediaSource media_source = MediaSource::None;
    std::string media_value;

    const char* kind_name() const {
        switch (kind) {
            case Kind::Null: return "null";
            case Kind::String: return "string";
            case Kind::Int: return "int";
            case Kind::Float: return "float";
            case Kind::Bool: return "bool";
            case Kind::Class: return "class";
            case Kind::Enum: return "enum";
            case Kind::List: return "list";
            case Kind::Map: return "map";
            case Kind::Handle: return "handle";
            case Kind::Media: return "media";
            case Kind::Bytes: return "bytes";
            case Kind::BigInt: return "bigint";
        }
        return "?";
    }
};

// Parses a BamlOutboundValue message. Union variants are unwrapped into
// their inner value (union metadata dropped, matching the Python bridge);
// literal values are widened to their base scalar kind.
inline OutboundValue parse_outbound_value(wire::Reader r);

inline OutboundValue parse_literal_value(wire::Reader r) {
    OutboundValue v;
    uint32_t field;
    wire::WireType wt;
    while (r.next(field, wt)) {
        switch (field) {
            case 1:
                v.kind = OutboundValue::Kind::String;
                v.string_v = r.len_string();
                break;
            case 2:
                v.kind = OutboundValue::Kind::Int;
                v.int_v = r.int64();
                break;
            case 3:
                v.kind = OutboundValue::Kind::Bool;
                v.bool_v = r.boolean();
                break;
            case 4:
                v.kind = OutboundValue::Kind::BigInt;
                v.string_v = r.len_string();
                break;
            case 5:
                // Float literal rides as source text.
                v.kind = OutboundValue::Kind::Float;
                v.float_v = std::stod(r.len_string());
                break;
            default: r.skip(wt); break;
        }
    }
    return v;
}

inline std::vector<std::pair<std::string, OutboundValue>> parse_entries(wire::Reader r,
                                                                        uint32_t entry_field) {
    // BamlValueClass.fields / BamlValueMap.entries: repeated
    // BamlOutboundMapEntry { string key = 1; BamlOutboundValue value = 2; }.
    std::vector<std::pair<std::string, OutboundValue>> out;
    uint32_t field;
    wire::WireType wt;
    while (r.next(field, wt)) {
        if (field != entry_field) {
            r.skip(wt);
            continue;
        }
        wire::Reader entry = r.len_payload();
        std::string key;
        OutboundValue value;
        uint32_t ef;
        wire::WireType ewt;
        while (entry.next(ef, ewt)) {
            switch (ef) {
                case 1: key = entry.len_string(); break;
                case 2: value = parse_outbound_value(entry.len_payload()); break;
                default: entry.skip(ewt); break;
            }
        }
        out.emplace_back(std::move(key), std::move(value));
    }
    return out;
}

inline OutboundValue parse_outbound_value(wire::Reader r) {
    OutboundValue v;
    uint32_t field;
    wire::WireType wt;
    while (r.next(field, wt)) {
        switch (field) {
            case 2:  // null_value
                r.skip(wt);
                v.kind = OutboundValue::Kind::Null;
                break;
            case 3:
                v.kind = OutboundValue::Kind::String;
                v.string_v = r.len_string();
                break;
            case 4:
                v.kind = OutboundValue::Kind::Int;
                v.int_v = r.int64();
                break;
            case 5:
                v.kind = OutboundValue::Kind::Float;
                v.float_v = r.fixed64_double();
                break;
            case 6:
                v.kind = OutboundValue::Kind::Bool;
                v.bool_v = r.boolean();
                break;
            case 7: {  // class_value
                v.kind = OutboundValue::Kind::Class;
                wire::Reader cls = r.len_payload();
                uint32_t cf;
                wire::WireType cwt;
                std::string raw;
                // Re-walk: name = 1, fields (entries) = 2, type_args = 3.
                std::vector<std::pair<std::string, OutboundValue>> fields;
                while (cls.next(cf, cwt)) {
                    switch (cf) {
                        case 1: v.name = cls.len_string(); break;
                        case 2: {
                            wire::Reader entry = cls.len_payload();
                            std::string key;
                            OutboundValue value;
                            uint32_t ef;
                            wire::WireType ewt;
                            while (entry.next(ef, ewt)) {
                                switch (ef) {
                                    case 1: key = entry.len_string(); break;
                                    case 2:
                                        value = parse_outbound_value(entry.len_payload());
                                        break;
                                    default: entry.skip(ewt); break;
                                }
                            }
                            fields.emplace_back(std::move(key), std::move(value));
                            break;
                        }
                        default: cls.skip(cwt); break;  // type_args: consumed by codegen later
                    }
                }
                v.fields = std::move(fields);
                break;
            }
            case 8: {  // enum_value { name = 1, value = 2, is_dynamic = 3 }
                v.kind = OutboundValue::Kind::Enum;
                wire::Reader en = r.len_payload();
                uint32_t ef;
                wire::WireType ewt;
                while (en.next(ef, ewt)) {
                    switch (ef) {
                        case 1: v.name = en.len_string(); break;
                        case 2: v.string_v = en.len_string(); break;
                        default: en.skip(ewt); break;
                    }
                }
                break;
            }
            case 9:  // literal_value -> widened to base scalar
                v = parse_literal_value(r.len_payload());
                break;
            case 11: {  // list_value { item_type = 1, items = 2 }
                v.kind = OutboundValue::Kind::List;
                wire::Reader list = r.len_payload();
                uint32_t lf;
                wire::WireType lwt;
                while (list.next(lf, lwt)) {
                    if (lf == 2) {
                        v.items.push_back(parse_outbound_value(list.len_payload()));
                    } else {
                        list.skip(lwt);
                    }
                }
                break;
            }
            case 12: {  // map_value { key_type = 1, value_type = 2, entries = 3 }
                v.kind = OutboundValue::Kind::Map;
                v.fields = parse_entries(r.len_payload(), 3);
                break;
            }
            case 13: {  // union_variant_value -> unwrap inner value (field 6)
                wire::Reader u = r.len_payload();
                uint32_t uf;
                wire::WireType uwt;
                bool saw_inner = false;
                while (u.next(uf, uwt)) {
                    if (uf == 6) {
                        v = parse_outbound_value(u.len_payload());
                        saw_inner = true;
                    } else {
                        u.skip(uwt);
                    }
                }
                if (!saw_inner) {
                    v.kind = OutboundValue::Kind::Null;
                }
                break;
            }
            case 16: {  // handle_value { key = 1, handle_type = 2, ty = 3 }
                v.kind = OutboundValue::Kind::Handle;
                wire::Reader h = r.len_payload();
                uint32_t hf;
                wire::WireType hwt;
                while (h.next(hf, hwt)) {
                    switch (hf) {
                        case 1: v.handle_key = h.varint(); break;
                        case 2: v.handle_type = static_cast<int32_t>(h.varint()); break;
                        default: h.skip(hwt); break;
                    }
                }
                break;
            }
            case 17: {  // media_value
                v.kind = OutboundValue::Kind::Media;
                wire::Reader m = r.len_payload();
                uint32_t mf;
                wire::WireType mwt;
                while (m.next(mf, mwt)) {
                    switch (mf) {
                        case 1: v.media_kind = static_cast<int32_t>(m.varint()); break;
                        case 2: v.media_mime = m.len_string(); break;
                        case 3:
                            v.media_source = OutboundValue::MediaSource::Url;
                            v.media_value = m.len_string();
                            break;
                        case 4:
                            v.media_source = OutboundValue::MediaSource::Base64;
                            v.media_value = m.len_string();
                            break;
                        case 5:
                            v.media_source = OutboundValue::MediaSource::File;
                            v.media_value = m.len_string();
                            break;
                        default: m.skip(mwt); break;
                    }
                }
                break;
            }
            case 19: {  // uint8array_value
                wire::Reader b = r.len_payload();
                v.kind = OutboundValue::Kind::Bytes;
                v.bytes_v.assign(b.data(), b.data() + b.size());
                break;
            }
            case 20:
                v.kind = OutboundValue::Kind::BigInt;
                v.string_v = r.len_string();
                break;
            default: r.skip(wt); break;
        }
    }
    return v;
}

// ---------------------------------------------------------------------------
// Result envelope (BamlOutboundResult)
// ---------------------------------------------------------------------------

struct OutboundResult {
    enum class Arm { Ok, Error, Panic };
    Arm arm = Arm::Ok;
    OutboundValue value;
    std::vector<std::string> trace;
    bool is_exit_panic = false;
    int64_t exit_code = 0;
    // The raw encoded BamlOutboundValue of an error/panic arm, kept so
    // BamlError::get<T>() can re-decode it as a typed value later.
    std::vector<uint8_t> raw_value;
};

inline OutboundResult parse_outbound_result(const std::vector<uint8_t>& envelope) {
    wire::Reader r(envelope.data(), envelope.size());
    OutboundResult out;
    uint32_t field;
    wire::WireType wt;
    bool saw_arm = false;
    while (r.next(field, wt)) {
        switch (field) {
            case 1:
                out.arm = OutboundResult::Arm::Ok;
                out.value = parse_outbound_value(r.len_payload());
                saw_arm = true;
                break;
            case 2:
            case 3: {
                out.arm = field == 2 ? OutboundResult::Arm::Error : OutboundResult::Arm::Panic;
                saw_arm = true;
                wire::Reader arm_r = r.len_payload();
                uint32_t af;
                wire::WireType awt;
                while (arm_r.next(af, awt)) {
                    switch (af) {
                        case 1: {
                            wire::Reader value_r = arm_r.len_payload();
                            out.raw_value.assign(value_r.data(),
                                                 value_r.data() + value_r.size());
                            out.value = parse_outbound_value(value_r);
                            break;
                        }
                        case 2: out.trace.push_back(arm_r.len_string()); break;
                        case 3: out.is_exit_panic = arm_r.boolean(); break;
                        case 4: out.exit_code = arm_r.int64(); break;
                        default: arm_r.skip(awt); break;
                    }
                }
                break;
            }
            default: r.skip(wt); break;
        }
    }
    if (!saw_arm) {
        wire::Reader::fail("result envelope has no ok/error/panic arm");
    }
    return out;
}

// ---------------------------------------------------------------------------
// Inbound encoding (CallFunctionArgs)
// ---------------------------------------------------------------------------

// Builds one CallFunctionArgs message: kwargs entries are appended via the
// value-writer callbacks that codec<T>::encode provides, then finish() stamps
// the engine call id.
class ArgsEncoder {
public:
    // `write_value` fills the InboundValue message body for this argument.
    template <typename WriteValue>
    void add_arg(const std::string& name, WriteValue&& write_value) {
        wire::Writer value_msg;
        write_value(value_msg);

        wire::Writer entry;  // InboundMapEntry
        entry.string_field(1, name);
        entry.message_field(6, value_msg);
        args_.message_field(1, entry);  // CallFunctionArgs.kwargs
    }

    // Adds an argument whose value is BAML null (absent oneof).
    void add_null_arg(const std::string& name) {
        add_arg(name, [](wire::Writer&) {});
    }

    // Adds one explicit TypeVar binding (CallFunctionArgs.type_args entry).
    // Bindings are added in De Bruijn order: enclosing class params first,
    // then the callee's own generic params. `write_ty` fills the BamlTy
    // message body for the concrete binding.
    template <typename WriteTy>
    void add_type_arg(const std::string& type_var, WriteTy&& write_ty) {
        wire::Writer ty_msg;
        write_ty(ty_msg);

        wire::Writer binding;  // BamlTyArg
        binding.string_field(1, type_var);
        binding.message_field(2, ty_msg);
        args_.message_field(3, binding);  // CallFunctionArgs.type_args
    }

    std::string finish(uint64_t call_id) {
        args_.uint64_field(2, call_id);  // CallFunctionArgs.call_id
        return args_.bytes();
    }

private:
    wire::Writer args_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_HPP
