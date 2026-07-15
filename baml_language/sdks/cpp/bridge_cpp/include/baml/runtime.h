#ifndef BAML_RUNTIME_H_
#define BAML_RUNTIME_H_

#include <baml/buffer.h>
#include <baml/detail/json.h>
#include <baml/detail/loader.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <cstdint>
#include <map>
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

// Initializes the process-global BAML runtime from source files
// (path -> content). Dev-only path (bridge tests); generated SDKs boot from
// bytecode. Uses the legacy create_baml_runtime symbol, which is outside
// the v1 table, and registers with the runtime's own version (the source
// path has no generated SDK to carry a stamp).
//
// Note: the legacy C ABI reports failure only as a null return; the detail
// goes to stderr on the engine side.
inline void InitializeRuntime(
    const std::string& root_path,
    const std::map<std::string, std::string>& src_files) {
  const std::string runtime_version = Version();
  detail::EnsureRegistered(runtime_version.c_str());
  using CreateFn = const void* (*)(const char*, const char*);
  auto create =
      reinterpret_cast<CreateFn>(detail::RawSymbol("create_baml_runtime"));
  if (create == nullptr) {
    throw BamlError("loaded runtime does not export create_baml_runtime");
  }
  const std::string src_files_json = detail::JsonEncodeStringMap(src_files);
  const void* runtime = create(root_path.c_str(), src_files_json.c_str());
  if (runtime == nullptr) {
    throw BamlError(
        "failed to initialize BAML runtime (engine details on stderr)");
  }
}

}  // namespace baml

#endif  // BAML_RUNTIME_H_
