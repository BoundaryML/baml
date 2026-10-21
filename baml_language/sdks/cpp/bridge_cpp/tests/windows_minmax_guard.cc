// Windows' <windows.h> defines `min` and `max` as function-like macros, and
// `baml/detail/loader.h` includes it before most of the public headers are
// parsed. A header that then calls `std::min(...)` unparenthesized is macro
// expanded into nonsense, and every Windows consumer fails to compile — a
// break no Linux or macOS build sees.
//
// This translation unit reproduces that ordering on any platform: protobuf's
// headers come first, exactly as they do on Windows (they defend themselves
// against these macros under `_WIN32`, which is compiled out elsewhere), then
// the macros, then the public headers under test.
//
// Nothing here runs. It exists so the smoke build fails at compile time.

#include <baml/detail/proto.h>

#ifndef min
#define min(a, b) (((a) < (b)) ? (a) : (b))
#endif
#ifndef max
#define max(a, b) (((a) > (b)) ? (a) : (b))
#endif

#include <baml/baml.h>
