#ifndef BAML_RUNTIME_H_
#define BAML_RUNTIME_H_

#include <baml/buffer.h>
#include <baml/detail/json.h>
#include <baml/errors.h>
#include <baml_cffi.h>

#include <map>
#include <string>

namespace baml {

// Canonical BAML version of the loaded native runtime.
inline std::string Version() {
  detail::OwnedBuffer buf{::version()};
  return buf.to_string();
}

// Initializes the process-global BAML runtime from source files
// (path -> content). Replaces any previously initialized runtime.
//
// Note: the C ABI reports initialization failure only as a null return; the
// failure detail goes to stderr on the engine side.
inline void InitializeRuntime(
    const std::string& root_path,
    const std::map<std::string, std::string>& src_files) {
  const std::string src_files_json = detail::JsonEncodeStringMap(src_files);
  const void* runtime =
      create_baml_runtime(root_path.c_str(), src_files_json.c_str());
  if (runtime == nullptr) {
    throw BamlError(
        "failed to initialize BAML runtime (engine details on stderr)");
  }
}

// Initializes the process-global BAML runtime from serialized bytecode (the
// payload generated SDKs embed). Replaces any previously initialized runtime.
inline void InitializeRuntimeFromBytecode(const uint8_t* bytecode,
                                          size_t length) {
  const void* runtime = create_baml_runtime_from_bytecode(bytecode, length);
  if (runtime == nullptr) {
    throw BamlError(
        "failed to initialize BAML runtime from bytecode (engine details on "
        "stderr)");
  }
}

}  // namespace baml

#endif  // BAML_RUNTIME_H_
