# Orchestrion SIGSEGV - Technical Analysis

## Summary

Successfully reproduced and analyzed issue #2575. The SIGSEGV occurs when Datadog's Orchestrion HTTP instrumentation attempts to access uninitialized state during BAML's package initialization.

## What Orchestrion Injected

### 1. Main Program Modifications (`main.go`)

Orchestrion added:
```go
import (
    __orchestrion_tracer "github.com/DataDog/dd-trace-go/v2/ddtrace/tracer"
    __orchestrion_env "github.com/DataDog/dd-trace-go/v2/instrumentation/env"
    __orchestrion_profiler "github.com/DataDog/dd-trace-go/v2/profiler"
)

func main() {
    // Added defer statements at the top
    {
        defer __orchestrion_profiler.Stop()
    }
    {
        defer __orchestrion_tracer.Stop()
    }
    // ... original main code
}

// Added init functions
func init() { __orchestrion_tracer.Start() }
func init() {
    // Conditional profiler start based on DD_PROFILING_ENABLED
}
```

### 2. HTTP Transport Modifications (`net/http/transport.go`)

Orchestrion modified `Transport.RoundTrip` method:

```go
func (t *Transport) RoundTrip(req *Request) (__result__0 *Response, __result__1 error) {
    {
        if !t.DD__tracer_internal {
            var (
                __after__ ddAfterRoundTrip
                __err__   error
            )
            // THIS IS WHERE IT CRASHES
            req, __after__, __err__ = __dd_httptrace_ObserveRoundTrip(req)
            if __err__ != nil {
                return nil, __err__
            }
            defer func() {
                __result__0, __result__1 = __after__(__result__0, __result__1)
            }()
        }
    }
    return t.roundTrip(req)
}

//go:linkname __dd_httptrace_ObserveRoundTrip github.com/DataDog/dd-trace-go/contrib/net/http/v2/internal/orchestrion.ObserveRoundTrip
func __dd_httptrace_ObserveRoundTrip(*Request) (*Request, ddAfterRoundTrip, error)
```

## The Crash Sequence

1. **Package Init**: Go runtime initializes packages in dependency order
2. **BAML Init**: `baml_go.init()` runs (line 68 of `lib_common.go`)
3. **Library Download**: BAML detects no cached library, starts download
4. **HTTP Request**: First HTTP GET to fetch checksum file (`lib_common.go:524`)
5. **Orchestrion Intercept**: `Transport.RoundTrip` is called
6. **Injected Code Runs**: `__dd_httptrace_ObserveRoundTrip(req)` is invoked
7. **Config Access**: Inside, it calls `defaultRoundTripperConfig()` (line 31)
8. **Nil Pointer**: Tries to access `Instrumentation.AnalyticsRate()` (line 38)
9. **CRASH**: Nil pointer dereference at `instrumentation.go:97`

Stack trace location:
```
github.com/DataDog/dd-trace-go/v2/instrumentation.(*Instrumentation).AnalyticsRate(...)
    instrumentation/instrumentation.go:97
```

## Root Cause

The instrumentation configuration (`defaultRoundTripperConfig`) is initialized using `sync.Once`, but it tries to access an `Instrumentation` object that hasn't been created yet because:

1. **Init Order**: BAML's `init()` runs before the tracer is fully initialized
2. **Early HTTP**: The HTTP request happens during package initialization
3. **Uninitialized State**: Datadog's instrumentation assumes the tracer is already running
4. **Nil Access**: The `Instrumentation` pointer is nil, causing the segfault

From `roundtrip.go:31-40`:
```go
var defaultRTConfig rtconfig.Config
var defaultRTConfigOnce sync.Once

func defaultRoundTripperConfig() rtconfig.Config {
    defaultRTConfigOnce.Do(func() {
        defaultRTConfig = rtconfig.New(
            rtconfig.WithAnalyticsRate(instrumentation.AnalyticsRate()),  // ← CRASHES HERE
            // ...
        )
    })
    return defaultRTConfig
}
```

