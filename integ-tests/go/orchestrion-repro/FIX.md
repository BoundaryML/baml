# Fix for Orchestrion SIGSEGV (Issue #2575)

## Problem

When BAML Go client initializes in an `init()` function with Datadog Orchestrion instrumentation enabled, it crashes with a SIGSEGV:

```
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x40 pc=0xdccbcc]

goroutine 1 [running]:
github.com/DataDog/dd-trace-go/v2/instrumentation.(*Instrumentation).AnalyticsRate(...)
```

**Root Cause**: BAML's `init()` function makes HTTP requests to download the native library, but Orchestrion's HTTP instrumentation tries to access an uninitialized Datadog tracer object, causing a nil pointer dereference.

## Solution

Create a custom HTTP client that bypasses Orchestrion's instrumentation by setting the `DD__tracer_internal` flag on the transport.

### Implementation

**File**: `engine/language_client_go/baml_go/lib_common.go`

#### 1. Add Custom HTTP Client Function

```go
// uninstrumentedHTTPClient creates an HTTP client that won't be instrumented by Orchestrion.
// This is needed because BAML initialization happens in init() before Datadog tracer is ready.
//
//orchestrion:ignore
func uninstrumentedHTTPClient() *http.Client {
	// Create a custom transport with a flag that tells Orchestrion to skip it
	transport := &http.Transport{
		DD__tracer_internal: true, // This field prevents Orchestrion instrumentation
		Proxy:               http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   30 * time.Second,
			KeepAlive: 30 * time.Second,
		}).DialContext,
		ForceAttemptHTTP2:     true,
		MaxIdleConns:          10,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}

	return &http.Client{
		Transport: transport,
		Timeout:   5 * time.Minute,
	}
}
```

#### 2. Update Download Functions

**In `downloadBamlLibrary()`:**
```go
//orchestrion:ignore
func downloadBamlLibrary(destDir string, filename string) error {
	// ... existing code ...

	// Use uninstrumented client to avoid Orchestrion crash during init()
	httpClient := uninstrumentedHTTPClient()
	resp, err := httpClient.Do(req)

	// ... rest of function ...
}
```

**In `downloadChecksum()`:**
```go
//orchestrion:ignore
func downloadChecksum(checksumURL string, targetFilename string) (string, error) {
	// Use uninstrumented client to avoid Orchestrion crash during init()
	httpClient := uninstrumentedHTTPClient()
	resp, err := httpClient.Get(checksumURL)

	// ... rest of function ...
}
```

#### 3. Add Required Import

Add `"net"` to the imports:
```go
import (
	// ... existing imports ...
	"net"
	"net/http"
	// ... more imports ...
)
```

## How It Works

1. **`DD__tracer_internal` Flag**: Orchestrion checks this field in the generated instrumentation code:
   ```go
   func (t *Transport) RoundTrip(req *Request) (*Response, error) {
       if !t.DD__tracer_internal {
           // ... instrumentation code ...
       }
       return t.roundTrip(req)
   }
   ```

2. **Custom Transport**: By setting `DD__tracer_internal: true`, our custom transport bypasses all Orchestrion instrumentation.

3. **`//orchestrion:ignore`**: This directive tells Orchestrion not to add instrumentation inside the function body (though it's the flag on the transport that actually prevents the crash).

## Testing

### Before Fix
```bash
$ docker run --rm baml-orchestrion-repro
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x40 pc=0xdccbcc]
```

### After Fix
```bash
$ docker run --rm baml-orchestrion-fixed2
Downloading libbaml_cffi-aarch64-unknown-linux-gnu.so [========================================] 52.4 MiB / 52.4 MiB 100%
2025-10-22T20:08:34.544 [BAML INFO] Checksum verified successfully
2025-10-22T20:08:34.544 [BAML INFO] Successfully downloaded and cached BAML library
2025-10-22T20:08:34.550 [BAML INFO] BAML (v0.211.2) loaded
```

✅ **No more SIGSEGV!** The HTTP requests complete successfully.

## Why This Approach?

### ✅ Advantages

1. **No Latency Penalty**: BAML still initializes eagerly in `init()`, no first-call lag
2. **Minimal Code Changes**: Only affects download functions
3. **No Breaking Changes**: Regular BAML usage (after init) can still be traced by Orchestrion
4. **Explicit and Clear**: The flag name clearly indicates its purpose

### ❌ Alternatives Considered

1. **Lazy Initialization**: Would add latency to first BAML call ❌
2. **Just `//orchestrion:ignore`**: Doesn't work - still instruments the HTTP calls we make ❌
3. **Wait for Tracer**: No clean way to wait, and would slow down startup ❌

## Commit Message

```
Fix: Prevent SIGSEGV with Orchestrion during BAML init

When Datadog Orchestrion is enabled, BAML's init() function crashes
with SIGSEGV because HTTP requests to download the native library
trigger Orchestrion's instrumentation before the tracer is initialized.

Solution: Create custom HTTP client with DD__tracer_internal flag set,
which bypasses Orchestrion instrumentation during library download.

- Add uninstrumentedHTTPClient() helper
- Update downloadBamlLibrary() to use custom client
- Update downloadChecksum() to use custom client
- Add "net" import for net.Dialer

Fixes #2575
```

## Related

- Issue: https://github.com/BoundaryML/baml/issues/2575
- Orchestrion docs: https://datadoghq.dev/orchestrion/docs/
- Files modified:
  - `engine/language_client_go/baml_go/lib_common.go`
