// Stdlib entry-point contract.
// Port of function_calls/customizable/test_stdlib_entrypoints.py. The
// intrinsics check greps the single generated header and bindings.cc
// instead of Python's per-module files; the forbidden wire names are the
// same.
#include <baml_sdk.h>
#include <baml_test.h>

#include <fstream>
#include <sstream>
#include <string>
#include <vector>

// Intrinsic-only modules are not emitted at all, so a missing file is fine;
// callers only need to confirm the symbol is absent when the file exists.
// Paths are relative to the fixture's generated/ dir, where test.sh runs the
// binary.
static std::string GeneratedSdkFile(const std::string& rel_path) {
  std::ifstream in(rel_path);
  if (!in) {
    return std::string();
  }
  std::ostringstream buf;
  buf << in.rdbuf();
  return buf.str();
}

// `baml.sys.argv() -> string[]` is a `$rust_function` ->
// `FunctionKind::Native`. Calling it as an entry point should run the native
// and return the argument array, not reject with `NotInvokableAsEntry`. The
// fixture host passes no program arguments, so the contents are not worth
// asserting on — that the call lands and is stable across invocations is.
BAML_TEST(stdlib_entrypoints_native_argv_callable_as_entry_point) {
  const std::vector<std::string> args = baml_sdk::baml::sys::argv();
  BAML_ASSERT(args == baml_sdk::baml::sys::argv());
}

// `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` ->
// `FunctionKind::SysOp`. Calling it as an entry point should run the
// filesystem sysop and return a bool. `.` exists in the generated fixture
// directory on the test host.
BAML_TEST(stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point) {
  BAML_ASSERT(baml_sdk::baml::fs::exists(".") == true);
}

BAML_TEST(
    stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points) {
  const char* files[] = {
      "baml_sdk/include/baml_sdk.h",
      "baml_sdk/src/bindings.cc",
  };
  // The entry-point registration strings are the precise signal, and they are
  // what the other language probes assert on (python/java/ts scope the check to
  // the generated `baml/events/` path). `namespace log {` is kept as a
  // structural backstop because no non-intrinsic `log` package is generated.
  //
  // `namespace events {` is NOT a valid probe: `ai.events` is an ordinary
  // stdlib namespace (the journal event catalog), so it generates a legitimate
  // `namespace events {` that has nothing to do with the `baml.events`
  // intrinsic this test guards against.
  const char* forbidden[] = {
      "\"log.info\"",  "\"log.debug\"",        "\"log.warn\"",
      "\"log.error\"", "\"baml.events.send\"", "namespace log {",
  };
  bool saw_any_file = false;
  for (const char* file : files) {
    const std::string contents = GeneratedSdkFile(file);
    if (contents.empty()) {
      continue;
    }
    saw_any_file = true;
    for (const char* snippet : forbidden) {
      BAML_ASSERT(contents.find(snippet) == std::string::npos);
    }
  }
  // The generated SDK must be where test.sh put it; an empty read on both
  // files means the path assumption broke, not that intrinsics are absent.
  BAML_ASSERT(saw_any_file);
}
