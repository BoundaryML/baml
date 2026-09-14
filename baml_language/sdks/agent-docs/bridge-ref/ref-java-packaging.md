---
date: 2026-07-16
repository: baml4
---
# Java packaging & generated-code placement

How BAML ships today (ground-truthed against canary), and the plan for
the Java/JVM bridge. Companion to `ref-java-state-of-completeness.md`
and `ref-java-codegen-conventions.md`.

## How BAML ships today (new system)

Three fully decoupled tiers:

1. **CLI — never on PyPI/npm.** Two binaries: the `baml` **wrapper**
   (`crates/baml`) is a version manager à la rustup — it reads a
   `[toolchain]` channel/version from `baml.toml`, installs toolchains
   under `~/.baml/toolchains/<version>/`, and execs the real
   **`baml-cli`** toolchain binary (`crates/baml_cli`: `generate`,
   `init`, `test`, `run`, `lsp`, …). Users install the wrapper via the
   curl installer (`scripts/install.sh`, manifests from
   `pkg.boundaryml.com`), Homebrew tap, or AUR. Channels: canary and
   nightly. **This tier is language-agnostic — Java needs no CLI
   distribution work at all.**
2. **Runtime bridge — one package per language ecosystem.** PyPI
   `baml_bridge`: a maturin wheel whose Rust engine is a pyo3 native
   extension module (`baml_bridge.baml_py`); one wheel per platform,
   pip selects automatically. npm `@boundaryml/baml-bridge`: a pure-JS
   umbrella plus 8 per-platform sub-packages each carrying the napi
   `.node` addon, wired via `optionalDependencies` so npm installs
   only the matching one. The CLI is not bundled in either.
3. **Generated code — never published.** `baml generate` writes
   `baml_sdk/` into the user's project; it imports tier 2 at runtime.

Release orchestration: `release-baml-language.yml` (channel plan →
build toolchain/wrapper/SDKs → publish PyPI/npm/pkg.boundaryml.com/
homebrew/AUR). npm uses OIDC trusted publishing with channel→dist-tag
(canary→`latest`, nightly→`nightly`).

## Java plan

**One Maven Central artifact family: `com.boundaryml:baml-bridge`.**

- **Main JAR** (pure Java): `BamlRuntime`, error hierarchy,
  `BamlStream`, media wrappers, protobuf codec.
- **Per-platform native JARs** carrying the `bridge_java` jni-rs
  cdylib (same 8-target matrix as Node). Platform selection — the
  Maven answer to npm's `optionalDependencies` — is **Gradle Module
  Metadata variants**, so a plain
  `implementation("com.boundaryml:baml-bridge:…")` resolves the
  right native JAR by OS/arch attributes. Maven-proper users use the
  `os-maven-plugin` classifier pattern (the gRPC/netty convention).
  No all-platforms fat JAR: at engine sizes that would be an
  unreasonable download.
- **Channels:** Maven has no dist-tags; canary = plain version
  (`0.15.0`), nightly = suffixed version
  (`0.15.0-nightly.YYYYMMDD.a`). Publish job slots into
  `release-baml-language.yml` as `build-java-sdk` / `publish-maven`.
- **CLI:** covered by the wrapper/toolchain tier; nothing to ship via
  Maven. What Java *does* need is a `"java"` `OutputType` variant in
  `baml_codegen_types::generator_fields` plus a dispatch arm in
  `crates/baml_cli/src/generate.rs` calling
  `sdkgen_java::to_source_code_with_bytecode`.

## Where generated code goes in a Java project

`baml generate` config lives in `baml.toml` `[generator.<name>]`
(`output_type`, `naming_convention`, `output_dir` — default `"."`,
with `baml_sdk` always appended). Generated files declare
`package baml_sdk.*`, so **the registered source root must be the
parent of the `baml_sdk/` directory** — which composes exactly with
the append behavior. The bytecode resource (`inlinedbaml.b64`) rides
in the same tree and must be registered as a resource root.

Three placement patterns, in order of adoption:

- **A (v0 quickstart): a source root inside the app.** Point
  `output_dir` into the app and add three lines of Gradle:
  `sourceSets.main.java.srcDir("<dir>")` + a matching resources entry.
  Closest analog to today's Python flow.
- **B (multi-module): a dedicated Gradle subproject** (`:baml-sdk`)
  with one-time boilerplate `build.gradle.kts`; the app depends on
  `project(":baml-sdk")`. Keeps app `src/` pristine; generated code
  gets its own compilation unit.
- **C (SHIPPED 2026-07-16): a Gradle plugin** (`com.boundaryml.baml`, at `sdks/java/gradle-plugin/`), the
  protobuf-gradle-plugin model: a `generateBaml` task with declared
  inputs (`baml_src/**`, `baml.toml`, toolchain version) and outputs
  (`build/generated/sources/baml/java/main`), wired into
  `compileJava` and the source sets. Generation happens at **build
  time, incrementally** — Gradle skips the task as `UP-TO-DATE` when
  no `.baml` input changed; running the built program never invokes
  generation. v0 of the plugin shells out to the installed `baml`
  wrapper (which already owns version resolution), erroring helpfully
  when missing; toolchain self-bootstrap is a later enhancement.
  Estimated at 2–3 days including publishing; deliberately sequenced
  **after** the first end-to-end slice works, because it wraps a
  `generate --output_type java` that must exist and be demonstrable
  first. A/B and C are compatible: A/B ship in bridge week, C is the
  first fast-follow.

## Open items

