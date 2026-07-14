#ifndef BAML_DETAIL_HOST_VALUE_H_
#define BAML_DETAIL_HOST_VALUE_H_

// Host callables: a std::function passed as a BAML argument crosses the
// boundary as InboundValue.handle{key, HOST_VALUE_CALLABLE}, where key
// references a type-erased dispatcher in the process-global
// HostValueRegistry. When BAML invokes the callable, the engine fires the
// registered HostDispatchFn; the trampoline here runs the dispatcher on a
// detached thread (dispatch must be fire-and-return -- the engine awaits
// the completion, so the user callable must not run on the engine's
// worker), and the dispatcher decodes the BamlToHostCall args, invokes the
// user function, and resolves the call via complete_host_call. Mirrors
// bridge_python::host_value.
//
// Host exceptions cross back in two shapes (Python parity):
//   - baml::HostThrow<T>            -> the typed BAML class value itself
//   - anything else                 -> a baml.errors.HostCallable instance
//     whose _handle field references the original exception_ptr in this
//     registry, so the result decoder rehydrates the original exception
//     on same-process round-trip (see ThrowFromResult in codec.h).

#include <baml/arg.h>
#include <baml/detail/proto.h>
#include <baml/detail/wire.h>
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
    uintptr_t length);
extern "C" inline void baml_cpp_host_release_trampoline(
    uint64_t host_value_key);

namespace baml {

template <typename T>
struct Codec;

namespace detail {

// BamlHandleType values used by the host-value protocol (baml_handle.proto).
constexpr int32_t kHandleHostValueCallable = 15;
constexpr int32_t kHandleHostValueOpaque = 16;

// Process-wide table of host values handed to BAML: callable dispatchers
// (registered when a std::function is encoded as an argument) and opaque
// exception_ptrs (registered when a host callable throws a native
// exception, so the thrown baml.errors.HostCallable can be rehydrated to
// the original exception on round-trip). Keys are never 0; entries are
// removed by the engine-driven release callback when the engine drops its
// last clone of the corresponding HostValueArc.
class HostValueRegistry {
 public:
  using Dispatcher =
      std::function<void(uint32_t call_id, std::vector<uint8_t> args)>;

  static HostValueRegistry& Instance() {
    static HostValueRegistry registry;
    return registry;
  }

  uint64_t AddDispatcher(Dispatcher dispatch) {
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, Entry{std::move(dispatch), nullptr});
    return key;
  }

  uint64_t AddException(std::exception_ptr exception) {
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, Entry{nullptr, std::move(exception)});
    return key;
  }

  void Release(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    table_.erase(key);
  }

  // Copies out under the lock so a concurrent release cannot invalidate
  // the returned dispatcher mid-call.
  Dispatcher FindDispatcher(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    return it == table_.end() ? Dispatcher() : it->second.dispatch;
  }

  std::exception_ptr FindException(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    return it == table_.end() ? nullptr : it->second.exception;
  }

 private:
  struct Entry {
    Dispatcher dispatch;
    std::exception_ptr exception;
  };

  HostValueRegistry() {
    register_host_dispatch_callback(&baml_cpp_host_dispatch_trampoline);
    register_host_release_callback(&baml_cpp_host_release_trampoline);
  }

  std::mutex mu_;
  std::unordered_map<uint64_t, Entry> table_;
  uint64_t next_key_ = 1;
};

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

// One supplied argument of an engine->host call (BamlToHostCall.args).
struct HostCallArg {
  OutboundValue value;
  std::string name;
  bool is_optional = false;
};

inline std::vector<HostCallArg> ParseToHostCall(const uint8_t* data,
                                                size_t len) {
  std::vector<HostCallArg> out;
  wire::Reader r(data, len);
  uint32_t field;
  wire::WireType wt;
  while (r.Next(field, wt)) {
    if (field != 1) {  // BamlToHostCall.args
      r.Skip(wt);
      continue;
    }
    wire::Reader arg = r.LenPayload();
    HostCallArg a;
    uint32_t af;
    wire::WireType awt;
    while (arg.Next(af, awt)) {
      switch (af) {
        case 1:
          a.value = ParseOutboundValue(arg.LenPayload());
          break;
        case 2:
          a.name = arg.LenString();
          break;
        case 3:
          a.is_optional = arg.Boolean();
          break;
        default:
          arg.Skip(awt);
          break;
      }
    }
    out.push_back(std::move(a));
  }
  return out;
}

