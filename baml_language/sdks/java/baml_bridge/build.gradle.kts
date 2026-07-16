// BAML Java runtime library (`baml_bridge`) — the JVM analog of the Python
// `baml_bridge` package. Hand-rolled protobuf codec + JNI entry points; no
// runtime dependencies (protobuf-java is deliberately avoided — only the
// primitives slice is needed and a hand-rolled varint codec keeps this
// zero-dep). Built with `--release 17` so it stays on the SDK's minimum
// supported Java (records + sealed interfaces; see
// ref-java-state-of-completeness.md). The JDK/gradle come from the repo-root
// mise.toml (temurin-23 / gradle 8.14); invoke via `mise exec -- gradle`.
//
// Set GRADLE_USER_HOME to <workspace>/target/gradle-home when invoking, to
// share the dependency/JDK caches with the sdk_test_java fixtures.

plugins {
    java
}

repositories {
    mavenCentral()
}

dependencies {
    // Test-only: exercise the native round-trip in BamlFfiSmokeTest. Versions
    // match the sdk_test_java fixtures so the shared Gradle cache is reused.
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.withType<JavaCompile> {
    // --release 17: check against the Java 17 API even on the newer JDK mise
    // provides (temurin-23), so no JDK-23-only symbols sneak in. No toolchain
    // download — the ambient JDK is used.
    options.release.set(17)
}

tasks.jar {
    // The sdk_test_java fixtures link this by exact name.
    archiveFileName.set("baml-bridge.jar")
}

tasks.withType<Test> {
    useJUnitPlatform()
    // Propagate the native-library path so the smoke test can System.load it.
    // BAML_JAVA_BRIDGE_LIB (env) → -Dbaml.bridge.lib (system property).
    System.getenv("BAML_JAVA_BRIDGE_LIB")?.let { systemProperty("baml.bridge.lib", it) }
    testLogging {
        events("failed", "skipped", "passed")
        showExceptions = true
        showStackTraces = true
    }
}
