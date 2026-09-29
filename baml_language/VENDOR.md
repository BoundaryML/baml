# Vendoring BAML

BAML provides build-time controls for organizations that compile and distribute the runtime within their own infrastructure. These controls let an integrator supply cryptographic and HTTP transport implementations and disable BAML's telemetry, authentication, and release-service requests.

This guide describes those controls, their scope, and how to verify a downstream build. It applies primarily to the native runtime and tools in the `baml_language` workspace at the revision being vendored; browser/WebAssembly differences are called out where relevant. Published packages and binaries use their release configuration; changing a downstream build does not change those artifacts. The legacy `engine` workspace is outside this guide's scope.

## Integration scope

The following controls are independent:

| Requirement | Control | Scope |
|---|---|---|
| Supply cryptographic implementations | Replace the `baml_crypto_provider` workspace dependency | Non-TLS hashing, signing, secure randomness, and AEAD. The HTTP provider owns TLS. No backend-selection feature flags are required. |
| Supply an HTTP transport | Replace the `baml_http_provider` workspace dependency | Runtime HTTP clients and servers, WebSockets, and requests routed through BAML's outbound registry. Raw sockets and auxiliary tooling have separate paths. |
| Disable telemetry and release services | `no-phone-home` | Rejects the outbound destinations listed below except `RemoteCache`. Application traffic remains enabled. |
| Omit optional tooling and dependency features | Disable `baml-defaults` with `--no-default-features` | Removes the capabilities listed under reduced builds. It does not replace providers or restrict network destinations. |

These controls do not provide a process-wide network sandbox or establish FIPS validation. For a deployment that restricts all network access, combine them with the host's network policy and review the additional network paths below.

## Prepare the source and build configuration

Pin the upstream commit and retain the source tree, `Cargo.lock`, `rust-toolchain.toml`, and Cargo configuration with the downstream import. Keep local patches and provider implementations identifiable alongside that revision. Some build inputs live outside `baml_language`, including release metadata and the CLI's embedded agent skill; preserve the repository layout when importing the source. See [development setup](../README-DEV.md) for build prerequisites.

The [build-script integration audit](BUILD_SCRIPTS.md) inventories BAML-owned generation steps, tools, inputs, outputs, and linker settings for build systems that replace Cargo scripts. It also identifies current integration gaps: protobuf scripts force the bundled compiler, and the bridge schema build writes generated SDK files into the source tree. Reduced runtime features do not yet remove those build-time requirements.

