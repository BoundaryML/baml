// Per-fixture Gradle build for the sdk_test_java crate. Written
// verbatim by `sdk_test_harness_setup::java::codegen_fixture` into
// `crates/java/<fixture>/generated/`.
//
// Source layout inside `generated/`:
//   baml_sdk/   codegen output (package root — `baml_sdk.<ns>` packages)
//   tests/      JUnit sources copied from `../customizable/`
//
// Java 17 is the SDK's minimum-supported release (records + sealed
// interfaces; see ref-java-state-of-completeness.md). The JDK itself
// comes from the repo-root mise.toml (temurin-23); `--release 17`
// makes javac check against the Java 17 API so newer-JDK-only
// symbols can't sneak into generated code or tests.

plugins {
    java
}

tasks.withType<JavaCompile> {
    options.release.set(17)
}

repositories {
    mavenCentral()
}

dependencies {
    // TODO(bridge-java): add the `baml-bridge` runtime dependency once
    // sdks/java/bridge_java exists (the analog of the TS fixtures'
    // `file:`-link to bridge_nodejs).
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    // Analog of pytest's monkeypatch.setenv for env-driven tests (see
    // llm_functions build_request tests). Caveat: patches the JVM's view
    // of the environment only, not the native getenv the engine reads.
    testImplementation("org.junit-pioneer:junit-pioneer:2.2.0")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

sourceSets {
    main {
        java {
            // `generated/` itself is the source root so files under
            // `baml_sdk/...` can declare `package baml_sdk...;`.
            setSrcDirs(listOf("."))
            include("baml_sdk/**")
        }
    }
    test {
        java {
            setSrcDirs(listOf("tests"))
        }
    }
}

tasks.withType<Test> {
    useJUnitPlatform()
    testLogging {
        events("failed", "skipped")
        showExceptions = true
        showStackTraces = true
    }
}
