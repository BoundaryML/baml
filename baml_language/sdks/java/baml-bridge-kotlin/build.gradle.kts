// com.boundaryml:baml-bridge-kotlin — Kotlin-idiomatic ergonomics over the
// baml-bridge runtime. Runtime-TYPE extensions only (coroutine bridges for
// BamlStream, `fold`/`armNOrNull` over the Union arity family, and a
// cancellation-aware BamlCallContext scope); per-generated-function sugar is
// explicitly out of scope.
//
// Built with the Kotlin JVM plugin (resolved from the Gradle Plugin Portal at
// build time), targeting the same Java 17 floor as baml_bridge (records + sealed
// interfaces; see ref-java-state-of-completeness.md). The JDK/gradle come from
// the repo-root mise.toml (temurin-23 / gradle 8.14); invoke via
// `mise exec -- gradle` and set GRADLE_USER_HOME to share the repo caches.
//
// Publishing mirrors baml_bridge / gradle-plugin: maven-publish + signing
// (in-memory PGP for CI, local gpg for `-PbamlSign`), sources + javadoc jars,
// and a file-based staging repo overridable via -PbamlStagingDir so the release
// job stages this into the SAME Central bundle as baml-bridge.

import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

plugins {
    // Kotlin JVM from the Gradle Plugin Portal (build-time). java-library (a
    // superset of the `java` base the Kotlin plugin sets up) so the `api`
    // configuration exists — baml-bridge + coroutines must be compile-scoped
    // TRANSITIVE dependencies of the published POM (consumers inherit them).
    kotlin("jvm") version "1.9.25"
    `java-library`
    `maven-publish`
    signing
}

repositories {
    mavenCentral()
}

// ---- Coordinates ---------------------------------------------------------
// Maven Central family: com.boundaryml:baml-bridge-kotlin. Same channel story as
// the rest of the family (canary = plain version, nightly = suffixed); CI injects
// the real value via -PbamlVersion. Plugin/bridge/kotlin all publish from one
// pipeline at one version by construction.
val bamlVersion = (project.findProperty("bamlVersion") as String?)?.takeIf { it.isNotBlank() }
    ?: "0.0.0-dev"

group = "com.boundaryml"
version = bamlVersion

dependencies {
    // The runtime library this extends. `api` (transitive) + composite-build
    // substitution: resolves against the sibling ../baml_bridge project locally
    // while the published POM carries com.boundaryml:baml-bridge at the family
    // version (see settings.gradle.kts). baml-bridge itself brings jspecify.
    api("com.boundaryml:baml-bridge:$bamlVersion")

    // Coroutine interop: Flow (asFlow) + CompletableFuture.await() (awaitFinal,
    // withBamlContext). `api` so consumers inherit the coroutine surface these
    // extensions expose in their public signatures. The `future.await` bridge
    // lives in core since 1.6 (jdk8 merged in) but jdk8 is kept explicit for
    // older toolchains, per the family's dependency contract.
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-jdk8:1.8.1")

    testImplementation(kotlin("test"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

// Java 17 floor. Kotlin targets JVM 17 bytecode and checks its stdlib calls
// against the JDK 17 API (`-Xjdk-release=17`) even on the newer ambient JDK
// (temurin-23), so no JDK-23-only symbol sneaks in — the Kotlin analog of
// baml_bridge's `--release 17`. No toolchain download; the ambient JDK is used.
tasks.withType<KotlinCompile>().configureEach {
    kotlinOptions {
        jvmTarget = "17"
        freeCompilerArgs = freeCompilerArgs + "-Xjdk-release=17"
    }
}

tasks.withType<JavaCompile>().configureEach {
    // No Java sources today, but keep the compile floor honest if any land.
    options.release.set(17)
}

// Maven Central requires sources + javadoc jars on releases. For a pure-Kotlin
// module the java-plugin `javadoc` task has no sources, so the javadoc jar is
// empty — which Central accepts (a packaging requirement, not reference docs;
// Dokka is a later enhancement).
java {
    withSourcesJar()
    withJavadocJar()
}

tasks.withType<Javadoc>().configureEach {
    (options as StandardJavadocDocletOptions).addStringOption("Xdoclint:none", "-quiet")
    isFailOnError = false
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
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
            groupId = "com.boundaryml"
            artifactId = "baml-bridge-kotlin"
            version = bamlVersion

            // Kotlin main jar + sources + javadoc, with api/runtime deps
            // (baml-bridge, coroutines) captured in the POM at compile/runtime
            // scope from the java component.
            from(components["java"])

            pom {
                name.set("BAML Kotlin bridge")
                description.set(
                    "Kotlin ergonomics over com.boundaryml:baml-bridge: coroutine Flow/await " +
                        "bridges for BamlStream, exhaustive fold over the Union arity family, and a " +
                        "cancellation-aware BamlCallContext scope.")
                url.set("https://github.com/BoundaryML/baml")
                developers {
                    developer {
                        id.set("antoniosarosi")
                        name.set("Antonio Sarosi")
                        email.set("antonio@boundaryml.com")
                        organization.set("Boundary ML")
                        organizationUrl.set("https://www.boundaryml.com")
                    }
                    developer {
                        id.set("hellovai")
                        name.set("Vaibhav Gupta")
                        email.set("vbv@boundaryml.com")
                        organization.set("Boundary ML")
                        organizationUrl.set("https://www.boundaryml.com")
                    }
                }
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
        // mavenLocal works out of the box via `publishToMavenLocal` (~/.m2).

        // File-based staging repo: writes the full Maven layout (jar, sources,
        // javadoc, POM, module, signatures, checksums) for the Central Portal
        // bundle. -PbamlStagingDir overrides the destination so the release job
        // points it at baml-bridge's staging-deploy tree and ships all three
        // coordinates (baml-bridge + gradle-plugin + this) in one Central bundle.
        // Default: this project's own build/staging-deploy.
        maven {
            name = "staging"
            val stagingDir = (project.findProperty("bamlStagingDir") as String?)?.takeIf { it.isNotBlank() }
                ?.let { file(it) }
                ?: layout.buildDirectory.dir("staging-deploy").get().asFile
            url = uri(stagingDir)
        }
    }
}

// Signing for Maven Central. Two mutually exclusive paths, priority order
// (mirrors baml_bridge/build.gradle.kts so all three artifacts sign identically):
//
//   1. CI in-memory key (env-gated): when GPG_PRIVATE_KEY is present the
//      publish-maven job injected the ASCII-armored secret key (+ GPG_PASSPHRASE)
//      — sign with an in-memory key. No gpg binary/keyring on the runner.
//   2. Local gpg agent (`-PbamlSign=true`): developer path, signs via local gpg.
//
// With neither, signing is off so publishToMavenLocal / staging dry-runs stay
// friction-free.
run {
    val inMemoryPgpKey = System.getenv("GPG_PRIVATE_KEY")?.takeIf { it.isNotBlank() }
    when {
        inMemoryPgpKey != null -> signing {
            useInMemoryPgpKeys(inMemoryPgpKey, System.getenv("GPG_PASSPHRASE"))
            sign(publishing.publications["maven"])
        }
        providers.gradleProperty("bamlSign").isPresent -> signing {
            useGpgCmd()
            sign(publishing.publications["maven"])
        }
    }
}
