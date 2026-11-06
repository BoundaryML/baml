#ifndef BAML_DETAIL_PROTO_H_
#define BAML_DETAIL_PROTO_H_

// The typed wire layer over the generated protobuf-lite bindings for the
// bridge_ctypes CFFI schemas (checked-in under bridge_cpp/pb/, pinned to
// the repo's vendored protoc). The helpers here normalize outbound values
// (union unwrap, arm naming) for the codec layer and release the engine
// references they carry. Copies are deliberate and visible (contract:
// coarse-grained boundary, measurable copies).

#include <baml/detail/loader.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

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

// The engine refuses a wire message nested more than this many messages
// deep: prost's recursion limit, bridge_ctypes' `WIRE_NESTING_LIMIT`.
inline constexpr std::size_t wire_nesting_limit = 100;

// The messages that enclose a call argument's `InboundValue`
// (`CallFunctionArgs` -> `InboundMapEntry`), and a callback's result or
// thrown value (`HostCallCompletion`): bridge_ctypes' `ARGUMENT_DEPTH` and
// `RESULT_DEPTH`.
inline constexpr std::size_t argument_depth = 2;
inline constexpr std::size_t result_depth = 1;

// The most levels an encode follows a value down, where each list, map and
// class opens one. A level costs the wire at least two messages (a list item
// is an `InboundListValue` and an `InboundValue`; a map entry or class field
// adds an `InboundMapEntry`), so a value nested deeper than this can never
// cross. It bounds the encoder's recursion, which otherwise follows the
// value down however deep it goes, on whatever stack the encode runs on;
// whether a value that stays inside it crosses is `require_fits_wire`'s
// exact measure.
inline constexpr std::size_t value_nesting_limit = wire_nesting_limit / 2;

[[noreturn]] inline void refuse_too_deep() {
  throw bridge_error("BAML encode error: the value nests deeper than the " +
                     std::to_string(wire_nesting_limit) +
                     " nested messages the wire carries");
}

// How many levels the encode running on this thread is inside. An encode
// never leaves the thread it starts on (an argument encodes on the caller's
// thread, a callback's result on the callback's), so a count per thread is
// the count per encode.
inline thread_local std::size_t encode_nesting_depth = 0;

// One level of the value being encoded, open for the guard's lifetime. Every
// encoder that nests an InboundValue inside another holds one while it
// writes the children; opening the level past `value_nesting_limit` refuses
// the value.
class encode_level {
 public:
  encode_level() {
    if (encode_nesting_depth == value_nesting_limit) refuse_too_deep();
    ++encode_nesting_depth;
  }
  ~encode_level() { --encode_nesting_depth; }
  encode_level(const encode_level&) = delete;
  encode_level& operator=(const encode_level&) = delete;
};

// How many messages a type nests below itself, as the decoder counts them
// (bridge_ctypes' `baml_ty_depth`). A primitive is 1 (`BamlTyPrimitive`);
// `list<int>` is 3 (`BamlTyList`, `BamlTy`, `BamlTyPrimitive`).
inline std::size_t baml_ty_depth(const pb::BamlTy& ty) {
  const auto nested = [](const pb::BamlTy& child) {
    return 1 + baml_ty_depth(child);
  };
  const auto deepest = [&](const auto& children) {
    std::size_t depth = 0;
    for (const pb::BamlTy& child : children)
      depth = (std::max)(depth, nested(child));
    return depth;
  };
  switch (ty.ty_case()) {
    case pb::BamlTy::TY_NOT_SET:
      return 0;
    case pb::BamlTy::kPrimitive:
    case pb::BamlTy::kEnum:
    case pb::BamlTy::kLiteral:
    case pb::BamlTy::kUnknown:
    case pb::BamlTy::kMedia:
    case pb::BamlTy::kEnumVariant:
    case pb::BamlTy::kMetaType:
    case pb::BamlTy::kResource:
    case pb::BamlTy::kPromptAst:
    case pb::BamlTy::kTypeVar:
    case pb::BamlTy::kNever:
      return 1;
    case pb::BamlTy::kClassTy:
      return 1 + deepest(ty.class_ty().type_args());
    case pb::BamlTy::kTypeAlias:
      return 1 + deepest(ty.type_alias().type_args());
    case pb::BamlTy::kList:
      return 1 + (ty.list().has_item() ? nested(ty.list().item()) : 0);
    case pb::BamlTy::kMap:
      return 1 +
             (std::max)(ty.map().has_key() ? nested(ty.map().key()) : 0,
                        ty.map().has_value() ? nested(ty.map().value()) : 0);
    case pb::BamlTy::kOptional:
      return 1 +
             (ty.optional().has_inner() ? nested(ty.optional().inner()) : 0);
    case pb::BamlTy::kUnion:
      return 1 + deepest(ty.union_().options());
    case pb::BamlTy::kInterface: {
      std::size_t bindings = 0;
      for (const pb::BamlTyAssociatedBinding& binding :
           ty.interface().bindings())
        bindings =
            (std::max)(bindings,
                       1 + (binding.has_ty() ? nested(binding.ty()) : 0));
      return 1 + (std::max)(deepest(ty.interface().type_args()), bindings);
    }
    case pb::BamlTy::kFunction: {
      const pb::BamlTyFunction& function = ty.function();
      std::size_t depth = 0;
      for (const pb::BamlTyFunctionParam& param : function.params())
        depth =
            (std::max)(depth, 1 + (param.has_ty() ? nested(param.ty()) : 0));
      if (function.has_ret()) depth = (std::max)(depth, nested(function.ret()));
      if (function.has_throws())
        depth = (std::max)(depth, nested(function.throws()));
      return 1 + depth;
    }
    case pb::BamlTy::kFuture:
      return 1 +
             (std::max)(ty.future().has_value() ? nested(ty.future().value())
                                                : 0,
                        ty.future().has_error() ? nested(ty.future().error())
                                                : 0);
    case pb::BamlTy::kAssociatedTypeProjection: {
      const pb::BamlTyAssociatedTypeProjection& projection =
          ty.associated_type_projection();
      return 1 +
             (std::max)(projection.has_base() ? nested(projection.base()) : 0,
                        projection.has_interface()
                            ? nested(projection.interface())
                            : 0);
    }
  }
  return 0;
}

