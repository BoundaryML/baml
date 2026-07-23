// BAML Java quickstart — the one-liner setup.
//
// The `com.boundaryml.baml` plugin does everything: runs `baml generate`
// before compilation (incrementally), registers the generated sources,
// and injects the version-locked `com.boundaryml:baml-bridge` runtime
// plus the right `natives-<platform>` jar for this machine. Write BAML
// in `baml_src/`, build, done.
//
// The plugin resolves from the Gradle Plugin Portal (default) and the
// runtime artifacts from Maven Central — no repository configuration
// needed beyond `mavenCentral()` for the injected dependencies.
plugins {
    java
    application
    // The plugin/runtime/CLI publish together at one family version — the
    // version here must match the installed `baml` CLI. Until the first
    // fully-matched family nightly ships, treat this example as the shape
    // of the setup rather than a copy-paste-runnable snapshot.
    id("com.boundaryml.baml") version "0.15.0-nightly.1"
}

repositories {
    mavenCentral()
}

tasks.withType<JavaCompile> {
    options.release.set(17)
}

application {
    mainClass.set("quickstart.Main")
}
