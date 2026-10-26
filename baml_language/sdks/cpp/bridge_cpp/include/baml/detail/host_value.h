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
// What the callable did crosses back as one outcome of the call's
// completion (see `host_call_completion`): a value it returned or a BAML
// value it threw, each measured against the wire; an exception of its own,
// `raised` with its registration here so the same process recovers the
// original object when the engine's baml.errors.HostCallable comes back (see
// throw_from_result in codec.h); or a failure of the bridge's own.

#include <baml/arg.h>
#include <baml/detail/loader.h>
#include <baml/detail/proto.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <any>
#include <array>
#include <atomic>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <exception>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <system_error>
#include <thread>
#include <tuple>
#include <type_traits>
#include <unordered_map>
#include <utility>
#include <variant>
#include <vector>

#if defined(_WIN32)
#include <process.h>

#include <cerrno>
#else
#include <pthread.h>
#endif

#if !defined(_MSC_VER)
#include <cxxabi.h>

#include <cstdlib>
#include <typeinfo>
#else
#include <typeinfo>
#endif

extern "C" inline void baml_cpp_host_dispatch_trampoline(const uint8_t* request,
                                                         size_t length);
extern "C" inline void baml_cpp_host_cancel_trampoline(uint32_t callback_id);
extern "C" inline void baml_cpp_host_release_trampoline(
    uint64_t host_value_key);

namespace baml {

template <typename T>
struct codec;

namespace detail {

// Process-wide table of host values handed to BAML: callables (registered
// when a std::function is encoded as an argument) and opaque exception_ptrs
// (registered when a host callable raises an exception of its own, so the
// engine's baml.errors.HostCallable around it is rehydrated to the original
// exception on round-trip). Keys are never 0; entries are removed by the
// engine-driven release callback when the engine drops its last reference
// to the corresponding host value.
class host_value_registry {
 public:
  using dispatcher =
      std::function<void(uint32_t call_id, std::vector<uint8_t> args)>;

  // A callable the host registered: how the engine's calls to it run, the
  // function type it was stated under (stated again whenever the host sends
  // it back), and the C++ callable itself, which a host that receives it
  // back from BAML calls directly.
  struct callable {
    dispatcher dispatch;
    pb::BamlTy ty;
    std::any original;
  };

  static host_value_registry& instance() {
    // Intentionally leaked, like call_registry: release callbacks can fire
    // from engine threads during process teardown.
    static host_value_registry* registry = new host_value_registry();
    return *registry;
  }

  uint64_t add_callable(callable registered) {
    auto shared = std::make_shared<const callable>(std::move(registered));
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, std::move(shared));
    return key;
  }

  uint64_t add_exception(std::exception_ptr exception) {
    std::lock_guard<std::mutex> lock(mu_);
    const uint64_t key = next_key_++;
    table_.emplace(key, std::move(exception));
    return key;
  }

  void release(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    table_.erase(key);
  }

  // Shared out under the lock so a concurrent release cannot invalidate the
  // returned callable mid-call. Null when `key` names no callable.
  std::shared_ptr<const callable> find_callable(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    if (it == table_.end()) return nullptr;
    const auto* found =
        std::get_if<std::shared_ptr<const callable>>(&it->second);
    return found == nullptr ? nullptr : *found;
  }

  std::exception_ptr find_exception(uint64_t key) {
    std::lock_guard<std::mutex> lock(mu_);
    auto it = table_.find(key);
    if (it == table_.end()) return nullptr;
    const auto* found = std::get_if<std::exception_ptr>(&it->second);
    return found == nullptr ? nullptr : *found;
  }

 private:
  using entry =
      std::variant<std::shared_ptr<const callable>, std::exception_ptr>;

  host_value_registry() {
    api().register_host_dispatch_v2(&baml_cpp_host_dispatch_trampoline);
    api().register_host_cancel_callback(&baml_cpp_host_cancel_trampoline);
    api().register_host_release_callback(&baml_cpp_host_release_trampoline);
  }

