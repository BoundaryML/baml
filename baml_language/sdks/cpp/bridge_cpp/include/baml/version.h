#ifndef BAML_VERSION_H_
#define BAML_VERSION_H_

namespace baml {

inline constexpr const char* kBridgeRuntimeName = "BAML C++ bridge";
inline constexpr const char* kToolchainVersion = "0.16.0";
inline constexpr const char* kBridgeRuntimeVersion = "0.16.0";

inline constexpr const char* toolchain_version() { return kToolchainVersion; }
inline constexpr const char* bridge_runtime_version() {
  return kBridgeRuntimeVersion;
}

}  // namespace baml

#endif  // BAML_VERSION_H_
