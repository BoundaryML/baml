// BAML Gradle plugin (`com.boundaryml.baml`) — the "generate at build time"
// integration (pattern C in ref-java-packaging.md). Registers a cacheable
// `generateBaml` task that shells out to the installed `baml` CLI (which owns
// toolchain/version resolution) and wires its output tree
// (build/generated/sources/baml/java/main) into the main source set, so the
// typed BAML Java SDK is (re)generated incrementally whenever baml_src/ or
// baml.toml change and skipped as UP-TO-DATE otherwise.
//
// The plugin also auto-manages the BAML runtime: applying it injects
// `com.boundaryml:baml-bridge` at the plugin's own version plus the native jar
// for the build machine, so `plugins { id("com.boundaryml.baml") version "X" }`
// is the entire setup (BamlPlugin reads its version from the generated
// `baml-plugin.properties` resource wired below).
//
// Built with `--release 17` (the SDK's minimum supported Java), matching
// baml_bridge. The JDK/gradle come from the repo-root mise.toml
// (temurin-23 / gradle 8.14); invoke via `mise exec -- gradle` and set
// GRADLE_USER_HOME=<workspace>/target/gradle-home to share the repo caches.
//
// Publishing:
//   - Gradle Plugin Portal (stable channels): `gradle publishPlugins`
//     (com.gradle.plugin-publish; reads GRADLE_PUBLISH_KEY/GRADLE_PUBLISH_SECRET).
//   - Maven Central nightly bundle: the plugin rides the same Central deployment
//     as baml-bridge via the maven-publish `pluginMaven` + plugin-marker
//     publications staged into build/staging-deploy (see the release workflow).
//   - Local rehearsal: `gradle publishToMavenLocal -PbamlVersion=0.15.0-nightly.1`.