  std::mutex mu_;
  std::unordered_map<uint64_t, entry> table_;
  uint64_t next_key_ = 1;
};

// The stack a host callable runs on: the 8 MiB a main thread gets on macOS
// and Linux. The thread runs the user's callable, encodes its result, and
// runs the engine's read of that result, which follows it down as deep as
// the wire lets it nest; a thread started with the platform's default
// would get too little for that read in a debug build of the runtime (512
// KiB for a secondary thread on macOS, 1 MiB on Windows).
inline constexpr std::size_t host_call_stack_bytes = std::size_t{8} << 20;

#if defined(_WIN32)
// The entry point of a host-call thread: runs and frees the task it is
// handed.
template <typename Body>
unsigned __stdcall run_host_call_task(void* raw) {
  const std::unique_ptr<Body> owned(static_cast<Body*>(raw));
  (*owned)();
  return 0;
}
#endif

// Runs `body` on a new detached thread with `host_call_stack_bytes` of
// stack. Throws std::system_error when the thread cannot start, as
// std::thread's constructor does. `body` must not throw.
template <typename Body>
void spawn_host_call_thread(Body body) {
  auto task = std::make_unique<Body>(std::move(body));
#if defined(_WIN32)
  // The size is the stack's reservation, as a main thread's is; without the
  // flag it would be the committed size.
  const uintptr_t thread =
      _beginthreadex(nullptr, static_cast<unsigned>(host_call_stack_bytes),
                     &run_host_call_task<Body>, task.get(),
                     STACK_SIZE_PARAM_IS_A_RESERVATION, nullptr);
  if (thread == 0)
    throw std::system_error(errno, std::generic_category(),
                            "host callable thread");
  // Detached: nothing joins the thread.
  CloseHandle(reinterpret_cast<HANDLE>(thread));
#else
  pthread_attr_t attributes;
  if (const int status = pthread_attr_init(&attributes); status != 0)
    throw std::system_error(status, std::generic_category(),
                            "host callable thread attributes");
  struct attributes_guard {
    pthread_attr_t& attributes;
    ~attributes_guard() { pthread_attr_destroy(&attributes); }
  } guard{attributes};
  if (const int status =
          pthread_attr_setstacksize(&attributes, host_call_stack_bytes);
      status != 0)
    throw std::system_error(status, std::generic_category(),
                            "host callable thread stack size");
  if (const int status =
          pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
      status != 0)
    throw std::system_error(status, std::generic_category(),
                            "host callable thread detach state");
  pthread_t thread;
  const int status = pthread_create(
      &thread, &attributes,
      [](void* raw) -> void* {
        const std::unique_ptr<Body> owned(static_cast<Body*>(raw));
        (*owned)();
        return nullptr;
      },
      task.get());
  if (status != 0)
    throw std::system_error(status, std::generic_category(),
                            "host callable thread");
#endif
  // The thread owns the task now.
  static_cast<void>(task.release());
}

struct callback_frame {
  pb::HostInvocation wire;
  std::shared_ptr<invocation_state> state;
  std::atomic<bool> cancelled{false};
  ~callback_frame() {
    if (!state && wire.effective_state() != 0)
      api().handle_release(wire.effective_state());
    if (!state && wire.has_cancel()) release_control_value(wire.cancel());
  }
};
inline std::mutex callback_frames_mutex;
inline std::unordered_map<uint32_t, std::weak_ptr<callback_frame>>
    callback_frames;

// ---------------------------------------------------------------------------
// Restating a received value
// ---------------------------------------------------------------------------

// Refuses a type the host cannot state: one that names a declaration by its
// `anonymous_label`, display text the engine puts beside a value that
// crosses by reference, which names nothing a statement could resolve.
inline void require_statable(const pb::BamlTy& ty) {
  pb::BamlTy walked(ty);
  for_each_nominal(walked, [](const auto& nominal) {
    if (nominal.has_anonymous_label())
      throw bridge_error(
          "a received value's type names a declaration that has no name the "
          "host can state");
  });
}

// The shape of `v` as an InboundValue, each node under the type the engine
// gave it, still holding the engine references `v` holds.
inline void restate_shape(pb::InboundValue& out,
                          const pb::BamlOutboundValue& v) {
  switch (v.value_case()) {
    case pb::BamlOutboundValue::VALUE_NOT_SET:
    case pb::BamlOutboundValue::kNullValue:
      return;  // absent oneof = null
    case pb::BamlOutboundValue::kUnionVariantValue:
      // The envelope says which member the value came out of; the value
      // itself implies or states its type.
      if (v.union_variant_value().has_value())
        restate_shape(out, v.union_variant_value().value());
      return;
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
      // A literal is the one value of its base type it is; the scalar
      // implies it.
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
        case pb::BamlLiteralValue::kFloatValue:
        case pb::BamlLiteralValue::LITERAL_NOT_SET:
          throw bridge_error("cannot restate this literal value");
      }
      throw bridge_error("cannot restate this literal value");
    case pb::BamlOutboundValue::kListValue: {
      const encode_level level;
      if (!v.list_value().has_item_type())
        throw bridge_error("a received list carries no element type to state");
      require_statable(v.list_value().item_type());
      pb::InboundListValue* list = out.mutable_list_value();
      for (const pb::BamlOutboundValue& item : v.list_value().items())
        restate_shape(*list->add_values(), item);
      *out.mutable_value_type()->mutable_list()->mutable_item() =
          v.list_value().item_type();
      return;
    }
    case pb::BamlOutboundValue::kMapValue: {
      const encode_level level;
      if (!v.map_value().has_key_type() || !v.map_value().has_value_type())
        throw bridge_error(
            "a received map carries no key and value types to state");
      require_statable(v.map_value().key_type());
      require_statable(v.map_value().value_type());
      pb::InboundMapValue* map = out.mutable_map_value();
      for (const pb::BamlOutboundMapEntry& e : v.map_value().entries()) {
        pb::InboundMapEntry* entry = map->add_entries();
        entry->set_string_key(e.key());
        restate_shape(*entry->mutable_value(), e.value());
      }
      pb::BamlTyMap* map_ty = out.mutable_value_type()->mutable_map();
      *map_ty->mutable_key() = v.map_value().key_type();
      *map_ty->mutable_value() = v.map_value().value_type();
      return;
    }
    case pb::BamlOutboundValue::kClassValue: {
      const encode_level level;
      for (const pb::BamlTy& type_arg : v.class_value().type_args())
        require_statable(type_arg);
      pb::InboundClassValue* cls = out.mutable_class_value();
      for (const pb::BamlOutboundMapEntry& e : v.class_value().fields()) {
        pb::InboundMapEntry* field = cls->add_fields();
        field->set_string_key(e.key());
        restate_shape(*field->mutable_value(), e.value());
      }
      pb::BamlTyClass* class_ty = out.mutable_value_type()->mutable_class_ty();
      class_ty->set_name(v.class_value().name());
      *class_ty->mutable_type_args() = v.class_value().type_args();
      return;
    }
    case pb::BamlOutboundValue::kEnumValue: {
      pb::InboundEnumValue* en = out.mutable_enum_value();
      en->set_name(v.enum_value().name());
      en->set_value(v.enum_value().value());
      return;
    }
    case pb::BamlOutboundValue::kHandleValue: {
      const pb::BamlOutboundHandle& received = v.handle_value();
      pb::BamlHandle* handle = out.mutable_handle();
      handle->set_key(received.key());
      handle->set_handle_type(received.handle_type());
      // A host callable goes back stated under the function type it was
      // registered with; the engine sends it by key alone.
      if (received.handle_type() == pb::HOST_VALUE_CALLABLE) {
        const auto callable =
            host_value_registry::instance().find_callable(received.key());
        if (!callable)
          throw bridge_error(
              "a received host callable is not registered in this process");
        *out.mutable_value_type() = callable->ty;
      }
      return;
    }
    case pb::BamlOutboundValue::kMediaValue:
      *out.mutable_media_value() = v.media_value();
      return;
    case pb::BamlOutboundValue::kPromptAstValue:
      *out.mutable_prompt_ast_value() = v.prompt_ast_value();
      return;
    case pb::BamlOutboundValue::kUint8ArrayValue:
      out.set_uint8array_value(v.uint8array_value());
      return;
    case pb::BamlOutboundValue::kBigintValue:
      out.set_bigint_value(v.bigint_value());
      return;
    case pb::BamlOutboundValue::kTyValue:
      require_statable(v.ty_value());
      *out.mutable_ty_value() = v.ty_value();
      return;
  }
  throw bridge_error("cannot restate this value");
}

