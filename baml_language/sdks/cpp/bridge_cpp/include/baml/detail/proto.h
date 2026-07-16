#ifndef BAML_DETAIL_PROTO_H_
#define BAML_DETAIL_PROTO_H_

// Encoders/decoders for the bridge_ctypes CFFI protobuf schemas
// (baml_inbound.proto / baml_outbound.proto / baml_handle.proto), built on
// the hand-rolled wire layer. The outbound side parses into a small DOM
// (OutboundValue) that Codec<T> then converts to typed values; copies are
// deliberate and visible (contract: coarse-grained boundary, measurable
// copies) and can be optimized later without changing the codec API.

#include <baml/detail/wire.h>

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace baml {
namespace detail {

// ---------------------------------------------------------------------------
// Wire schema: field numbers
// ---------------------------------------------------------------------------
// The bridge_ctypes CFFI schemas, spelled once. Parser arms, codec encoders,
// and sdkgen-emitted code all reference these constants; they mirror the
// .proto sources in crates/bridge_ctypes/types/baml_bridge/cffi/v1/ (the
// authority on the wire contract). Names follow the proto field names.

namespace fields {

// CallFunctionArgs (baml_inbound.proto)
namespace call_args {
constexpr uint32_t kKwargs = 1;
constexpr uint32_t kCallId = 2;
}  // namespace call_args

// InboundValue oneof arms
namespace in_value {
constexpr uint32_t kStringValue = 2;
constexpr uint32_t kIntValue = 3;
constexpr uint32_t kFloatValue = 4;
constexpr uint32_t kBoolValue = 5;
constexpr uint32_t kListValue = 6;
constexpr uint32_t kMapValue = 7;
constexpr uint32_t kClassValue = 8;
constexpr uint32_t kEnumValue = 9;
constexpr uint32_t kUint8ArrayValue = 11;
}  // namespace in_value

// InboundListValue / InboundMapValue / InboundMapEntry
namespace in_list {
constexpr uint32_t kValues = 1;
}  // namespace in_list
namespace in_map {
constexpr uint32_t kEntries = 1;
}  // namespace in_map
namespace in_entry {
constexpr uint32_t kStringKey = 1;
constexpr uint32_t kValue = 6;
}  // namespace in_entry

// InboundClassValue / InboundEnumValue / BamlTyClass
namespace in_class {
constexpr uint32_t kFields = 2;
constexpr uint32_t kClassTy = 3;
}  // namespace in_class
namespace in_enum {
constexpr uint32_t kName = 1;
constexpr uint32_t kValue = 2;
}  // namespace in_enum
namespace ty_class {
constexpr uint32_t kName = 1;
}  // namespace ty_class

// BamlOutboundResult oneof arms (baml_outbound.proto)
namespace out_result {
constexpr uint32_t kOk = 1;
constexpr uint32_t kError = 2;
constexpr uint32_t kPanic = 3;
}  // namespace out_result

// BamlOutboundError / BamlOutboundPanic (field layouts agree)
namespace out_thrown {
constexpr uint32_t kValue = 1;
constexpr uint32_t kTrace = 2;
constexpr uint32_t kIsExitPanic = 3;
constexpr uint32_t kExitCode = 4;
}  // namespace out_thrown

// BamlOutboundValue oneof arms
namespace out_value {
constexpr uint32_t kNullValue = 2;
constexpr uint32_t kStringValue = 3;
constexpr uint32_t kIntValue = 4;
constexpr uint32_t kFloatValue = 5;
constexpr uint32_t kBoolValue = 6;
constexpr uint32_t kClassValue = 7;
constexpr uint32_t kEnumValue = 8;
constexpr uint32_t kLiteralValue = 9;
constexpr uint32_t kListValue = 11;
constexpr uint32_t kMapValue = 12;
constexpr uint32_t kUnionVariantValue = 13;
constexpr uint32_t kHandleValue = 16;
constexpr uint32_t kMediaValue = 17;
constexpr uint32_t kUint8ArrayValue = 19;
constexpr uint32_t kBigintValue = 20;
}  // namespace out_value

// BamlValueList / BamlOutboundMapEntry / BamlValueMap
namespace out_list {
constexpr uint32_t kItems = 2;
}  // namespace out_list
namespace out_entry {
constexpr uint32_t kKey = 1;
constexpr uint32_t kValue = 2;
}  // namespace out_entry
namespace out_map {
constexpr uint32_t kEntries = 3;
}  // namespace out_map

// BamlValueClass / BamlValueEnum / BamlValueUnionVariant / BamlLiteralValue
namespace out_class {
constexpr uint32_t kName = 1;
constexpr uint32_t kFields = 2;
}  // namespace out_class
namespace out_enum {
constexpr uint32_t kName = 1;
constexpr uint32_t kValue = 2;
}  // namespace out_enum
namespace out_union {
constexpr uint32_t kValue = 6;
}  // namespace out_union
namespace out_literal {
constexpr uint32_t kStringValue = 1;
constexpr uint32_t kIntValue = 2;
constexpr uint32_t kBoolValue = 3;
constexpr uint32_t kBigintValue = 4;
constexpr uint32_t kFloatValue = 5;
}  // namespace out_literal

}  // namespace fields

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
  std::string string_v;          // String / BigInt / Enum variant name
  std::vector<uint8_t> bytes_v;  // Bytes
  std::string name;              // Class / Enum FQN
  std::vector<std::pair<std::string, OutboundValue>> fields;  // Class / Map
  std::vector<OutboundValue> items;                           // List

  const char* KindName() const {
    switch (kind) {
      case Kind::Null:
        return "null";
      case Kind::String:
        return "string";
      case Kind::Int:
        return "int";
      case Kind::Float:
        return "float";
      case Kind::Bool:
        return "bool";
      case Kind::Class:
        return "class";
      case Kind::Enum:
        return "enum";
      case Kind::List:
        return "list";
      case Kind::Map:
        return "map";
      case Kind::Handle:
        return "handle";
      case Kind::Media:
        return "media";
      case Kind::Bytes:
        return "bytes";
      case Kind::BigInt:
        return "bigint";
    }
    return "?";
  }
};

// Parses a BamlOutboundValue message. Union variants are unwrapped into
// their inner value (union metadata dropped, matching the Python bridge);
// literal values are widened to their base scalar kind.
inline OutboundValue ParseOutboundValue(wire::Reader r);

inline OutboundValue ParseLiteralValue(wire::Reader r) {
  OutboundValue v;
  uint32_t field;
  wire::WireType wt;
  while (r.Next(field, wt)) {
    switch (field) {
      case fields::out_literal::kStringValue:
        v.kind = OutboundValue::Kind::String;
        v.string_v = r.LenString();
        break;
      case fields::out_literal::kIntValue:
        v.kind = OutboundValue::Kind::Int;
        v.int_v = r.Int64();
        break;
      case fields::out_literal::kBoolValue:
        v.kind = OutboundValue::Kind::Bool;
        v.bool_v = r.Boolean();
        break;
      case fields::out_literal::kBigintValue:
        v.kind = OutboundValue::Kind::BigInt;
        v.string_v = r.LenString();
        break;
      case fields::out_literal::kFloatValue:
        // Float literal rides as source text.
        v.kind = OutboundValue::Kind::Float;
        v.float_v = std::stod(r.LenString());
        break;
      default:
        r.Skip(wt);
        break;
    }
  }
  return v;
}

inline std::vector<std::pair<std::string, OutboundValue>> ParseEntries(
    wire::Reader r, uint32_t entry_field) {
  // BamlValueClass.fields / BamlValueMap.entries: repeated
  // BamlOutboundMapEntry { string key = 1; BamlOutboundValue value = 2; }.
  std::vector<std::pair<std::string, OutboundValue>> out;
  uint32_t field;
  wire::WireType wt;
  while (r.Next(field, wt)) {
    if (field != entry_field) {
      r.Skip(wt);
      continue;
    }
    wire::Reader entry = r.LenPayload();
    std::string key;
    OutboundValue value;
    uint32_t ef;
    wire::WireType ewt;
    while (entry.Next(ef, ewt)) {
      switch (ef) {
        case fields::out_entry::kKey:
          key = entry.LenString();
          break;
        case fields::out_entry::kValue:
          value = ParseOutboundValue(entry.LenPayload());
          break;
        default:
          entry.Skip(ewt);
          break;
      }
    }
    out.emplace_back(std::move(key), std::move(value));
  }
  return out;
}

inline OutboundValue ParseOutboundValue(wire::Reader r) {
  OutboundValue v;
  uint32_t field;
  wire::WireType wt;
  while (r.Next(field, wt)) {
    switch (field) {
      case fields::out_value::kNullValue:
        r.Skip(wt);
        v.kind = OutboundValue::Kind::Null;
        break;
      case fields::out_value::kStringValue:
        v.kind = OutboundValue::Kind::String;
        v.string_v = r.LenString();
        break;
      case fields::out_value::kIntValue:
        v.kind = OutboundValue::Kind::Int;
        v.int_v = r.Int64();
        break;
      case fields::out_value::kFloatValue:
        v.kind = OutboundValue::Kind::Float;
        v.float_v = r.Fixed64Double();
        break;
      case fields::out_value::kBoolValue:
        v.kind = OutboundValue::Kind::Bool;
        v.bool_v = r.Boolean();
        break;
      case fields::out_value::kClassValue: {
        v.kind = OutboundValue::Kind::Class;
        wire::Reader cls = r.LenPayload();
        uint32_t cf;
        wire::WireType cwt;
        std::vector<std::pair<std::string, OutboundValue>> parsed_fields;
        while (cls.Next(cf, cwt)) {
          switch (cf) {
            case fields::out_class::kName:
              v.name = cls.LenString();
              break;
            case fields::out_class::kFields: {
              wire::Reader entry = cls.LenPayload();
              std::string key;
              OutboundValue value;
              uint32_t ef;
              wire::WireType ewt;
              while (entry.Next(ef, ewt)) {
                switch (ef) {
                  case fields::out_entry::kKey:
                    key = entry.LenString();
                    break;
                  case fields::out_entry::kValue:
                    value = ParseOutboundValue(entry.LenPayload());
                    break;
                  default:
                    entry.Skip(ewt);
                    break;
                }
              }
              parsed_fields.emplace_back(std::move(key), std::move(value));
              break;
            }
            default:
              cls.Skip(cwt);  // incl. type_args (unused this slice)
              break;
          }
        }
        v.fields = std::move(parsed_fields);
        break;
      }
      case fields::out_value::kEnumValue: {
        v.kind = OutboundValue::Kind::Enum;
        wire::Reader en = r.LenPayload();
        uint32_t ef;
        wire::WireType ewt;
        while (en.Next(ef, ewt)) {
          switch (ef) {
            case fields::out_enum::kName:
              v.name = en.LenString();
              break;
            case fields::out_enum::kValue:
              v.string_v = en.LenString();
              break;
            default:
              en.Skip(ewt);
              break;
          }
        }
        break;
      }
      case fields::out_value::kLiteralValue:  // widened to base scalar
        v = ParseLiteralValue(r.LenPayload());
        break;
      case fields::out_value::kListValue: {
        v.kind = OutboundValue::Kind::List;
        wire::Reader list = r.LenPayload();
        uint32_t lf;
        wire::WireType lwt;
        while (list.Next(lf, lwt)) {
          if (lf == fields::out_list::kItems) {
            v.items.push_back(ParseOutboundValue(list.LenPayload()));
          } else {
            list.Skip(lwt);
          }
        }
        break;
      }
      case fields::out_value::kMapValue: {
        v.kind = OutboundValue::Kind::Map;
        v.fields = ParseEntries(r.LenPayload(), fields::out_map::kEntries);
        break;
      }
      case fields::out_value::kUnionVariantValue: {  // unwrap inner value
        wire::Reader u = r.LenPayload();
        uint32_t uf;
        wire::WireType uwt;
        bool saw_inner = false;
        while (u.Next(uf, uwt)) {
          if (uf == fields::out_union::kValue) {
            v = ParseOutboundValue(u.LenPayload());
            saw_inner = true;
          } else {
            u.Skip(uwt);
          }
        }
        if (!saw_inner) {
          v.kind = OutboundValue::Kind::Null;
        }
        break;
      }
      case fields::out_value::kHandleValue:
        // No handle-typed surface this slice; the kind is kept so a
        // mismatch reports "handle", not "null".
        v.kind = OutboundValue::Kind::Handle;
        r.Skip(wt);
        break;
      case fields::out_value::kMediaValue:  // same treatment as handles
        v.kind = OutboundValue::Kind::Media;
        r.Skip(wt);
        break;
      case fields::out_value::kUint8ArrayValue: {
        wire::Reader b = r.LenPayload();
        v.kind = OutboundValue::Kind::Bytes;
        v.bytes_v.assign(b.data(), b.data() + b.size());
        break;
      }
      case fields::out_value::kBigintValue:
        v.kind = OutboundValue::Kind::BigInt;
        v.string_v = r.LenString();
        break;
      default:
        r.Skip(wt);
        break;
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

inline OutboundResult ParseOutboundResult(
    const std::vector<uint8_t>& envelope) {
  wire::Reader r(envelope.data(), envelope.size());
  OutboundResult out;
  uint32_t field;
  wire::WireType wt;
  bool saw_arm = false;
  while (r.Next(field, wt)) {
    switch (field) {
      case fields::out_result::kOk:
        out.arm = OutboundResult::Arm::Ok;
        out.value = ParseOutboundValue(r.LenPayload());
        saw_arm = true;
        break;
      case fields::out_result::kError:
      case fields::out_result::kPanic: {
        out.arm = field == fields::out_result::kError
                      ? OutboundResult::Arm::Error
                      : OutboundResult::Arm::Panic;
        saw_arm = true;
        wire::Reader arm_r = r.LenPayload();
        uint32_t af;
        wire::WireType awt;
        while (arm_r.Next(af, awt)) {
          switch (af) {
            case fields::out_thrown::kValue: {
              wire::Reader value_r = arm_r.LenPayload();
              out.raw_value.assign(value_r.data(),
                                   value_r.data() + value_r.size());
              out.value = ParseOutboundValue(value_r);
              break;
            }
            case fields::out_thrown::kTrace:
              out.trace.push_back(arm_r.LenString());
              break;
            case fields::out_thrown::kIsExitPanic:
              out.is_exit_panic = arm_r.Boolean();
              break;
            case fields::out_thrown::kExitCode:
              out.exit_code = arm_r.Int64();
              break;
            default:
              arm_r.Skip(awt);
              break;
          }
        }
        break;
      }
      default:
        r.Skip(wt);
        break;
    }
  }
  if (!saw_arm) {
    wire::Reader::Fail("result envelope has no ok/error/panic arm");
  }
  return out;
}

// ---------------------------------------------------------------------------
// Inbound encoding (CallFunctionArgs)
// ---------------------------------------------------------------------------

// Builds one CallFunctionArgs message: kwargs entries are appended via the
// value-writer callbacks that Codec<T>::Encode provides, then Finish() stamps
// the engine call id.
class ArgsEncoder {
 public:
  // `write_value` fills the InboundValue message body for this argument.
  template <typename WriteValue>
  void AddArg(const std::string& name, WriteValue&& write_value) {
    wire::Writer value_msg;
    write_value(value_msg);

    wire::Writer entry;  // InboundMapEntry
    entry.StringField(fields::in_entry::kStringKey, name);
    entry.MessageField(fields::in_entry::kValue, value_msg);
    args_.MessageField(fields::call_args::kKwargs, entry);
  }

  std::string Finish(uint64_t call_id) {
    args_.Uint64Field(fields::call_args::kCallId, call_id);
    return args_.bytes();
  }

 private:
  wire::Writer args_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_H_
