# Pinned protobuf-lite runtime for the BAML C++ SDK, fetched hermetically.
#
# The version MUST match the protoc that generated the checked-in pb
# sources (protoc-bin-vendored -> protoc 31.1; see
# sdkgen_cpp/tests/pb_generation.rs). protobuf fetches its own pinned
# abseil (protobuf_FORCE_FETCH_DEPENDENCIES), so no system dependency is
# consulted and no version skew is possible.
#
# Offline/dev overrides (standard CMake FetchContent knobs):
#   -DFETCHCONTENT_SOURCE_DIR_PROTOBUF=/path/to/protobuf-31.1  (no network)
#   -DFETCHCONTENT_BASE_DIR=/shared/cache  (share the clone across builds)

include(FetchContent)

# protobuf and abseil MUST compile at the same C++ standard as their
# consumers: abseil's ABI changes with the dialect (absl::string_view is
# std::string_view only from C++17), and a split inside one build tree is
# a link error. Directory-scoped, so a consumer's own setting is untouched
# unless absent.
if(NOT DEFINED CMAKE_CXX_STANDARD)
  set(CMAKE_CXX_STANDARD 17)
endif()
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(protobuf_BUILD_TESTS OFF)
set(protobuf_BUILD_PROTOC_BINARIES OFF)
set(protobuf_BUILD_LIBUPB OFF)
set(protobuf_INSTALL OFF)
set(protobuf_FORCE_FETCH_DEPENDENCIES ON)
# Match CMake's default dynamic MSVC runtime used by SDK consumers.
set(protobuf_MSVC_STATIC_RUNTIME OFF)

FetchContent_Declare(protobuf
  GIT_REPOSITORY https://github.com/protocolbuffers/protobuf.git
  GIT_TAG v31.1
  GIT_SHALLOW TRUE)
FetchContent_MakeAvailable(protobuf)