// Replaces every engine reference in `value` with a clone of its own: each
// engine handle and each `live` head of a type value (a host key is the
// host's own registration and goes back as it is). On failure releases the
// clones it made and leaves the received references alone.
inline void take_own_references(pb::InboundValue& value) {
  std::vector<uint64_t> cloned;
  std::vector<const pb::BamlTy*> restated;
  const std::function<void(pb::InboundValue&)> visit =
      [&](pb::InboundValue& node) {
        if (node.has_handle() &&
            node.handle().handle_type() != pb::HOST_VALUE_CALLABLE &&
            node.handle().handle_type() != pb::HOST_VALUE_OPAQUE) {
          uint64_t key = 0;
          if (api().handle_clone(node.handle().key(), &key) != 0 || key == 0)
            throw bridge_error("BAML handle clone failed");
          cloned.push_back(key);
          node.mutable_handle()->set_key(key);
        }
        if (node.has_ty_value()) {
          *node.mutable_ty_value() = restate_live_heads(node.ty_value());
          restated.push_back(&node.ty_value());
        }
        if (node.has_list_value())
          for (pb::InboundValue& item :
               *node.mutable_list_value()->mutable_values())
            visit(item);
        if (node.has_map_value())
          for (pb::InboundMapEntry& entry :
               *node.mutable_map_value()->mutable_entries())
            visit(*entry.mutable_value());
        if (node.has_class_value())
          for (pb::InboundMapEntry& field :
               *node.mutable_class_value()->mutable_fields())
            visit(*field.mutable_value());
      };
  try {
    visit(value);
  } catch (...) {
    for (const uint64_t key : cloned) api().handle_release(key);
    for (const pb::BamlTy* ty : restated) release_live_heads(*ty);
    throw;
  }
}

