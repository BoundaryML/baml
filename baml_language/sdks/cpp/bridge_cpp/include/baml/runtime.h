#ifndef BAML_RUNTIME_H_
#define BAML_RUNTIME_H_

#include <baml/buffer.h>
#include <baml/detail/loader.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <cstdint>
#include <string>

namespace baml {

// Canonical BAML version of the loaded native runtime.
inline std::string version() {
  detail::owned_buffer buf{detail::api().version()};
  return buf.to_string();
}

// Initializes the process-global BAML runtime from serialized bytecode (the
// payload generated SDKs embed). Registers this bridge (language + SDK
// canonical version) first -- the contract-required ordering -- then boots.
// Replaces any previously initialized runtime.
inline void initialize_runtime_from_bytecode(const uint8_t* bytecode,
                                             size_t length,
                                             const char* sdk_version) {
  detail::ensure_registered(sdk_version);
  detail::owned_buffer failure{
      detail::api().initialize_runtime_from_bytecode(bytecode, length)};
  if (!failure.empty()) {
    throw error("BAML_RUNTIME_INITIALIZATION_FAILED: " + failure.to_string());
  }
}

}  // namespace baml

#endif  // BAML_RUNTIME_H_
