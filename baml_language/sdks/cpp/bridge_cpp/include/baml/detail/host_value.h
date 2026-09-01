#ifndef BAML_DETAIL_HOST_VALUE_H_
#define BAML_DETAIL_HOST_VALUE_H_

// Host callables: a std::function passed as a BAML argument crosses the
// boundary as InboundValue.handle{key, HOST_VALUE_CALLABLE}, where key
// references a type-erased dispatcher in the process-global
// host_value_registry. When BAML invokes the callable, the engine fires
// the registered host-dispatch callback; the trampoline here runs the
// dispatcher on a detached thread (dispatch must be fire-and-return --
// the engine awaits the completion, so the user callable must not run on
// the engine's worker), and the dispatcher decodes the BamlToHostCall
// args, invokes the user function, and resolves the call via
// complete_host_call. Mirrors bridge_python::host_value.
//
// Host exceptions cross back in two shapes (Python parity):
//   - baml::host_throw<T>           -> the typed BAML class value itself
//   - anything else                 -> a baml.errors.HostCallable instance
//     whose _handle field references the original exception_ptr in this
//     registry, so the result decoder rehydrates the original exception
//     on same-process round-trip (see throw_from_result in codec.h).

#include <baml/arg.h>
#include <baml/detail/loader.h>
#include <baml/detail/proto.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <array>
#include <cstdint>
#include <exception>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <tuple>
#include <type_traits>
#include <unordered_map>
#include <utility>
#include <vector>

#if !defined(_MSC_VER)
#include <cxxabi.h>

#include <cstdlib>
#include <typeinfo>
#else
#include <typeinfo>
#endif

extern "C" inline void baml_cpp_host_dispatch_trampoline(
    uint64_t host_value_key, uint32_t call_id, const uint8_t* args,
    size_t length);
extern "C" inline void baml_cpp_host_release_trampoline(
    uint64_t host_value_key);

namespace baml {

template <typename T>
struct codec;

namespace detail {

// Process-wide table of host values handed to BAML: callable dispatchers
// (registered when a std::function is encoded as an argument) and opaque
// exception_ptrs (registered when a host callable throws a native
// exception, so the thrown baml.errors.HostCallable can be rehydrated to
// the original exception on round-trip). Keys are never 0; entries are
// removed by the engine-driven release callback when the engine drops its
// last clone of the corresponding host value.
class host_value_registry {
 public:
  using dispatcher =
      std::function<void(uint32_t call_id, std::vector<uint8_t> args)>;

  static host_value_registry& instance() {
    // Intentionally leaked, like call_registry: release callbacks can fire
    // from engine threads during process teardown.
    static host_value_registry* registry = new host_value_registry();
    return *registry;
  }

  uint64_t add_dispatcher(dispatcher dispatch) {
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, entry{std::move(dispatch), nullptr});
    return key;
  }

  uint64_t add_exception(std::exception_ptr exception) {
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, entry{nullptr, std::move(exception)});
    return key;
  }

  void release(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    table_.erase(key);
  }

  // Copies out under the lock so a concurrent release cannot invalidate
  // the returned dispatcher mid-call.
  dispatcher find_dispatcher(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    return it == table_.end() ? dispatcher() : it->second.dispatch;
  }

  std::exception_ptr find_exception(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    return it == table_.end() ? nullptr : it->second.exception;
  }

 private:
  struct entry {
    dispatcher dispatch;
    std::exception_ptr exception;
  };

  host_value_registry() {
    api().register_host_dispatch_callback(&baml_cpp_host_dispatch_trampoline);
    api().register_host_release_callback(&baml_cpp_host_release_trampoline);
  }

  std::mutex mu_;
  std::unordered_map<uint64_t, entry> table_;
  uint64_t next_key_ = 1;
};

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

inline void complete_host_success(uint32_t call_id,
                                  const std::string& inbound_bytes) {
  api().complete_host_call(
      call_id, 0, reinterpret_cast<const int8_t*>(inbound_bytes.data()),
      inbound_bytes.size());
}

