#ifndef BAML_INVOCATION_H_
#define BAML_INVOCATION_H_

#include <baml/detail/call.h>

#include <functional>
#include <map>

namespace baml {
class input {
 public:
  template <typename T>
  input(T value)
      : encode_([value = std::move(value)](detail::pb::InboundValue& out) {
          codec<T>::encode(out, value);
        }) {}
  void encode(detail::pb::InboundValue& out) const { encode_(out); }

 private:
  std::function<void(detail::pb::InboundValue&)> encode_;
};
using arguments = std::map<std::string, input>;
using type_bindings = std::map<std::string, detail::pb::BamlTy>;

class value {
 public:
  template <typename T>
  T as() const {
    return codec<T>::decode(detail::clone_control_value(owner_->wire));
  }

 private:
  struct owner {
    detail::pb::BamlOutboundValue wire;
    explicit owner(const detail::pb::BamlOutboundValue& wire) : wire(wire) {}
    ~owner() { detail::release_control_value(wire); }
  };
  explicit value(const detail::pb::BamlOutboundValue& wire)
      : owner_(std::make_shared<owner>(wire)) {}
  std::shared_ptr<owner> owner_;
  friend struct codec<value>;
};
template <>
struct codec<value> {
  static value decode(const detail::pb::BamlOutboundValue& wire) {
    return value(wire);
  }
  static void encode(detail::pb::InboundValue& out, const value& value) {
    detail::transcode_outbound_to_inbound(
        out, detail::clone_control_value(value.owner_->wire));
  }
};

class target {
 public:
  explicit target(std::string name) : name_(std::move(name)) {
    if (name_.empty()) throw error("empty invocation target");
  }
  template <typename Signature>
  explicit target(const function<Signature>& function)
      : function_(function.state_) {}
  bool callable() const { return function_ != nullptr; }
  uint64_t key() const { return function_->key; }
  const std::string& name() const { return name_; }

 private:
  std::string name_;
  std::shared_ptr<detail::function_handle_state> function_;
};

inline future<value> invoke_async(const target& target,
                                  const arguments& arguments,
                                  const type_bindings& types,
                                  const invocation_options& controls = {}) {
  if (target.callable() && !types.empty())
    throw error("specialized callable rejects type bindings");
  detail::args_encoder args(controls);
  for (const auto& argument : arguments)
    args.add_arg(argument.first, [&](detail::pb::InboundValue& out) {
      argument.second.encode(out);
    });
  for (const auto& binding : types)
    args.add_type(binding.first, binding.second);
  return target.callable()
             ? detail::start_handle_call<value>(target.key(), std::move(args))
             : detail::start_call<value>(target.name(), std::move(args));
}
inline value invoke(const target& target, const arguments& arguments,
                    const type_bindings& types,
                    const invocation_options& controls = {}) {
  return invoke_async(target, arguments, types, controls).get();
}
}  // namespace baml
#endif
