#ifndef BAML_RUNTIME_H_
#define BAML_RUNTIME_H_

#include <baml/buffer.h>
#include <baml/codec.h>
#include <baml/detail/loader.h>
#include <baml/errors.h>
#include <baml/version.h>
#include <baml_cffi.h>

#include <cstdint>
#include <cstdlib>
#include <exception>
#include <iostream>
#include <mutex>
#include <string>
#include <vector>

namespace baml {

namespace detail {

inline void host_default_unhandled_spawn_error(std::exception_ptr error,
                                               bool cancelled) {
  if (cancelled) {
    try {
      std::rethrow_exception(error);
    } catch (const std::exception& exception) {
      std::cerr << "BAML spawned work was cancelled: " << exception.what()
                << std::endl;
    }
    return;
  }
  std::rethrow_exception(error);
}

inline void report_unhandled_spawn_error(std::vector<uint8_t> payload,
                                         bool cancelled) {
  try {
    pb::BamlOutboundResult result = parse_result_envelope(payload);
    if (result.result_case() == pb::BamlOutboundResult::kOk) {
      throw error("BAML spawned work failed without an error result");
    }
    throw_from_result(result);
  } catch (...) {
    host_default_unhandled_spawn_error(std::current_exception(), cancelled);
  }
}

}  // namespace detail

extern "C" inline void baml_cpp_unhandled_spawn_error_trampoline(
    const int8_t* content, size_t length, int32_t cancelled) {
  std::vector<uint8_t> payload;
  if (content != nullptr && length != 0) {
    payload.assign(reinterpret_cast<const uint8_t*>(content),
                   reinterpret_cast<const uint8_t*>(content) + length);
  }
  try {
    detail::report_unhandled_spawn_error(std::move(payload), cancelled != 0);
  } catch (...) {
    std::terminate();
  }
}

inline void install_shutdown_hook();

inline void register_program(uint64_t key, const uint8_t* bytecode, size_t length, const char* metadata = nullptr) {
  detail::ensure_registered(toolchain_version(), kBridgeRuntimeName, bridge_runtime_version());
  const auto& api = detail::api();
  const size_t required = offsetof(BamlApiV1, call_function_for_runtime) + sizeof(api.call_function_for_runtime);
  if (api.struct_size < required || !api.register_program || !api.call_function_for_runtime)
    throw error("The BAML library does not support uint64 runtime registration");
  api.register_unhandled_spawn_error_callback(baml_cpp_unhandled_spawn_error_trampoline);
  install_shutdown_hook();
  detail::owned_buffer failure{api.register_program(key, bytecode, length, metadata)};
  if (!failure.empty()) throw error(failure.to_string());
}

inline void shutdown_runtime();

inline void install_shutdown_hook() {
  static std::once_flag shutdown_hook;
  std::call_once(shutdown_hook, [] {
    std::atexit([] {
      try {
        shutdown_runtime();
      } catch (const std::exception& exception) {
        std::cerr << exception.what() << std::endl;
      }
    });
  });
}

// Canonical BAML version of the loaded native runtime.
inline std::string version() {
  detail::owned_buffer buf{detail::api().version()};
  return buf.to_string();
}

// Initializes the process-global BAML runtime from serialized bytecode (the
// payload generated SDKs embed). Registers this bridge (language + SDK
// canonical version) first -- the contract-required ordering -- then boots.
// Creates a legacy unkeyed registration; generated SDKs use register_program.
inline void initialize_runtime_from_bytecode(const uint8_t* bytecode,
                                             size_t length,
                                             const char* sdk_version) {
  static_cast<void>(sdk_version);
  detail::ensure_registered(toolchain_version(), kBridgeRuntimeName,
                            bridge_runtime_version());
  detail::api().register_unhandled_spawn_error_callback(
      baml_cpp_unhandled_spawn_error_trampoline);
  install_shutdown_hook();
  detail::owned_buffer failure{
      detail::api().initialize_runtime_from_bytecode(bytecode, length)};
  if (!failure.empty()) {
    throw error(failure.to_string());
  }
}

inline void initialize_runtime_from_bytecode_with_metadata(
    const uint8_t* bytecode, size_t length, const char* embedded_baml_toml) {
  detail::ensure_registered(toolchain_version(), kBridgeRuntimeName,
                            bridge_runtime_version());
  detail::api().register_unhandled_spawn_error_callback(
      baml_cpp_unhandled_spawn_error_trampoline);
  install_shutdown_hook();
  detail::owned_buffer failure{
      detail::api().initialize_runtime_from_bytecode_with_metadata(
          bytecode, length, embedded_baml_toml)};
  if (!failure.empty()) {
    throw error(failure.to_string());
  }
}

inline void shutdown_runtime() {
  detail::owned_buffer failure{detail::api().shutdown_runtime()};
  if (!failure.empty()) {
    throw error("BAML_RUNTIME_SHUTDOWN_FAILED: " + failure.to_string());
  }
}

}  // namespace baml

#endif  // BAML_RUNTIME_H_
