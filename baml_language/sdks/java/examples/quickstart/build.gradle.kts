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
