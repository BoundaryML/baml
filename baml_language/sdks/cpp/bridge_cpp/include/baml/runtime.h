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
inline std::string Version() {
  detail::OwnedBuffer buf{detail::Api().version()};
  return buf.to_string();
}

// Initializes the process-global BAML runtime from serialized bytecode (the
// payload generated SDKs embed). Registers this bridge (language + SDK
// canonical version) first -- the contract-required ordering -- then boots.
// Replaces any previously initialized runtime.
inline void InitializeRuntimeFromBytecode(const uint8_t* bytecode,
                                          size_t length,
                                          const char* sdk_version) {
  detail::EnsureRegistered(sdk_version);
  detail::OwnedBuffer error{
      detail::Api().initialize_runtime_from_bytecode(bytecode, length)};
  if (!error.empty()) {
    throw BamlError("BAML_RUNTIME_INITIALIZATION_FAILED: " + error.to_string());
  }
}

}  // namespace baml

#endif  // BAML_RUNTIME_H_
