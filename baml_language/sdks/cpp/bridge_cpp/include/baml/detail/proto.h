#ifndef BAML_DETAIL_PROTO_H_
#define BAML_DETAIL_PROTO_H_

// The typed wire layer over the generated protobuf-lite bindings for the
// bridge_ctypes CFFI schemas (checked-in under bridge_cpp/pb/, pinned to
// the repo's vendored protoc). args_encoder builds one CallFunctionArgs;
// the helpers here normalize outbound values (union unwrap, arm naming)
// for the codec layer. Copies are deliberate and visible (contract:
// coarse-grained boundary, measurable copies).

#include <baml/detail/loader.h>

#include <cstdint>
#include <memory>
#include <string>

#include "baml_bridge/cffi/v1/baml_inbound.pb.h"
#include "baml_bridge/cffi/v1/baml_outbound.pb.h"

namespace baml {

namespace detail {

namespace pb = ::baml_bridge::cffi::v1;

// Explicitly owned callback state can be carried to another thread by SDK
// bindings.
struct invocation_state {
  uint64_t key;
  explicit invocation_state(uint64_t key) : key(key) {}
  ~invocation_state() { api().handle_release(key); }
};
inline thread_local std::shared_ptr<invocation_state> current_invocation_state;

inline void release_control_value(const pb::BamlOutboundValue& value) {
  switch (value.value_case()) {
    case pb::BamlOutboundValue::kHandleValue:
      if (value.handle_value().handle_type() != pb::HOST_VALUE_CALLABLE &&
          value.handle_value().handle_type() != pb::HOST_VALUE_OPAQUE)
        api().handle_release(value.handle_value().key());
      break;
    case pb::BamlOutboundValue::kListValue:
      for (const auto& v : value.list_value().items()) release_control_value(v);
      break;
    case pb::BamlOutboundValue::kMapValue:
      for (const auto& v : value.map_value().entries())
        release_control_value(v.value());
      break;
    case pb::BamlOutboundValue::kClassValue:
      for (const auto& v : value.class_value().fields())
        release_control_value(v.value());
      break;
    case pb::BamlOutboundValue::kUnionVariantValue:
      release_control_value(value.union_variant_value().value());
      break;
    default:
      break;
  }
}

// Human-readable arm name for decode diagnostics.
inline const char* arm_name(pb::BamlOutboundValue::ValueCase c) {
  switch (c) {
    case pb::BamlOutboundValue::kNullValue:
    case pb::BamlOutboundValue::VALUE_NOT_SET:
      return "null";
    case pb::BamlOutboundValue::kStringValue:
      return "string";
    case pb::BamlOutboundValue::kIntValue:
      return "int";
    case pb::BamlOutboundValue::kFloatValue:
      return "float";
    case pb::BamlOutboundValue::kBoolValue:
      return "bool";
    case pb::BamlOutboundValue::kClassValue:
      return "class";
    case pb::BamlOutboundValue::kEnumValue:
      return "enum";
    case pb::BamlOutboundValue::kLiteralValue:
      return "literal";
    case pb::BamlOutboundValue::kListValue:
      return "list";
    case pb::BamlOutboundValue::kMapValue:
      return "map";
    case pb::BamlOutboundValue::kUnionVariantValue:
      return "union variant";
    case pb::BamlOutboundValue::kHandleValue:
      return "handle";
    case pb::BamlOutboundValue::kMediaValue:
      return "media";
    case pb::BamlOutboundValue::kPromptAstValue:
      return "prompt ast";
    case pb::BamlOutboundValue::kUint8ArrayValue:
      return "bytes";
    case pb::BamlOutboundValue::kBigintValue:
      return "bigint";
    case pb::BamlOutboundValue::kTyValue:
      return "type";
    case pb::BamlOutboundValue::kTyDefValue:
      return "runtime type definition (requires BEP-066 reflection support, "
             "which the C++ SDK does not provide)";
  }
  return "?";
}

// variant variants carry metadata the C++ surface drops (Python parity):
// resolve to the innermost non-union value. A variant with no inner value
// resolves to the default instance, whose arm is VALUE_NOT_SET (= null).
inline const pb::BamlOutboundValue& unwrap(const pb::BamlOutboundValue& v) {
  const pb::BamlOutboundValue* cur = &v;
  while (cur->value_case() == pb::BamlOutboundValue::kUnionVariantValue) {
    cur = &cur->union_variant_value().value();
  }
  return *cur;
}

// Builds one CallFunctionArgs message: kwargs entries are filled via the
// value-writer callbacks that codec<T>::encode provides, then finish()
// stamps the engine call id and serializes.
class args_encoder {
 public:
  // `write_value` fills the InboundValue for this argument.
  template <typename WriteValue>
  void add_arg(const std::string& name, WriteValue&& write_value) {
    pb::InboundMapEntry* entry = args_.add_kwargs();
    entry->set_string_key(name);
    write_value(*entry->mutable_value());
  }

  std::string finish(uint64_t call_id, const std::string& function_name) {
    args_.set_call_id(call_id);
    args_.mutable_invocation()->set_inherited_state(
        current_invocation_state ? current_invocation_state->key : 0);
    args_.mutable_invocation()->set_host_environment(call_id);
    args_.set_function_name(function_name);
    return args_.SerializeAsString();
  }

  std::string finish(uint64_t call_id, uint64_t function_handle) {
    args_.set_call_id(call_id);
    args_.mutable_invocation()->set_inherited_state(
        current_invocation_state ? current_invocation_state->key : 0);
    args_.mutable_invocation()->set_host_environment(call_id);
    args_.set_function_handle(function_handle);
    return args_.SerializeAsString();
  }

 private:
  pb::CallFunctionArgs args_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_H_
