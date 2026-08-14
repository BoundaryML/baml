// Plugin resolution: stable versions resolve from the Gradle Plugin Portal
// with no configuration at all. Nightly versions publish to Maven Central,
// so this stanza makes their resolution guaranteed by documented Gradle
// behavior. (In practice the Portal's endpoint also proxies Central, which
// is why a bare plugins block resolves nightlies too — but an example
// should not depend on infrastructure behavior.)
pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}

rootProject.name = "baml-java-quickstart"