inline void complete_host_error(uint32_t call_id,
                                const std::string& inbound_bytes) {
  api().complete_host_call(
      call_id, 1, reinterpret_cast<const int8_t*>(inbound_bytes.data()),
      inbound_bytes.size());
}

// Builds the InboundValue bytes of a baml.errors.HostCallable class value.
// class_name / message / language are debugging metadata; _handle
// (required on the wire) references the original exception in the
// registry for same-process rehydration. traceback is declared nullable
// but must be present, so it is sent as an explicit null (C++ has no
// portable traceback).
inline std::string build_host_callable_inbound(const std::string& class_name,
                                               const std::string& message,
                                               uint64_t handle_key) {
  pb::InboundValue value_msg;
  pb::InboundClassValue* cls = value_msg.mutable_class_value();
  const auto string_field = [cls](const char* key, const std::string& v) {
    pb::InboundMapEntry* field = cls->add_fields();
    field->set_string_key(key);
    field->mutable_value()->set_string_value(v);
  };
  string_field("message", message);
  string_field("class_name", class_name);
  string_field("language", "cpp");
  {
    pb::InboundMapEntry* field = cls->add_fields();
    field->set_string_key("traceback");
    field->mutable_value();  // present but empty InboundValue = explicit null
  }
  {
    pb::InboundMapEntry* field = cls->add_fields();
    field->set_string_key("_handle");
    pb::BamlHandle* handle = field->mutable_value()->mutable_handle();
    handle->set_key(handle_key);
    handle->set_handle_type(pb::HOST_VALUE_OPAQUE);
  }
  value_msg.mutable_value_type()->mutable_class_ty()->set_name(
      "baml.errors.HostCallable");
  return value_msg.SerializeAsString();
}

// Re-encodes an outbound value as an InboundValue message: the propagation
// path for a baml::error caught from a nested BAML call and rethrown out
// of a host callable (the analog of Python re-encoding `BamlError.value`).
// Class identity round-trips, so the BAML caller's typed catch still
// matches.
inline void transcode_outbound_to_inbound(pb::InboundValue& out,
                                          const pb::BamlOutboundValue& raw) {
  const pb::BamlOutboundValue& v = unwrap(raw);
  switch (v.value_case()) {
    case pb::BamlOutboundValue::VALUE_NOT_SET:
    case pb::BamlOutboundValue::kNullValue:
      return;  // absent oneof = null
    case pb::BamlOutboundValue::kStringValue:
      out.set_string_value(v.string_value());
      return;
    case pb::BamlOutboundValue::kIntValue:
      out.set_int_value(v.int_value());
      return;
    case pb::BamlOutboundValue::kFloatValue:
      out.set_float_value(v.float_value());
      return;
    case pb::BamlOutboundValue::kBoolValue:
      out.set_bool_value(v.bool_value());
      return;
    case pb::BamlOutboundValue::kLiteralValue:
      switch (v.literal_value().literal_case()) {
        case pb::BamlLiteralValue::kStringValue:
          out.set_string_value(v.literal_value().string_value());
          return;
        case pb::BamlLiteralValue::kIntValue:
          out.set_int_value(v.literal_value().int_value());
          return;
        case pb::BamlLiteralValue::kBoolValue:
          out.set_bool_value(v.literal_value().bool_value());
          return;
        case pb::BamlLiteralValue::kBigintValue:
          out.set_bigint_value(v.literal_value().bigint_value());
          return;
        default:
          throw error("cannot transcode this literal value back inbound");
      }
    case pb::BamlOutboundValue::kListValue: {
      pb::InboundListValue* list = out.mutable_list_value();
      for (const pb::BamlOutboundValue& item : v.list_value().items()) {
        transcode_outbound_to_inbound(*list->add_values(), item);
      }
      return;
    }
    case pb::BamlOutboundValue::kMapValue: {
      pb::InboundMapValue* map = out.mutable_map_value();
      for (const pb::BamlOutboundMapEntry& e : v.map_value().entries()) {
        pb::InboundMapEntry* entry = map->add_entries();
        entry->set_string_key(e.key());
        transcode_outbound_to_inbound(*entry->mutable_value(), e.value());
      }
      return;
    }
    case pb::BamlOutboundValue::kClassValue: {
      pb::InboundClassValue* cls = out.mutable_class_value();
      for (const pb::BamlOutboundMapEntry& e : v.class_value().fields()) {
        pb::InboundMapEntry* field = cls->add_fields();
        field->set_string_key(e.key());
        transcode_outbound_to_inbound(*field->mutable_value(), e.value());
      }
      pb::BamlTyClass* class_ty = out.mutable_value_type()->mutable_class_ty();
      class_ty->set_name(v.class_value().name());
      for (const pb::BamlTy& type_arg : v.class_value().type_args()) {
        class_ty->add_type_args()->CopyFrom(type_arg);
      }
      return;
    }
    case pb::BamlOutboundValue::kEnumValue: {
      pb::InboundEnumValue* en = out.mutable_enum_value();
      en->set_name(v.enum_value().name());
      en->set_value(v.enum_value().value());
      return;
    }
    case pb::BamlOutboundValue::kHandleValue: {
      pb::BamlHandle* handle = out.mutable_handle();
      handle->set_key(v.handle_value().key());
      handle->set_handle_type(v.handle_value().handle_type());
      return;
    }
    case pb::BamlOutboundValue::kMediaValue:
      out.mutable_media_value()->CopyFrom(v.media_value());
      return;
    case pb::BamlOutboundValue::kPromptAstValue:
      out.mutable_prompt_ast_value()->CopyFrom(v.prompt_ast_value());
      return;
    case pb::BamlOutboundValue::kUint8ArrayValue:
      out.set_uint8array_value(v.uint8array_value());
      return;
    case pb::BamlOutboundValue::kBigintValue:
      out.set_bigint_value(v.bigint_value());
      return;
    case pb::BamlOutboundValue::kTyDefValue:
      throw error(
          "BAML decode error: a runtime type definition requires BEP-066 "
          "reflection support, which the C++ SDK does not provide");
    default:
      throw error("cannot transcode this value back inbound");
  }
}

