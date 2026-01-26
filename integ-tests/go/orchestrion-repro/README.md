# Orchestrion SIGSEGV Reproduction

This directory contains a minimal reproduction case for issue #2575: SIGSEGV when using BAML with Datadog Orchestrion.

## Issue

When building a Go application with BAML and Datadog's Orchestrion APM instrumentation, a segmentation violation occurs during initialization when BAML attempts to download its native library.

Error: `panic: runtime error: invalid memory address or nil pointer dereference [signal SIGSEGV: segmentation violation code=0x1 addr=0x50 pc=0xf992ec]`

## Setup

This test program:
1. Initializes the BAML Go client (which triggers library download)
2. Calls a simple BAML function
3. Should trigger the SIGSEGV when built with Orchestrion instrumentation

## Usage

### Build and run with Docker (reproduces the issue):
```bash
cd /path/to/baml/integ-tests/go
make build-docker
make run-docker
```

Or with debug logging:
```bash
make debug-docker
```

### Test without Orchestrion (should work fine):
```bash
cd /path/to/baml/integ-tests/go
make test
```

### Build locally with Orchestrion:
```bash
# First install orchestrion
go install github.com/DataDog/orchestrion@v1.4.0

# Then build
cd /path/to/baml/integ-tests/go
make build
./app
```

## Expected Behavior

- **Without Orchestrion**: Program runs successfully, downloads BAML library, executes function
- **With Orchestrion**: SIGSEGV during HTTP request when downloading checksum file

## Reproduction Results

✅ **Successfully reproduced the SIGSEGV!**

### Error Details

```
panic: runtime error: invalid memory address or nil pointer dereference
[signal SIGSEGV: segmentation violation code=0x1 addr=0x40 pc=0xdccbcc]

goroutine 1 [running]:
github.com/DataDog/dd-trace-go/v2/instrumentation.(*Instrumentation).AnalyticsRate(...)
    /go/pkg/mod/github.com/!data!dog/dd-trace-go/v2@v2.3.0/instrumentation/instrumentation.go:97
```

### What's Happening

1. BAML initialization (`init()`) runs when the package is first imported
2. BAML detects no cached library and attempts to download from GitHub
3. First HTTP request: Fetch checksum file at `libbaml_cffi-aarch64-unknown-linux-gnu.so.sha256`
4. Orchestrion intercepts the HTTP transport via `ObserveRoundTrip()`
5. Datadog instrumentation tries to access `Instrumentation.AnalyticsRate()` with a nil pointer
6. **CRASH** - nil pointer dereference at `instrumentation.go:97`

### Root Cause

The crash occurs in Orchestrion's HTTP instrumentation layer before BAML can complete its first network request. The issue is in `dd-trace-go/contrib/net/http/v2/internal/orchestrion/roundtrip.go:38` where it attempts to configure HTTP tracing but the instrumentation object is not properly initialized.

This happens during package initialization (in an `init()` function), which may be too early for Orchestrion's instrumentation to be fully set up.

## Debugging

### Inspect Orchestrion Code Transformations

The Dockerfile already builds with `-work` flag. To inspect the transformed code:

```bash
# Build the image targeting only the build stage
cd /path/to/baml
docker build --target build -t baml-orchestrion-build \
    -f integ-tests/go/orchestrion-repro/Dockerfile .

# List all orchestrion transformation directories
docker run --rm baml-orchestrion-build \
    find /build/orchestrion-work -type d -name "orchestrion"

# View the modified main.go
docker run --rm baml-orchestrion-build \
    cat /build/orchestrion-work/b001/orchestrion/src/main/main.go

# View the modified HTTP roundtrip code
docker run --rm baml-orchestrion-build \
    cat /build/orchestrion-work/b103/orchestrion/src/net/http/roundtrip.go

# Find all HTTP-related modifications
docker run --rm baml-orchestrion-build \
    find /build/orchestrion-work -path "*/orchestrion/src/*" -name "*.go" | \
    grep -E "(http|transport)"
```

### Enable Detailed Logging

Run the container with debug logging:
```bash
docker run --rm \
    -e ORCHESTRION_LOG_LEVEL=DEBUG \
    -e BAML_LOG=DEBUG \
    baml-orchestrion-repro
```

### Local Testing with Orchestrion

If testing locally with orchestrion installed:
```bash
cd /path/to/baml/integ-tests/go
orchestrion go build -work -o app ./orchestrion-repro
# The WORK= directory will be printed
# Inspect: ls $WORK_DIR/*/orchestrion/src/
./app
```

## Potential Workarounds

To investigate further or work around this issue:

1. **Pre-download the library**: Set `BAML_LIBRARY_PATH` to a pre-downloaded library file to skip the download during init
2. **Lazy initialization**: Modify BAML to defer library download until first use instead of during package init
3. **Disable auto-download**: Set `BAML_LIBRARY_DISABLE_DOWNLOAD=true` and provide the library via other means
4. **Use Orchestrion work directory**: Build with `orchestrion go build -work` to inspect the instrumented code

## Files in This Directory

- `main.go` - Minimal BAML test program that triggers the crash
- `Dockerfile` - Multi-stage build with Orchestrion (based on issue #2575)
- `Makefile` - Build and test helpers
- `README.md` - This file
- `.dockerignore` - Excludes large build artifacts from Docker context

## Related Links

- Issue: https://github.com/BoundaryML/baml/issues/2575
- Orchestrion docs: https://datadoghq.dev/orchestrion/docs/troubleshooting/
- BAML docs: https://docs.boundaryml.com/
