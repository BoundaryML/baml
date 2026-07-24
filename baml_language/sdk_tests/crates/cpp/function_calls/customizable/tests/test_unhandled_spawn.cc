#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdlib>
#include <string>

namespace {

void set_child_environment(bool enabled) {
#ifdef _WIN32
  _putenv_s("BAML_CPP_UNHANDLED_SPAWN_CHILD", enabled ? "1" : "");
  _putenv_s("BAML_TEST_FILTER",
            enabled ? "unhandled_spawn_error_uses_host_default" : "");
#else
  if (enabled) {
    setenv("BAML_CPP_UNHANDLED_SPAWN_CHILD", "1", 1);
    setenv("BAML_TEST_FILTER", "unhandled_spawn_error_uses_host_default", 1);
  } else {
    unsetenv("BAML_CPP_UNHANDLED_SPAWN_CHILD");
    unsetenv("BAML_TEST_FILTER");
  }
#endif
}

}  // namespace

BAML_TEST(unhandled_spawn_error_uses_host_default) {
  if (std::getenv("BAML_CPP_UNHANDLED_SPAWN_CHILD") != nullptr) {
    BAML_ASSERT_EQ(baml_sdk::spawn_unhandled_error(), int64_t{1});
    baml::shutdown_runtime();
    baml_test::fail("unhandled spawn error did not terminate the process");
  }

  set_child_environment(true);
  const std::string command =
      "\"" + std::string(baml_test::executable_path()) + "\"";
  const int status = std::system(command.c_str());
  set_child_environment(false);
  BAML_ASSERT(status != 0);
}
