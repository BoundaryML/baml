#if defined(_WIN32)
#define BAML_PROBE_EXPORT __declspec(dllexport)
#else
#define BAML_PROBE_EXPORT __attribute__((visibility("default")))
#endif

BAML_PROBE_EXPORT int baml_probe_not_the_canonical_getter(void) {
  return 1;
}
