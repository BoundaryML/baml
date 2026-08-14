// Per-fixture Gradle settings for the sdk_test_java crate.
// `__PROJECT_NAME__` is substituted per fixture by
// `sdk_test_harness_setup::java::codegen_fixture`. The JDK comes from
// the repo-root mise.toml; no toolchain auto-provisioning needed.

rootProject.name = "__PROJECT_NAME__"