inline std::size_t media_depth(const pb::BamlValueMedia& media) {
  switch (media.value_case()) {
    case pb::BamlValueMedia::VALUE_NOT_SET:
    case pb::BamlValueMedia::kUrl:
    case pb::BamlValueMedia::kBase64:
      return 0;
    case pb::BamlValueMedia::kFileContent:
      return 1;
  }
  return 0;
}

inline std::size_t prompt_simple_depth(
    const pb::BamlValuePromptAstSimple& simple) {
  switch (simple.value_case()) {
    case pb::BamlValuePromptAstSimple::VALUE_NOT_SET:
    case pb::BamlValuePromptAstSimple::kString:
      return 0;
    case pb::BamlValuePromptAstSimple::kMedia:
      return 1 + media_depth(simple.media());
    case pb::BamlValuePromptAstSimple::kMultiple: {
      std::size_t depth = 0;
      for (const pb::BamlValuePromptAstSimple& item : simple.multiple().items())
        depth = (std::max)(depth, 1 + prompt_simple_depth(item));
      return 1 + depth;
    }
  }
  return 0;
}

inline std::size_t prompt_depth(const pb::BamlValuePromptAst& prompt) {
  switch (prompt.value_case()) {
    case pb::BamlValuePromptAst::VALUE_NOT_SET:
      return 0;
    case pb::BamlValuePromptAst::kSimple:
      return 1 + prompt_simple_depth(prompt.simple());
    case pb::BamlValuePromptAst::kMessage:
      return 1 + (prompt.message().has_content()
                      ? 1 + prompt_simple_depth(prompt.message().content())
                      : 0);
    case pb::BamlValuePromptAst::kMultiple: {
      std::size_t depth = 0;
      for (const pb::BamlValuePromptAst& item : prompt.multiple().items())
        depth = (std::max)(depth, 1 + prompt_depth(item));
      return 1 + depth;
    }
  }
  return 0;
}

inline std::size_t inbound_value_depth(const pb::InboundValue& value);

// What an `InboundMapEntry` nests: its value, or an `InboundEnumValue` key,
// the one key kind that is a message.
inline std::size_t inbound_entry_depth(const pb::InboundMapEntry& entry) {
  const std::size_t key =
      entry.key_case() == pb::InboundMapEntry::kEnumKey ? 1 : 0;
  return (std::max)(key, entry.has_value()
                             ? 1 + inbound_value_depth(entry.value())
                             : 0);
}