// Builds the InboundValue bytes of a baml.errors.HostCallable class value.
// class_name / message / language are debugging metadata; _handle
// (required on the wire) references the original exception in the registry
// for same-process rehydration. traceback is declared nullable but must be
// present, so it is sent as an explicit null (C++ has no portable
// traceback).
inline std::string BuildHostCallableInbound(const std::string& class_name,
                                            const std::string& message,
                                            uint64_t handle_key) {
  wire::Writer cls;  // InboundClassValue
  const auto field_entry = [&cls](const char* key, const wire::Writer& val) {
    wire::Writer entry;
    entry.StringField(1, key);
    entry.MessageField(6, val);
    cls.MessageField(2, entry);
  };
  const auto string_field_entry = [&field_entry](const char* key,
                                                 const std::string& value) {
    wire::Writer val;
    val.StringField(2, value);
    field_entry(key, val);
  };
  string_field_entry("message", message);
  string_field_entry("class_name", class_name);
  string_field_entry("language", "cpp");
  field_entry("traceback", wire::Writer());  // empty InboundValue = null
  wire::Writer handle;                       // BamlHandle
  handle.Uint64Field(1, handle_key);
  handle.Int64Field(2, kHandleHostValueOpaque);
  wire::Writer handle_val;
  handle_val.MessageField(10, handle);
  field_entry("_handle", handle_val);
  wire::Writer class_ty;
  class_ty.StringField(1, "baml.errors.HostCallable");
  cls.MessageField(3, class_ty);
  wire::Writer value_msg;
  value_msg.MessageField(8, cls);  // InboundValue.class_value
  return value_msg.bytes();
}

inline void CompleteHostSuccess(uint32_t call_id,
                                const std::string& inbound_bytes) {
  complete_host_call(call_id, 0,
                     reinterpret_cast<const int8_t*>(inbound_bytes.data()),
                     inbound_bytes.size());
}

inline void CompleteHostError(uint32_t call_id,
                              const std::string& inbound_bytes) {
  complete_host_call(call_id, 1,
                     reinterpret_cast<const int8_t*>(inbound_bytes.data()),
                     inbound_bytes.size());
}

// Re-encodes a decoded outbound value as an InboundValue message body:
// the propagation path for a BamlError caught from a nested BAML call and
// rethrown out of a host callable (the analog of Python re-encoding
// `BamlError.value`). Class identity round-trips, so the BAML caller's
// typed catch still matches.
inline void TranscodeOutboundToInbound(wire::Writer& value_msg,
                                       const OutboundValue& v) {
  switch (v.kind) {
    case OutboundValue::Kind::Null:
      break;  // absent oneof
    case OutboundValue::Kind::String:
      value_msg.StringField(2, v.string_v);
      break;
    case OutboundValue::Kind::Int:
      value_msg.Int64Field(3, v.int_v);
      break;
    case OutboundValue::Kind::Float:
      value_msg.DoubleField(4, v.float_v);
      break;
    case OutboundValue::Kind::Bool:
      value_msg.BoolField(5, v.bool_v);
      break;
    case OutboundValue::Kind::List: {
      wire::Writer list;
      for (const OutboundValue& item : v.items) {
        wire::Writer item_msg;
        TranscodeOutboundToInbound(item_msg, item);
        list.MessageField(1, item_msg);
      }
      value_msg.MessageField(6, list);
      break;
    }
    case OutboundValue::Kind::Map: {
      wire::Writer map;
      for (const auto& field : v.fields) {
        wire::Writer val;
        TranscodeOutboundToInbound(val, field.second);
        wire::Writer entry;
        entry.StringField(1, field.first);
        entry.MessageField(6, val);
        map.MessageField(1, entry);
      }
      value_msg.MessageField(7, map);
      break;
    }
    case OutboundValue::Kind::Class: {
      wire::Writer cls;
      for (const auto& field : v.fields) {
        wire::Writer val;
        TranscodeOutboundToInbound(val, field.second);
        wire::Writer entry;
        entry.StringField(1, field.first);
        entry.MessageField(6, val);
        cls.MessageField(2, entry);
      }
      wire::Writer class_ty;
      class_ty.StringField(1, v.name);
      cls.MessageField(3, class_ty);
      value_msg.MessageField(8, cls);
      break;
    }
    case OutboundValue::Kind::Enum: {
      wire::Writer en;
      en.StringField(1, v.name);
      en.StringField(2, v.string_v);
      value_msg.MessageField(9, en);
      break;
    }
    case OutboundValue::Kind::Handle: {
      wire::Writer handle;
      handle.Uint64Field(1, v.handle_key);
      handle.Int64Field(2, v.handle_type);
      value_msg.MessageField(10, handle);
      break;
    }
    case OutboundValue::Kind::Bytes:
      value_msg.BytesField(11, v.bytes_v.data(), v.bytes_v.size());
      break;
    case OutboundValue::Kind::BigInt:
      value_msg.StringField(12, v.string_v);
      break;
    case OutboundValue::Kind::Media:
      throw BamlError("cannot transcode a media value back inbound");
  }
}

