#ifndef BAML_SPEC_H_
#define BAML_SPEC_H_

// Typed host proxies for the two live capabilities used by authored LLM
// functions: ai.FunctionSpec<Out> and ai.stream.Stream<Partial, Out>.
// A FunctionSpec is obtained by calling an authored function with the SPEC
// operation. Generated flat stream shortcuts call the compiler-private
// `Fn@stream` projection through the STREAM boundary operation; the spec does
// not carry the partial type and has no streaming method.

#include <baml/codec.h>
#include <baml/detail/call.h>
#include <baml/future.h>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <utility>

namespace baml {

template <typename Out>
class function_spec;

template <typename Partial, typename Out>
class stream;

class prompt;

// No value of this type can cross the boundary.
struct never {};

namespace detail {

struct capability_state {
  capability_state(uint64_t value, pb::BamlHandleType type)
      : key(value), handle_type(type) {}

  ~capability_state() noexcept {
    if (key == 0) return;
    try {
      (void)api().handle_release(key);
    } catch (...) {
    }
  }

  uint64_t key;
  pb::BamlHandleType handle_type;
};

inline std::shared_ptr<capability_state> decode_capability(
    const pb::BamlOutboundValue& raw, pb::BamlHandleType expected,
    const char* expected_name) {
  const pb::BamlOutboundValue& value = unwrap(raw);
  if (value.value_case() != pb::BamlOutboundValue::kHandleValue ||
      value.handle_value().key() == 0 ||
      value.handle_value().handle_type() != expected) {
    kind_mismatch(expected_name, value);
  }
  return std::make_shared<capability_state>(value.handle_value().key(),
                                            expected);
}

inline void encode_capability(pb::InboundValue& target,
                              const capability_state& state) {
  uint64_t cloned = 0;
  const uint32_t status = api().handle_clone(state.key, &cloned);
  if (status != 0 || cloned == 0) {
    throw error("BAML handle clone failed with status " +
                std::to_string(status));
  }
  pb::BamlHandle* handle = target.mutable_handle();
  handle->set_key(cloned);
  handle->set_handle_type(state.handle_type);
}

inline bool prompt_media_equal(const pb::BamlValueMedia& lhs,
                               const pb::BamlValueMedia& rhs) {
  if (lhs.media() != rhs.media() ||
      lhs.has_mime_type() != rhs.has_mime_type() ||
      (lhs.has_mime_type() && lhs.mime_type() != rhs.mime_type()) ||
      lhs.value_case() != rhs.value_case()) {
    return false;
  }
  switch (lhs.value_case()) {
    case pb::BamlValueMedia::kUrl:
      return lhs.url() == rhs.url();
    case pb::BamlValueMedia::kBase64:
      return lhs.base64() == rhs.base64();
    case pb::BamlValueMedia::kFile:
      return lhs.file() == rhs.file();
    case pb::BamlValueMedia::VALUE_NOT_SET:
      return true;
  }
  return false;
}

inline bool prompt_simple_equal(const pb::BamlValuePromptAstSimple& lhs,
                                const pb::BamlValuePromptAstSimple& rhs);

inline bool prompt_ast_equal(const pb::BamlValuePromptAst& lhs,
                             const pb::BamlValuePromptAst& rhs) {
  if (lhs.value_case() != rhs.value_case()) return false;
  switch (lhs.value_case()) {
    case pb::BamlValuePromptAst::kSimple:
      return prompt_simple_equal(lhs.simple(), rhs.simple());
    case pb::BamlValuePromptAst::kMessage: {
      const auto& left = lhs.message();
      const auto& right = rhs.message();
      return left.role() == right.role() &&
             left.metadata_as_json() == right.metadata_as_json() &&
             left.has_content() == right.has_content() &&
             (!left.has_content() ||
              prompt_simple_equal(left.content(), right.content()));
    }
    case pb::BamlValuePromptAst::kMultiple: {
      const auto& left = lhs.multiple().items();
      const auto& right = rhs.multiple().items();
      if (left.size() != right.size()) return false;
      for (int i = 0; i < left.size(); ++i) {
        if (!prompt_ast_equal(left.Get(i), right.Get(i))) return false;
      }
      return true;
    }
    case pb::BamlValuePromptAst::VALUE_NOT_SET:
      return true;
  }
  return false;
}

inline bool prompt_simple_equal(const pb::BamlValuePromptAstSimple& lhs,
                                const pb::BamlValuePromptAstSimple& rhs) {
  if (lhs.value_case() != rhs.value_case()) return false;
  switch (lhs.value_case()) {
    case pb::BamlValuePromptAstSimple::kString:
      return lhs.string() == rhs.string();
    case pb::BamlValuePromptAstSimple::kMedia:
      return prompt_media_equal(lhs.media(), rhs.media());
    case pb::BamlValuePromptAstSimple::kMultiple: {
      const auto& left = lhs.multiple().items();
      const auto& right = rhs.multiple().items();
      if (left.size() != right.size()) return false;
      for (int i = 0; i < left.size(); ++i) {
        if (!prompt_simple_equal(left.Get(i), right.Get(i))) return false;
      }
      return true;
    }
    case pb::BamlValuePromptAstSimple::VALUE_NOT_SET:
      return true;
  }
  return false;
}

template <typename... Args>
pb::BamlTy generic_nominal_ty(const char* fqn) {
  pb::BamlTy ty;
  pb::BamlTyClass* cls = ty.mutable_class_ty();
  cls->set_name(fqn);
  (cls->add_type_args()->CopyFrom(codec<Args>::baml_ty()), ...);
  return ty;
}

}  // namespace detail

// The result of one typed stream pull. `done()` is distinct from a partial
// whose own value is null (stream_item<std::optional<T>> with
// value()==nullopt).
template <typename T>
class stream_item {
 public:
  static stream_item finished() { return stream_item(); }
  static stream_item value(T value) {
    stream_item item;
    item.value_.emplace(std::move(value));
    return item;
  }

