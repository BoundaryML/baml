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
    // The `baml_bridge` runtime library (the analog of the TS fixtures'
    // `file:`-link to bridge_nodejs). Path is relative to this fixture's
    // `generated/` dir: five levels up reaches `baml_language/`, then into
    // `sdks/java/baml_bridge`. The jar is produced by
    // `gradle -p sdks/java/baml_bridge jar` (see crates/java/setup.sh).
    // `implementation` so it lands on both the generated `baml_sdk/**`
    // compile classpath and the test runtime classpath.
    implementation(files("../../../../../sdks/java/baml_bridge/build/libs/baml-bridge.jar"))
    // JSpecify nullness annotations. The generated code carries
    // `org.jspecify.annotations.@Nullable` on nullable positions; the real
    // baml-bridge POM exposes this as an `api` (transitive) dependency, but the
    // fixtures link the runtime by bare jar via `files(...)` (no POM), so the
    // annotation jar must be declared explicitly here to satisfy javac.
    implementation("org.jspecify:jspecify:1.0.0")
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
    main {
        resources {
            // The embedded BAML bytecode rides as a classpath resource
            // (`baml_sdk/inlinedbaml.b64`); the generated `Baml` anchor
            // reads it in its static initializer.
            setSrcDirs(listOf("."))
            include("baml_sdk/**/*.b64")
        }
    }
    test {
        java {
            setSrcDirs(listOf("tests"))
            // Per-fixture compile exclusions: test sources for
            // capabilities that don't exist yet (Java compiles every
            // test source together, so one missing API would block the
            // whole suite). See tests/compile-excludes.txt.
            val excludesFile = file("tests/compile-excludes.txt")
            if (excludesFile.exists()) {
                excludesFile
                    .readLines()
                    .map { it.trim() }
                    .filter { it.isNotEmpty() && !it.startsWith("#") }
                    .forEach { exclude(it) }
            }
        }
    }
}

tasks.withType<Test> {
    useJUnitPlatform()
    // The generated `Baml` anchor loads the native `bridge_java` library from
    // this path; propagate it from the test invocation's environment so tests
    // reach the real engine. Must point at target/debug/libbridge_java.so.
    environment("BAML_JAVA_BRIDGE_LIB", System.getenv("BAML_JAVA_BRIDGE_LIB") ?: "")
    testLogging {
        events("failed", "skipped")
        showExceptions = true
        showStackTraces = true
    }
}