// How many messages `value` nests below itself, counting its own type
// statement and the payload of every leaf, exactly as the decoder counts
// them (bridge_ctypes' `inbound_value_depth`). A scalar is 0; a list of
// scalars is 2 (`InboundListValue`, `InboundValue`).
inline std::size_t inbound_value_depth(const pb::InboundValue& value) {
  const std::size_t statement =
      value.has_value_type() ? 1 + baml_ty_depth(value.value_type()) : 0;
  std::size_t payload = 0;
  switch (value.value_case()) {
    case pb::InboundValue::VALUE_NOT_SET:
    case pb::InboundValue::kStringValue:
    case pb::InboundValue::kIntValue:
    case pb::InboundValue::kFloatValue:
    case pb::InboundValue::kBoolValue:
    case pb::InboundValue::kUint8ArrayValue:
    case pb::InboundValue::kBigintValue:
    case pb::InboundValue::kJsNumberValue:
      break;
    // `InboundEnumValue`, `BamlHandle`: one message, no nesting.
    case pb::InboundValue::kEnumValue:
    case pb::InboundValue::kHandle:
      payload = 1;
      break;
    // `InboundListValue` -> each `InboundValue`.
    case pb::InboundValue::kListValue: {
      std::size_t items = 0;
      for (const pb::InboundValue& item : value.list_value().values())
        items = (std::max)(items, 1 + inbound_value_depth(item));
      payload = 1 + items;
      break;
    }
    // `InboundMapValue` -> `InboundMapEntry` -> `InboundValue`.
    case pb::InboundValue::kMapValue: {
      std::size_t entries = 0;
      for (const pb::InboundMapEntry& entry : value.map_value().entries())
        entries = (std::max)(entries, 1 + inbound_entry_depth(entry));
      payload = 1 + entries;
      break;
    }
    // `InboundClassValue` -> `InboundMapEntry` -> `InboundValue`.
    case pb::InboundValue::kClassValue: {
      std::size_t fields = 0;
      for (const pb::InboundMapEntry& field : value.class_value().fields())
        fields = (std::max)(fields, 1 + inbound_entry_depth(field));
      payload = 1 + fields;
      break;
    }
    case pb::InboundValue::kTyValue:
      payload = 1 + baml_ty_depth(value.ty_value());
      break;
    case pb::InboundValue::kMediaValue:
      payload = 1 + media_depth(value.media_value());
      break;
    case pb::InboundValue::kPromptAstValue:
      payload = 1 + prompt_depth(value.prompt_ast_value());
      break;
  }
  return (std::max)(statement, payload);
}

// Refuses `value` unless it fits the wire `enclosing` messages below the
// message that carries it (`argument_depth` or `result_depth`).
inline void require_fits_wire(const pb::InboundValue& value,
                              std::size_t enclosing) {
  if (enclosing + inbound_value_depth(value) > wire_nesting_limit)
    refuse_too_deep();
}

// Calls `visit` on every class, enum, alias, interface and enum-variant
// message under `ty` (the messages whose `head` names a declaration), once
// per occurrence. Unset singular children are skipped: reaching one through
// its mutable accessor would create it.
template <typename Visit>
void for_each_nominal(pb::BamlTy& ty, const Visit& visit) {
  const auto walk = [&](pb::BamlTy& child) { for_each_nominal(child, visit); };
  const auto walk_all = [&](auto& children) {
    for (pb::BamlTy& child : children) walk(child);
  };
  switch (ty.ty_case()) {
    case pb::BamlTy::kClassTy:
      visit(*ty.mutable_class_ty());
      walk_all(*ty.mutable_class_ty()->mutable_type_args());
      return;
    case pb::BamlTy::kEnum:
      visit(*ty.mutable_enum_());
      return;
    case pb::BamlTy::kTypeAlias:
      visit(*ty.mutable_type_alias());
      walk_all(*ty.mutable_type_alias()->mutable_type_args());
      return;
    case pb::BamlTy::kInterface: {
      pb::BamlTyInterface& interface_ty = *ty.mutable_interface();
      visit(interface_ty);
      walk_all(*interface_ty.mutable_type_args());
      for (pb::BamlTyAssociatedBinding& binding :
           *interface_ty.mutable_bindings())
        if (binding.has_ty()) walk(*binding.mutable_ty());
      return;
    }
    case pb::BamlTy::kEnumVariant:
      visit(*ty.mutable_enum_variant());
      return;
    case pb::BamlTy::kList:
      if (ty.list().has_item()) walk(*ty.mutable_list()->mutable_item());
      return;
    case pb::BamlTy::kMap:
      if (ty.map().has_key()) walk(*ty.mutable_map()->mutable_key());
      if (ty.map().has_value()) walk(*ty.mutable_map()->mutable_value());
      return;
    case pb::BamlTy::kOptional:
      if (ty.optional().has_inner())
        walk(*ty.mutable_optional()->mutable_inner());
      return;
    case pb::BamlTy::kUnion:
      walk_all(*ty.mutable_union_()->mutable_options());
      return;
    case pb::BamlTy::kFunction: {
      pb::BamlTyFunction& function = *ty.mutable_function();
      for (pb::BamlTyFunctionParam& param : *function.mutable_params())
        if (param.has_ty()) walk(*param.mutable_ty());
      if (function.has_ret()) walk(*function.mutable_ret());
      if (function.has_throws()) walk(*function.mutable_throws());
      return;
    }
    case pb::BamlTy::kFuture:
      if (ty.future().has_value()) walk(*ty.mutable_future()->mutable_value());
      if (ty.future().has_error()) walk(*ty.mutable_future()->mutable_error());
      return;
    case pb::BamlTy::kAssociatedTypeProjection: {
      pb::BamlTyAssociatedTypeProjection& projection =
          *ty.mutable_associated_type_projection();
      if (projection.has_base()) walk(*projection.mutable_base());
      if (projection.has_interface()) walk(*projection.mutable_interface());
      return;
    }
    case pb::BamlTy::kPrimitive:
    case pb::BamlTy::kLiteral:
    case pb::BamlTy::kUnknown:
    case pb::BamlTy::kMedia:
    case pb::BamlTy::kMetaType:
    case pb::BamlTy::kResource:
    case pb::BamlTy::kPromptAst:
    case pb::BamlTy::kTypeVar:
    case pb::BamlTy::kNever:
    case pb::BamlTy::TY_NOT_SET:
      return;
  }
}

