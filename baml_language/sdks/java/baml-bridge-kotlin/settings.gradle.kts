// Standalone Gradle build for com.boundaryml:baml-bridge-kotlin — the Kotlin
// ergonomics layer over the baml-bridge runtime (coroutine Flow/await bridges
// for BamlStream, exhaustive `fold` over the Union arity family, and a
// cancellation-aware BamlCallContext scope). Published as its own Maven Central
// coordinate at the family version.
//
// baml-bridge is pulled in as an INCLUDED build (composite): a plain
// `api("com.boundaryml:baml-bridge:<version>")` in build.gradle.kts resolves
// against the sibling ../baml_bridge project locally (compile + test), while the
// published POM still carries the real Maven coordinate at the family version.
// This is the same "reference baml-bridge at our own version, one pipeline"
// stance the gradle-plugin takes.
rootProject.name = "baml-bridge-kotlin"

includeBuild("../baml_bridge")