// Restates a received value as an InboundValue the engine takes back: a
// received `baml::value` passed back as an argument, and a BAML error or
// panic a host callable lets through from a nested call. The value goes back
// under the types the engine gave it -- a container states its element
// types, a class its identity and type arguments, a host callable its
// registered function type -- with engine references of its own, so the
// received value keeps its own. A value the host cannot state back (a type
// naming a declaration by its display label, a container the engine sent
// without its types) or a reference it cannot clone is refused as a
// bridge_error.
inline void transcode_outbound_to_inbound(pb::InboundValue& out,
                                          const pb::BamlOutboundValue& v) {
  pb::InboundValue restated;
  restate_shape(restated, v);
  take_own_references(restated);
  out.Swap(&restated);
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
// Completions
// ---------------------------------------------------------------------------

// Complete a BAML->host call with what the callable did.
inline void complete_host_call(uint32_t call_id,
                               const pb::HostCallCompletion& completion) {
  const std::string bytes = completion.SerializeAsString();
  api().complete_host_call(
      call_id, reinterpret_cast<const int8_t*>(bytes.data()), bytes.size());
}

// A failure of the bridge's own, which the engine surfaces as SdkPanic.
inline pb::HostCallCompletion bridge_failure_completion(std::string message) {
  pb::HostCallCompletion completion;
  completion.set_bridge_failure(std::move(message));
  return completion;
}

// An exception of the callable's own: registered here, so the same process
// recovers it by identity when the engine's baml.errors.HostCallable around
// it comes back.
inline pb::HostCallCompletion raised_completion(std::exception_ptr exception,
                                                std::string class_name,
                                                std::string message) {
  pb::HostCallCompletion completion;
  pb::HostException* raised = completion.mutable_raised();
  raised->set_message(std::move(message));
  raised->set_class_name(std::move(class_name));
  raised->set_language("cpp");
  raised->set_handle(
      host_value_registry::instance().add_exception(std::move(exception)));
  return completion;
}

// The two outcomes that carry a value the callable produced.
enum class host_call_value { returned, thrown };

// `write` encodes a value the callable produced. A value that fits the wire
// goes back as `outcome`. One the bridge cannot carry (a bridge_error: too
// deep for the wire, a reference it could not clone) is the bridge's own
// failure; one the encoder found not to fit its type is the callable's
// breach of contract. A refused value's references are released.
template <typename Write>
pb::HostCallCompletion value_completion(host_call_value outcome,
                                        const Write& write) {
  pb::InboundValue value;
  try {
    write(value);
    require_fits_wire(value, result_depth);
  } catch (const bridge_error& e) {
    release_input_value(value);
    return bridge_failure_completion(e.message());
  } catch (const error& e) {
    release_input_value(value);
    pb::HostCallCompletion completion;
    completion.set_contract_violation(e.message());
    return completion;
  } catch (const std::exception& e) {
    release_input_value(value);
    return bridge_failure_completion(
        std::string("host callable value could not be encoded: ") + e.what());
  }
  pb::HostCallCompletion completion;
  switch (outcome) {
    case host_call_value::returned:
      completion.mutable_returned()->Swap(&value);
      break;
    case host_call_value::thrown:
      completion.mutable_thrown()->Swap(&value);
      break;
  }
  return completion;
}

// The outcome of an exception the callable let escape. A BAML value it
// throws (baml::host_throw) goes back `thrown`; so does a BAML error or
// panic from a nested call it let through, as the value BAML threw (a panic
// stays a panic whatever the callable declares). Anything else is an
// exception of its own, `raised`.
inline pb::HostCallCompletion escaped_completion(std::exception_ptr escaped) {
  try {
    std::rethrow_exception(escaped);
  } catch (const host_throw_base& typed) {
    return value_completion(host_call_value::thrown, [&](pb::InboundValue& v) {
      typed.encode_value(v);
    });
  } catch (const error& e) {
    if (const std::shared_ptr<const thrown_value>& value = e.thrown_value())
      return value_completion(host_call_value::thrown,
                              [&](pb::InboundValue& v) {
                                transcode_outbound_to_inbound(v, value->wire);
                              });
    return raised_completion(escaped, exception_class_name(e), e.message());
  } catch (const std::exception& e) {
    return raised_completion(escaped, exception_class_name(e), e.what());
  } catch (...) {
    return raised_completion(escaped, "unknown",
                             "host callable threw a non-std::exception value");
  }
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
    throw bridge_error("host callable dispatched without a required argument");
  }
}