- **Stale-file hazard (upstream, affects all languages, bites Java
  hardest):** `generate` does not clean its output dir; a renamed or
  deleted BAML class leaves a stale `.java` that still compiles into
  the user's app. Since generate owns `baml_sdk/` outright,
  clean-before-write is safe — proposed as an upstream fix alongside
  the `"java"` OutputType wiring.
- Gradle Plugin Portal publishing needs an org account (coordinate
  with the CI/publishing owner); nightly channel on the Portal is
  awkward (immutable versions) — likely nightly via Maven-style
  suffixed versions only.
- Maven-plugin twin of the Gradle plugin: post-GA.


## Plugin distribution (dual-channel, decided 2026-07-20)

The Gradle plugin (`com.boundaryml.baml`) ships through two channels at the family version:

- **Gradle Plugin Portal** — canary/stable cuts only (Portal versions are immutable;
  its catalog is user-facing). Zero-config resolution: the bare `plugins {}` block works.
  First publish of the new namespace requires a one-time human approval by the Gradle
  team (warming workflow: `publish-gradle-plugin-manual.yml`).
- **Maven Central** — every channel, riding the `baml-bridge` bundle (plugin jar +
  sources/javadoc + the `com.boundaryml.baml.gradle.plugin` marker POM, all signed).
  Nightly consumers (or anyone pre-Portal-approval) add one `pluginManagement` stanza:

  ```kotlin
  // settings.gradle.kts — once per project
  pluginManagement {
      repositories {
          mavenCentral()
          gradlePluginPortal()
      }
  }
  ```

The plugin manages the consumer's dependencies (one-liner UX): it injects the
version-locked `com.boundaryml:baml-bridge` implementation dep and the host-detected
`natives-<platform>` classifier (overrides: `baml { nativePlatforms }` incl. `"all"`,
`baml { manageDependencies.set(false) }`; a pre-existing explicit baml-bridge dep
suppresses injection). Plugin version == bridge version by construction, published
from one pipeline. The quickstart example flips to the pure one-liner once the first
plugin version is live on a registry.


## Maven Central artifacts (family, one pipeline, one version)

Three coordinates, all published by `release-baml-language.yml` → `publish-maven`
in a single signed Central bundle at the same family version (per-coordinate
idempotency: each is (re)published only when that version is missing from Central):

| Coordinate | What it is | How consumers get it |
|---|---|---|
| `com.boundaryml:baml-bridge` | Pure-Java runtime (hand-rolled protobuf codec + JNI) plus per-platform `natives-<platform>` classifier jars carrying the `bridge_java` cdylib. Exposes `org.jspecify:jspecify` as an `api` (transitive, compile-scope) dep so generated `@Nullable` annotations resolve. | `implementation("…:baml-bridge:…")` + a `natives-*` classifier; or auto-injected by the plugin. |
| `com.boundaryml:baml-gradle-plugin` (+ marker `com.boundaryml.baml:com.boundaryml.baml.gradle.plugin`) | Build-time codegen plugin (`com.boundaryml.baml`). | `plugins { id("com.boundaryml.baml") version "X" }` (Gradle Plugin Portal, stable channels) or from Central via a `pluginManagement` stanza (every channel). |
| `com.boundaryml:baml-bridge-kotlin` | Kotlin ergonomics over the runtime (see below). Depends on `baml-bridge` at the same family version (composite build locally; the POM carries the Maven coordinate). | `implementation("…:baml-bridge-kotlin:…")`; or auto-injected by the plugin when `org.jetbrains.kotlin.jvm` is applied. |

**JSpecify nullness (all consumers).** The Java emitter (`sdkgen_java`) writes
`org.jspecify.annotations.@Nullable` on the genuinely-nullable positions of the
generated code (nullable field accessors/constructor params, nullable binding
params + returns, `CompletableFuture<@Nullable T>` async elements, `$Opts` setters
for nullable optionals, the always-nullable `IntOptCallback`-style `Opts`
accessors, and `List<@Nullable T>` / `Map<…, @Nullable V>` element positions). So
Kotlin sees real nullness instead of platform types (`String?` vs `String!`), and
Java IDEs improve. JSpecify is a transitive dep of `baml-bridge` (no new coordinate,
no action for consumers).

### Kotlin — `baml-bridge-kotlin`

The generated Java SDK is already directly usable from Kotlin; `baml-bridge-kotlin`
adds idiomatic **runtime-type** ergonomics (per-generated-function sugar is
explicitly out of scope):

- `stream.asFlow(): Flow<P>` — a cold flow of partials, draining `next_async()`
  until the `ai.stream.Done` sentinel (never emitted); `stream.awaitFinal(): F`.
- `fold` over the `UnionN` arity family (`Union2`…`Union10`) — one lambda per arm,
  exhaustive by signature — plus `armIOrNull()` narrowing accessors.
- `withBamlContext { ctx -> … }` — runs the block with a fresh `BamlCallContext`
  and calls `ctx.abort()` if the coroutine is cancelled (wires the coroutine's
  `CancellationException` → engine abort), rethrowing to preserve structured
  concurrency. The Java surface instead holds a `BamlCallContext` and calls
  `abort()` explicitly.
- `_async` bindings return `CompletableFuture`, so plain `kotlinx.coroutines`
  `.await()` already works — the library adds the stream/union/context sugar that
  isn't a one-liner.

It is **one new Central coordinate** (no Portal involvement — the Portal is
plugin-only) and depends on `baml-bridge` at the same version. The Gradle plugin
auto-injects it whenever the consumer applies the Kotlin JVM plugin (same
version-lock + defer-to-explicit rules as `baml-bridge`), so a Kotlin consumer
using the plugin needs no extra dependency line.