  bool done() const noexcept { return !value_.has_value(); }
  bool has_value() const noexcept { return value_.has_value(); }

  const T& value() const {
    if (!value_) throw error("BAML stream is finished");
    return *value_;
  }

  T& value() {
    if (!value_) throw error("BAML stream is finished");
    return *value_;
  }

 private:
  std::optional<T> value_;
};

// An opaque, engine-owned bound ai.FunctionSpec<Out>.
template <typename Out>
class function_spec {
 public:
  function_spec(const function_spec&) = default;
  function_spec(function_spec&&) noexcept = default;
  function_spec& operator=(const function_spec&) = default;
  function_spec& operator=(function_spec&&) noexcept = default;

  friend bool operator==(const function_spec& lhs,
                         const function_spec& rhs) noexcept {
    return lhs.state_ == rhs.state_;
  }

  friend bool operator!=(const function_spec& lhs,
                         const function_spec& rhs) noexcept {
    return !(lhs == rhs);
  }

  Out call() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<Out>("ai.FunctionSpec.call", std::move(args));
  }

  future<Out> call_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<Out>("ai.FunctionSpec.call", std::move(args));
  }

  Out parse(const std::string& json) const {
    detail::args_encoder args;
    add_self(args);
    args.add_arg("json", [&](detail::pb::InboundValue& target) {
      codec<std::string>::encode(target, json);
    });
    return detail::call_sync<Out>("ai.FunctionSpec.parse", std::move(args));
  }

  future<Out> parse_async(const std::string& json) const {
    detail::args_encoder args;
    add_self(args);
    args.add_arg("json", [&](detail::pb::InboundValue& target) {
      codec<std::string>::encode(target, json);
    });
    return detail::start_call<Out>("ai.FunctionSpec.parse", std::move(args));
  }

  ::baml::prompt prompt() const;
  future<::baml::prompt> prompt_async() const;

  template <typename Request>
  Request build_request() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<Request>("ai.FunctionSpec.build_request",
                                      std::move(args));
  }

  template <typename Request>
  future<Request> build_request_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<Request>("ai.FunctionSpec.build_request",
                                       std::move(args));
  }

  std::string name() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<std::string>("ai.FunctionSpec.name",
                                          std::move(args));
  }

  future<std::string> name_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<std::string>("ai.FunctionSpec.name",
                                           std::move(args));
  }

 private:
  explicit function_spec(std::shared_ptr<detail::capability_state> state)
      : state_(std::move(state)) {}

  void add_self(detail::args_encoder& args) const {
    args.add_arg("self", [&](detail::pb::InboundValue& target) {
      detail::encode_capability(target, *state_);
    });
  }

  std::shared_ptr<detail::capability_state> state_;

  friend struct codec<function_spec<Out>>;
};

// A live, engine-owned ai.stream.Stream<Partial, Out>.
template <typename Partial, typename Out>
class stream {
 public:
  stream(const stream&) = default;
  stream(stream&&) noexcept = default;
  stream& operator=(const stream&) = default;
  stream& operator=(stream&&) noexcept = default;

  friend bool operator==(const stream& lhs, const stream& rhs) noexcept {
    return lhs.state_ == rhs.state_;
  }

  friend bool operator!=(const stream& lhs, const stream& rhs) noexcept {
    return !(lhs == rhs);
  }

  stream_item<Partial> next() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<stream_item<Partial>>("ai.stream.Stream.next",
                                                   std::move(args));
  }

  future<stream_item<Partial>> next_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<stream_item<Partial>>("ai.stream.Stream.next",
                                                    std::move(args));
  }

  Out final_() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<Out>("ai.stream.Stream.final", std::move(args));
  }

  future<Out> final_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<Out>("ai.stream.Stream.final", std::move(args));
  }

 private:
  explicit stream(std::shared_ptr<detail::capability_state> state)
      : state_(std::move(state)) {}

  void add_self(detail::args_encoder& args) const {
    args.add_arg("self", [&](detail::pb::InboundValue& target) {
      detail::encode_capability(target, *state_);
    });
  }

  std::shared_ptr<detail::capability_state> state_;

  friend struct codec<stream<Partial, Out>>;
};

