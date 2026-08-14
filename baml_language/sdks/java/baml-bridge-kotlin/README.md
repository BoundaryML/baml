# baml-bridge-kotlin

Kotlin-idiomatic ergonomics over [`com.boundaryml:baml-bridge`](../baml_bridge) —
the JVM BAML runtime. **Runtime-type extensions only**: coroutine bridges for
`BamlStream`, exhaustive `fold` over the union arity family, and a
cancellation-aware `BamlCallContext` scope. Per-generated-function sugar is out of
scope (the generated Java SDK is already directly callable from Kotlin).

Published to Maven Central as `com.boundaryml:baml-bridge-kotlin` at the BAML
family version, alongside `baml-bridge` and the Gradle plugin (one pipeline, one
version).

## Setup

If you use the BAML Gradle plugin, **applying the Kotlin JVM plugin is enough** —
the plugin auto-injects `baml-bridge-kotlin` at its own version:

```kotlin
// build.gradle.kts
plugins {
    id("org.jetbrains.kotlin.jvm") version "…"
    id("com.boundaryml.baml") version "X"   // injects baml-bridge + baml-bridge-kotlin
}
```

Nightlies (or pre-Portal-approval) resolve the plugin from Central — add this once
per project (see `ref-java-packaging.md` for the full note):

```kotlin
// settings.gradle.kts
pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}
```

Without the plugin, depend on it directly (it brings `baml-bridge` transitively):

```kotlin
dependencies {
    implementation("com.boundaryml:baml-bridge-kotlin:X")
    // plus a baml-bridge natives-<platform> classifier for the engine, e.g.:
    runtimeOnly("com.boundaryml:baml-bridge:X:natives-linux-x86_64")
}
```

## Usage

```kotlin
import com.boundaryml.baml.kotlin.asFlow
import com.boundaryml.baml.kotlin.awaitFinal
import com.boundaryml.baml.kotlin.fold
import com.boundaryml.baml.kotlin.withBamlContext
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.future.await

// _async bindings return CompletableFuture — plain kotlinx.coroutines .await()
// already works, no wrapper needed:
val answer = baml_sdk.my_pkg.Fns.extract_async(input).await()

// Stream a BAML function as a cold Flow of partials, then get the final value:
val stream = baml_sdk.my_pkg.Fns.summarize_stream(doc)
stream.asFlow().collect { partial -> render(partial) }
val final = stream.awaitFinal()

// Exhaustive fold over an anonymous-union return (one lambda per arm):
val label: String = result.fold(
    { i -> "int:$i" },
    { s -> "str:$s" },
)
// …or narrow a single arm without a full fold:
val maybeInt: Long? = result.arm0OrNull()

// A call context that aborts on coroutine cancellation — pass ctx to any
// generated binding's trailing-ctx overload:
val out = withBamlContext { ctx ->
    baml_sdk.my_pkg.Fns.slow_async(input, ctx).await()
}
// If the surrounding coroutine is cancelled, ctx.abort() cancels the in-flight
// engine call before the CancellationException propagates.
```

## Building

Standalone Gradle build (composite `includeBuild("../baml_bridge")` resolves the
runtime locally while the published POM carries the Maven coordinate):

```sh
# JDK + gradle from the repo-root mise.toml; share the repo Gradle caches.
export GRADLE_USER_HOME=<repo>/baml_language/target/gradle-home-release
mise exec -- gradle -p sdks/java/baml-bridge-kotlin test
```

Tests run fully offline (the union/fold and `withBamlContext` paths need no
engine; `asFlow`'s drain loop is tested through an internal seam with a fake
stream supplier).
