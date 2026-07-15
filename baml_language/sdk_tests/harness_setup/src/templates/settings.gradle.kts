// Per-fixture Gradle settings for the sdk_test_java crate.
// `__PROJECT_NAME__` is substituted per fixture by
// `sdk_test_harness_setup::java::codegen_fixture`.
//
// The foojay resolver lets Gradle download a matching JDK when the
// toolchain requested in build.gradle.kts isn't installed locally.
plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "0.8.0"
}

rootProject.name = "__PROJECT_NAME__"
