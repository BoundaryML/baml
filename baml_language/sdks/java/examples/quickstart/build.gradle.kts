// BAML Java quickstart: consumes com.boundaryml.baml:baml-bridge from a
// Maven repository (mavenLocal for the rehearsal; Central once published)
// and registers the `baml generate --output_type java` output as sources.
plugins {
    java
    application
}

repositories {
    mavenLocal()
    mavenCentral()
}

val bamlVersion = providers.gradleProperty("bamlVersion").getOrElse("0.15.0-nightly.local")

dependencies {
    implementation("com.boundaryml.baml:baml-bridge:$bamlVersion")
    // Native engine for this platform (classifier jar). Variant-aware
    // auto-selection is a follow-up; today the platform is explicit.
    runtimeOnly("com.boundaryml.baml:baml-bridge:$bamlVersion:natives-linux-x86_64")
}

sourceSets {
    main {
        java {
            srcDir(".")
            include("baml_sdk/**", "src/main/java/**")
        }
        resources {
            srcDir(".")
            include("baml_sdk/**/*.b64")
        }
    }
}

tasks.withType<JavaCompile> {
    options.release.set(17)
}

application {
    mainClass.set("quickstart.Main")
}
