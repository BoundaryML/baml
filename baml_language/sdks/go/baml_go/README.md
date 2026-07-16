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

Build the runtime and point generated Go tests at it:

```bash
cargo build -p bridge_cffi
export BAML_RUNTIME_PATH="$PWD/target/debug/libbridge_cffi.dylib"
```

`BAML_RUNTIME_PATH` is the highest-priority local override and never performs
a download.

## Verified cache proof

The artifact resolver selects the current platform from the shared `cffi`
artifact map in the immutable BAML language release manifest and fetches the
same native library published for every dynamically loaded SDK into
`~/.baml/runtimes/<version>/abi-v1/<target>/` before loading it. Until the
release is published, local tests can override discovery with:

```text
BAML_RUNTIME_URL
BAML_RUNTIME_VERSION
BAML_RUNTIME_TARGET
BAML_RUNTIME_FILENAME
BAML_RUNTIME_SHA256
BAML_RUNTIME_ARCHIVE_SHA256
BAML_RUNTIME_FORMAT
BAML_CACHE_DIR
BAML_HOME
BAML_DISABLE_DOWNLOAD
BAML_RUNTIME_MANIFEST_BASE_URL
```

Applications may instead call `ConfigureRuntime` before their first generated
BAML function. `RuntimeArtifact` requires the same version, target, filename,
URL, and SHA-256 identity.

Resolution order is:

1. `RuntimeConfig.LibraryPath` or `BAML_RUNTIME_PATH`;
2. the cached exact-version release manifest;
3. a verified exact-version artifact already in the cache;
4. manifest/artifact download, SHA-256 verification, and atomic cache
   installation.

Explicit `RuntimeArtifact` overrides may additionally use `Format: "gzip"`
with `ArchiveSHA256`; release-manifest artifacts use the shared raw CFFI files.

Set `BAML_DISABLE_DOWNLOAD=true` to prohibit every network request made by this
package, including release-manifest requests. The environment setting is a
one-way safety control: programmatic configuration cannot turn downloads back
on. Resolution then requires an explicit path or both an already-cached
manifest and its verified runtime artifact. A missing, corrupt,
ABI-incompatible, or version-incompatible runtime fails before any BAML program
is initialized.

## Current scope

The manually loaded runtime is implemented for macOS, Linux, and Windows with
cgo. Release CI builds the full canonical target matrix. A cgo-free/WASM
runtime remains separate future work.
