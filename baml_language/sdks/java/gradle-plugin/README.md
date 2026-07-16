# BAML Gradle plugin (`com.boundaryml.baml`)

Generate the typed BAML Java SDK from your `baml_src/` **at build time** — the
protobuf-gradle-plugin model (pattern C in the Java packaging plan). A cacheable
`generateBaml` task runs the installed `baml` CLI, writes the SDK into
`build/generated/sources/baml/java/main`, and wires that tree into your main
source set. Generation is incremental: Gradle skips it as `UP-TO-DATE` when
nothing under `baml_src/` (or `baml.toml`, or the CLI version) changed, and the
generated code is never checked into your repo.

## Apply

`settings.gradle.kts`:

```kotlin
pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}
```

`build.gradle.kts`:

```kotlin
plugins {
    id("com.boundaryml.baml") version "0.15.0-nightly.1"
}

// The generated code depends on the BAML runtime. Add it (and the native
// engine jar for your platform) so the generated SDK compiles and runs:
repositories {
    mavenCentral()
}
dependencies {
    implementation("com.boundaryml:baml-bridge:0.15.0-nightly.1")
    runtimeOnly("com.boundaryml:baml-bridge:0.15.0-nightly.1:natives-linux-x86_64")
}
```

The plugin applies the `java` plugin for you, so a project with a `baml.toml`
and a `baml_src/` directory needs nothing more. Build as usual:

```sh
gradle build          # runs generateBaml first, then compileJava
```

## Configuration

The optional `baml { … }` block:

| Option           | Type              | Default                | Meaning                                                                                   |
| ---------------- | ----------------- | ---------------------- | ----------------------------------------------------------------------------------------- |
| `srcDir`         | `DirectoryProperty` | the project directory | Directory containing `baml.toml` and `baml_src/`; passed to the CLI as `--from`.           |
| `bamlExecutable` | `Property<String>`  | `"baml"`               | The CLI to run — a bare name resolved on `PATH`, or an absolute path to a specific binary. |
| `outputType`     | `Property<String>`  | `"java"`               | Informational only. The real generator config lives in `baml.toml` `[generator.<name>]`.   |

```kotlin
baml {
    srcDir.set(layout.projectDirectory)
    bamlExecutable.set("baml")
}
```

## What the task does

`generateBaml` (group `baml`, `@CacheableTask`):

- **Inputs** — `baml.toml`, every file under `srcDir/baml_src/` (path-sensitive,
  so renames/deletes are tracked), and the resolved CLI version (`baml
  --version`).
- **Output** — `build/generated/sources/baml/java/main`. The generated files
  declare `package baml_sdk.*`, so the emitter writes them under a `baml_sdk/`
  subdirectory (`baml generate --from <srcDir> -o <outputDir>/baml_sdk`) and that
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

Coordinates `com.boundaryml:baml-gradle-plugin`; the plugin marker
(`com.boundaryml.baml:com.boundaryml.baml.gradle.plugin`) is generated
automatically by `java-gradle-plugin`. Local rehearsal:

```sh
gradle publishToMavenLocal -PbamlVersion=0.15.0-nightly.1
```

| Property      | Default     | Meaning                                                     |
| ------------- | ----------- | ----------------------------------------------------------- |
| `bamlVersion` | `0.0.0-dev` | Published Maven version (canary = plain, nightly = suffixed). |
| `bamlSign`    | *(unset)*   | When present, GPG-signs every publication (Maven Central).    |

Signing uses the local gpg agent; always pass
`-Psigning.gnupg.keyName=<KEYID>` (see `baml_bridge/PUBLISHING.md`).
