# baml-sys

Low-level FFI bindings to the BAML runtime library with runtime dynamic loading.

## Overview

This crate provides the FFI bindings to `libbaml_cffi`, the C FFI interface to the BAML runtime.
Unlike traditional `-sys` crates that link at compile time, `baml-sys` loads the library
dynamically at runtime using `libloading`.

## Library Resolution

The library is searched in the following order:

1. **Explicit path** - Set via `baml_sys::set_library_path()` before first use
2. **Environment variable** - `BAML_LIBRARY_PATH`
3. **User cache** - `~/.cache/baml/libs/{VERSION}/` (Linux), `~/Library/Caches/baml/libs/{VERSION}/` (macOS), `%LOCALAPPDATA%\baml\libs\{VERSION}\` (Windows)
4. **Auto-download** - From GitHub releases (if `download` feature enabled)
5. **System paths** - `/usr/local/lib/`, etc.

## Usage

### Runtime Loading (Default)

```rust
use baml_sys::version;

fn main() -> Result<(), baml_sys::BamlSysError> {
    // Library is loaded on first access
    let v = version()?;
    println!("BAML version: {v}");
    Ok(())
}
```

### Build-time Hook

In your `build.rs`:

```rust
fn main() {
    // Ensure library is available before build completes
    let lib_path = baml_sys::ensure_library()
        .expect("Failed to find/download BAML library");
    println!("cargo:rerun-if-changed={}", lib_path.display());
}
```

## Environment Variables

- `BAML_LIBRARY_PATH` - Explicit path to the library file
- `BAML_CACHE_DIR` - Override the cache directory location
- `BAML_LIBRARY_DISABLE_DOWNLOAD` - Set to "true" to disable auto-download

## Features

- `download` (default) - Enable automatic download from GitHub releases
- `no-download` - Disable download functionality

## Platform Support

- macOS (x86_64, aarch64)
- Linux (x86_64, aarch64, glibc)
- Windows (x86_64, MSVC)
