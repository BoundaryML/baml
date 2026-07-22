# BAML Java quickstart

The complete setup is one plugin line — write BAML in `baml_src/`, build, done:

```kotlin
// build.gradle.kts
plugins {
    java
    application
    id("com.boundaryml.baml") version "0.15.0-nightly.1"
}
repositories { mavenCentral() }
```

> ⚠ The plugin, runtime artifacts, and `baml` CLI publish together at one
> family version, and the version in the `plugins` block must match your
> installed CLI. Until the first fully-matched family nightly ships, this
> example shows the shape of the setup; it becomes copy-paste-runnable the
> moment that nightly publishes.

The plugin resolves from the Gradle Plugin Portal, runs `baml generate`
before compilation (incrementally — it reruns only when a generation
input changes: the `.baml` sources, `baml.toml`, or the resolved CLI
version), registers the generated sources for the compiler and IDE,
and injects the version-locked `com.boundaryml:baml-bridge` runtime
plus the correct `natives-<platform>` jar for your machine (Kotlin
projects also get `baml-bridge-kotlin`). Overrides live on the `baml {}`
extension (`nativePlatforms`, `manageDependencies`, `bamlExecutable`).

Requirements: JDK 17+, the `baml` CLI on PATH at a version matching the
plugin (install via the standard BAML install flow — the CLI, plugin,
and runtime artifacts publish together at one version).

Run it:

```console
gradle run
# add(2, 3) = 5
```