plugins {
    `java-gradle-plugin`
    `maven-publish`
    signing
    // Gradle Plugin Portal publishing (current 1.x). Adds the `publishPlugins`
    // task + Portal metadata validation; reads the Portal API key/secret from
    // the GRADLE_PUBLISH_KEY / GRADLE_PUBLISH_SECRET env vars (or the
    // gradle.publish.key/secret properties) natively.
    id("com.gradle.plugin-publish") version "1.3.1"
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

// ---- Plugin-version resource ---------------------------------------------
// BamlPlugin injects `com.boundaryml:baml-bridge:<pluginVersion>`; it reads that
// version at runtime from this generated classpath resource (works under
// TestKit, where the plugin is loaded from build/ with no jar manifest to read
// Implementation-Version from). Registered as a resource source dir via the task
// provider so processResources — and the java-gradle-plugin's
// pluginUnderTestMetadata (the TestKit classpath) — depend on it and see it.
val versionResourceDir = layout.buildDirectory.dir("generated/sources/baml-plugin-version")
val generateBamlPluginVersionResource by tasks.registering {
    description = "Writes baml-plugin.properties (version=<bamlVersion>) for BamlPlugin to read at runtime."
    val outDir = versionResourceDir
    val versionValue = bamlVersion
    inputs.property("bamlVersion", versionValue)
    outputs.dir(outDir)
    doLast {
        val file = outDir.get()
            .dir("com/boundaryml/baml/gradle")
            .file("baml-plugin.properties")
            .asFile
        file.parentFile.mkdirs()
        file.writeText("version=$versionValue\n")
    }
}
sourceSets["main"].resources.srcDir(generateBamlPluginVersionResource)

// ---- Plugin declaration + Portal metadata --------------------------------
// Produces the `com.boundaryml.baml` plugin id + its marker publication, and
// carries the metadata the Gradle Plugin Portal requires (website, vcsUrl,
// per-plugin displayName/description/tags).
gradlePlugin {
    website.set("https://github.com/BoundaryML/baml")
    vcsUrl.set("https://github.com/BoundaryML/baml")
    plugins {
        create("baml") {
            id = "com.boundaryml.baml"
            implementationClass = "com.boundaryml.baml.gradle.BamlPlugin"
            displayName = "BAML Gradle Plugin"
            description =
                "Generate the typed BAML Java SDK from baml_src/ at build time and auto-manage " +
                    "the matching baml-bridge runtime (and its native engine jar) — so " +
                    "`plugins { id(\"com.boundaryml.baml\") version \"X\" }` is the whole setup."
            tags.set(listOf("baml", "llm", "codegen"))
        }
    }
}

// Maven Central requires sources + javadoc jars on releases. (com.gradle
// .plugin-publish also produces them for the Portal upload; withSourcesJar/
// withJavadocJar are idempotent — a no-op if the task already exists — so this
// stays safe alongside it and guarantees the jars ride the java component into
// the pluginMaven publication that feeds the Central bundle.)
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

tasks.withType<Javadoc>().configureEach {
    // Central requires a javadoc jar, but it is a packaging requirement, not
    // reference docs. Disable doclint so generated-adjacent boilerplate never
    // fails the release build (mirrors baml_bridge).
    (options as StandardJavadocDocletOptions).addStringOption("Xdoclint:none", "-quiet")
    isFailOnError = false
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
// com.boundaryml.baml:com.boundaryml.baml.gradle.plugin, a POM whose only body is
// a dependency on the main artifact). Apply the Central POM metadata to BOTH
// (Central requires developers/license/scm on every artifact, marker included).
publishing {
    publications.withType<MavenPublication>().configureEach {
        pom {
            name.set("BAML Gradle Plugin")
            description.set(
                "Gradle plugin that generates the typed BAML Java SDK from baml_src/ at " +
                    "build time and auto-manages the baml-bridge runtime + native jar.",
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

        // File-based staging repo: writes the full Maven layout (jars, POM,
        // module, signatures, checksums) for the Central Portal bundle. The
        // destination is overridable via -PbamlStagingDir so the release job can
        // point it at baml-bridge's staging-deploy tree and ship BOTH artifact
        // families (baml-bridge + this plugin + its marker) in one Central
        // bundle for nightlies (the Portal is stable-channels-only). Default:
        // this project's own build/staging-deploy.
        maven {
            name = "staging"
            val stagingDir = (project.findProperty("bamlStagingDir") as String?)?.takeIf { it.isNotBlank() }
                ?.let { file(it) }
                ?: layout.buildDirectory.dir("staging-deploy").get().asFile
            url = uri(stagingDir)
        }
    }
}

// Signing for Maven Central. com.gradle.plugin-publish AUTO-SIGNS every
// publication (pluginMaven main+sources+javadoc + the bamlPluginMarkerMaven POM)
// whenever the `signing` plugin is applied — it creates signPluginMavenPublication
// and signBamlPluginMarkerMavenPublication for us. So here we only (a) configure
// the signatory and (b) gate those auto-created Sign tasks so they run ONLY when
// a key is actually available. Two signatory paths, priority order (mirrors
// baml_bridge/build.gradle.kts so the plugin signs identically):
//
//   1. CI in-memory key (env-gated): when GPG_PRIVATE_KEY is present the release
//      job has injected the ASCII-armored secret key (+ GPG_PASSPHRASE) — sign
//      with an in-memory key. No gpg binary/keyring on the runner.
//   2. Local gpg agent (`-PbamlSign`): developer path, signs via the local gpg
//      (pin the key with `-Psigning.gnupg.keyName=<KEYID>`; see
//      baml_bridge/PUBLISHING.md — the BoundaryML key is A5006CD3995646B6).
//
// With neither, the Sign tasks are skipped (onlyIf false), so `publishPlugins`
// (the Portal — GPG not required there), `publishToMavenLocal`, and TestKit stay
// friction-free. When a key IS present, the signatures ride the pluginMaven +
// marker publications into the Central staging bundle, which Central requires.
run {
    val inMemoryPgpKey = System.getenv("GPG_PRIVATE_KEY")?.takeIf { it.isNotBlank() }
    val useLocalGpg = providers.gradleProperty("bamlSign").isPresent
    val signingEnabled = inMemoryPgpKey != null || useLocalGpg
    if (inMemoryPgpKey != null) {
        signing { useInMemoryPgpKeys(inMemoryPgpKey, System.getenv("GPG_PASSPHRASE")) }
    } else if (useLocalGpg) {
        signing { useGpgCmd() }
    }
    tasks.withType<org.gradle.plugins.signing.Sign>().configureEach {
        onlyIf { signingEnabled }
    }
}