template <typename Staging, size_t... I>
std::tuple<typename std::tuple_element_t<I, Staging>::value_type...>
materialize_host_slots(Staging& staging, std::index_sequence<I...>) {
  return {materialize_host_slot(std::get<I>(staging))...};
}

// The callable's arguments, decoded from the engine's BamlToHostCall:
// required ones in declared order, supplied optional ones by name. `names`
// are the callable's declared BAML parameter names ("" for unnamed required
// ones). Throws for a payload the bridge cannot decode, or one that does not
// match the callable's parameters (an unknown or repeated name, more or
// fewer required arguments than declared).
template <typename... Ps>
std::tuple<std::decay_t<Ps>...> decode_host_arguments(
    const std::array<std::string, sizeof...(Ps)>& names,
    const std::vector<uint8_t>& args_bytes) {
  constexpr size_t n = sizeof...(Ps);
  using Staging = std::tuple<std::optional<std::decay_t<Ps>>...>;
  Staging staging;
  constexpr std::array<bool, n> optional_slot = {
      {is_arg<std::decay_t<Ps>>::value...}};
  const auto fillers =
      make_host_slot_fillers<Staging>(std::make_index_sequence<n>{});

  pb::BamlToHostCall call;
  if (args_bytes.size() > static_cast<size_t>(INT_MAX) ||
      !call.ParseFromArray(args_bytes.data(),
                           static_cast<int>(args_bytes.size())))
    throw bridge_error("malformed BamlToHostCall payload");
  // Decoding only borrows the arguments' type values.
  struct release_borrowed_types {
    pb::BamlToHostCall& call;
    ~release_borrowed_types() {
      for (pb::BamlToHostArg& arg : *call.mutable_args())
        release_type_values(*arg.mutable_value());
    }
  } borrowed{call};
  std::array<bool, n> filled{};
  size_t required_cursor = 0;
  for (const pb::BamlToHostArg& arg : call.args()) {
    size_t slot = n;
    if (arg.is_optional_arg()) {
      for (size_t i = 0; i < n; ++i) {
        if (optional_slot[i] && names[i] == arg.arg_name()) {
          slot = i;
          break;
        }
      }
      if (slot == n)
        throw bridge_error(
            "host callable received an unknown optional argument");
      if (filled[slot])
        throw bridge_error("host callable received an optional argument twice");
    } else {
      while (required_cursor < n && optional_slot[required_cursor])
        ++required_cursor;
      if (required_cursor == n)
        throw bridge_error(
            "host callable received more required arguments than declared "
            "parameters");
      slot = required_cursor++;
    }
    fillers[slot](staging, arg.value());
    filled[slot] = true;
  }
  return materialize_host_slots(staging, std::make_index_sequence<n>{});
}

