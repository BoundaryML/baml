#ifndef BAML_RUNTIME_HPP
#define BAML_RUNTIME_HPP

#include <map>
#include <string>

#include <baml_cffi.h>

#include <baml/buffer.hpp>
#include <baml/detail/json.hpp>
#include <baml/errors.hpp>

namespace baml {

// Canonical BAML version of the loaded native runtime.
inline std::string version() {
    detail::OwnedBuffer buf{::version()};
    return buf.to_string();
}

// Initializes the process-global BAML runtime from source files
// (path -> content). Replaces any previously initialized runtime.
//
// Note: the C ABI reports initialization failure only as a null return; the
// failure detail goes to stderr on the engine side.
inline void initialize_runtime(const std::string& root_path,
                               const std::map<std::string, std::string>& src_files) {
    const std::string src_files_json = detail::json_encode_string_map(src_files);
    const void* runtime = create_baml_runtime(root_path.c_str(), src_files_json.c_str());
    if (runtime == nullptr) {
        throw BamlError("failed to initialize BAML runtime (engine details on stderr)");
    }
}

// initialize_runtime_from_bytecode: pending the C ABI export of the engine's
// bytecode-init entry point (bridge_cffi has it as a Rust function only).

}  // namespace baml

#endif  // BAML_RUNTIME_HPP