// Best-effort dynamic class name of a host exception, for the
// baml.errors.HostCallable metadata (Python puts the exception class name
// there; C++ uses the demangled dynamic type).
inline std::string exception_class_name(const std::exception& e) {
#if defined(_MSC_VER)
  return typeid(e).name();
#else
  int status = 0;
  char* demangled =
      abi::__cxa_demangle(typeid(e).name(), nullptr, nullptr, &status);
  std::string out =
      (status == 0 && demangled != nullptr) ? demangled : typeid(e).name();
  std::free(demangled);
  return out;
#endif
}

// ---------------------------------------------------------------------------
// Typed dispatch
// ---------------------------------------------------------------------------

template <typename T>
struct is_arg : std::false_type {};
template <typename U>
struct is_arg<::baml::arg<U>> : std::true_type {};

template <typename T>
struct arg_inner {
  using type = T;
};
template <typename U>
struct arg_inner<::baml::arg<U>> {
  using type = U;
};

template <typename Slot>
Slot decode_host_slot(const pb::BamlOutboundValue& v) {
  if constexpr (is_arg<Slot>::value) {
    return Slot(codec<typename arg_inner<Slot>::type>::decode(v));
  } else {
    return codec<Slot>::decode(v);
  }
}

// Staging is std::tuple<std::optional<Slot>...>: absent stays disengaged
// so an omitted optional param materializes as an unset arg.
template <typename Staging, size_t I>
void fill_host_slot(Staging& staging, const pb::BamlOutboundValue& v) {
  using Slot = typename std::tuple_element_t<I, Staging>::value_type;
  std::get<I>(staging).emplace(decode_host_slot<Slot>(v));
}

template <typename Staging, size_t... I>
std::array<void (*)(Staging&, const pb::BamlOutboundValue&), sizeof...(I)>
make_host_slot_fillers(std::index_sequence<I...>) {
  return {{&fill_host_slot<Staging, I>...}};
}

