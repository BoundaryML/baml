#ifndef BAML_DETAIL_LOADER_H_
#define BAML_DETAIL_LOADER_H_

// Runtime loader: the bridge dlopens the shared BAML runtime and resolves
// exactly one symbol, baml_get_api_v1, per the multi-language bridge
// contract. Every C-ABI call goes through the returned BamlApiV1 table.
//
// Resolution order (contract "Canonical runtime-resolution algorithm"):
//   1. Programmatic path set via baml::set_runtime_path before first load.
//   2. BAML_RUNTIME_PATH (compatibility alias: BAML_LIBRARY_PATH; both set
//      to different values is BAML_RUNTIME_CONFIG_CONFLICT).
//   3. Executable-adjacent library (application-bundled delivery).
//   4. The shared runtime cache:
//      <cache-root>/prod/<version>/abi-v1/<target>/<filename>, where
//      cache-root is BAML_RUNTIME_CACHE_DIR, then BAML_CACHE_DIR, then
//      <BAML_HOME>/runtimes, then <home>/.baml/runtimes.
// The bridge never downloads; a miss produces BAML_RUNTIME_NOT_FOUND with
// every searched path and `baml runtime install` remediation.
//
// The library handle is deliberately never closed: the runtime lives for
// the process, and unloading a library with live engine threads is UB.

#include <baml/errors.h>
#include <baml_cffi.h>

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>
#include <vector>

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <dlfcn.h>
#include <limits.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <mach-o/dyld.h>
#endif
#endif

namespace baml {

// Structured runtime-loading failure (contract "Standard error and
// remediation contract"). code() is the stable machine identity; what()
// carries the rendered message with searched paths and remediation.
class runtime_error : public error {
 public:
  runtime_error(std::string code, const std::string& message)
      : error(code + ": " + message), code_(std::move(code)) {}

  const std::string& code() const { return code_; }

