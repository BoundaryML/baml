# BAML Java quickstart

The complete setup is one plugin line — write BAML in `baml_src/`, build, done:

```kotlin
// build.gradle.kts
plugins {
    java
    application
    id("com.boundaryml.baml") version "0.15.1-nightly.20260722.c"
}
repositories { mavenCentral() }
```

Nightly plugins resolve from Maven Central. Add its plugin marker repository once:

```kotlin
// settings.gradle.kts
pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}
```

The plugin, runtime artifacts, and `baml` CLI publish together at one family version, and the version in the `plugins` block must match your installed CLI. The version above is the latest published nightly as of 2026-07-22; use a newer family version consistently when upgrading.

The plugin resolves from Maven Central for nightlies (canary/stable releases also publish to the Gradle Plugin Portal), runs `baml generate` before compilation (incrementally — it reruns only when a generation input changes: the `.baml` sources, `baml.toml`, or the resolved CLI version), registers the generated sources for the compiler and IDE, and injects the version-locked `com.boundaryml:baml-bridge` runtime plus the correct `natives-<platform>` jar for your machine (Kotlin projects also get `baml-bridge-kotlin`). Overrides live on the `baml {}` extension (`nativePlatforms`, `manageDependencies`, `bamlExecutable`).

Requirements: JDK 17+, the `baml` CLI on PATH at a version matching the
plugin (install via the standard BAML install flow — the CLI, plugin,
and runtime artifacts publish together at one version).

Run it:

```console
gradle run
# add(2, 3) = 5
```