template <typename Slot>
Slot materialize_host_slot(std::optional<Slot>& staged) {
  if (staged.has_value()) {
    return std::move(*staged);
  }
  if constexpr (is_arg<Slot>::value) {
    return Slot();  // omitted optional -> unset
  } else {
    throw error("host callable dispatched without a required argument");
  }
}

template <typename R, typename Fn, typename Staging, size_t... I>
R invoke_host_callable(const Fn& fn, Staging& staging,
                       std::index_sequence<I...>) {
  return fn(materialize_host_slot<
            typename std::tuple_element_t<I, Staging>::value_type>(
      std::get<I>(staging))...);
}

// Decodes the BamlToHostCall, invokes the user callable, and completes the
// in-flight call -- exactly once, on every exit path. `names` are the
// callable's declared BAML parameter names ("" for unnamed required
// params); supplied optionals are matched to their slot by name, required
// args by declared order.
template <typename R, typename... Ps>
void run_host_callable(const std::function<R(Ps...)>& fn,
                       const std::array<std::string, sizeof...(Ps)>& names,
                       uint32_t call_id,
                       const std::vector<uint8_t>& args_bytes) {
  try {
    constexpr size_t n = sizeof...(Ps);
    using Staging = std::tuple<std::optional<std::decay_t<Ps>>...>;
    Staging staging;
    constexpr std::array<bool, n> optional_slot = {
        {is_arg<std::decay_t<Ps>>::value...}};
    const auto fillers =
        make_host_slot_fillers<Staging>(std::make_index_sequence<n>{});

    pb::BamlToHostCall call;
    if (!call.ParseFromArray(args_bytes.data(),
                             static_cast<int>(args_bytes.size()))) {
      throw error("malformed BamlToHostCall payload");
    }
    size_t required_cursor = 0;
    for (const pb::BamlToHostArg& arg : call.args()) {
      size_t slot = n;
      if (arg.is_optional_arg()) {
        for (size_t i = 0; i < n; ++i) {
          if (names[i] == arg.arg_name()) {
            slot = i;
            break;
          }
        }
        if (slot == n) {
          throw error("host callable received unknown optional argument '" +
                      arg.arg_name() + "'");
        }
      } else {
        while (required_cursor < n && optional_slot[required_cursor]) {
          ++required_cursor;
        }
        if (required_cursor == n) {
          throw error(
              "host callable received more required arguments than "
              "declared parameters");
        }
        slot = required_cursor++;
      }
      fillers[slot](staging, arg.value());
    }

    pb::InboundValue result_msg;
    if constexpr (std::is_void<R>::value) {
      invoke_host_callable<void>(fn, staging, std::make_index_sequence<n>{});
      // void result = BAML null: leave the InboundValue empty.
    } else {
      R result =
          invoke_host_callable<R>(fn, staging, std::make_index_sequence<n>{});
      codec<R>::encode(result_msg, result);
    }
    complete_host_success(call_id, result_msg.SerializeAsString());
  } catch (const host_throw_base& typed) {
    pb::InboundValue value_msg;
    typed.encode_value(value_msg);
    complete_host_error(call_id, value_msg.SerializeAsString());
  } catch (const error& e) {
    // A BAML-originated error rethrown through the callable: transcode
    // its payload so the class identity survives the round-trip.
    if (!e.payload().empty()) {
      try {
        pb::BamlOutboundValue payload;
        if (payload.ParseFromArray(e.payload().data(),
                                   static_cast<int>(e.payload().size()))) {
          pb::InboundValue value_msg;
          transcode_outbound_to_inbound(value_msg, payload);
          complete_host_error(call_id, value_msg.SerializeAsString());
          return;
        }
      } catch (...) {
        // Untranscodable payload: fall through to the opaque path.
      }
    }
    const uint64_t key =
        host_value_registry::instance().add_exception(std::current_exception());
    complete_host_error(
        call_id,
        build_host_callable_inbound(exception_class_name(e), e.message(), key));
  } catch (const std::exception& e) {
    const uint64_t key =
        host_value_registry::instance().add_exception(std::current_exception());
    complete_host_error(call_id, build_host_callable_inbound(
                                     exception_class_name(e), e.what(), key));
  } catch (...) {
    const uint64_t key =
        host_value_registry::instance().add_exception(std::current_exception());
    complete_host_error(
        call_id,
        build_host_callable_inbound(
            "unknown", "host callable threw a non-std::exception value", key));
  }
}

