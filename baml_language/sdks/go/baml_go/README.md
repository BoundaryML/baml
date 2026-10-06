# BAML Go runtime

This is the source of the read-only
[`github.com/boundaryml/baml-go`](https://github.com/BoundaryML/baml-go)
module mirror. Changes land in the BAML monorepo and release automation copies
this directory to the mirror with an immutable `v<language-version>` tag.

Generated Go SDKs use this package for wire encoding and native-runtime calls.
The Go binary does not link `bridge_cffi` at process startup. On supported
desktop platforms, the package resolves a local runtime artifact and loads the
versioned `baml_get_api_v1` function table with the platform dynamic loader.

Before initializing a BAML program, the package registers itself as the Go
bridge. The shared Rust runtime validates the Go SDK's exact release version
and retains the bridge identity for consistent diagnostics and telemetry.

## Local development

Build the runtime and point generated Go tests at the platform library.

macOS or Linux:

```bash
cargo build -p bridge_cffi
case "$(uname -s)" in
  Darwin) export BAML_BRIDGE_PATH="$PWD/target/debug/libbridge_cffi.dylib" ;;
  Linux)  export BAML_BRIDGE_PATH="$PWD/target/debug/libbridge_cffi.so" ;;
esac
```

Windows PowerShell:

```powershell
cargo build -p bridge_cffi
$env:BAML_BRIDGE_PATH = "$PWD\target\debug\bridge_cffi.dll"
```

`BAML_BRIDGE_PATH` must be an absolute path. It is the highest-priority local
override: it never performs a download, and an invalid path is an error with
no fallback. A library built from a different toolchain version than this
module fails the version check; set `DEV_BAML_BRIDGE_SKIP_VERSION_CHECK=1`
(dev-only, valid only together with `BAML_BRIDGE_PATH`) to skip the
toolchain-version match when testing a locally built library. The ABI table
check is never skipped.

## Verified cache proof

The artifact resolver selects the current platform from the shared `cffi`
artifact map in the immutable BAML language release manifest and fetches the
same native library published for every dynamically loaded SDK into
`$BAML_HOME/bridges/<version>/<target>/` before loading it. `BAML_HOME`
defaults to `~/.baml` (`%USERPROFILE%\.baml` on Windows). The module always
uses the version compiled into it; to test another build, use
`BAML_BRIDGE_PATH`.

Environment variables:

```text
BAML_BRIDGE_PATH
BAML_BRIDGE_DISABLE_DOWNLOAD
BAML_MANIFEST_BASE_URL
BAML_HOME
DEV_BAML_BRIDGE_SKIP_VERSION_CHECK
```

Applications may instead call `ConfigureRuntime` before their first generated
BAML function. Programmatic configuration takes precedence over the
environment. A `RuntimeArtifact` override (explicit version, target, filename,
URL, and SHA-256 identity, optionally gzip with `ArchiveSHA256`) can only be
set this way; `RuntimeConfig.CacheDir` likewise replaces the derived
`$BAML_HOME/bridges` root.

Resolution order is:

1. `RuntimeConfig.LibraryPath` or `BAML_BRIDGE_PATH`;
2. the cached exact-version release manifest;
3. a verified exact-version artifact already in the cache;
4. manifest/artifact download, SHA-256 verification, and atomic cache
   installation.

Release-manifest artifacts use the shared raw CFFI files.

Set `BAML_BRIDGE_DISABLE_DOWNLOAD=true` to prohibit every network request made
by this package, including release-manifest requests. The environment setting
is a one-way safety control: programmatic configuration cannot turn downloads
back on. Resolution then requires an explicit path or both an already-cached
manifest and its verified runtime artifact. A missing, corrupt,
ABI-incompatible, or version-incompatible runtime fails before any BAML program
is initialized.

## Current scope

The manually loaded runtime is implemented for macOS, Linux, and Windows with
cgo. Release CI builds the full canonical target matrix. A cgo-free/WASM
runtime remains separate future work.