// Best-effort dynamic class name of a host exception, for the
// baml.errors.HostCallable metadata (Python puts the exception class name
// there; C++ uses the demangled dynamic type).
inline std::string ExceptionClassName(const std::exception& e) {
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
struct is_arg<::baml::Arg<U>> : std::true_type {};

template <typename T>
struct arg_inner {
  using type = T;
};
template <typename U>
struct arg_inner<::baml::Arg<U>> {
  using type = U;
};

template <typename Slot>
Slot DecodeHostSlot(const OutboundValue& v) {
  if constexpr (is_arg<Slot>::value) {
    return Slot(Codec<typename arg_inner<Slot>::type>::Decode(v));
  } else {
    return Codec<Slot>::Decode(v);
  }
}

// Staging is std::tuple<std::optional<Slot>...>: absent stays disengaged so
// an omitted optional param materializes as an unset Arg.
template <typename Staging, size_t I>
void FillHostSlot(Staging& staging, const OutboundValue& v) {
  using Slot = typename std::tuple_element_t<I, Staging>::value_type;
  std::get<I>(staging).emplace(DecodeHostSlot<Slot>(v));
}

template <typename Staging, size_t... I>
std::array<void (*)(Staging&, const OutboundValue&), sizeof...(I)>
MakeHostSlotFillers(std::index_sequence<I...>) {
  return {{&FillHostSlot<Staging, I>...}};
}

template <typename Slot>
Slot MaterializeHostSlot(std::optional<Slot>& staged) {
  if (staged.has_value()) {
    return std::move(*staged);
  }
  if constexpr (is_arg<Slot>::value) {
    return Slot();  // omitted optional -> unset
  } else {
    throw BamlError("host callable dispatched without a required argument");
  }
}

template <typename R, typename Fn, typename Staging, size_t... I>
R InvokeHostCallable(const Fn& fn, Staging& staging,
                     std::index_sequence<I...>) {
  return fn(MaterializeHostSlot<
            typename std::tuple_element_t<I, Staging>::value_type>(
      std::get<I>(staging))...);
}

// Decodes the BamlToHostCall, invokes the user callable, and completes the
// in-flight call -- exactly once, on every exit path. `names` are the
// callable's declared BAML parameter names ("" for unnamed required
// params); supplied optionals are matched to their slot by name, required
// args by declared order.
template <typename R, typename... Ps>
void RunHostCallable(const std::function<R(Ps...)>& fn,
                     const std::array<std::string, sizeof...(Ps)>& names,
                     uint32_t call_id, const std::vector<uint8_t>& args_bytes) {
  try {
    constexpr size_t n = sizeof...(Ps);
    using Staging = std::tuple<std::optional<std::decay_t<Ps>>...>;
    Staging staging;
    constexpr std::array<bool, n> optional_slot = {
        {is_arg<std::decay_t<Ps>>::value...}};
    const auto fillers =
        MakeHostSlotFillers<Staging>(std::make_index_sequence<n>{});

    size_t required_cursor = 0;
    for (const HostCallArg& arg :
         ParseToHostCall(args_bytes.data(), args_bytes.size())) {
      size_t slot = n;
      if (arg.is_optional) {
        for (size_t i = 0; i < n; ++i) {
          if (names[i] == arg.name) {
            slot = i;
            break;
          }
        }
        if (slot == n) {
          throw BamlError("host callable received unknown optional argument '" +
                          arg.name + "'");
        }
      } else {
        while (required_cursor < n && optional_slot[required_cursor]) {
          ++required_cursor;
        }
        if (required_cursor == n) {
          throw BamlError(
              "host callable received more required arguments than "
              "declared parameters");
        }
        slot = required_cursor++;
      }
      fillers[slot](staging, arg.value);
    }

    wire::Writer result_msg;
    if constexpr (std::is_void<R>::value) {
      InvokeHostCallable<void>(fn, staging, std::make_index_sequence<n>{});
      // void result = BAML null: leave the InboundValue empty.
    } else {
      R result =
          InvokeHostCallable<R>(fn, staging, std::make_index_sequence<n>{});
      Codec<R>::Encode(result_msg, result);
    }
    CompleteHostSuccess(call_id, result_msg.bytes());
  } catch (const HostThrowBase& typed) {
    wire::Writer value_msg;
    typed.EncodeValue(value_msg);
    CompleteHostError(call_id, value_msg.bytes());
  } catch (const BamlError& e) {
    // A BAML-originated error rethrown through the callable: transcode
    // its payload so the class identity survives the round-trip.
    if (!e.payload().empty()) {
      try {
        wire::Reader r(e.payload().data(), e.payload().size());
        const OutboundValue payload = ParseOutboundValue(r);
        wire::Writer value_msg;
        TranscodeOutboundToInbound(value_msg, payload);
        CompleteHostError(call_id, value_msg.bytes());
        return;
      } catch (...) {
        // Untranscodable payload: fall through to the opaque path.
      }
    }
    const uint64_t key =
        HostValueRegistry::Instance().AddException(std::current_exception());
    CompleteHostError(call_id, BuildHostCallableInbound(ExceptionClassName(e),
                                                        e.message(), key));
  } catch (const std::exception& e) {
    const uint64_t key =
        HostValueRegistry::Instance().AddException(std::current_exception());
    CompleteHostError(call_id, BuildHostCallableInbound(ExceptionClassName(e),
                                                        e.what(), key));
  } catch (...) {
    const uint64_t key =
        HostValueRegistry::Instance().AddException(std::current_exception());
    CompleteHostError(
        call_id,
        BuildHostCallableInbound(
            "unknown", "host callable threw a non-std::exception value", key));
  }
}

// Registers `fn` as a host callable and writes the InboundValue handle that
// references it. Called from generated bindings for callable-typed
// arguments; `names` are the callable's declared parameter names in
// declared order (used to key supplied optional args).
template <typename R, typename... Ps>
void EncodeCallable(wire::Writer& value_msg, std::function<R(Ps...)> fn,
                    std::array<std::string, sizeof...(Ps)> names) {
  auto shared_names =
      std::make_shared<const std::array<std::string, sizeof...(Ps)>>(
          std::move(names));
  const uint64_t key = HostValueRegistry::Instance().AddDispatcher(
      [fn = std::move(fn), shared_names](uint32_t call_id,
                                         std::vector<uint8_t> args_bytes) {
        RunHostCallable<R, Ps...>(fn, *shared_names, call_id, args_bytes);
      });
  wire::Writer handle;  // BamlHandle
  handle.Uint64Field(1, key);
  handle.Int64Field(2, kHandleHostValueCallable);
  value_msg.MessageField(10, handle);  // InboundValue.handle
}

}  // namespace detail
}  // namespace baml