// What a call of the host callable `fn` completes with: the arguments are
// decoded (the bridge's own failure if they cannot be), `fn` runs, and what
// it did is encoded as one outcome.
template <typename R, typename... Ps>
pb::HostCallCompletion host_call_completion(
    const std::function<R(Ps...)>& fn,
    const std::array<std::string, sizeof...(Ps)>& names,
    const std::vector<uint8_t>& args_bytes) {
  std::optional<std::tuple<std::decay_t<Ps>...>> arguments;
  try {
    arguments.emplace(decode_host_arguments<Ps...>(names, args_bytes));
  } catch (const std::exception& e) {
    return bridge_failure_completion(
        std::string("host callable arguments could not be decoded: ") +
        e.what());
  }
  if constexpr (std::is_void<R>::value) {
    try {
      std::apply(fn, std::move(*arguments));
    } catch (...) {
      return escaped_completion(std::current_exception());
    }
    // A unit result is BAML null: the InboundValue stays empty.
    return value_completion(host_call_value::returned,
                            [](pb::InboundValue&) {});
  } else {
    std::optional<R> result;
    try {
      result.emplace(std::apply(fn, std::move(*arguments)));
    } catch (...) {
      return escaped_completion(std::current_exception());
    }
    return value_completion(
        host_call_value::returned,
        [&](pb::InboundValue& v) { codec<R>::encode(v, *result); });
  }
}

// The function type a host callable is bound to, from the codecs of its C++
// signature: an `arg<U>` parameter is an optional one by its wire name, the
// others are positional; a `void` result is the unit type `null`; `throws`
// is what the generated binding declares for it.
template <typename R, typename... Ps>
pb::BamlTy callable_baml_ty(const std::array<std::string, sizeof...(Ps)>& names,
                            const pb::BamlTy& throws) {
  pb::BamlTy ty;
  pb::BamlTyFunction* function = ty.mutable_function();
  std::size_t index = 0;
  (
      [&] {
        using P = std::decay_t<Ps>;
        pb::BamlTyFunctionParam* param = function->add_params();
        if constexpr (is_arg<P>::value) {
          param->set_name(names[index]);
          param->set_mode(pb::BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL);
          param->mutable_ty()->CopyFrom(
              codec<typename arg_inner<P>::type>::baml_ty());
        } else {
          param->set_mode(pb::BAML_TY_FUNCTION_PARAM_MODE_REQUIRED);
          param->mutable_ty()->CopyFrom(codec<P>::baml_ty());
        }
        ++index;
      }(),
      ...);
  if constexpr (std::is_void<R>::value) {
    function->mutable_ret()->mutable_primitive()->set_kind(
        pb::BAML_TY_PRIMITIVE_NULL);
  } else {
    function->mutable_ret()->CopyFrom(codec<R>::baml_ty());
  }
  function->mutable_throws()->CopyFrom(throws);
  return ty;
}

// Registers `fn` for the engine to invoke and writes the handle that
// references it, stating the function type beside it: a host callable has
// no type of its own, so the one its slot declares is what the engine binds
// it to. Called from generated bindings for callable-typed arguments;
// `names` are the callable's declared parameter names in declared order
// (used to key supplied optional args).
template <typename R, typename... Ps>
void encode_callable(pb::InboundValue& value_msg, std::function<R(Ps...)> fn,
                     std::array<std::string, sizeof...(Ps)> names,
                     const pb::BamlTy& throws) {
  pb::BamlTy ty = callable_baml_ty<R, Ps...>(names, throws);
  value_msg.mutable_value_type()->CopyFrom(ty);
  host_value_registry::dispatcher dispatch =
      [fn, names = std::move(names)](uint32_t call_id,
                                     std::vector<uint8_t> args_bytes) {
        complete_host_call(
            call_id, host_call_completion<R, Ps...>(fn, names, args_bytes));
      };
  const uint64_t key = host_value_registry::instance().add_callable(
      {std::move(dispatch), std::move(ty), std::any(std::move(fn))});
  pb::BamlHandle* handle = value_msg.mutable_handle();
  handle->set_key(key);
  handle->set_handle_type(pb::HOST_VALUE_CALLABLE);
}

}  // namespace detail
}  // namespace baml

