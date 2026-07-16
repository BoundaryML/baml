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
//
// Publishing (see PUBLISHING.md):
//   gradle publishToMavenLocal \
//     -PbamlVersion=0.15.0-nightly.local \
//     -PbamlNativeLib=<abs path to libbridge_java.so> \
//     -PbamlNativePlatform=linux-x86_64

plugins {
    java
    `maven-publish`
    signing
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

// ---- Coordinates ---------------------------------------------------------
// Maven Central family: com.boundaryml.baml:baml-bridge. Channels have no
// dist-tag equivalent, so canary = plain version (0.15.0), nightly = suffixed
// (0.15.0-nightly.YYYYMMDD). CI injects the real value via -PbamlVersion.
val bamlVersion = (project.findProperty("bamlVersion") as String?)?.takeIf { it.isNotBlank() }
    ?: "0.0.0-dev"

group = "com.boundaryml.baml"
version = bamlVersion

// ---- Per-platform native jar --------------------------------------------
// The cdylib is shipped in a classifier jar (`natives-<platform>`) at the
// resource path NativeLibraryLoader reads: /native/<platform>/<mapped-lib-name>.
// CI runs this once per target with the platform's built .so; no all-platforms
// fat jar. Both properties default to the local dev (linux-x86_64) case.
val nativePlatform = (project.findProperty("bamlNativePlatform") as String?)?.takeIf { it.isNotBlank() }
    ?: "linux-x86_64"
val nativeLibPath = (project.findProperty("bamlNativeLib") as String?)?.takeIf { it.isNotBlank() }

// Map a platform token (linux-*/macos-*/windows-*) to the OS-appropriate cdylib
// file name. Cross-target, so it can't use System.mapLibraryName (host-only).
fun mappedNativeLibName(platform: String): String = when {
    platform.startsWith("linux") -> "libbridge_java.so"
    platform.startsWith("macos") || platform.startsWith("darwin") || platform.startsWith("osx") ->
        "libbridge_java.dylib"
    platform.startsWith("windows") || platform.startsWith("win") -> "bridge_java.dll"
    else -> throw GradleException(
        "unknown bamlNativePlatform '$platform'; expected linux-<arch>, macos-<arch>, or windows-<arch>")
}

val nativeJar by tasks.registering(Jar::class) {
    description = "Packages the bridge_java cdylib as a natives-<platform> classifier jar."
    archiveClassifier.set("natives-$nativePlatform")
    if (nativeLibPath != null) {
        val libFile = file(nativeLibPath)
        // /native/<platform>/<mapped-lib-name> — matches NativeLibraryLoader.nativeResourcePath().
        from(libFile) {
            into("native/$nativePlatform")
            rename { mappedNativeLibName(nativePlatform) }
        }
        doFirst {
            if (!libFile.exists()) {
                throw GradleException(
                    "bamlNativeLib does not point at an existing file: ${libFile.absolutePath}")
            }
        }
    } else {
        // No native lib provided → produce nothing and say why. The publication
        // omits this artifact entirely (see below), so a plain
        // `publishToMavenLocal` still succeeds with just the main jar + POM.
        onlyIf {
            logger.lifecycle(
                "nativeJar: skipped — no -PbamlNativeLib set. Pass " +
                    "-PbamlNativeLib=<abs path to libbridge_java.so> " +
                    "(and optionally -PbamlNativePlatform=<os>-<arch>, default linux-x86_64) " +
                    "to package the native artifact.")
            false
        }
    }
}

// Maven Central requires sources + javadoc jars on releases.
java {
    withSourcesJar()
    withJavadocJar()
}

tasks.withType<JavaCompile> {
    // --release 17: check against the Java 17 API even on the newer JDK mise
    // provides (temurin-23), so no JDK-23-only symbols sneak in. No toolchain
    // download — the ambient JDK is used.
    options.release.set(17)
}

tasks.jar {
    // The sdk_test_java fixtures link this by exact name via
    // `implementation(files(".../build/libs/baml-bridge.jar"))`, so the plain
    // jar must stay version-less. Maven publication rewrites the published file
    // name to baml-bridge-<version>.jar independently of this.
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

// ---- Publishing ----------------------------------------------------------
publishing {
    publications {
        create<MavenPublication>("maven") {
            groupId = "com.boundaryml.baml"
            artifactId = "baml-bridge"
            version = bamlVersion

            // Pure-Java main jar (build/libs/baml-bridge.jar → published as
            // baml-bridge-<version>.jar).
            from(components["java"])

            // Attach the per-platform native jar only when one was built, so
            // `publishToMavenLocal` with no -PbamlNativeLib still works.
            if (nativeLibPath != null) {
                artifact(nativeJar)
            }

            pom {
                name.set("BAML Java bridge")
                description.set(
                    "In-process BAML runtime for the JVM: the Java entry point into the " +
                        "bridge_java engine (encode/decode + JNI). Ships with per-platform " +
                        "native jars carrying the bridge_java cdylib.")
                url.set("https://github.com/BoundaryML/baml")
                licenses {
                    license {
                        name.set("Apache License 2.0")
                        url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                    }
                }
                scm {
                    url.set("https://github.com/BoundaryML/baml")
                    connection.set("scm:git:https://github.com/BoundaryML/baml.git")
                    developerConnection.set("scm:git:ssh://git@github.com/BoundaryML/baml.git")
                }
            }
        }
    }

    repositories {
        // mavenLocal works out of the box via the built-in `publishToMavenLocal`
        // task (publishes to ~/.m2) — no repository declaration needed here.

        // Maven Central (Sonatype Central Portal). Commented placeholder: the CI
        // publish job owns the real credentials and injects them via the
        // CENTRAL_USERNAME / CENTRAL_PASSWORD env vars (Portal user token). Left
        // disabled so local `publishToMavenLocal` never needs credentials.
        //
        // maven {
        //     name = "central"
        //     url = uri("https://central.sonatype.com/api/v1/publisher/upload")
        //     credentials {
        //         username = System.getenv("CENTRAL_USERNAME")
        //         password = System.getenv("CENTRAL_PASSWORD")
        //     }
        // }
    }
}

// Signing for Maven Central (skipped unless -PbamlSign=true so local
// dev/test publishes stay friction-free). Uses the local gpg agent.
if (providers.gradleProperty("bamlSign").isPresent) {
    signing {
        useGpgCmd()
        sign(publishing.publications["maven"])
    }
}

// File-based staging repo: `publishMavenPublicationToStagingRepository`
// writes the full Maven layout (jars, POM, module, signatures,
// checksums) under build/staging-deploy — zip it for the Central
// Portal bundle upload (see PUBLISHING.md).
publishing {
    repositories {
        maven {
            name = "staging"
            url = uri(layout.buildDirectory.dir("staging-deploy"))
        }
    }
}
