// Windows' <windows.h> defines `min` and `max` as function-like macros, and
// `baml/detail/loader.h` includes it before the rest of the bridge headers
// are parsed. A header that then calls `std::min(...)` or
// `std::numeric_limits<T>::max()` unparenthesized is macro expanded into
// nonsense, and every Windows consumer fails to compile — a break no Linux
// or macOS build sees.
//
// This translation unit reproduces that ordering on any platform. The
// standard, platform and protobuf headers that `loader.h` and `proto.h`
// include come first: on Windows they are immune to these macros, while
// libstdc++ and libc++ are not. Then the macros, then the bridge headers
// under test, `loader.h` and `proto.h` included.
//
// Nothing here runs. It exists so the smoke build fails at compile time.

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <limits>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#if !defined(_WIN32)
#include <dlfcn.h>
#include <limits.h>
#include <unistd.h>
#endif
#if defined(__APPLE__)
#include <mach-o/dyld.h>
#endif

#include "baml_bridge/cffi/v1/baml_inbound.pb.h"
#include "baml_bridge/cffi/v1/baml_outbound.pb.h"

#ifndef min
#define min(a, b) (((a) < (b)) ? (a) : (b))
#endif
#ifndef max
#define max(a, b) (((a) > (b)) ? (a) : (b))
#endif

#include <baml/baml.h>
#include <baml/detail/loader.h>
#include <baml/detail/proto.h>
