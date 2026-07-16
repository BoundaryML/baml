// BAML Gradle plugin (`com.boundaryml.baml`) — the "generate at build time"
// integration (pattern C in ref-java-packaging.md). Registers a cacheable
// `generateBaml` task that shells out to the installed `baml` CLI (which owns
// toolchain/version resolution) and wires its output tree
// (build/generated/sources/baml/java/main) into the main source set, so the
// typed BAML Java SDK is (re)generated incrementally whenever baml_src/ or
// baml.toml change and skipped as UP-TO-DATE otherwise.
//
// Built with `--release 17` (the SDK's minimum supported Java), matching
// baml_bridge. The JDK/gradle come from the repo-root mise.toml
// (temurin-23 / gradle 8.14); invoke via `mise exec -- gradle` and set
// GRADLE_USER_HOME=<workspace>/target/gradle-home to share the repo caches.
//
// Publishing (see baml_bridge/PUBLISHING.md for the conventions this mirrors):
//   gradle publishToMavenLocal -PbamlVersion=0.15.0-nightly.1
// The `java-gradle-plugin` generates the plugin marker publication
// automatically; signing is gated on -PbamlSign.

plugins {
    `java-gradle-plugin`
    `maven-publish`
    signing
}

repositories {
    mavenCentral()
}

// ---- Coordinates ---------------------------------------------------------
// Maven Central family: com.boundaryml:baml-gradle-plugin. Channels have no
// dist-tag equivalent, so canary = plain version (0.15.0), nightly = suffixed
// (0.15.0-nightly.N). CI injects the real value via -PbamlVersion.
val bamlVersion = (project.findProperty("bamlVersion") as String?)?.takeIf { it.isNotBlank() }
    ?: "0.0.0-dev"

group = "com.boundaryml"
version = bamlVersion

dependencies {
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    // GradleRunner (TestKit) for the functional tests.
    testImplementation(gradleTestKit())
}

// ---- Plugin declaration --------------------------------------------------
// Produces the `com.boundaryml.baml` plugin id + its marker publication.
gradlePlugin {
    plugins {
        create("baml") {
            id = "com.boundaryml.baml"
            implementationClass = "com.boundaryml.baml.gradle.BamlPlugin"
            displayName = "BAML Gradle plugin"
            description = "Generates the typed BAML Java SDK from baml_src/ at build time."
        }
    }
}

// Maven Central requires sources + javadoc jars on releases.
java {
    withSourcesJar()
    withJavadocJar()
}

tasks.withType<JavaCompile>().configureEach {
    // --release 17: check against the Java 17 API even on the newer JDK mise
    // provides (temurin-23), so no JDK-23-only symbols sneak in. Matches
    // baml_bridge and the SDK's minimum supported Java.
    options.release.set(17)
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
    // Keep TestKit's scratch state inside build/ so `clean` collects it and it
    // never pollutes the shared GRADLE_USER_HOME.
    systemProperty(
        "org.gradle.testkit.dir",
        layout.buildDirectory.dir("testkit").get().asFile.absolutePath,
    )
    testLogging {
        events("failed", "skipped", "passed")
        showExceptions = true
        showStackTraces = true
    }
}

// ---- Publishing ----------------------------------------------------------
// The `java-gradle-plugin` + `maven-publish` combo auto-creates two
// publications: `pluginMaven` (the main jar → com.boundaryml:baml-gradle-plugin)
// and `bamlPluginMarkerMaven` (the plugin marker →
// com.boundaryml.baml:com.boundaryml.baml.gradle.plugin). Apply the Central POM
// metadata to both (Central requires developers/license/scm on every artifact).
publishing {
    publications.withType<MavenPublication>().configureEach {
        pom {
            name.set("BAML Gradle plugin")
            description.set(
                "Gradle plugin that generates the typed BAML Java SDK from baml_src/ at " +
                    "build time (the protobuf-gradle-plugin model): a cacheable generateBaml " +
                    "task wired into compileJava and the main source set.",
            )
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

    repositories {
        // mavenLocal works out of the box via the built-in `publishToMavenLocal`
        // task (publishes to ~/.m2) — no repository declaration needed here.

        // Maven Central (Sonatype Central Portal). Commented placeholder: the CI
        // publish job owns the real credentials and injects them via the
        // CENTRAL_USERNAME / CENTRAL_PASSWORD env vars. Left disabled so local
        // `publishToMavenLocal` never needs credentials.
        //
        // maven {
        //     name = "central"
        //     url = uri("https://central.sonatype.com/api/v1/publisher/upload")
        //     credentials {
        //         username = System.getenv("CENTRAL_USERNAME")
        //         password = System.getenv("CENTRAL_PASSWORD")
        //     }
        // }

        // File-based staging repo: writes the full Maven layout (jars, POM,
        // module, signatures, checksums) under build/staging-deploy — zip it for
        // the Central Portal bundle upload.
        maven {
            name = "staging"
            url = uri(layout.buildDirectory.dir("staging-deploy"))
        }
    }
}

// Signing for Maven Central (skipped unless -PbamlSign so local dev/test
// publishes stay friction-free). Signs every publication (main jar + marker).
// Uses the local gpg agent; always pass -Psigning.gnupg.keyName=<KEYID> so
// useGpgCmd() does not fall back to gpg's default secret key (see
// baml_bridge/PUBLISHING.md "Signing key pin").
if (providers.gradleProperty("bamlSign").isPresent) {
    signing {
        useGpgCmd()
    }
    publishing.publications.configureEach {
        signing.sign(this)
    }
}