// Owned, provider-neutral prompt data. Unlike FunctionSpec and Stream this is
// a portable value, so encoding copies the structural protobuf payload rather
// than cloning an engine handle.
class prompt {
 public:
  friend bool operator==(const prompt& lhs, const prompt& rhs) {
    return detail::prompt_ast_equal(lhs.value_, rhs.value_);
  }

  friend bool operator!=(const prompt& lhs, const prompt& rhs) {
    return !(lhs == rhs);
  }

  std::string text() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<std::string>("ai.Prompt.text", std::move(args));
  }

  future<std::string> text_async() const {
    detail::args_encoder args;
    add_self(args);
    return detail::start_call<std::string>("ai.Prompt.text", std::move(args));
  }

  template <typename Messages>
  Messages messages() const {
    detail::args_encoder args;
    add_self(args);
    return detail::call_sync<Messages>("ai.Prompt.messages", std::move(args));
  }

 private:
  explicit prompt(detail::pb::BamlValuePromptAst value)
      : value_(std::move(value)) {}

  void add_self(detail::args_encoder& args) const {
    args.add_arg("self", [&](detail::pb::InboundValue& target) {
      target.mutable_prompt_ast_value()->CopyFrom(value_);
      target.mutable_value_type()->mutable_prompt_ast();
    });
  }

  detail::pb::BamlValuePromptAst value_;

  friend struct codec<prompt>;
};

template <typename Out>
::baml::prompt function_spec<Out>::prompt() const {
  detail::args_encoder args;
  add_self(args);
  return detail::call_sync<::baml::prompt>("ai.FunctionSpec.prompt",
                                           std::move(args));
}

template <typename Out>
future<::baml::prompt> function_spec<Out>::prompt_async() const {
  detail::args_encoder args;
  add_self(args);
  return detail::start_call<::baml::prompt>("ai.FunctionSpec.prompt",
                                            std::move(args));
}

template <>
struct codec<never> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_never();
    return ty;
  }
  static void encode(detail::pb::InboundValue&, never) {
    throw error("BAML never has no values");
  }
  static never decode(const detail::pb::BamlOutboundValue&) {
    throw error("BAML never has no values");
  }
};

template <typename Out>
struct codec<function_spec<Out>> {
  static detail::pb::BamlTy baml_ty() {
    return detail::generic_nominal_ty<Out>("ai.FunctionSpec");
  }

  static void encode(detail::pb::InboundValue& target,
                     const function_spec<Out>& value) {
    detail::encode_capability(target, *value.state_);
  }

  static function_spec<Out> decode(const detail::pb::BamlOutboundValue& value) {
    return function_spec<Out>(detail::decode_capability(
        value, detail::pb::ADT_FUNCTION_SPEC, "ai.FunctionSpec handle"));
  }
};

template <typename Partial, typename Out>
struct codec<stream<Partial, Out>> {
  static detail::pb::BamlTy baml_ty() {
    return detail::generic_nominal_ty<Partial, Out>("ai.stream.Stream");
  }

  static void encode(detail::pb::InboundValue& target,
                     const stream<Partial, Out>& value) {
    detail::encode_capability(target, *value.state_);
  }

  static stream<Partial, Out> decode(
      const detail::pb::BamlOutboundValue& value) {
    return stream<Partial, Out>(detail::decode_capability(
        value, detail::pb::ADT_TAGGED_HEAP_HANDLE, "ai.stream.Stream handle"));
  }
};

template <typename Partial>
struct codec<stream_item<Partial>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_unknown();
    return ty;
  }

  static stream_item<Partial> decode(const detail::pb::BamlOutboundValue& raw) {
    const detail::pb::BamlOutboundValue& value = detail::unwrap(raw);
    if (value.value_case() == detail::pb::BamlOutboundValue::kClassValue &&
        value.class_value().name() == "ai.stream.Done") {
      return stream_item<Partial>::finished();
    }
    return stream_item<Partial>::value(codec<Partial>::decode(raw));
  }
};

template <>
struct codec<prompt> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_prompt_ast();
    return ty;
  }

  static void encode(detail::pb::InboundValue& target, const prompt& value) {
    target.mutable_prompt_ast_value()->CopyFrom(value.value_);
    target.mutable_value_type()->CopyFrom(baml_ty());
  }

  static prompt decode(const detail::pb::BamlOutboundValue& raw) {
    const detail::pb::BamlOutboundValue& value = detail::unwrap(raw);
    if (value.value_case() != detail::pb::BamlOutboundValue::kPromptAstValue) {
      detail::kind_mismatch("ai.Prompt", value);
    }
    return prompt(value.prompt_ast_value());
  }
};

}  // namespace baml

#endif  // BAML_SPEC_H_