## Testing Results

### SIGSEGV Reproduction
✅ **Confirmed** - Crashes consistently

### With CGO_ENABLED=1
✅ **Still crashes** - Same error, not CGO-related

### Without Orchestrion
✅ **Works fine** - BAML downloads library and runs successfully

## Code Analysis from Docker Work Directory

### Commands Used to Inspect Orchestrion Transformations

1. **Build Docker image targeting build stage**:
```bash
docker build --target build -t baml-orchestrion-build \
    -f integ-tests/go/orchestrion-repro/Dockerfile .
```

2. **Find orchestrion subdirectories**:
```bash
docker run --rm baml-orchestrion-build \
    find /build/orchestrion-work -type d -name "orchestrion"
```

3. **Find modified source files**:
```bash
docker run --rm baml-orchestrion-build \
    find /build/orchestrion-work -name "main.go" -path "*/orchestrion/src/*"
```

4. **View modified main.go**:
```bash
docker run --rm baml-orchestrion-build \
    cat /build/orchestrion-work/b001/orchestrion/src/main/main.go
```

5. **Find HTTP-related modifications**:
```bash
docker run --rm baml-orchestrion-build \
    find /build/orchestrion-work -path "*/orchestrion/src/*" -name "*.go" | \
    grep -E "(http|transport|client)"
```

6. **View modified HTTP transport**:
```bash
docker run --rm baml-orchestrion-build \
    cat /build/orchestrion-work/b103/orchestrion/src/net/http/roundtrip.go
```

### Work Directory Structure

Build with `-work` flag preserved transformations at:
- **Work root**: `/tmp/go-build1696063241/` (printed at build time)
- **Copied to**: `/build/orchestrion-work/` (in Docker image)
- **Main program**: `b001/orchestrion/src/main/main.go`
- **HTTP transport**: `b103/orchestrion/src/net/http/roundtrip.go`
- **218 package directories** total with orchestrion modifications

## Potential Solutions

### 1. **Lazy BAML Initialization** (Recommended)
Move library download out of `init()` to first function call:
```go
var initOnce sync.Once

func ensureInitialized() error {
    initOnce.Do(func() {
        initErr = initializeBaml()
    })
    return initErr
}

// Call ensureInitialized() at the start of each exported function
```

### 2. **Pre-download Library**
Use `BAML_LIBRARY_PATH` to provide pre-downloaded library:
```dockerfile
RUN mkdir -p /app/baml-libs && \
    curl -L -o /app/baml-libs/libbaml.so https://github.com/.../libbaml_cffi-*.so
ENV BAML_LIBRARY_PATH=/app/baml-libs/libbaml.so
```

### 3. **Disable Auto-Download**
Use `BAML_LIBRARY_DISABLE_DOWNLOAD=true` and bundle library in container

### 4. **Orchestrion Fix**
Report to Datadog - their instrumentation should gracefully handle being called before tracer initialization

## Files Generated

- `orchestrion-repro/` - Complete reproduction environment
- `orchestrion-repro/Dockerfile` - Multi-stage build with `-work` flag
- `.dockerignore` - Optimized context (10.7KB vs 5+ GB)
- `FINDINGS.md` - This document

## References

- Issue: https://github.com/BoundaryML/baml/issues/2575
- BAML lib_common.go:68-86 - Initialization code
- BAML lib_common.go:524 - Crash point (downloadChecksum)
- dd-trace-go/v2 instrumentation.go:97 - Nil dereference location
- dd-trace-go contrib/net/http/v2/internal/orchestrion/roundtrip.go:22-40

## Environment Details

- **Go Version**: 1.24.0
- **Orchestrion**: v1.4.0
- **dd-trace-go**: v2.3.0
- **BAML Version**: 0.211.2
- **Platform**: linux/arm64 (Docker)