 private:
  std::string code_;
};

namespace detail {

// The canonical target triple this translation unit was compiled for.
inline const char* canonical_target() {
#if defined(__APPLE__) && defined(__aarch64__)
  return "aarch64-apple-darwin";
#elif defined(__APPLE__) && defined(__x86_64__)
  return "x86_64-apple-darwin";
#elif defined(_WIN32) && (defined(_M_ARM64) || defined(__aarch64__))
  return "aarch64-pc-windows-msvc";
#elif defined(_WIN32) && (defined(_M_X64) || defined(__x86_64__))
  return "x86_64-pc-windows-msvc";
#elif defined(__linux__) && defined(__aarch64__)
#if defined(__GLIBC__)
  return "aarch64-unknown-linux-gnu";
#else
  return "aarch64-unknown-linux-musl";
#endif
#elif defined(__linux__) && defined(__x86_64__)
#if defined(__GLIBC__)
  return "x86_64-unknown-linux-gnu";
#else
  return "x86_64-unknown-linux-musl";
#endif
#else
  return "unsupported";
#endif
}

// The canonical runtime filename for this platform (contract "Native
// filenames").
inline const char* runtime_filename() {
#if defined(_WIN32)
  return "bridge_cffi.dll";
#elif defined(__APPLE__)
  return "libbridge_cffi.dylib";
#else
  return "libbridge_cffi.so";
#endif
}

inline std::string env_or_empty(const char* name) {
  const char* v = std::getenv(name);
  return v == nullptr ? std::string() : std::string(v);
}

// Directory containing the running executable, or empty when undeterminable.
inline std::string executable_dir() {
#if defined(_WIN32)
  char buf[MAX_PATH];
  const DWORD n = GetModuleFileNameA(nullptr, buf, MAX_PATH);
  if (n == 0 || n >= MAX_PATH) {
    return std::string();
  }
  std::string path(buf, n);
#elif defined(__APPLE__)
  uint32_t size = 0;
  _NSGetExecutablePath(nullptr, &size);
  std::string path(size, '\0');
  if (_NSGetExecutablePath(&path[0], &size) != 0) {
    return std::string();
  }
  path.resize(path.find('\0'));
#else
  char buf[PATH_MAX];
  const ssize_t n = ::readlink("/proc/self/exe", buf, sizeof(buf) - 1);
  if (n <= 0) {
    return std::string();
  }
  std::string path(buf, static_cast<size_t>(n));
#endif
  const size_t slash = path.find_last_of("/\\");
  return slash == std::string::npos ? std::string() : path.substr(0, slash);
}

inline bool file_exists(const std::string& path) {
#if defined(_WIN32)
  const DWORD attrs = GetFileAttributesA(path.c_str());
  return attrs != INVALID_FILE_ATTRIBUTES &&
         !(attrs & FILE_ATTRIBUTE_DIRECTORY);
#else
  return ::access(path.c_str(), F_OK) == 0;
#endif
}

// Shared runtime cache path for `version` (contract "Runtime cache layout").
inline std::string cache_path(const std::string& version) {
  std::string root = env_or_empty("BAML_RUNTIME_CACHE_DIR");
  const std::string alias = env_or_empty("BAML_CACHE_DIR");
  if (!root.empty() && !alias.empty() && root != alias) {
    throw runtime_error(
        "BAML_RUNTIME_CONFIG_CONFLICT",
        "BAML_RUNTIME_CACHE_DIR and BAML_CACHE_DIR are both set and differ");
  }
  if (root.empty()) {
    root = alias;
  }
  if (root.empty()) {
    std::string home = env_or_empty("BAML_HOME");
    if (home.empty()) {
#if defined(_WIN32)
      home = env_or_empty("USERPROFILE");
#else
      home = env_or_empty("HOME");
#endif
      if (home.empty()) {
        return std::string();
      }
      home += "/.baml";
    }
    root = home + "/runtimes";
  }
  return root + "/prod/" + version + "/abi-v1/" + canonical_target() + "/" +
         runtime_filename();
}

// Programmatic runtime path (contract precedence step 2, after one-way env
// safety controls). Set-before-load only.
inline std::string& programmatic_path_storage() {
  static std::string path;
  return path;
}

inline bool& api_loaded_flag() {
  static bool loaded = false;
  return loaded;
}

inline void* open_library(const std::string& path, std::string& error) {
#if defined(_WIN32)
  HMODULE handle = LoadLibraryA(path.c_str());
  if (handle == nullptr) {
    error = "LoadLibrary failed with code " + std::to_string(GetLastError());
  }
  return reinterpret_cast<void*>(handle);
#else
  void* handle = ::dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL);
  if (handle == nullptr) {
    const char* msg = ::dlerror();
    error = msg == nullptr ? "dlopen failed" : msg;
  }
  return handle;
#endif
}

struct loaded_api {
  const BamlApiV1* table = nullptr;
};

// Loads the runtime and resolves the v1 table. Runs once; throws
// runtime_error on every failure path.
inline const loaded_api& load_api() {
  static const loaded_api loaded = [] {
    // Candidate paths in contract order. The version used for the cache
    // path is the SDK's expected version when registration has provided
    // one; before that the cache probe is skipped (steps 1-3 don't need
    // a version).
    std::vector<std::string> searched;
    std::string chosen;

    const std::string programmatic = programmatic_path_storage();
    const std::string env_path = env_or_empty("BAML_RUNTIME_PATH");
    const std::string alias_path = env_or_empty("BAML_LIBRARY_PATH");
    if (!env_path.empty() && !alias_path.empty() && env_path != alias_path) {
      throw runtime_error(
          "BAML_RUNTIME_CONFIG_CONFLICT",
          "BAML_RUNTIME_PATH and BAML_LIBRARY_PATH are both set and differ");
    }

    if (!programmatic.empty()) {
      chosen = programmatic;
    } else if (!env_path.empty() || !alias_path.empty()) {
      chosen = env_path.empty() ? alias_path : env_path;
    } else {
      const std::string exe_dir = executable_dir();
      if (!exe_dir.empty()) {
        const std::string adjacent = exe_dir + "/" + runtime_filename();
        if (file_exists(adjacent)) {
          chosen = adjacent;
        } else {
          searched.push_back(adjacent);
        }
      }
      if (chosen.empty()) {
        const std::string version = env_or_empty("BAML_RUNTIME_VERSION");
        if (!version.empty()) {
          const std::string cached = cache_path(version);
          if (!cached.empty()) {
            if (file_exists(cached)) {
              chosen = cached;
            } else {
              searched.push_back(cached);
            }
          }
        }
      }
    }

    if (chosen.empty()) {
      std::string message = "no BAML runtime found for target " +
                            std::string(canonical_target()) + ".";
      if (searched.empty()) {
        message += " No candidate locations were configured.";
      } else {
        message += " Searched:";
        for (const std::string& p : searched) {
          message += "\n  " + p;
        }
      }
      message +=
          "\nRun `baml runtime install`, set BAML_RUNTIME_PATH, or bundle "
          "the runtime next to the executable.";
      throw runtime_error("BAML_RUNTIME_NOT_FOUND", message);
    }

    std::string load_error;
    void* library = open_library(chosen, load_error);
    if (library == nullptr) {
      throw runtime_error("BAML_RUNTIME_LOAD_FAILED",
                          chosen + ": " + load_error);
    }

    using get_api_fn = const BamlApiV1* (*)();
    get_api_fn get_api = nullptr;
#if defined(_WIN32)
    FARPROC symbol =
        GetProcAddress(reinterpret_cast<HMODULE>(library), "baml_get_api_v1");
    static_assert(sizeof(symbol) == sizeof(get_api),
                  "Windows function pointer size mismatch");
    std::memcpy(&get_api, &symbol, sizeof(get_api));
#else
    get_api = reinterpret_cast<get_api_fn>(::dlsym(library, "baml_get_api_v1"));
#endif
    if (get_api == nullptr) {
      throw runtime_error(
          "BAML_RUNTIME_ABI_MISMATCH",
          chosen +
              " does not export baml_get_api_v1; it is not a BAML "
              "runtime or predates the versioned ABI");
    }
    const BamlApiV1* table = get_api();
    if (table == nullptr || table->abi_version != 2 ||
        table->struct_size < sizeof(BamlApiV1)) {
      throw runtime_error("BAML_RUNTIME_ABI_MISMATCH",
                          "expected ABI revision 2 table of at least " +
                              std::to_string(sizeof(BamlApiV1)) +
                              " bytes from " + chosen);
    }
    if (table->version == nullptr ||
        table->initialize_runtime_from_bytecode == nullptr ||
        table->free_buffer == nullptr || table->register_callback == nullptr ||
        table->call_function == nullptr ||
        table->new_function_call == nullptr ||
        table->cancel_function_call == nullptr ||
        table->release_function_call == nullptr ||
        table->register_program == nullptr ||
        table->create_runtime == nullptr ||
        table->unregister_runtime == nullptr ||
        table->call_function_for_runtime == nullptr ||
        table->program_key == nullptr ||
        table->register_host_dispatch_callback == nullptr ||
        table->register_host_release_callback == nullptr ||
        table->complete_host_call == nullptr ||
        table->handle_clone == nullptr || table->handle_release == nullptr ||
        table->media_from_url == nullptr || table->media_from_file == nullptr ||
        table->media_from_base64 == nullptr || table->media_url == nullptr ||
        table->media_file == nullptr || table->media_base64 == nullptr ||
        table->media_mime_type == nullptr ||
        table->register_bridge == nullptr ||
        table->register_unhandled_spawn_error_callback == nullptr ||
        table->shutdown_runtime == nullptr ||
        table->initialize_runtime_from_bytecode_with_metadata == nullptr) {
      throw runtime_error(
          "BAML_RUNTIME_ABI_MISMATCH",
          "runtime ABI table contains a null required operation");
    }

    api_loaded_flag() = true;
    // The library handle is deliberately dropped: the runtime is never
    // dlclosed (engine threads may outlive any unload point).
    return loaded_api{table};
  }();
  return loaded;
}

// The resolved v1 function table. Loads the runtime on first use.
inline const BamlApiV1& api() { return *load_api().table; }

// Registers this bridge (language + canonical SDK version) with the loaded
// runtime; the contract-required first semantic operation. Idempotent.
// A non-empty diagnostic from the runtime (version mismatch, conflicting
// registration) throws with the runtime's shared message preserved.
inline void ensure_registered(const char* toolchain_version,
                              const char* bridge_runtime_name,
                              const char* bridge_runtime_version) {
  static std::once_flag flag;
  std::call_once(flag, [toolchain_version, bridge_runtime_name,
                        bridge_runtime_version] {
    const BamlApiV1& table = api();
    const std::string version(toolchain_version);
    const std::string name(bridge_runtime_name);
    const std::string runtime_version(bridge_runtime_version);
    BamlBridgeInfoV1 info;
    info.struct_size = sizeof(BamlBridgeInfoV1);
    info.language = BAML_BRIDGE_LANGUAGE_CPP;
    info.sdk_version = reinterpret_cast<const uint8_t*>(version.data());
    info.sdk_version_len = version.size();
    info.bridge_runtime_name = reinterpret_cast<const uint8_t*>(name.data());
    info.bridge_runtime_name_len = name.size();
    info.bridge_runtime_version =
        reinterpret_cast<const uint8_t*>(runtime_version.data());
    info.bridge_runtime_version_len = runtime_version.size();
    BamlBuffer diagnostic = table.register_bridge(&info);
    if (diagnostic.ptr != nullptr && diagnostic.len != 0) {
      std::string message(reinterpret_cast<const char*>(diagnostic.ptr),
                          diagnostic.len);
      table.free_buffer(diagnostic);
      throw runtime_error("BAML_RUNTIME_VERSION_MISMATCH", message);
    }
    table.free_buffer(diagnostic);
  });
}

}  // namespace detail

// Points the loader at an explicit runtime library. Must be called before
// the first BAML operation; afterwards the configuration is frozen.
inline void set_runtime_path(const std::string& path) {
  if (detail::api_loaded_flag()) {
    throw runtime_error("BAML_RUNTIME_ALREADY_LOADED",
                        "set_runtime_path called after the runtime was loaded");
  }
  detail::programmatic_path_storage() = path;
}

}  // namespace baml

#endif  // BAML_DETAIL_LOADER_H_
