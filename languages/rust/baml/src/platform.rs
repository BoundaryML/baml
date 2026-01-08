//! Platform support validation with compile-time errors for unsupported
//! targets.
//!
//! BAML uses dynamic library loading (`dlopen`/`LoadLibrary`) to load the CFFI
//! runtime. This module provides clear compile-time errors for platforms that
//! cannot support this.
//!
//! # Supported Platforms
//!
//! - macOS (x86_64, aarch64)
//! - Linux (x86_64, aarch64) with glibc or musl
//! - Windows (x86_64, aarch64) with MSVC
//!
//! # Unsupported Platforms
//!
//! Unsupported platforms will produce compile-time errors with detailed
//! explanations and suggested workarounds.

// ============================================================================
// WebAssembly
// ============================================================================

#[cfg(target_family = "wasm")]
compile_error!(
    r#"

================================================================================
BAML does not support WebAssembly targets (wasm32/wasm64).

BAML requires dynamic library loading (dlopen/LoadLibrary) to load the native
CFFI runtime. WebAssembly does not support loading native code dynamically.

Workaround:
  Run BAML on a backend server and call it via HTTP.

For more information: https://docs.boundaryml.com
================================================================================

"#
);

// ============================================================================
// iOS/tvOS/watchOS/visionOS
// ============================================================================

#[cfg(any(
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "visionos"
))]
compile_error!(
    r#"

================================================================================
BAML does not currently provide prebuilt binaries for iOS/tvOS/watchOS/visionOS.

Technical details:
  - dlopen() exists on iOS but Apple's App Store guidelines prohibit downloading
    and loading executable code at runtime
  - The CFFI library would need to be bundled with your app at build time
  - We would need to provide it as a static library or XCFramework

If you need iOS support, please open an issue at:
  https://github.com/BoundaryML/baml/issues

Workaround:
  Run BAML on a backend server and call it via HTTP from your mobile app.

For more information: https://docs.boundaryml.com
================================================================================

"#
);

// ============================================================================
// Android
// ============================================================================

#[cfg(target_os = "android")]
compile_error!(
    r#"

================================================================================
BAML does not currently provide prebuilt binaries for Android.

Technical details:
  - dlopen() works fine on Android (it's how JNI/NDK work)
  - We simply don't compile the CFFI library for Android targets yet

If you need Android support, please open an issue at:
  https://github.com/BoundaryML/baml/issues

Workaround:
  Run BAML on a backend server and call it via HTTP from your mobile app.

For more information: https://docs.boundaryml.com
================================================================================

"#
);

// ============================================================================
// BSD family
// ============================================================================

#[cfg(any(
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
compile_error!(
    r#"

================================================================================
BAML does not provide prebuilt binaries for BSD operating systems.

If you need BSD support, please open an issue at:
  https://github.com/BoundaryML/baml/issues

For more information: https://docs.boundaryml.com
================================================================================

"#
);

// ============================================================================
// Solaris/illumos
// ============================================================================

#[cfg(any(target_os = "solaris", target_os = "illumos"))]
compile_error!(
    r#"

================================================================================
BAML does not provide prebuilt binaries for Solaris or illumos.

For more information: https://docs.boundaryml.com
================================================================================

"#
);

// ============================================================================
// 32-bit architectures
// ============================================================================

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
compile_error!(
    r#"

================================================================================
BAML only supports 64-bit architectures (x86_64, aarch64).

32-bit architectures like i686, armv7, etc. are not supported.

For more information: https://docs.boundaryml.com
================================================================================

"#
);