// A `live` head is a key in the engine's handle table, owned like an engine
// handle: each occurrence the engine sends is one reference the host holds.
// The host never resolves one; two live heads are the same declaration to
// the host only when their keys are equal.
//
// Releases the reference of every `live` head under `ty`. Walks its own copy.
inline void release_live_heads(pb::BamlTy ty) {
  for_each_nominal(ty, [](const auto& nominal) {
    if (nominal.has_live()) api().handle_release(nominal.live());
  });
}

// `ty` with a fresh reference per `live` head, as a host stating a type must
// send it (the engine takes one per occurrence). On failure releases the
// clones it made and leaves the caller's references alone.
inline pb::BamlTy restate_live_heads(pb::BamlTy ty) {
  std::vector<uint64_t> cloned;
  try {
    for_each_nominal(ty, [&](auto& nominal) {
      if (!nominal.has_live()) return;
      uint64_t key = 0;
      if (api().handle_clone(nominal.live(), &key) != 0)
        throw bridge_error("type reference clone failed");
      cloned.push_back(key);
      nominal.set_live(key);
    });
  } catch (...) {
    for (const uint64_t key : cloned) api().handle_release(key);
    throw;
  }
  return ty;
}

// Calls `visit` on every type value in `value`, through its containers.
template <typename Visit>
void for_each_type_value(pb::BamlOutboundValue& value, const Visit& visit) {
  switch (value.value_case()) {
    case pb::BamlOutboundValue::kTyValue:
      visit(*value.mutable_ty_value());
      return;
    case pb::BamlOutboundValue::kListValue:
      for (auto& item : *value.mutable_list_value()->mutable_items())
        for_each_type_value(item, visit);
      return;
    case pb::BamlOutboundValue::kMapValue:
      for (auto& entry : *value.mutable_map_value()->mutable_entries())
        for_each_type_value(*entry.mutable_value(), visit);
      return;
    case pb::BamlOutboundValue::kClassValue:
      for (auto& field : *value.mutable_class_value()->mutable_fields())
        for_each_type_value(*field.mutable_value(), visit);
      return;
    case pb::BamlOutboundValue::kUnionVariantValue:
      if (value.union_variant_value().has_value())
        for_each_type_value(
            *value.mutable_union_variant_value()->mutable_value(), visit);
      return;
    default:
      return;
  }
}

// Whether a handle of `type` is a host key: the host's own registration,
// never an engine row.
inline bool is_host_handle(pb::BamlHandleType type) {
  return type == pb::HOST_VALUE_CALLABLE || type == pb::HOST_VALUE_OPAQUE;
}

// A host key the engine sends comes with a pin, an engine row that keeps the
// engine's references to the key while the host reads it. The wire it came on
// releases it once (0 is none).
inline void release_pin(const pb::BamlOutboundHandle& handle) {
  if (handle.pin() != 0) api().handle_release(handle.pin());
}

