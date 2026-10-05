// Windows' <windows.h> defines `min` and `max` as function-like macros, and
// `baml/detail/loader.h` includes it before the rest of the bridge headers
// are parsed. A header that then calls `std::min(...)` or
// `std::numeric_limits<T>::max()` unparenthesized is macro expanded into
// nonsense, and every Windows consumer fails to compile — a break no Linux
// or macOS build sees.
//
// This translation unit reproduces that ordering on any platform. The
// standard library, platform and protobuf headers come first: on Windows
// they are immune to these macros (MSVC's STL parenthesizes its own calls,
// protobuf defends itself under `_WIN32`), while libstdc++ and libc++ are
// not. Then the macros, then every bridge header under test, including the
// `baml/detail` internals. When a bridge header starts using a standard
// header that is missing from the list below, add it to the list.
//
// Nothing here runs. It exists so the smoke build fails at compile time.

#include <algorithm>
#include <array>
#include <atomic>
#include <cctype>
#include <chrono>
#include <climits>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <functional>
#include <future>
#include <iostream>
#include <limits>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <ostream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <thread>
#include <tuple>
#include <type_traits>
#include <typeinfo>
#include <unordered_map>
#include <utility>
#include <variant>
#include <vector>

#if defined(__cpp_impl_coroutine) && __has_include(<coroutine>)
#include <coroutine>
#endif
#if !defined(_MSC_VER)
#include <cxxabi.h>
#endif
#if !defined(_WIN32)
#include <dlfcn.h>
#include <limits.h>
#include <unistd.h>
#endif
#if defined(__APPLE__)
#include <mach-o/dyld.h>
#endif

#include "baml_bridge/cffi/v1/baml_handle.pb.h"
#include "baml_bridge/cffi/v1/baml_inbound.pb.h"
#include "baml_bridge/cffi/v1/baml_outbound.pb.h"
#include "baml_bridge/cffi/v1/baml_type.pb.h"

#ifndef min
#define min(a, b) (((a) < (b)) ? (a) : (b))
#endif
#ifndef max
#define max(a, b) (((a) > (b)) ? (a) : (b))
#endif

#include <baml/baml.h>
#include <baml/detail/loader.h>
#include <baml/detail/proto.h>
