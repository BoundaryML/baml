# Windows Support for Go BAML Client

## Overview
The Go BAML client now supports Windows (x86_64 and ARM64) in addition to macOS and Linux.

## Building on Windows
The Go client requires CGO to interface with the BAML runtime library. When building on Windows:

```bash
# Build normally on Windows
go build ./...

# The library will automatically be downloaded to:
# %LOCALAPPDATA%\baml\libs\{VERSION}\baml_cffi-{target}.dll
```

## Cross-Compilation Limitations
Due to CGO requirements, cross-compiling from Unix to Windows requires a Windows cross-compiler:

```bash
# This will NOT work without a cross-compiler:
GOOS=windows go build ./...

# With a cross-compiler installed (e.g., mingw-w64):
CGO_ENABLED=1 CC=x86_64-w64-mingw32-gcc GOOS=windows GOARCH=amd64 go build ./...
```

## Recommended Build Approaches

### 1. Build on Target Platform (Recommended)
Build directly on Windows for Windows binaries, on Linux for Linux binaries, etc.

### 2. Use CI/CD Pipeline
The GitHub Actions workflow builds native binaries for each platform:
- Windows: `windows-2022` runner
- macOS: `macos-latest` runner
- Linux: `ubuntu-latest` runner

### 3. Use Pre-built Libraries
Download pre-built CFFI libraries from GitHub releases and set:
```bash
export BAML_LIBRARY_PATH=/path/to/baml_cffi.dll
```

## Platform-Specific Library Names
- **Windows**: `baml_cffi-{target}.dll` (no "lib" prefix)
- **macOS**: `libbaml_cffi-{target}.dylib`
- **Linux**: `libbaml_cffi-{target}.so`

Where `{target}` is:
- `x86_64-pc-windows-msvc` (Windows x64)
- `aarch64-pc-windows-msvc` (Windows ARM64)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS Apple Silicon)
- `x86_64-unknown-linux-gnu` (Linux x64)
- `aarch64-unknown-linux-gnu` (Linux ARM64)

## Troubleshooting

### "build constraints exclude all Go files"
This error occurs when trying to cross-compile without CGO. Solutions:
1. Build on the target platform
2. Use a cross-compiler with CGO_ENABLED=1
3. Use pre-built binaries from CI

### "LoadLibrary failed"
Ensure the BAML CFFI library is available:
1. Check `%LOCALAPPDATA%\baml\libs\`
2. Set `BAML_LIBRARY_PATH` environment variable
3. Enable automatic download (default behavior)

### Version Mismatch
The Go package version must match the CFFI library version. Check:
```go
const VERSION = "0.211.2" // in lib_common.go
```