extern "C" inline void baml_cpp_host_dispatch_trampoline(const uint8_t* args,
                                                         size_t length) {
  baml::detail::pb::HostInvocation wire;
  if (length > static_cast<size_t>(INT_MAX) ||
      !wire.ParseFromArray(args, static_cast<int>(length)))
    return;
  const uint64_t host_value_key = wire.host_value_key();
  const uint32_t call_id = wire.callback_id();
  std::shared_ptr<baml::detail::callback_frame> frame;
  // Bridge-layer faults (not user exceptions) complete through the C ABI's
  // one completion channel as the bridge's own failure, which the engine
  // surfaces as baml.panics.SdkPanic.
  const auto bridge_failure = [call_id](const std::string& message) {
    baml::detail::complete_host_call(
        call_id, baml::detail::bridge_failure_completion(message));
  };
  // No C++ exception may cross the C ABI (bridge contract), and every
  // setup fault must still complete the call or the engine awaits forever:
  // bytes.assign can throw bad_alloc, the registry lock can throw, and
  // starting the thread throws system_error under thread exhaustion.
  try {
    frame = std::make_shared<baml::detail::callback_frame>();
    frame->wire = std::move(wire);
    frame->state = std::make_shared<baml::detail::invocation_state>(
        frame->wire.effective_state());
    frame->state->cancel.Swap(frame->wire.mutable_cancel());
    std::vector<uint8_t> bytes;
    const auto& application = frame->wire.application_args();
    bytes.assign(application.begin(), application.end());
    auto callable = baml::detail::host_value_registry::instance().find_callable(
        host_value_key);
    if (!callable) {
      // The engine knows a handle this registry does not: a bridge fault.
      bridge_failure("no host callable registered for key " +
                     std::to_string(host_value_key));
      return;
    }
    // Fire-and-return: the engine awaits the completion, so the user
    // callable runs on its own thread (the analog of Python's spawned
    // tokio task). The dispatcher completes the call on every exit path.
    {
      std::lock_guard<std::mutex> lock(baml::detail::callback_frames_mutex);
      baml::detail::callback_frames.emplace(call_id, frame);
    }
    baml::detail::spawn_host_call_thread([callable = std::move(callable),
                                          bridge_failure, call_id, frame,
                                          bytes = std::move(bytes)]() mutable {
      baml::detail::current_invocation_state = frame->state;
      try {
        if (frame->cancelled.load())
          // The engine retired this call's waiter before the callable ran;
          // the completion only reports the exit, as the engine's own
          // discarded-before-execution path words it.
          bridge_failure("host callback was discarded before execution");
        else
          callable->dispatch(call_id, std::move(bytes));
      } catch (...) {
        bridge_failure(
            "host callable dispatch failed outside the user callable");
      }
      baml::detail::current_invocation_state.reset();
      std::lock_guard<std::mutex> lock(baml::detail::callback_frames_mutex);
      baml::detail::callback_frames.erase(call_id);
    });
  } catch (...) {
    if (!frame) {
      baml::detail::api().handle_release(wire.effective_state());
      if (wire.has_cancel()) baml::detail::release_control_value(wire.cancel());
    }
    try {
      {
        std::lock_guard<std::mutex> lock(baml::detail::callback_frames_mutex);
        baml::detail::callback_frames.erase(call_id);
      }
      bridge_failure("host callable dispatch could not be spawned");
    } catch (...) {
      // An empty payload carries no outcome, which the engine lands as a
      // bridge failure: the exit is reported without allocating.
      baml::detail::api().complete_host_call(call_id, nullptr, 0);
    }
  }
}

extern "C" inline void baml_cpp_host_cancel_trampoline(uint32_t callback_id) {
  try {
    std::lock_guard<std::mutex> lock(baml::detail::callback_frames_mutex);
    auto it = baml::detail::callback_frames.find(callback_id);
    if (it != baml::detail::callback_frames.end())
      if (auto frame = it->second.lock()) frame->cancelled.store(true);
  } catch (...) {
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
