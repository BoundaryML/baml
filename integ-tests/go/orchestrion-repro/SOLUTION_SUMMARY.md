# Solution Summary: Fixed Orchestrion SIGSEGV in BAML Go Client

## Issue

**GitHub Issue**: #2575
**Problem**: SIGSEGV crash when using BAML with Datadog Orchestrion APM instrumentation

```
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x40]
at github.com/DataDog/dd-trace-go/v2/instrumentation.(*Instrumentation).AnalyticsRate()
```

## Root Cause

1. BAML's Go client initializes in an `init()` function
2. During init, it downloads the native library via HTTP requests
3. Orchestrion instruments all HTTP transports globally
4. The instrumentation tries to access Datadog tracer state
5. **Crash**: Tracer not yet initialized when BAML's init() runs

## Solution

✅ **Create custom HTTP client with `DD__tracer_internal` flag to bypass Orchestrion instrumentation**

### Changes Made

**File**: `engine/language_client_go/baml_go/lib_common.go`

1. **Added custom HTTP client function** (35 lines):
   - Sets `DD__tracer_internal: true` on transport
   - This flag tells Orchestrion to skip instrumentation
   - Marked with `//orchestrion:ignore` directive

2. **Updated `downloadBamlLibrary()`**:
   - Use `uninstrumentedHTTPClient()` instead of `http.DefaultClient`

3. **Updated `downloadChecksum()`**:
   - Use `uninstrumentedHTTPClient()` instead of `http.Get()`

4. **Added `"net"` import**:
   - Required for `net.Dialer`

**Total**: ~50 lines added/changed

## Test Results

### Before Fix ❌
```
$ docker run --rm baml-orchestrion-repro
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x40 pc=0xdccbcc]
```

### After Fix ✅
```
$ docker run --rm baml-orchestrion-fixed2
Downloading libbaml_cffi-aarch64-unknown-linux-gnu.so [========] 52.4 MiB 100%
2025-10-22T20:08:34.544 [BAML INFO] Checksum verified successfully
2025-10-22T20:08:34.544 [BAML INFO] Successfully downloaded and cached BAML library
2025-10-22T20:08:34.550 [BAML INFO] BAML (v0.211.2) loaded
```

✅ **Library downloads successfully**
✅ **Checksum verification works**
✅ **BAML loads without crash**
✅ **No SIGSEGV!**

## Why This Solution?

### ✅ Advantages

1. **Minimal Code Changes**: Only affects download functions, ~50 lines
2. **No Performance Impact**: Still eager initialization, no first-call latency
3. **Explicit Intent**: Flag name clearly documents purpose
4. **Uses Orchestrion's Built-in Mechanism**: The `DD__tracer_internal` flag is Orchestrion's intended way to skip instrumentation
5. **No Breaking Changes**: Rest of BAML can still be traced normally

### 🔍 How It Works

Orchestrion generates this code in `net/http/transport.go`:
```go
func (t *Transport) RoundTrip(req *Request) (*Response, error) {
    if !t.DD__tracer_internal {  // ← Checks our flag
        // ... instrumentation code ...
    }
    return t.roundTrip(req)
}
```

By setting `DD__tracer_internal: true`, we tell Orchestrion: "This is internal Datadog traffic, don't instrument it."

## Reproduction Environment Created

Created complete test environment in `/integ-tests/go/orchestrion-repro/`:

- ✅ `Dockerfile` - Multi-stage build with Orchestrion + `-work` flag
- ✅ `main.go` - Minimal reproduction test
- ✅ `Makefile` - Build and test helpers
- ✅ `README.md` - Usage documentation
- ✅ `FINDINGS.md` - Detailed technical analysis
- ✅ `FIX.md` - Fix implementation details
- ✅ `SOLUTION_SUMMARY.md` - This file
- ✅ `.dockerignore` - Optimized (10KB context vs 5GB+)

## Files Modified

```
engine/language_client_go/baml_go/lib_common.go
  + Add uninstrumentedHTTPClient() function
  + Update downloadBamlLibrary() to use it
  + Update downloadChecksum() to use it
  + Add "net" import
  + Add //orchestrion:ignore directives
```

## Verification Commands

```bash
# Build with the fix
cd /path/to/baml
docker build -t baml-orchestrion-fixed2 -f integ-tests/go/orchestrion-repro/Dockerfile .

# Run and verify no crash
docker run --rm baml-orchestrion-fixed2

# Should see:
# - Library downloading with progress bar
# - Checksum verification
# - "BAML (v0.211.2) loaded"
# - NO SIGSEGV
```

## Next Steps

1. ✅ **Solution is ready to commit**
2. Review the changes in `lib_common.go`
3. Run existing Go tests to ensure no regressions
4. Update issue #2575 with fix details
5. Consider adding a test case for Orchestrion compatibility

## Credits

- Issue Reporter: @[username] in #2575
- Solution: Using Orchestrion's `DD__tracer_internal` flag
- Testing: Docker-based reproduction environment
- Analysis: Inspected Orchestrion's code transformations via `-work` flag
