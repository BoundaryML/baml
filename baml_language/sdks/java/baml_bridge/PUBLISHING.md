# Publishing `baml-bridge` to Maven

The BAML Java runtime ships as one Maven Central artifact family,
`com.boundaryml:baml-bridge`:

- a pure-Java **main jar** (`BamlFfi`, the error hierarchy, `BamlStream`, the
  media wrappers, the hand-rolled protobuf codec), plus
- one **per-platform native jar** per target (classifier `natives-<os>-<arch>`)
  carrying the `bridge_java` cdylib.

There is no all-platforms fat jar — at engine sizes that would be an
unreasonable download.

## Native load ladder (self-contained runtime)

At first use, `BamlFfi`'s static initializer resolves and `System.load`s the
`bridge_java` cdylib via `baml_bridge.internal.NativeLibraryLoader`, **first hit
wins**:

1. **System property `baml.bridge.lib`** — dev override, an absolute path to the
   `.so`/`.dylib`/`.dll`.
2. **Environment variable `BAML_JAVA_BRIDGE_LIB`** — dev/test override, same
   meaning. (The Gradle `test` task forwards this env var to the test JVM as
   `-Dbaml.bridge.lib`.)
3. **Classpath resource `/native/{os}-{arch}/{libname}`** — the bundled path
   inside a `natives-*` jar. The loader extracts it to a
   `Files.createTempDirectory("baml-bridge-native")` temp file (marked
   `deleteOnExit`) and loads that.

Tokens:

- `{os}` ∈ `{linux, macos, windows}`, derived from `os.name`.
- `{arch}` ∈ `{x86_64, aarch64}`, derived from `os.arch`
  (`amd64`/`x64` → `x86_64`, `arm64`/`aarch64` → `aarch64`).
- `{libname}` = `System.mapLibraryName("bridge_java")` →
  `libbridge_java.so` / `libbridge_java.dylib` / `bridge_java.dll`.

If none of the three resolves, the loader throws an `IllegalStateException`
listing all three attempted sources (property name, env var name, and the exact
classpath resource path it looked for).

So a consumer that puts `baml-bridge` **and** the matching `natives-<platform>`
jar on the classpath needs no environment setup — step 3 handles it. A dev
working against a locally built cdylib uses step 1 or 2.

## How consumers pick the native jar

- **Maven / Gradle by classifier** (the gRPC/netty `os-maven-plugin`
  convention): depend on
  `com.boundaryml:baml-bridge:<version>:natives-<os>-<arch>` alongside the
  main artifact. This is the supported path today.
- **Automatic platform selection via Gradle Module Metadata variants** (a plain
  `implementation("com.boundaryml:baml-bridge:<version>")` resolving the
  right native jar by OS/arch attributes) is the target-state enhancement; it is
  not wired yet. The published `.module` currently lists only the main jar in
  its variants; the native jar is a classifier artifact.

## Publish command line

Build the release cdylib first (from the repo root):

```
cargo build -p bridge_java --release
# → target/release/libbridge_java.so
```

Then, from `sdks/java/baml_bridge`:

```
gradle publishToMavenLocal \
  -PbamlVersion=0.15.0-nightly.local \
  -PbamlNativePlatform=linux-x86_64 \
  -PbamlNativeLib=/abs/path/to/target/release/libbridge_java.so
```

(Invoke via `mise exec -- gradle` and set
`GRADLE_USER_HOME=<workspace>/target/gradle-home` to share the repo's Gradle/JDK
caches, matching how the sdk_test_java fixtures build.)

This publishes, under
`~/.m2/repository/com/boundaryml/baml-bridge/<version>/`:

- `baml-bridge-<version>.jar` — main jar,
- `baml-bridge-<version>-natives-linux-x86_64.jar` — native jar containing
  `native/linux-x86_64/libbridge_java.so`,
- `baml-bridge-<version>.pom` and `.module`.

### Gradle properties

| Property             | Default        | Meaning                                                    |
| -------------------- | -------------- | ---------------------------------------------------------- |
| `bamlVersion`        | `0.0.0-dev`    | Published Maven version.                                    |
| `bamlNativePlatform` | `linux-x86_64` | Classifier/target: `<os>-<arch>` (`linux`/`macos`/`windows`). |
| `bamlNativeLib`      | *(unset)*      | Absolute path to the built cdylib for that platform.       |

If `bamlNativeLib` is unset, the `nativeJar` task skips gracefully (with a
message) and the publication contains only the main jar + POM — a plain
`publishToMavenLocal` therefore works with no native build.

The plain `jar` task's file name is pinned to `baml-bridge.jar` (no version
suffix) because the sdk_test_java fixtures link `build/libs/baml-bridge.jar` by
exact name. Maven publication rewrites the published file name to
`baml-bridge-<version>.jar` independently, so both hold.

## What CI must inject

The `build-java-sdk` / `publish-maven` jobs slot into
`release-baml-language.yml`. CI is responsible for:

1. **Version** — `-PbamlVersion`. Maven has no dist-tags, so:
   - canary → plain version, e.g. `0.15.0`;
   - nightly → suffixed version, e.g. `0.15.0-nightly.YYYYMMDD.a`, where the
     trailing letter distinguishes repeat cuts for the same night.
2. **Per-platform native builds** — one publish invocation per target in the
   8-target matrix, each passing that platform's
   `-PbamlNativePlatform=<os>-<arch>` and `-PbamlNativeLib=<built cdylib>`. The
   main jar/POM/module are identical across targets (upload once or let Central
   dedupe); the `natives-*` jars differ by classifier.
3. **Central credentials** — enable the `central` Maven repository in
   `build.gradle.kts` (currently a commented placeholder) and provide the
   Sonatype Central Portal token via the `CENTRAL_USERNAME` / `CENTRAL_PASSWORD`
   environment variables. Real values live only in CI secrets; local
   `publishToMavenLocal` never needs them.
   - Central also requires GPG-signed artifacts; wiring the `signing` plugin
     with the CI signing key is part of the `publish-maven` job (out of scope
     for the local flow above).

## Signing key pin

Always pass `-Psigning.gnupg.keyName=<KEYID>` — `useGpgCmd()` otherwise
signs with gpg's *default* secret key, which on a dev machine may be a
personal key that no keyserver knows (Central then rejects every
signature with "could not find a public key by the key fingerprint").
The BoundaryML publishing key is `A5006CD3995646B6`
(fingerprint 3B14D8AC406FCE34249AC7E8A5006CD3995646B6), published to
keyserver.ubuntu.com and keys.openpgp.org; CI gets its own key as a
secret.
