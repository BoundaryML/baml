# BAML Gradle plugin (`com.boundaryml.baml`)

Generate the typed BAML Java SDK from your `baml_src/` **at build time** — the
protobuf-gradle-plugin model (pattern C in the Java packaging plan). A cacheable
`generateBaml` task runs the installed `baml` CLI, writes the SDK into
`build/generated/sources/baml/java/main`, and wires that tree into your main
source set. Generation is incremental: Gradle skips it as `UP-TO-DATE` when
nothing under `baml_src/` (or `baml.toml`, or the CLI version) changed, and the
generated code is never checked into your repo.

## Apply

The plugin is the entire setup — `build.gradle.kts`:

```kotlin
plugins {
    id("com.boundaryml.baml") version "0.15.0-nightly.1"
}

repositories {
    mavenCentral()
}
```

That's it. Applying the plugin:

- applies the `java` plugin for you (so a project with a `baml.toml` and a
  `baml_src/` directory needs nothing more);
- injects `implementation("com.boundaryml:baml-bridge:<pluginVersion>")` — the
  BAML runtime the generated SDK compiles against, **at the plugin's own
  version** (the plugin and `baml-bridge` publish from one pipeline at one
  version, so they always match);
- injects `runtimeOnly("com.boundaryml:baml-bridge:<pluginVersion>:natives-<platform>")`
  for the **build machine's platform**, auto-detected from `os.name`/`os.arch`
  (the classifier the runtime's native-library loader will look for).

Build as usual:

```sh
gradle build          # runs generateBaml first, then compileJava
```

If you resolve the plugin from Maven Central rather than the Gradle Plugin
Portal (nightlies publish only to Central), add both to `settings.gradle.kts`:

```kotlin
pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
    }
}
```

## Configuration

The optional `baml { … }` block:

| Option               | Type                 | Default                | Meaning                                                                                                              |
| -------------------- | -------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `srcDir`             | `DirectoryProperty`  | the project directory  | Directory containing `baml.toml` and `baml_src/`; passed to the CLI as `--project`.                                   |
| `bamlExecutable`     | `Property<String>`   | `"baml"`               | The CLI to run — a bare name resolved on `PATH`, or an absolute path to a specific binary.                            |
| `outputType`         | `Property<String>`   | `"java"`               | Informational only. The real generator config lives in `baml.toml` `[generator.<name>]`.                             |
| `nativePlatforms`    | `ListProperty<String>` | *(empty → detect host)* | Which native-jar classifiers to depend on. Empty auto-detects the build machine; an explicit list **replaces** detection; `["all"]` adds every known platform. |
| `manageDependencies` | `Property<Boolean>`  | `true`                 | Whether the plugin auto-injects the `baml-bridge` runtime + native jar. Set `false` to own those dependencies yourself. |

```kotlin
baml {
    srcDir.set(layout.projectDirectory)
    bamlExecutable.set("baml")

    // Cross-platform artifact: depend on every platform's native jar. Safe —
    // the runtime loader picks the right one by os/arch; the extras are inert.
    nativePlatforms.set(listOf("all"))
    // Or name them explicitly: listOf("linux-x86_64", "macos-aarch64").
}
```

Known `nativePlatforms` classifiers: `linux-x86_64`, `linux-aarch64`,
`macos-x86_64`, `macos-aarch64`, `windows-x86_64`, `windows-aarch64`. (The
experimental musl classifiers are never auto-detected and are not part of
`"all"` — request `linux-<arch>-musl` explicitly on Alpine.)

### Opting out of dependency management

If you already declare a `com.boundaryml:baml-bridge` dependency, the plugin
detects it and injects **nothing** (it defers to your declaration, logging at
`info`). For full manual control — a vendored runtime, a custom coordinate, an
exotic platform — set `manageDependencies.set(false)` and add the dependencies
yourself:

```kotlin
baml {
    manageDependencies.set(false)
}
dependencies {
    implementation("com.boundaryml:baml-bridge:0.15.0-nightly.1")
    runtimeOnly("com.boundaryml:baml-bridge:0.15.0-nightly.1:natives-linux-x86_64")
}
```

## What the task does

`generateBaml` (group `baml`, `@CacheableTask`):

- **Inputs** — `baml.toml`, every file under `srcDir/baml_src/` (path-sensitive,
  so renames/deletes are tracked), and the resolved CLI version (`baml
  --version`).
- **Output** — `build/generated/sources/baml/java/main`. The generated files
  declare `package baml_sdk.*`, so the emitter writes them under a `baml_sdk/`
  subdirectory (`baml bridge generate --project <srcDir> -o <outputDir>/baml_sdk`) and that
  `<outputDir>` is registered as the Java source root. The output directory is
  cleaned before every run so a renamed or deleted BAML class never leaves a
  stale `.java` behind.
- **Wiring** — the generated `.java` are added to `sourceSets.main.java`; the
  `baml_sdk/**/*.b64` bytecode is packaged as a resource (it must ride on the
  runtime classpath at `/baml_sdk/inlinedbaml.b64`); and `compileJava` /
  `processResources` depend on `generateBaml`.

### Incremental / UP-TO-DATE

Because the inputs and output are declared, Gradle runs `generateBaml` only when
a `.baml` source, `baml.toml`, or the CLI version changed. Otherwise it is
`UP-TO-DATE` and the task body — including the CLI invocation — is skipped
entirely. Running the built program never triggers generation.

### Missing CLI

If the `baml` executable cannot be found or run, the **task** fails at execution
(configuration always succeeds) with an install hint:

```
curl -fsSL https://pkg.boundaryml.com/install.sh | sh
```

(see `scripts/install.sh`). Set `baml { bamlExecutable.set("/path/to/baml") }` or
add `baml` to your `PATH` to fix it.

## Publishing

The plugin publishes to **two** places, from the release pipeline:

- **Gradle Plugin Portal** (`com.gradle.plugin-publish` → `publishPlugins`) —
  the canonical home for `plugins { id("com.boundaryml.baml") version "X" }`.
  Stable channels only (canary/stable); the `publishPlugins` task reads the
  Portal API key/secret from the `GRADLE_PUBLISH_KEY` / `GRADLE_PUBLISH_SECRET`
  environment variables natively.
- **Maven Central** — Portal-required metadata (`displayName`, `description`,
  `website`/`vcsUrl`, tags) plus the marker POM
  (`com.boundaryml.baml:com.boundaryml.baml.gradle.plugin`) ride the same signed
  Central bundle as `baml-bridge`, so **nightlies** (which the Portal doesn't
  take) reach Gradle consumers via `mavenCentral()`.

Coordinates `com.boundaryml:baml-gradle-plugin`; the marker publication is
generated automatically by `java-gradle-plugin`. Local rehearsals:

```sh
# Publish to ~/.m2 (no credentials needed).
gradle publishToMavenLocal -PbamlVersion=0.15.0-nightly.1

# Validate Portal metadata without publishing (needs Portal credentials to
# reach the auth step; publishes nothing).
gradle publishPlugins --validate-only -PbamlVersion=0.15.0-nightly.1

# Stage the signed Central layout (into build/staging-deploy, or a shared tree
# via -PbamlStagingDir so the plugin + marker join baml-bridge's bundle).
gradle publishAllPublicationsToStagingRepository -PbamlVersion=0.15.0-nightly.1
```

| Property          | Default                  | Meaning                                                                             |
| ----------------- | ------------------------ | ----------------------------------------------------------------------------------- |
| `bamlVersion`     | `0.0.0-dev`              | Published version (canary = plain, nightly = suffixed).                             |
| `bamlStagingDir`  | `build/staging-deploy`   | File-repo destination for the staging publications (point at a shared Central tree). |
| `bamlSign`        | *(unset)*                | When present, GPG-signs every publication via the local gpg agent (Maven Central).   |

Signing has two paths (mirroring `baml_bridge`): the CI in-memory key
(`GPG_PRIVATE_KEY` / `GPG_PASSPHRASE` env) takes precedence; otherwise
`-PbamlSign` uses the local gpg agent — always pass
`-Psigning.gnupg.keyName=<KEYID>` (see `baml_bridge/PUBLISHING.md`). With no key
configured, the auto-created `sign*` tasks skip, so `publishToMavenLocal`,
`publishPlugins`, and TestKit stay friction-free.

The first-ever Portal submission needs a one-time human approval of the new
plugin id by the Gradle team; fire it with the
`publish-gradle-plugin-manual.yml` workflow (`workflow_dispatch`, version input).
