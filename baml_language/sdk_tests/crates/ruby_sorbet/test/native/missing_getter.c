#if defined(_WIN32)
#  define TEST_EXPORT __declspec(dllexport)
#else
#  define TEST_EXPORT __attribute__((visibility("default")))
#endif

TEST_EXPORT int baml_test_not_the_api(void) {
  return 1;
}
