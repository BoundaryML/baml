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

  uint64_t handle_key = 0;
  int32_t handle_type = 0;

  // Media: which source variant is set is encoded in media_source.
  enum class MediaSource { None, Url, Base64, File };
  int32_t media_kind = 0;
  std::string media_mime;
  MediaSource media_source = MediaSource::None;
  std::string media_value;

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
      case 1:
        v.kind = OutboundValue::Kind::String;
        v.string_v = r.LenString();
        break;
      case 2:
        v.kind = OutboundValue::Kind::Int;
        v.int_v = r.Int64();
        break;
      case 3:
        v.kind = OutboundValue::Kind::Bool;
        v.bool_v = r.Boolean();
        break;
      case 4:
        v.kind = OutboundValue::Kind::BigInt;
        v.string_v = r.LenString();
        break;
      case 5:
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
        case 1:
          key = entry.LenString();
          break;
        case 2:
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
      case 2:  // null_value
        r.Skip(wt);
        v.kind = OutboundValue::Kind::Null;
        break;
      case 3:
        v.kind = OutboundValue::Kind::String;
        v.string_v = r.LenString();
        break;
      case 4:
        v.kind = OutboundValue::Kind::Int;
        v.int_v = r.Int64();
        break;
      case 5:
        v.kind = OutboundValue::Kind::Float;
        v.float_v = r.Fixed64Double();
        break;
      case 6:
        v.kind = OutboundValue::Kind::Bool;
        v.bool_v = r.Boolean();
        break;
      case 7: {  // class_value
        v.kind = OutboundValue::Kind::Class;
        wire::Reader cls = r.LenPayload();
        uint32_t cf;
        wire::WireType cwt;
        std::string raw;
        // Re-walk: name = 1, fields (entries) = 2, type_args = 3.
        std::vector<std::pair<std::string, OutboundValue>> fields;
        while (cls.Next(cf, cwt)) {
          switch (cf) {
            case 1:
              v.name = cls.LenString();
              break;
            case 2: {
              wire::Reader entry = cls.LenPayload();
              std::string key;
              OutboundValue value;
              uint32_t ef;
              wire::WireType ewt;
              while (entry.Next(ef, ewt)) {
                switch (ef) {
                  case 1:
                    key = entry.LenString();
                    break;
                  case 2:
                    value = ParseOutboundValue(entry.LenPayload());
                    break;
                  default:
                    entry.Skip(ewt);
                    break;
                }
              }
              fields.emplace_back(std::move(key), std::move(value));
              break;
            }
            default:
              cls.Skip(cwt);
              break;  // type_args: consumed by codegen later
          }
        }
        v.fields = std::move(fields);
        break;
      }
      case 8: {  // enum_value { name = 1, value = 2, is_dynamic = 3 }
        v.kind = OutboundValue::Kind::Enum;
        wire::Reader en = r.LenPayload();
        uint32_t ef;
        wire::WireType ewt;
        while (en.Next(ef, ewt)) {
          switch (ef) {
            case 1:
              v.name = en.LenString();
              break;
            case 2:
              v.string_v = en.LenString();
              break;
            default:
              en.Skip(ewt);
              break;
          }
        }
        break;
      }
      case 9:  // literal_value -> widened to base scalar
        v = ParseLiteralValue(r.LenPayload());
        break;
      case 11: {  // list_value { item_type = 1, items = 2 }
        v.kind = OutboundValue::Kind::List;
        wire::Reader list = r.LenPayload();
        uint32_t lf;
        wire::WireType lwt;
        while (list.Next(lf, lwt)) {
          if (lf == 2) {
            v.items.push_back(ParseOutboundValue(list.LenPayload()));
          } else {
            list.Skip(lwt);
          }
        }
        break;
      }
      case 12: {  // map_value { key_type = 1, value_type = 2, entries = 3 }
        v.kind = OutboundValue::Kind::Map;
        v.fields = ParseEntries(r.LenPayload(), 3);
        break;
      }
      case 13: {  // union_variant_value -> unwrap inner value (field 6)
        wire::Reader u = r.LenPayload();
        uint32_t uf;
        wire::WireType uwt;
        bool saw_inner = false;
        while (u.Next(uf, uwt)) {
          if (uf == 6) {
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
      case 16: {  // handle_value { key = 1, handle_type = 2, ty = 3 }
        v.kind = OutboundValue::Kind::Handle;
        wire::Reader h = r.LenPayload();
        uint32_t hf;
        wire::WireType hwt;
        while (h.Next(hf, hwt)) {
          switch (hf) {
            case 1:
              v.handle_key = h.Varint();
              break;
            case 2:
              v.handle_type = static_cast<int32_t>(h.Varint());
              break;
            default:
              h.Skip(hwt);
              break;
          }
        }
        break;
      }
      case 17: {  // media_value
        v.kind = OutboundValue::Kind::Media;
        wire::Reader m = r.LenPayload();
        uint32_t mf;
        wire::WireType mwt;
        while (m.Next(mf, mwt)) {
          switch (mf) {
            case 1:
              v.media_kind = static_cast<int32_t>(m.Varint());
              break;
            case 2:
              v.media_mime = m.LenString();
              break;
            case 3:
              v.media_source = OutboundValue::MediaSource::Url;
              v.media_value = m.LenString();
              break;
            case 4:
              v.media_source = OutboundValue::MediaSource::Base64;
              v.media_value = m.LenString();
              break;
            case 5:
              v.media_source = OutboundValue::MediaSource::File;
              v.media_value = m.LenString();
              break;
            default:
              m.Skip(mwt);
              break;
          }
        }
        break;
      }
      case 19: {  // uint8array_value
        wire::Reader b = r.LenPayload();
        v.kind = OutboundValue::Kind::Bytes;
        v.bytes_v.assign(b.data(), b.data() + b.size());
        break;
      }
      case 20:
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
      case 1:
        out.arm = OutboundResult::Arm::Ok;
        out.value = ParseOutboundValue(r.LenPayload());
        saw_arm = true;
        break;
      case 2:
      case 3: {
        out.arm = field == 2 ? OutboundResult::Arm::Error
                             : OutboundResult::Arm::Panic;
        saw_arm = true;
        wire::Reader arm_r = r.LenPayload();
        uint32_t af;
        wire::WireType awt;
        while (arm_r.Next(af, awt)) {
          switch (af) {
            case 1: {
              wire::Reader value_r = arm_r.LenPayload();
              out.raw_value.assign(value_r.data(),
                                   value_r.data() + value_r.size());
              out.value = ParseOutboundValue(value_r);
              break;
            }
            case 2:
              out.trace.push_back(arm_r.LenString());
              break;
            case 3:
              out.is_exit_panic = arm_r.Boolean();
              break;
            case 4:
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
    entry.StringField(1, name);
    entry.MessageField(6, value_msg);
    args_.MessageField(1, entry);  // CallFunctionArgs.kwargs
  }

  // Adds an argument whose value is BAML null (absent oneof).
  void AddNullArg(const std::string& name) {
    AddArg(name, [](wire::Writer&) {});
  }

  // Adds one explicit TypeVar binding (CallFunctionArgs.type_args entry).
  // Bindings are added in De Bruijn order: enclosing class params first,
  // then the callee's own generic params. `write_ty` fills the BamlTy
  // message body for the concrete binding.
  template <typename WriteTy>
  void AddTypeArg(const std::string& type_var, WriteTy&& write_ty) {
    wire::Writer ty_msg;
    write_ty(ty_msg);

    wire::Writer binding;  // BamlTyArg
    binding.StringField(1, type_var);
    binding.MessageField(2, ty_msg);
    args_.MessageField(3, binding);  // CallFunctionArgs.type_args
  }

  std::string Finish(uint64_t call_id) {
    args_.Uint64Field(2, call_id);  // CallFunctionArgs.call_id
    return args_.bytes();
  }

 private:
  wire::Writer args_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_H_