// Decoding only borrows a value's type values and pins: no typed codec keeps
// one, `baml::value` restates its own type values, and nothing holds a pin
// past the wire. So the wire they arrived on releases them once decoding is
// done, however it ends.
inline void release_borrowed(const pb::BamlOutboundValue& value) {
  switch (value.value_case()) {
    case pb::BamlOutboundValue::kTyValue:
      release_live_heads(value.ty_value());
      return;
    case pb::BamlOutboundValue::kHandleValue:
      if (is_host_handle(value.handle_value().handle_type()))
        release_pin(value.handle_value());
      return;
    case pb::BamlOutboundValue::kListValue:
      for (const auto& item : value.list_value().items())
        release_borrowed(item);
      return;
    case pb::BamlOutboundValue::kMapValue:
      for (const auto& entry : value.map_value().entries())
        release_borrowed(entry.value());
      return;
    case pb::BamlOutboundValue::kClassValue:
      for (const auto& field : value.class_value().fields())
        release_borrowed(field.value());
      return;
    case pb::BamlOutboundValue::kUnionVariantValue:
      release_borrowed(value.union_variant_value().value());
      return;
    case pb::BamlOutboundValue::VALUE_NOT_SET:
    case pb::BamlOutboundValue::kNullValue:
    case pb::BamlOutboundValue::kStringValue:
    case pb::BamlOutboundValue::kIntValue:
    case pb::BamlOutboundValue::kFloatValue:
    case pb::BamlOutboundValue::kBoolValue:
    case pb::BamlOutboundValue::kEnumValue:
    case pb::BamlOutboundValue::kLiteralValue:
    case pb::BamlOutboundValue::kMediaValue:
    case pb::BamlOutboundValue::kPromptAstValue:
    case pb::BamlOutboundValue::kUint8ArrayValue:
    case pb::BamlOutboundValue::kBigintValue:
      return;
  }
}

// Calls `visit` on every host-key handle in `value`, through its containers.
template <typename Visit>
void for_each_host_handle(pb::BamlOutboundValue& value, const Visit& visit) {
  if (value.has_handle_value() &&
      is_host_handle(value.handle_value().handle_type()))
    visit(*value.mutable_handle_value());
  if (value.has_list_value())
    for (auto& item : *value.mutable_list_value()->mutable_items())
      for_each_host_handle(item, visit);
  if (value.has_map_value())
    for (auto& entry : *value.mutable_map_value()->mutable_entries())
      for_each_host_handle(*entry.mutable_value(), visit);
  if (value.has_class_value())
    for (auto& field : *value.mutable_class_value()->mutable_fields())
      for_each_host_handle(*field.mutable_value(), visit);
  if (value.has_union_variant_value())
    for_each_host_handle(*value.mutable_union_variant_value()->mutable_value(),
                         visit);
}

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
      if (is_host_handle(value.handle_value().handle_type()))
        release_pin(value.handle_value());
      else
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
    case pb::BamlOutboundValue::kTyValue:
      release_live_heads(value.ty_value());
      break;
    default:
      break;
  }
}

inline invocation_state::~invocation_state() {
  api().handle_release(key);
  release_control_value(cancel);
}

// A thrown BAML value with the engine references the engine handed over
// with it: one per handle occurrence and per `live` head of a type value.
// Every copy of an error shares it, and the last one released lets the
// references go. Reading the value decodes a clone with references of its
// own, so it reads any number of times; a host re-sending it restates it
// with clones of its own too.
struct thrown_value {
  explicit thrown_value(pb::BamlOutboundValue owned) : wire(std::move(owned)) {}
  ~thrown_value() {
    try {
      release_control_value(wire);
    } catch (...) {
    }
  }
  thrown_value(const thrown_value&) = delete;
  thrown_value& operator=(const thrown_value&) = delete;

  pb::BamlOutboundValue wire;
};

// Takes the references `value` carries.
inline std::shared_ptr<const thrown_value> take_thrown_value(
    pb::BamlOutboundValue& value) {
  pb::BamlOutboundValue owned;
  owned.Swap(&value);
  return std::make_shared<const thrown_value>(std::move(owned));
}

// A copy whose engine handles are its own. Its type values stay borrowed
// from `value`: decoding keeps none, and restating one clones its own. Its
// host keys carry no pin: `value` holds those.
inline pb::BamlOutboundValue clone_control_value(
    const pb::BamlOutboundValue& value) {
  pb::BamlOutboundValue result(value);
  for_each_host_handle(result,
                       [](pb::BamlOutboundHandle& host) { host.clear_pin(); });
  std::function<void(pb::BamlOutboundValue*)> visit =
      [&](pb::BamlOutboundValue* item) {
        if (item->has_handle_value() &&
            !is_host_handle(item->handle_value().handle_type())) {
          uint64_t key = 0;
          if (api().handle_clone(item->handle_value().key(), &key) != 0)
            throw bridge_error("invocation handle clone failed");
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

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_PROTO_H_