The [`baml-generate-builtins` and `baml-generate-stdlib` commands](BUILD_SCRIPTS.md#standalone-generation-commands) expose the existing Rust generators for external build actions. They write to explicit output directories and share their implementations with Cargo's build scripts. Ordinary runtime builds continue to call those implementations directly.

When importing without Git metadata, set `BAML_GIT_SHA` to the full lowercase commit ID identifying the imported source. BAML uses this value in its artifact fingerprint; development builds require it when a commit cannot be read from Git. Retain downstream patches separately so that the commit ID and patch set together identify the build inputs.

Run the following commands from `baml_language/`. After replacing the provider dependencies described below, build the Python native bridge with outbound restrictions enabled:

```sh
cargo build --locked --release -p bridge_python --features no-phone-home
```

The native runtime obtains its non-TLS crypto implementations from `baml_crypto_provider` in every build. The repository's implementation uses AWS-LC directly (ring on iOS) for hashing, signing, and secure randomness, and RustCrypto for the four AEAD algorithms exposed by `baml.crypto`. The default HTTP provider selects its TLS backend independently. Replacing either dependency requires no backend-selection feature flags on runtime crates or language bridges.

The `no-phone-home` feature is available on `baml_cli`, `baml`, `baml_pack_host`, `bridge_cffi`, `bridge_typescript`, `bridge_python`, `bridge_java`, and `bridge_swift`. For example, build the shared engine library with:

```sh
cargo build --locked --release -p bridge_cffi --features no-phone-home
```

The native bridges enable `bundle-http` by default. It includes the integration with `baml_http_provider`, including a replacement transport. If you disable default features, enable `bundle-http` explicitly to retain that integration. The Rust SDK crate, `baml_bridge`, loads the shared library and has its own downloader configuration; see the additional network paths below.

Build and inspect the intended artifact with the same package selection, target, and features. A workspace-wide build can include tools and other implementations that are absent from the individual artifact being distributed.

### Reduced builds

`baml-defaults` is a single, default-on feature for the following capabilities. Disabling default features on the selected CLI or bridge removes the corresponding dependencies and dependency features from that artifact:

| Capability | Behavior without `baml-defaults` |
|---|---|
| Arbitrary-precision JSON numbers | `serde_json/arbitrary_precision` is disabled. JSON conversion of large `bigint` values may fail or lose precision. This configuration does not guarantee bigint JSON round trips. |
| Python stub generation | `pyo3-stub-gen` and the `stub_gen` binary are omitted. The native Python runtime remains available; provision type stubs separately when packaging. |
| `baml-cli pack` | The command, `libsui`, and `baml_release` are omitted. Telemetry, authentication, and feedback obtain their configuration directory through the small `baml_home` crate. |
| CLI allocator | The CLI uses Rust's default allocator instead of mimalloc. |
| Playground server | `baml-cli playground` and the language server's playground panels are omitted, including Axum's WebSocket and tower-http's filesystem serving dependencies. The stdio language server remains available. |
| Deadlock detection | The language server omits its deadlock watchdog and `parking_lot/deadlock_detection`. |
| Bundled timezone database | Jiff uses the system timezone database. Named timezone operations require that database to be provisioned; systems without one, including typical Windows installations, lose the bundled fallback. |

For example, after replacing the providers, build the CLI and Python bridge with optional capabilities disabled and service requests restricted:

```sh
cargo build --locked --release -p baml_cli -p bridge_python --no-default-features --features no-phone-home,bridge_python/bundle-http
python3 scripts/check_reduced_build.py
python3 scripts/check_no_rustls.py -p baml_cli -p bridge_python --no-default-features --features no-phone-home,bridge_python/bundle-http
```

Retain `bundle-http` to use the replacement HTTP provider in the Python bridge. Cargo features are additive: another dependency or selected workspace member that enables `baml-defaults` can restore these capabilities. Apply `default-features = false` to the relevant dependency edges and verify the final application's graph. `no-phone-home` is a separate choice; disabling `baml-defaults` alone does not disable service requests.

The workspace does not request `clap/cargo`, `tar/xattr`, or `smol_str/borsh`. BAML's `Name` type owns the Borsh encoding of compiler names while retaining `SmolStr` storage. Its encoding remains compatible with existing compiler artifacts and does not require Borsh support in the shared `smol_str` dependency.

### Query support

`baml-cli query` is a stub at this revision. This guide does not identify a release with a working query command. The CLI does not depend on `baml_query`, DataFusion, Arrow, or sqlparser, and neither does the Python native bridge. The separate `baml_query` workspace crate uses DataFusion and remains available for development. Its manifest pins DataFusion to `54.1.0`; the workspace lockfile resolves Arrow to `58.4.0` and sqlparser to `0.62.0`. Their presence in the shared lockfile does not imply inclusion in the CLI or bridge.

### Offline builds

Source vendoring and runtime network restrictions are separate concerns. To prepare registry and Git dependencies for an offline Cargo build, use [`cargo vendor`](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html) in the dependency acquisition environment:

```sh
cargo vendor --locked --versioned-dirs third_party/rust
```

Merge the source configuration printed by this command into the existing `.cargo/config.toml`. Preserve its target and environment settings. Use a separate directory from the repository's `vendor/`, which already contains workspace source. Local path dependencies, including replacement providers, must be retained separately.

Provision the pinned Rust toolchain, native build tools, and any SDK packaging tools before moving to the restricted build environment. Then use `--frozen` instead of `--locked` for the build. `--frozen` prevents Cargo from updating the lockfile or accessing the network; network isolation for build scripts and other tools must be enforced by the build environment. If a provider replacement changes dependency resolution, update and review the lockfile during acquisition.

## Cryptographic provider

[`baml_crypto`](crates/baml_crypto/src/lib.rs) routes non-TLS cryptographic operations to [`baml_crypto_provider`](crates/baml_crypto_provider/src/lib.rs). The public contracts in [`baml_crypto_types`](crates/baml_crypto_types/src/lib.rs) contain no backend or TLS dependencies. Replacing the provider removes the default implementation from the selected runtime's dependency graph.

| Operation | Provider capability |
|---|---|
| Google Vertex service-account authentication | RSA PKCS#1 v1.5 with SHA-256 (RS256) |
| AWS Bedrock request signing | SHA-256 and HMAC-SHA256 for SigV4 |
| AWS CLI SSO token-cache lookup | SHA-1 for the AWS CLI's cache filename convention |
| Release download checksum verification | SHA-256 |
| `baml.crypto.Sha256` | Incremental SHA-256 |
| `baml.random.SystemRandom` | Secure random bytes |
| `baml.crypto.Aes128GcmSiv`, `Aes256GcmSiv`, `ChaCha20Poly1305`, and `XChaCha20Poly1305` | AEAD cipher creation, encryption, and authenticated decryption |

### Supply a provider

Replace the `baml_crypto_provider` entry in `[workspace.dependencies]` in [`Cargo.toml`](Cargo.toml):

```toml
baml_crypto_provider = { path = "../my_crypto_provider", package = "my_crypto_provider" }
```

The replacement depends on `baml_crypto_types` from the same BAML revision and exposes:

```rust
pub fn provider() -> std::sync::Arc<dyn baml_crypto_types::CryptoProvider> {
    std::sync::Arc::new(MyCryptoProvider::new())
}
```

`MyCryptoProvider` is the integrator's implementation. BAML initializes and retains the returned provider on first use. No process-wide rustls provider is involved in these operations. For Bazel, Buck, or another build system, replace the corresponding dependency target and preserve this interface.

| Method | Contract |
|---|---|
| `sha1` | Return exactly 20 bytes. Used only to locate an AWS CLI SSO token-cache file, not for signing or integrity verification. A provider may reject this capability if SSO cache lookup is not needed. |
| `sha256` | Return an incremental `Sha256Context`. `update` accepts another chunk; `finish` returns exactly 32 bytes and resets the context to an empty message. After successful creation, updates and finalization are infallible. |
| `hmac_sha256` | Compute HMAC-SHA256 for the supplied key and message, including keys longer than the SHA-256 block size. Return exactly 32 bytes. |
| `sign_rs256` | Accept an unencrypted PKCS#8 or PKCS#1 PEM RSA private key and return a PKCS#1 v1.5 SHA-256 signature. |
| `fill_random` | Fill the entire buffer with cryptographically secure random bytes or return an error. Callers discard the buffer on error. |
| `aead` | Return an initialized cipher for the exact requested algorithm and key, or report that the algorithm is unsupported. |

The trait's default implementations reject unsupported operations. A provider can implement only the capabilities required by its deployment. BAML propagates failures and does not select another backend. `CryptoError` distinguishes unsupported operations, invalid keys, and operational failures. Providers own key handling and cleanup; error messages must never contain keys, plaintext, or other secret inputs.

### AEAD capabilities and errors

Providers may implement any subset of the four AEAD algorithms. `aead` returns an initialized, thread-safe `AeadCipher` or `AeadError::Unsupported(algorithm)`. BAML does not substitute another algorithm or use a built-in cipher when a provider rejects one.

`AeadCipher::encrypt` receives a caller-supplied nonce, plaintext, and additional authenticated data and returns ciphertext with the 16-byte authentication tag appended. `decrypt` receives the same nonce and authenticated data and must authenticate the complete ciphertext before returning any plaintext.

BAML validates key and nonce lengths and algorithm size limits before invoking the provider. The shared interface exposes those validation helpers for provider implementations as well. Errors retain their language-level meaning:

| Provider result | BAML behavior |
|---|---|
| `Unsupported` | `baml.errors.Unsupported`; no fallback implementation |
| `InvalidArgument` | `baml.errors.InvalidArgument` |
| `AuthenticationFailed` or `CiphertextTooShort` from decryption | `baml.crypto.DecryptionFailure` |
| `Failed` | `baml.errors.Io` |

Authentication failures must not distinguish an incorrect key, nonce, authenticated data, or modified ciphertext. The default provider's AES-GCM-SIV and ChaCha dependencies are contained in that crate. AEAD also uses this interface on WebAssembly; browser TLS and entropy and the portable SHA-256 implementation remain separate.

### Cryptographic policy and validated modules

BAML is not itself a FIPS-validated cryptographic module. Selecting a validated module is one input to a deployment review; the module version, approved operating environment, algorithms, and TLS configuration also matter. Implement the BAML crypto contract using the organization's selected module and configure the HTTP provider's TLS policy separately. Installing a rustls provider in the host does not configure BAML's non-TLS crypto.

A replacement crypto provider may reject algorithms outside its policy, including all AEAD algorithms. A replacement HTTP provider is responsible for its own TLS implementation, trust configuration, and validation requirements. If retaining the default rustls transport, follow the [rustls FIPS guidance](https://docs.rs/rustls/latest/rustls/manual/_06_fips/index.html) and verify the actual TLS configurations; changing the non-TLS crypto provider does not change that transport's backend. For an AWS-LC integration, consult its [build requirements](https://aws.github.io/aws-lc-rs/requirements/) for the selected version and target.

Internal fingerprints, including compile-cache keys, program identities, and generated-file signatures, still use `sha2` outside this interface. Seeded generators such as `baml.random.ChaCha20` are separate from `baml.random.SystemRandom`. These dependencies contain no rustls code, but a deployment with broader cryptographic restrictions must review them as well.

## HTTP and WebSocket transport

The native runtime uses [`baml_http_provider`](crates/baml_http_provider/src/lib.rs) for LLM requests and associated HTTP credential acquisition, `baml.http` clients and SSE streams, `baml.ws` clients, and `baml.http.Server`, including TLS and WebSocket upgrades. Requests in BAML's outbound registry use the same provider. The default implementation uses reqwest, hyper, rustls, and tungstenite. It selects AWS-LC for TLS on native targets and ring on iOS, while retaining a rustls provider installed by the host before its first TLS operation. This selection is independent of the non-TLS crypto provider.

To use an organizational transport, replace the workspace dependency:

```toml
baml_http_provider = { path = "../my_http_provider", package = "my_http_provider" }
```

The replacement crate implements [`baml_http_types::HttpProvider`](crates/baml_http_types/src/lib.rs) and exposes:

```rust
pub fn provider() -> std::sync::Arc<dyn baml_http_types::HttpProvider> {
    std::sync::Arc::new(MyTransport::new())
}
```

`MyTransport` is the integrator's implementation. Depend on `baml_http_types` from the same BAML revision. BAML initializes and retains the returned provider on first use.

| Interface | Required behavior |
|---|---|
| `send` | Send an HTTP request and return a streaming response. Honor the whole-exchange timeout, including body consumption, the connection timeout, redirect setting, and environment-proxy setting. |
| `connect_websocket` | Provide text, binary, and close messages; handle protocol pings within the transport. |
| `bind` and `Listener::serve` | Bind and serve with the requested HTTP versions, TLS settings, connection limits, and deadlines. Complete accepted WebSocket handshakes. Dropping the serving future ends its connections. |
| `check_tls` | Validate the PEM certificate and private key when a BAML TLS configuration is created. |

The trait definitions specify the full contract. BAML retains SSE parsing, request-body limits, BAML handler execution, and WebSocket resource state. Transport `Timeout` errors map to `baml.errors.Timeout`; other transport errors map to `baml.errors.Io`.

A custom provider can enforce destination policy, proxy routing, certificate trust, and connection auditing for these interfaces. Apply destination checks to redirects as well as initial URLs. This interface does not mediate raw socket operations or subprocess traffic.

## Telemetry and service requests

[`baml_http::outbound::Destination`](crates/baml_http/src/outbound.rs) records BAML's telemetry, account, release, and remote-cache request categories. The table below is checked against that registry by a unit test. It lists default service locations, not every possible host: configuration overrides, redirects, and service-provided download or upload URLs can change the actual destination.

<!-- outbound:begin -->
| Destination | Default URL | When | Purpose |
|---|---|---|---|
| `AnonymousTelemetry` | `https://us.i.posthog.com` | Each CLI run, unless telemetry is turned off | Anonymous CLI usage events |
| `Feedback` | `https://us.i.posthog.com` | `baml feedback` | The feedback the user submits |
| `CloudTelemetry` | configured by the user | Only when cloud recording is configured | Uploads recordings and sends a heartbeat |
| `Auth` | `https://api.workos.com` | `baml auth login` | Device-code login |
| `ReleaseManifest` | `https://pkg.boundaryml.com/manifest/v1` | Self-update and `baml` toolchain installs | Finds the available releases |
| `ReleaseDownload` | `https://github.com` | Self-update and `baml` toolchain installs | Downloads release archives and checksums |
| `RemoteCache` | configured by the user | Only when `BAML_CACHE_REMOTE` is set | Shares compiled bytecode through the user's own server |
<!-- outbound:end -->

With `no-phone-home`, registry requests for every category except `RemoteCache` return an error before the transport is called. The registry's built-in service URLs are also excluded at compile time. Changing a URL does not bypass the category check. Leave `BAML_CACHE_REMOTE` unset to disable the remote cache.

This feature leaves application-requested network operations available, including LLM requests and credential acquisition. It also disables cloud recording uploads even if a recording destination has been configured. Local recording and local files are separate from these network controls.

Native language bridges do not run the CLI's usage telemetry, login, feedback, or self-update paths. Cloud recording can initiate uploads when configured; source-compilation paths may also use a configured remote cache.

Without `no-phone-home`, CLI usage telemetry honors `BAML_TELEMETRY_DISABLED=1`, `DO_NOT_TRACK=1`, and `enabled = false` in `telemetry.toml`. These settings apply to usage telemetry, not every category in the registry. See [Telemetry](../TELEMETRY.md) for the payload and user controls. Usage telemetry is delivered by a detached child process with a two-second timeout per request. Failed deliveries may be retried on later runs; queued files expire after 24 hours. Feedback can also be retained locally for later delivery, and login may associate submitted feedback with an authenticated identity. The CLI's [telemetry queue](crates/baml_cli/src/telemetry/queue.rs) and [feedback implementation](crates/baml_cli/src/feedback_command.rs) define those behaviors.

### Additional network paths

Review these separately when the requirement applies to the entire application:

| Component | Behavior and integration requirement |
|---|---|
| Raw TCP and UDP | `baml.net` operations use Tokio sockets in `sys_native` directly. They are outside `HttpProvider` and remain available with `no-phone-home`. |
| Subprocesses and host callbacks | Programs and credential helpers can perform their own I/O. BAML's HTTP provider cannot enforce policy inside those processes or callbacks. |
| CLI playground/language server | With `baml-defaults`, the local playground server uses Axum/Tokio directly and retains its WebSocket dependencies when the HTTP provider is replaced. Disable `baml-defaults` to omit the playground server while retaining the stdio language server. |
| Rust SDK loader | `baml_bridge` can download a missing engine library from GitHub using its own `ureq` client. Build `baml_bridge` with `default-features = false` and set `BAML_LIBRARY_PATH` to the built library to omit its downloader and TLS dependencies. With downloading compiled in, `BAML_LIBRARY_DISABLE_DOWNLOAD=true` disables acquisition at runtime but leaves those dependencies present. Its `aws-crypto`, `ring-crypto`, and `external-crypto` features enable downloading and configure only that loader, independently of the loaded engine provider. |
| Browser/WebAssembly | Uses browser networking and entropy, and `sha2` for `baml.crypto.Sha256`; native TLS and HTTP substitutions do not apply. AEAD does use the replaceable crypto provider. |
| Build and installation tools | Cargo, SDK package managers, and packaging scripts have their own acquisition behavior. Provision their inputs and enforce build-network policy separately. |

The raw-socket implementation is in [`sys_native`](crates/sys_native/src/io_impls.rs); the auxiliary server is in [`playground_server.rs`](crates/baml_lsp_server/src/playground_server.rs); and the Rust download path is in the [SDK loader](sdks/rust/bridge_rust/src/loader/mod.rs).

## Build without rustls

Replace both `baml_crypto_provider` and `baml_http_provider` with implementations that do not depend on rustls. No extra runtime feature flag is required. Replacing only the crypto provider leaves the default HTTP transport's TLS stack in place; replacing only HTTP leaves the selected non-TLS crypto implementation in place.

```toml
[workspace.dependencies]
baml_crypto_provider = { path = "../my_crypto_provider", package = "my_crypto_provider" }
baml_http_provider = { path = "../my_http_provider", package = "my_http_provider" }
```

After resolving and reviewing the updated lockfile in the dependency acquisition environment, build the intended artifacts and verify their normal and build dependencies:

```sh
cargo build --locked --release -p baml_cli -p bridge_python --features no-phone-home
python3 scripts/check_no_rustls.py -p baml_cli -p bridge_python --features no-phone-home
```

The check fails for any package whose name starts with `rustls` or ends with `-rustls`, including `rustls-pki-types`, `rustls-webpki`, `hyper-rustls`, and `tokio-rustls`. It includes build dependencies and proc macros. Pass the same `--target` and feature arguments used for the artifact; `--target all` additionally checks dependency paths across targets. The remaining playground WebSocket dependencies do not require rustls. To omit them as well, disable `baml-defaults` as described under reduced builds.

For Rust SDK consumers, also disable the loader's default features and provision the engine library:

```toml
baml_bridge = { version = "<matching-release>", default-features = false }
```

Set `BAML_LIBRARY_PATH` to the matching `bridge_cffi` library. Without the loader's `download` feature, missing libraries produce an error and no download is attempted. Apply `default-features = false` to the dependency in the generated SDK as well, retaining that setting in the downstream generation or patch process. An additional direct dependency in the application cannot disable defaults enabled by the generated SDK. Verify the consumer's complete graph because Cargo combines feature selections across dependents.

This guarantee applies to the selected artifacts and their build dependencies. The shared workspace lockfile can still contain rustls for the repository's default provider crates or other packages. Building every workspace member explicitly also builds those default implementations. Cargo and other acquisition tools run outside the artifact graph and may have their own TLS implementations.

## Verify the downstream artifact

Use the actual target and feature selection throughout verification. The commands below use the host target and the Python bridge as an example.

First, capture the resolved production dependency and feature graph:

```sh
cargo tree --locked -p bridge_python --features no-phone-home -e normal,build,features
```

Review which paths enable `aws-lc-rs`, `ring`, `aes-gcm-siv`, `chacha20poly1305`, and other cryptographic libraries. Their runtime inclusion should follow the selected provider's dependencies. A replacement that supplies no AEAD implementations should not retain the default AEAD libraries through the VM. Verify the selected backend against the intended module and version. After an HTTP-provider replacement, likewise inspect paths to reqwest, hyper, and tungstenite. Account for dependencies introduced by the replacement and the auxiliary tooling described above. A lockfile lists more than a single artifact uses, and a Cargo graph does not prove which code is present in the final binary.

Run the registry checks in both configurations:

```sh
cargo test --locked -p baml_http --lib
cargo test --locked -p baml_http --lib --features no-phone-home
```

The first checks the documentation table and default destination policy. The second checks that restricted destinations are refused and have no built-in URLs. These tests cover the registry; they do not prove that all code in an application uses it.

Run the crypto tests against the replacement provider:

```sh
cargo test --locked -p baml_crypto --lib
```

The tests include SHA-1, SHA-256, HMAC-SHA256, RSA signing, and secure randomness checks. They fail if a tested capability is unsupported. Test the replacement's supported AEAD algorithms against published vectors and exercise rejection of unsupported algorithms through BAML.

The repository's default provider has its own AEAD vector and error tests. The substitution check imports the source into a temporary directory and replaces both provider dependencies. It checks all-target native dependency graphs, builds default and reduced configurations of the CLI and Python bridge while inspecting Cargo's build-artifact records, tests a ring-based crypto implementation without rustls, and exercises AWS SSO cache lookup, transport substitution, and unsupported operations through BAML. It also imports the Python extension and checks the reduced CLI's available commands and stdio language-server handshake. A second replacement rejects all cryptographic operations to verify that no fallback backend is introduced. The script requires Python 3.10 or newer for the Python bridge build; set `PYO3_PYTHON` if a different interpreter is selected by default:

```sh
cargo test --locked -p baml_crypto_provider -p baml_crypto_types -p baml_crypto -p baml_http_provider
./scripts/check-crypto-backends.sh
```

Finally, exercise the packaged artifact with the intended SDK, provider, and network policy. Verify the operations the application uses, including credential acquisition, streaming, timeouts, TLS failures, and server/WebSocket behavior where applicable. Check that disabled service requests do not reach the transport and that a configured remote cache behaves as intended. Observe raw sockets, helper processes, and loaders as part of the same deployment test. Binary string searches can supplement this review but cannot establish the absence of network behavior or an unapproved cryptographic path.

## Maintain the vendored copy

Treat the provider interfaces as source integration points tied to the vendored revision. On an upstream update, review changes to the provider traits, feature propagation, outbound registry, SDK loaders, and dependency lockfile; rebuild the affected artifacts and repeat the integration checks. Keep the SDK, generated bindings, and runtime versions aligned for the release being deployed.

Toolchain releases are tagged at their source revision. The release packaging workflow includes the resolved `Cargo.lock` alongside the binaries in each toolchain archive. Use the tag's lockfile when rebuilding the source as checked in, and retain the archive's copy when auditing the published binaries: release version stamping can change local package versions before the build.

Retain the upstream revision, downstream patches, toolchain and target, effective features, provider versions, dependency inventory, and validation results with each internal release. Include the repository's [license](../LICENSE) and applicable third-party license and notice files in the import and distribution review.

For integration issues, use the [BAML issue tracker](https://github.com/BoundaryML/baml/issues) with the upstream revision, target, feature selection, provider details, and a minimal reproduction that excludes credentials and proprietary source. This guide describes build behavior; it does not establish a long-term support or security-backport schedule for a pinned downstream revision.
