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
#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <string>
#include <utility>

#include "baml_bridge/cffi/v1/baml_inbound.pb.h"
#include "baml_bridge/cffi/v1/baml_outbound.pb.h"

namespace baml {

// Private lowering used by the generated, strongly typed baml_options facade.
struct invocation_options {
  std::function<uint64_t()> trace_key;
  std::function<void(::baml_bridge::cffi::v1::InboundValue&)> encode_cancel;
  std::optional<int64_t> timeout;
};

namespace detail {

namespace pb = ::baml_bridge::cffi::v1;

// Explicitly owned callback state can be carried to another thread by SDK
// bindings.
struct invocation_state {
  uint64_t key;
  pb::BamlOutboundValue cancel;
  explicit invocation_state(uint64_t key) : key(key) {}
  ~invocation_state();
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

inline invocation_state::~invocation_state() {
  api().handle_release(key);
  release_control_value(cancel);
}
inline pb::BamlOutboundValue clone_control_value(
    const pb::BamlOutboundValue& value) {
  pb::BamlOutboundValue result(value);
  std::function<void(pb::BamlOutboundValue*)> visit =
      [&](pb::BamlOutboundValue* item) {
        if (item->has_handle_value() &&
            item->handle_value().handle_type() != pb::HOST_VALUE_CALLABLE &&
            item->handle_value().handle_type() != pb::HOST_VALUE_OPAQUE) {
          uint64_t key = 0;
          if (api().handle_clone(item->handle_value().key(), &key) != 0)
            throw error("invocation handle clone failed");
          item->mutable_handle_value()->set_key(key);
        }
        if (item->has_list_value())
          for (auto& child : *item->mutable_list_value()->mutable_items())
            visit(&child);
        if (item->has_map_value())
          for (auto& child : *item->mutable_map_value()->mutable_entries())
            visit(child.mutable_value());
        if (item->has_class_value())
          for (auto& child : *item->mutable_class_value()->mutable_fields())
            visit(child.mutable_value());
        if (item->has_union_variant_value())
          visit(item->mutable_union_variant_value()->mutable_value());
      };
  visit(&result);
  return result;
}
inline pb::BamlOutboundValue current_trace_context() {
  BamlBuffer buffer{};
  if (api().invocation_context(
          current_invocation_state ? current_invocation_state->key : 0,
          &buffer) != 0)
    throw error("invocation context read failed");
  pb::BamlOutboundValue value;
  const bool ok =
      value.ParseFromArray(buffer.ptr, static_cast<int>(buffer.len));
  api().free_buffer(buffer);
  if (!ok) throw error("invalid invocation context payload");
  return value;
}
inline void release_input_value(const pb::InboundValue& value) {
  if (value.has_handle() &&
      value.handle().handle_type() != pb::HOST_VALUE_CALLABLE &&
      value.handle().handle_type() != pb::HOST_VALUE_OPAQUE)
    api().handle_release(value.handle().key());
  if (value.has_list_value())
    for (const auto& item : value.list_value().values())
      release_input_value(item);
  if (value.has_map_value())
    for (const auto& item : value.map_value().entries())
      release_input_value(item.value());
  if (value.has_class_value())
    for (const auto& item : value.class_value().fields())
      release_input_value(item.value());
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
  explicit args_encoder(const invocation_options& controls = {}) {
    if (controls.timeout &&
        (*controls.timeout < 0 || *controls.timeout > 2147483647))
      throw error("timeout_ms must be between 0 and 2147483647");
    id_ = api().new_function_call();
    if (!id_) throw error("runtime call allocation failed");
    try {
      auto* invocation = args_.mutable_invocation();
      invocation->set_host_environment(id_);
      inherited_ = current_invocation_state;
      invocation->set_inherited_state(inherited_ ? inherited_->key : 0);
      if (controls.timeout) {
        uint64_t now = 0;
        if (api().invocation_clock_ns(id_, &now) != 0)
          throw error("runtime clock failed");
        const uint64_t duration =
            static_cast<uint64_t>(*controls.timeout) * 1000000;
        if (duration > std::numeric_limits<uint64_t>::max() - now)
          throw error("deadline overflow");
        invocation->set_deadline_ns(now + duration);
      }
      if (controls.trace_key) {
        BamlBuffer buffer{};
        const auto status = api().trace_selection(id_, controls.trace_key(),
                                                  &buffer, &reservation_);
        if (status != 0) throw error("invalid trace selection");
        const bool ok = invocation->mutable_trace()->ParseFromArray(
            buffer.ptr, static_cast<int>(buffer.len));
        api().free_buffer(buffer);
        if (!ok) throw error("invalid trace selection payload");
      }
      if (controls.encode_cancel)
        controls.encode_cancel(*invocation->mutable_cancel());
    } catch (...) {
      cleanup();
      throw;
    }
  }
  args_encoder(const args_encoder&) = delete;
  args_encoder& operator=(const args_encoder&) = delete;
  ~args_encoder() { cleanup(); }
  uint64_t id() const { return id_; }
  void submitted() {
    id_ = 0;
    reservation_ = 0;
  }

  // `write_value` fills the InboundValue for this argument.
  template <typename WriteValue>
  void add_arg(const std::string& name, WriteValue&& write_value) {
    pb::InboundMapEntry* entry = args_.add_kwargs();
    entry->set_string_key(name);
    write_value(*entry->mutable_value());
  }

  void add_type(const std::string& name, const pb::BamlTy& type) {
    auto* binding = args_.add_type_args();
    binding->set_type_var(name);
    *binding->mutable_type_value() = type;
  }

  std::string finish(uint64_t call_id, const std::string& function_name) {
    args_.set_call_id(call_id);
    args_.set_function_name(function_name);
    return args_.SerializeAsString();
  }

  std::string finish(uint64_t call_id, uint64_t function_handle) {
    args_.set_call_id(call_id);
    args_.set_function_handle(function_handle);
    return args_.SerializeAsString();
  }

 private:
  void cleanup() noexcept {
    if (!id_) return;
    try {
      for (const auto& argument : args_.kwargs())
        release_input_value(argument.value());
      if (args_.invocation().has_cancel())
        release_input_value(args_.invocation().cancel());
      if (reservation_) api().handle_release(reservation_);
      api().release_function_call(id_);
    } catch (...) {
    }
    id_ = 0;
    reservation_ = 0;
  }
  uint64_t id_ = 0, reservation_ = 0;
  std::shared_ptr<invocation_state> inherited_;
  pb::CallFunctionArgs args_;
};

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_H_