extern "C" inline void baml_cpp_host_dispatch_trampoline(
    uint64_t host_value_key, uint32_t call_id, const uint8_t* args,
    uintptr_t length) {
  std::vector<uint8_t> bytes;
  if (args != nullptr && length != 0) {
    bytes.assign(args, args + static_cast<size_t>(length));
  }
  // Bridge-layer faults (not user exceptions) also complete through
  // complete_host_call -- the C ABI's only completion channel -- as a
  // HostCallable error whose _handle carries a synthesized BamlError
  // (the wire shape requires a real handle).
  const auto bridge_failure = [call_id](const std::string& message) {
    const uint64_t key =
        baml::detail::HostValueRegistry::Instance().AddException(
            std::make_exception_ptr(baml::BamlError(message)));
    baml::detail::CompleteHostError(
        call_id,
        baml::detail::BuildHostCallableInbound("BridgeFailure", message, key));
  };
  auto dispatcher = baml::detail::HostValueRegistry::Instance().FindDispatcher(
      host_value_key);
  if (!dispatcher) {
    // The engine knows a handle this registry does not: a bridge fault.
    bridge_failure("no host callable registered for key " +
                   std::to_string(host_value_key));
    return;
  }
  // Fire-and-return: the engine awaits the completion, so the user
  // callable runs on its own thread (the analog of Python's spawned tokio
  // task). RunHostCallable completes the call on every exit path.
  std::thread([dispatcher = std::move(dispatcher), bridge_failure, call_id,
               bytes = std::move(bytes)]() mutable {
    try {
      dispatcher(call_id, std::move(bytes));
    } catch (...) {
      bridge_failure("host callable dispatch failed outside the user callable");
    }
  }).detach();
}

extern "C" inline void baml_cpp_host_release_trampoline(
    uint64_t host_value_key) {
  baml::detail::HostValueRegistry::Instance().Release(host_value_key);
}

#endif  // BAML_DETAIL_HOST_VALUE_H_