// Registers `fn` as a host callable and writes the InboundValue handle
// that references it. Called from generated bindings for callable-typed
// arguments; `names` are the callable's declared parameter names in
// declared order (used to key supplied optional args).
template <typename R, typename... Ps>
void encode_callable(pb::InboundValue& value_msg, std::function<R(Ps...)> fn,
                     std::array<std::string, sizeof...(Ps)> names) {
  auto shared_names =
      std::make_shared<const std::array<std::string, sizeof...(Ps)>>(
          std::move(names));
  const uint64_t key = host_value_registry::instance().add_dispatcher(
      [fn = std::move(fn), shared_names](uint32_t call_id,
                                         std::vector<uint8_t> args_bytes) {
        run_host_callable<R, Ps...>(fn, *shared_names, call_id, args_bytes);
      });
  pb::BamlHandle* handle = value_msg.mutable_handle();
  handle->set_key(key);
  handle->set_handle_type(pb::HOST_VALUE_CALLABLE);
}

}  // namespace detail
}  // namespace baml

extern "C" inline void baml_cpp_host_dispatch_trampoline(
    uint64_t host_value_key, uint32_t call_id, const uint8_t* args,
    size_t length) {
  // Bridge-layer faults (not user exceptions) also complete through
  // complete_host_call -- the C ABI's only completion channel -- as a
  // HostCallable error whose _handle carries a synthesized baml::error
  // (the wire shape requires a real handle).
  const auto bridge_failure = [call_id](const std::string& message) {
    const uint64_t key =
        baml::detail::host_value_registry::instance().add_exception(
            std::make_exception_ptr(baml::error(message)));
    baml::detail::complete_host_error(
        call_id, baml::detail::build_host_callable_inbound("BridgeFailure",
                                                           message, key));
  };
  // No C++ exception may cross the C ABI (bridge contract), and every
  // setup fault must still complete the call or the engine awaits forever:
  // bytes.assign can throw bad_alloc, the registry lock can throw, and the
  // std::thread constructor throws system_error under thread exhaustion.
  try {
    std::vector<uint8_t> bytes;
    if (args != nullptr && length != 0) {
      bytes.assign(args, args + length);
    }
    auto dispatcher =
        baml::detail::host_value_registry::instance().find_dispatcher(
            host_value_key);
    if (!dispatcher) {
      // The engine knows a handle this registry does not: a bridge fault.
      bridge_failure("no host callable registered for key " +
                     std::to_string(host_value_key));
      return;
    }
    // Fire-and-return: the engine awaits the completion, so the user
    // callable runs on its own thread (the analog of Python's spawned
    // tokio task). run_host_callable completes the call on every exit
    // path.
    std::thread([dispatcher = std::move(dispatcher), bridge_failure, call_id,
                 bytes = std::move(bytes)]() mutable {
      try {
        dispatcher(call_id, std::move(bytes));
      } catch (...) {
        bridge_failure(
            "host callable dispatch failed outside the user callable");
      }
    }).detach();
  } catch (...) {
    try {
      bridge_failure("host callable dispatch could not be spawned");
    } catch (...) {
      // Even the failure path failed (e.g. allocation): swallowing is all
      // the ABI permits -- that call hangs rather than the process
      // hitting UB.
    }
  }
}

extern "C" inline void baml_cpp_host_release_trampoline(
    uint64_t host_value_key) {
  // No C++ exception may cross the C ABI (bridge contract).
  try {
    baml::detail::host_value_registry::instance().release(host_value_key);
  } catch (...) {
  }
}

#endif  // BAML_DETAIL_HOST_VALUE_H_
