<!-- Keep a Changelog guide -> https://keepachangelog.com -->

# IntelliJ Platform Plugin Template Changelog

## [Unreleased]

## [2.1.0] - 2025-03-28

### Added

- Example code – `ProjectActivity`
- Added `opentest4j` test dependency, see: [Missing opentest4j dependency in Test Framework](https://plugins.jetbrains.com/docs/intellij/tools-intellij-platform-gradle-plugin-faq.html#missing-opentest4j-dependency-in-test-framework)

### Removed

- Example code – `MyApplicationActivationListener`
- Remove redundant IntelliJ Platform dependency helpers: `instrumentationTools()`, `pluginVerifier()`, `zipSigner()`
- GitHub Actions: Remove `gradle/actions/wrapper-validation` because validation is performed by default in `gradle/actions/setup-gradle@v4`

### Changed

- Change JVM version to `21`
- Upgrade Gradle Wrapper to `8.13`
- Update `platformVersion` to `2024.2.5`
- Change since/until build to `242-252.*` (2024.2-2025.2.*)
- Dependencies - upgrade `org.jetbrains.intellij.platform` to `2.5.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `2.1.20`
- Dependencies - upgrade `org.jetbrains.qodana` to `2024.3.4`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.9.1`
- Dependencies (GitHub Actions) - upgrade `gradle/actions/wrapper-validation` to `v4`
- Dependencies (GitHub Actions) - upgrade `codecov/codecov-action` to `v5`

## [2.0.2] - 2024-10-07

### Changed

- Upgrade Gradle Wrapper to `8.10.2`
- Update `platformVersion` to `2023.3.8`
- Dependencies - upgrade `org.jetbrains.intellij.platform` to `2.1.0`
- Dependencies - upgrade `org.jetbrains.qodana` to `2024.2.3`
- Dependencies (GitHub Actions) - upgrade `gradle/actions/setup-gradle` to `v4`
- Add back the `org.gradle.toolchains.foojay-resolver-convention` Gradle settings plugin

### Fixed

- Fixed _Run Plugin_ run configuration logs location

### Removed

- Removed _Run Qodana_ and _Run UI for UI Tests_ run configurations

## [2.0.1] - 2024-08-09

### Changed

- Update `platformVersion` to `2023.3.7`
- Change since/until build to `233-242.*` (2023.3-2024.2.*)
- Cleanup registering the `runIdeForUiTests` task
- Dependencies - upgrade `org.jetbrains.intellij.platform` to `2.0.1`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.9.25`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.8.3`
- Dependencies - upgrade `org.jetbrains.qodana` to `2024.1.9`

## [2.0.0] - 2024-07-30

### Changed

- Migrate to [IntelliJ Platform Gradle Plugin 2.0](https://blog.jetbrains.com/platform/2024/07/intellij-platform-gradle-plugin-2-0/).

## [1.14.2] - 2024-07-12

### Changed

- Upgrade Gradle Wrapper to `8.9`

### Removed

- Remove default plugin icon (`pluginIcon.svg`)

## [1.14.1] - 2024-06-19

### Changed

- Update `platformVersion` to `2023.2.7`
- Upgrade Gradle Wrapper to `8.8`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.17.4`
- Dependencies - downgrade `org.jetbrains.kotlin.jvm` to `1.9.24`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.8.1`

## [1.14.0] - 2024-05-30

### Changed

- Update `platformVersion` to `2023.2.6`
- Change since/until build to `232-242.*` (2023.2-2024.2.*)
- Upgrade Gradle Wrapper to `8.7`
- Update Kover configuration
- Replace `org.jetbrains:annotations` library with an `com.example:exampleLibrary` placeholder
- Dependencies - upgrade `org.jetbrains.intellij` to `1.17.3`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `2.0.0`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.8.0`
- Dependencies - upgrade `org.jetbrains.qodana` to `2024.1.5`
- Dependencies (GitHub Actions) - replace `gradle/wrapper-validation-action@v2` with `gradle/actions/wrapper-validation@v3`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2024.1.5`
- Dependencies (GitHub Actions) - upgrade `jtalk/url-health-check-action` to `v4`

## [1.13.0] - 2024-03-11

### Changed

- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.9.23`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.7.6`
- Dependencies - upgrade `org.jetbrains.qodana` to `2023.3.2`
- Dependencies (GitHub Actions) - upgrade `actions/upload-artifact` to `4`
- Dependencies (GitHub Actions) - upgrade `codecov/codecov-action` to `4`
- Dependencies (GitHub Actions) - upgrade `gradle/wrapper-validation-action` to `2`
- Dependencies (GitHub Actions) - upgrade `actions/cache` to `4`
- Gradle - upgrade `org.gradle.toolchains.foojay-resolver-convention` to `0.8.0`
- Gradle - cleanup the `jvmToolchain` setup
- Run Configurations - `Run Qodana` runs the `qodanaScan` Gradle task

### Fixed

- Fixed calculation of the plugin publication channel
- Run Configurations - `Run Tests` uses the `RunAsTest` IDE feature
- Replace the whole `IntelliJ Platform Plugin Template` with the new project name when running the GitHub Actions Cleanup workflow

### Removed

- GitHub Actions: Remove the `Setup Java` step from the `releaseDraft` build step
- Gradle - Removed Qodana Gradle Plugin configuration to rely on defaults

## [1.12.0] - 2024-02-20

### Added

- GitHub Actions: Reduce the number of concurrent builds

### Changed

- Change since/until build to `223-241.*` (2022.3-2024.1.*)
- Upgrade Gradle Wrapper to `8.6`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.17.2`
- Dependencies (GitHub Actions) - upgrade `gradle/gradle-build-action@v2` to `gradle/actions/setup-gradle@v3`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.3.1`

### Fixed

- Adjusted obtaining the value for `publishPlugin.channels` property in `build.gradle.kts`
- Fixed bash variable access in the Create Release Draft step.

### Removed

- Remove Gradle Kotlin DSL Lazy Property Assignment because it's default now

## [1.11.3] - 2023-12-01

### Changed

- Upgrade Gradle Wrapper to `8.5`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.16.1`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.9.21`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.7.5`
- Dependencies - upgrade `annotations` to `24.1.0`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.2.8`
- Dependencies (GitHub Actions) - upgrade `actions/setup-java` to `4`

## [1.11.2] - 2023-10-06

### Changed

- Upgrade Gradle Wrapper to `8.4`
- Gradle - use JetBrains Runtime
- Gradle - upgrade `org.gradle.toolchains.foojay-resolver-convention` to `0.7.0`
- Change since/until build to `223-233.*` (2022.3-2023.3.*)
- Dependencies - upgrade `org.jetbrains.intellij` to `1.16.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.9.10`
- Dependencies - upgrade `org.jetbrains.changelog` to `2.2.0`
- Dependencies (GitHub Actions) - upgrade `actions/checkout` to `4`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.2.6`

## [1.11.1] - 2023-08-17

### Fixed

- GitHub Actions: publish workflow fails due to the existing `release changelog` label

### Changed

- Upgrade Gradle Wrapper to `8.3`

## [1.11.0] - 2023-08-07

### Changed

- GitHub Actions: move the `Maximize Build Space` step from the `Build` job to `Inspect code` and `Verify plugin` jobs
- Update `platformVersion` to `2022.3.3`
- Change since/until build to `223-232.*` (2022.3-2023.2.*)
- `MyBundle`: remove `SpreadOperator` suppression
- `MyBundle.properties`: remove unused `name` property

## [1.10.0] - 2023-08-04

### Added

- [Foojay Toolchains Plugin](https://github.com/gradle/foojay-toolchains) integration
- GitHub Actions: set `gradle-home-cache-cleanup: true` flag to the Gradle Build Action
- GitHub Actions: use `jlumbroso/free-disk-space` to free disk space

### Changed

- GitHub Actions: rearrange the Build workflow
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.7.3`
- Dependencies (GitHub Actions) - upgrade `gradle/wrapper-validation-action` to `v1.1.0`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.2.1`

### Fixed

- GitHub Actions: Fixed Kover report path when uploading the code coverage report

## [1.9.0] - 2023-07-11

### Added

- GitHub Actions — use Java `17` explicitly

### Changed

- Upgrade Gradle Wrapper to `8.2.1`
- Dependencies — upgrade `org.jetbrains.changelog` to `2.1.2`
- GitHub Actions — rearrange the Build workflow

### Fixed

- Fixed copying files issue when running the `Template Cleanup` workflow

## [1.8.0] - 2023-07-07

### Changed

- GitHub Actions: Build workflow refactoring
- Upgrade Gradle Wrapper to `8.2`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.9.0`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.15.0`
- Dependencies - upgrade `org.jetbrains.changelog` to `2.1.1`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.7.2`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.1.5`

### Fixed

- Dependabot workflow points to `next` branch in a newly created project

### Removed

- Removed workaround for Kotlin Compiler `OutOfMemoryError` as the issue is gone with Kotlin `1.9.0`

## [1.7.0] - 2023-06-07

### Added

- GitHub Actions — enable caching
- Specify `projectJDK: 17` in `qodana.yml`
- Specify `linter` property in `qodana.yml`

### Changed

- Use Java `17` for JVM Toolchain
- Change since/until build to `222-232.*` (2022.2–2023.2.*)
- Dependencies - upgrade `org.jetbrains.intellij` to `1.14.1`
- Dependencies - upgrade `org.jetbrains.changelog` to `2.1.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.8.21`
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.7.1`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2023.1.0`
- Upgrade Gradle Wrapper to `8.1.1`
- GitHub Actions — switch to Java 17
- Update Run Configuration entries
- Adjust Kover configuration

### Fixed

- Example code — Fixed deprecated usage of `ContentFactory` in `MyToolWindowFactory`
- Example code — Migrate from the deprecated `FrameStateListener.onFrameActivated()` to `ApplicationActivationListener.applicationActivated(IdeFrame)`

### Removed

- Remove `gradleJvm` property from the `.idea/gradle.xml` file
- GitHub Actions — Drop unused `name` output from the `Build` workflow

## [1.6.0] - 2023-04-13

### Added

- Temporary workaround for Kotlin Compiler `OutOfMemoryError` -> https://jb.gg/intellij-platform-kotlin-oom
- Gradle version catalog integration
- Gradle Kotlin DSL Lazy Property Assignment
- Enable Gradle Build Cache

### Changed

- Dependencies - upgrade `org.jetbrains.intellij` to `1.13.3`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.8.20`
- Upgrade Gradle Wrapper to `8.1`
- Remove `UnusedProperty` suppression in `gradle.properties` file
- Rename `org.gradle.unsafe.configuration-cache` to `org.gradle.configuration-cache` in `gradle.properties`

## [1.5.0] - 2023-03-10

### Added

- Migrate to Gradle Provider API improving configuration cache compatibility
- Example code - `FrameStateListener` application listener
- Example code - `MyToolWindowFactory` tool window basic implementation

### Changed

- Update `platformVersion` to `2022.1.4`
- Change since/until build to `221-231.*` (2022.1-2023.1.*)
- Example code - registered project service changed into a lightweight one
- GitHub Actions — pass changelog release notes as a multi-line content
- GitHub Actions — provide `plugin.verifier.home.dir` variable as a system property instead of project property
- Template Cleanup: remove default `pluginIcon.svg` icon
- Upgrade Gradle Wrapper to `8.0.2`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.8.10`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.13.2`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2022.3.4`
- Dependencies (GitHub Actions) - upgrade `gradle/wrapper-validation-action` to `v1.0.6`

### Fixed

- Resolving the content for the `patchPluginXML.changeNotes` property

### Removed

- Example code — application service
- Example code — deprecated `ProjectManagerListener` application listener

## [1.4.0] - 2023-01-13

### Changed

- GitHub Actions — use `GITHUB_OUTPUT` environment file instead of `::set-output`.
- Upgrade Gradle Wrapper to `7.6`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.8.0`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.12.0`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2022.3.0`

### Fixed

- Fallback to the unreleased change notes when the plugin in current was not released yet

## [1.3.0] - 2022-11-17

### Added

- [Kover](https://github.com/Kotlin/kotlinx-kover) integration
- Enable [Gradle Configuration Cache](https://docs.gradle.org/current/userguide/configuration_cache.html) in `gradle.properties`
- GitHub Actions — mark the pull request created with _Publish Plugin_ workflow with `release changelog` label
- GitHub Actions — send code coverage reports to [CodeCov](https://codecov.io)
- Dependencies - upgrade `org.jetbrains.kotlinx.kover` to `0.6.1`

### Changed

- Update `changelog` extension configuration in `build.gradle.kts` file
- Update `pluginUntilBuild` to include `223.*` (2022.3.*)
- Use `kotlin.jvmToolchain(11)` shorthand in Gradle configuration
- Dependencies - upgrade `org.jetbrains.intellij` to `1.10.0`
- Dependencies - upgrade `org.jetbrains.changelog` to `2.0.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.7.21`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2022.2.3`
- Dependencies (GitHub Actions) - upgrade `gradle/wrapper-validation-action` to `v1.0.5`
- Dependencies (GitHub Actions) - upgrade `jtalk/url-health-check-action` to `v3`
- Use `file` instead of `projectDir.resolve` in Gradle configuration file

### Fixed

- Update broken link in `gradle.properties`
- GitHub Actions — use `$BRANCH` for creating changelog pull request

## [1.2.0] - 2022-08-07

### Added

- Use JVM toolchain for configuring source/target compilation compatibility
- Make sure GitHub Actions release jobs have write permissions
- Example implementation: Add `TODO()` with a hint to remove stale sample code
- Exclude `.qodana` directory from Qodana analysis
- Maximize disk space on GitHub Actions

### Changed

- Upgrade Gradle Wrapper to `7.5.1`
- Update `platformVersion` to `2021.3.3`
- Change since/until build to `213-222.*` (2021.3-2022.2)
- Dependencies - upgrade `org.jetbrains.intellij` to `1.8.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.7.10`
- Dependencies (GitHub Actions) - upgrade `actions/checkout` to `3`
- Dependencies (GitHub Actions) - upgrade `actions/cache` to `3`
- Dependencies (GitHub Actions) - upgrade `actions/setup-java` to `3`
- Dependencies (GitHub Actions) - upgrade `actions/upload-artifact` to `3`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `v2022.2.1`

### Fixed

- Pass Plugin Signing secrets as environment variables in the Release workflow

### Removed

- Removed Gradle caching from GitHub Actions

## [1.1.2] - 2022-02-11

### Changed

- Update `platformVersion` to `2021.1.3` for compatibility with Apple M1
- Change since/until build to `211-213.*` (2021.1-2021.3)
- Upgrade Gradle Wrapper to `7.4`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.4.0`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `4.2.5`

## [1.1.1] - 2022-01-24

### Changed

- GitHub Actions — fixed duplicated `.zip` extension in artifact file's name of the build flow
- Upgrade Gradle Wrapper to `7.3.3`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.3.1`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.6.10`
- Dependencies (GitHub Actions) - upgrade `JetBrains/qodana-action` to `4.2.3`
- Dependencies (GitHub Actions) - upgrade `actions/cache` to `2.1.7`

## [1.1.0] - 2021-11-16

### Added

- GitHub Actions: Collect Qodana/Tests/Plugin Verifier results as artifacts

### Changed

- Dependencies - upgrade `org.jetbrains.intellij` to `1.3.0`
- Dependencies - upgrade `org.jetbrains.changelog` to `1.3.1`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.6.0`
- Dependencies (GitHub Actions) - upgrade `jtalk/url-health-check-action` to `2`
- Dependencies (GitHub Actions) - upgrade `actions/checkout` to `2.3.5`
- GitHub Actions general performance refactoring
- GitHub Actions — prepare plugin archive content to be archived at once
- GitHub Actions — patch changelog only if change notes are provided
- Update `pluginUntilBuild` to include `213.*` (2021.3.*)
- Upgrade Gradle Wrapper to `7.3`

### Fixed

- Fixed passing change notes from `CHANGELOG.md` to the Release Draft
- Fixed passing updated change notes from the Release Draft to `patchChangelog` Gradle task
- Fixed `QODANA_SHOW_REPORT` environment variable resolving for Gradle `6.x`

### Removed

- Removed the `pluginVerifierIdeVersions` configuration to use a default IDEs list provided by the `listProductsReleases` task for `runPluginVerifier`
- Removed `platformDownloadSources` from Gradle configuration to use default value
- Removed `updateSinceUntilBuild.set(true)` from Gradle configuration to use default value

## [1.0.0] - 2021-09-07

### Added

- Plugin Signing
- Qodana integration
- Functional tests
- Compatibility with Java 11
- `Run Qodana` and `Run IDE for UI Tests` run configurations
- Use Gradle `wrapper` task to handle Gradle updates
- JVM compatibility version extracted to `gradle.properties` file
- Suppress `UnusedProperty` inspection for the `kotlin.stdlib.default.dependency` in `gradle.properties`

### Changed

- GitHub Actions: Switch to Java 11
- GitHub Actions: Update Build and Release flows
- GitHub Actions: Use Gradle cache provided with `actions/setup-java`
- Update `pluginVerifierIdeVersions` to `2020.3.4, 2021.1.3, 2021.2.1`
- Change since/until build to `203-212.*`
- Upgrade Gradle Wrapper to `7.2`
- Gradle – Changelog plugin configuration update
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.5.30`
- Dependencies - upgrade `org.jetbrains.changelog` to `1.3.0`
- Dependencies - upgrade `org.jetbrains.intellij` to `1.1.6`
- Dependencies (GitHub Actions) - upgrade `actions/upload-artifact` to `v2.2.4`

### Fixed

- Use `DynamicBundle` instead of `AbstractBundle` in `MyBundle.kt`

### Removed

- Removed `detekt`/`ktlint` integration

## [0.10.1] - 2021-05-31

### Added

- Introduced `next` branch in the root repository to make `main` always a stable one

### Changed

- Dependencies (GitHub Actions) - upgrade `actions/cache` to `v2.1.6`
- Trigger GitHub Actions `Build` workflows only on pushes to `main` branch or pull request to avoid duplicated checks

## [0.10.0] - 2021-05-27

### Changed

- Remove reference to the `jcenter()` from Gradle configuration file
- Update `pluginVerifierIdeVersions` to `2020.2.4, 2020.3.4, 2021.1.2`
- Update `pluginUntilBuild` to include `211.*` (2021.1.*)
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.5.10`
- Dependencies - upgrade `detekt-formatting from` to `1.17.1`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.17.1`
- Dependencies (GitHub Actions) - upgrade `actions/cache` to `v2.1.5`
- Dependencies (GitHub Actions) - upgrade `actions/checkout` to `v2.3.4`
- Dependencies (GitHub Actions) - upgrade `actions/upload-release-asset` to `v1.0.2`
- Dependencies (GitHub Actions) - upgrade `actions/create-release` to `v1.1.4`
- Upgrade Gradle Wrapper to `7.0.2`

## [0.9.0] - 2021-03-29

### Added

- `properties` shorthand function for accessing `gradle.properties` in a cleaner way
- Dependabot check for GitHub Actions used in [workflow files](.github/workflows)

### Changed

- Dependencies - upgrade `detekt-formatting from` to `1.16.0`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.16.0`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.32`
- Dependencies (GitHub Actions) - upgrade `actions/upload-artifact` to `v2.2.2`
- Dependencies (GitHub Actions) - upgrade `actions/cache` to `v2.1.4`

### Fixed

- Fix `README.md` file resolution in the `build.gradle.kts`

## [0.8.3] - 2021-02-23

### Changed

- Dependencies - upgrade `org.jetbrains.intellij` to `0.7.2`
- Dependencies - upgrade `org.jlleitschuh.gradle.ktlint` to `10.0.0`
- Update `platformVersion` to `2020.2.4` for compatibility with macOS Big Sur
- Upgrade Gradle Wrapper to `6.8.3`

## [0.8.2] - 2021-02-09

### Changed

- Use `-bin` distribution of the Gradle Wrapper
- Upgrade Gradle Wrapper to `6.8.2`
- Update `pluginVerifierIdeVersions` in `gradle.properties` files
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.30`
- Dependencies - upgrade `org.jetbrains.changelog` to `1.1.1`
- Configure the `changelog` Gradle plugin

## [0.8.1] - 2021-01-12

### Added

- README: Dependencies management section

### Changed

- Upgrade Gradle Wrapper to `6.8`
- Dependencies - upgrade `org.jetbrains.changelog` to `1.0.0`

### Fixed

- Template Cleanup: Escape GitHub username to avoid incorrect characters in class package name
- Template Cleanup: Run `ktlintFormat` task to fix imports order
- GitHub Actions: Use the correct property in the "Upload artifact" step

## [0.8.0] - 2020-12-21

### Added

- Dependabot integration
- Show `idea.log` logs of the run IDE in the Run console
- README: FAQ section

### Changed

- `build.gradle.kts`: simpler syntax for configuring `KotlinCompile`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.21`
- Dependencies - upgrade `detekt-formatting` to `1.15.0`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.15.0`
- README: Clarifying the Java usage in the project
- `pluginVerifierIdeVersions` - upgrade to `2020.1.4, 2020.2.3, 2020.3.1`

### Fixed

- Return `Supplier<@Nls String>` instead of `String` in `MyBundle.messagePointer`

## [0.7.1] - 2020-12-02

### Changed

- Upgrade Gradle Wrapper to `6.7.1`
- Dependencies - upgrade `org.jetbrains.intellij` to `0.6.5`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.20`
- Update the base platform version to 2020.1
- Change since/until build to `201-203.*`

## [0.7.0] - 2020-11-16

### Added

- Predefined Run/Debug Configurations
- Project icon for development purposes

### Changed

- Dependencies - upgrade `org.jetbrains.intellij` to `0.6.3`

## [0.6.1] - 2020-11-05

### Added

- GitHub Actions — use hash based on `pluginVerifierIdeVersions` in `Setup Plugin Verifier IDEs Cache` step

### Changed

- Use [Kotlin extension function](https://plugins.jetbrains.com/docs/intellij/plugin-services.html#retrieving-a-service) to retrieve the `MyProjectService` in the `MyProjectManagerListener`
- Dependencies - upgrade `org.jetbrains.intellij` to `0.6.2`
- Update `pluginVerifierIdeVersions` in the `gradle.properties` files

## [0.6.0] - 2020-10-29

### Added

- Integration with [IntelliJ Plugin Verifier](https://github.com/JetBrains/intellij-plugin-verifier) through the [Gradle IntelliJ Plugin](https://plugins.jetbrains.com/docs/intellij/tools-gradle-intellij-plugin.html#runpluginverifier-task) `runPluginVerifier` task
- Cache downloaded IDEs used by Plugin Verifier for the verification

### Changed

- Switch Gradle Wrapper to `-all` to improve the IntelliSense
- Update detekt config to be in line with IJ settings
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.14.2`
- Dependencies - upgrade `org.jetbrains.intellij` to `0.6.1`
- GitHub Actions — `gradleValidation` update to `gradle/wrapper-validation-action@v1.0.3`
- GitHub Actions — `releaseDraft` update to `actions/download-artifact@v2`

### Removed

- Remove Third-party IntelliJ Plugin Verifier GitHub Action

## [0.5.1] - 2020-10-15

### Added

- Missing properties in the `gradle.properties` template file

### Changed

- Upgrade Gradle Wrapper to `6.7`
- Dependencies - upgrade `org.jetbrains.changelog` to `0.6.2`

## [0.5.0] - 2020-10-12

### Added

- Introduced `platformPlugins` property in `gradle.properties` for configuring dependencies to bundled/external plugins

### Changed

- Disable "Release Draft" job for pull requests in the "Build" GitHub Actions Workflow
- Dependencies - upgrade `org.jetbrains.intellij` to `0.5.0`
- Dependencies - upgrade `org.jetbrains.changelog` to `0.6.1`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.14.1`
- Dependencies - upgrade `org.jlleitschuh.gradle.ktlint` to `9.4.1`
- Remove LICENSE file during the Template Cleanup workflow

## [0.4.0] - 2020-10-02

### Added

- Fix default to opt-out of bundling Kotlin standard library in plugin distribution

### Changed

- GitHub Actions: allow releasing plugin even for the base project
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.10`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.13.1`

### Fixed

- `pluginName` variable name collision with `intellij` closure getter in Gradle configuration #29

## [0.3.2] - 2020-08-09

### Changed

- Simplify and optimize GitHub Actions
- Gradle Wrapper upgrade to `6.6.1`
- Dependencies - upgrade `org.jetbrains.kotlin.jvm` to `1.4.0`
- Dependencies - upgrade `org.jetbrains.intellij` to `0.4.22`
- Dependencies - upgrade `org.jetbrains.changelog` to `0.5.0`
- Dependencies - upgrade `io.gitlab.arturbosch.detekt` to `1.12.0`
- Dependencies - upgrade `org.jlleitschuh.gradle.ktlint` to `9.4.0`
- Rename `master` branch to `main`

### Fixed

- GitHub Actions — cache Gradle dependencies and wrapper separately

## [0.3.1] - 2020-07-31

### Added

- Better handling of the Gradle plugin description extraction from the README file
- GitHub Actions — cache Gradle Wrapper

### Changed

- Gradle - remove kotlin("stdlib-jdk8") dependency to decrease the plugin artifact size
- Dependencies - bump ktlint to `9.3.0`
- GitHub Actions — make *Update Changelog* job dependent on the *Publish Plugin*
- GitHub Actions — run plugin verifier against `2019.3` `2020.1` `2020.2`

### Fixed

- Resolve ktlint reports
- GitHub Actions — Plugin Verifier broken for artifacts with spaces in name

## [0.3.0] - 2020-07-07

### Added

- Set the publishing channel depending on the plugin version, i.e. `1.0.0-beta` -> `beta` channel

### Changed

- Update `org.jetbrains.changelog` dependency to `v0.3.3`
- Update Gradle Wrapper to `v6.5.1`
- Run GitHub Actions Release workflow on `prereleased` event
- GitHub Actions — Release, separate changelog-related job from the release

### Fixed

- Remove vendor website from `plugin.xml`
- Update Template Cleanup workflow test to avoid running it on forks

## [0.2.0] - 2020-07-02

### Added

- JetBrains Plugin badges and TODO list for the end users
- `ktlint` integration

### Changed

- `pluginUntilBuild` set to the correct format: `201.*`
- Bump detekt dependency to `1.10.0`

### Fixed

- GitHub Actions — Template Cleanup, fixed adding files to git
- Update Template plugin name on cleanup
- Set `buildUponDefaultConfig = true` in detekt configuration

## [0.1.0] - 2020-06-26

### Added

- `settings.gradle.kts` for the [performance purposes](https://docs.gradle.org/current/userguide/organizing_gradle_projects.html#always_define_a_settings_file)
- `#REMOVE-ON-CLEANUP#` token to mark content to be removed with **Template Cleanup** workflow

### Changed

- README proofreading
- GitHub Actions — Update IDE versions for the Plugin Verifier
- Update platformVersion to `2020.1.2`

## [0.0.2] - 2020-06-22

### Added

- [Gradle Changelog Plugin](https://github.com/JetBrains/gradle-changelog-plugin) integration

### Changed

- Bump Detekt version
- Change pluginSinceBuild to 193

## [0.0.1]

### Added

- Initial project scaffold
- GitHub Actions to automate testing and deployment
- Kotlin support

[Unreleased]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v2.1.0...HEAD
[2.1.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v2.0.2...v2.1.0
[2.0.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v2.0.1...v2.0.2
[2.0.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v2.0.0...v2.0.1
[2.0.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.14.2...v2.0.0
[1.14.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.14.1...v1.14.2
[1.14.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.14.0...v1.14.1
[1.14.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.13.0...v1.14.0
[1.13.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.12.0...v1.13.0
[1.12.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.11.3...v1.12.0
[1.11.3]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.11.2...v1.11.3
[1.11.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.11.1...v1.11.2
[1.11.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.11.0...v1.11.1
[1.11.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.10.0...v1.11.0
[1.10.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.9.0...v1.10.0
[1.9.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.8.0...v1.9.0
[1.8.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.6.0...v1.7.0
[1.6.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.1.2...v1.2.0
[1.1.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.1.1...v1.1.2
[1.1.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.10.1...v1.0.0
[0.10.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.8.3...v0.9.0
[0.8.3]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.8.2...v0.8.3
[0.8.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.0.2...v0.1.0
[0.0.2]: https://github.com/JetBrains/intellij-platform-plugin-template/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/JetBrains/intellij-platform-plugin-template/commits/v0.0.1
