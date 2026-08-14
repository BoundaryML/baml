#include "abi_assertions.h"

#include <stdio.h>
#include <string.h>

#if defined(_WIN32)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
typedef HMODULE BamlLibrary;

static BamlLibrary baml_open_library(const char *path) {
  return LoadLibraryA(path);
}

static int baml_load_getter(BamlLibrary library, BamlGetApiV1Fn *out) {
  FARPROC symbol = GetProcAddress(library, "baml_get_api_v1");
  if (symbol == NULL) {
    fprintf(stderr, "GetProcAddress(baml_get_api_v1) failed: %lu\n", (unsigned long)GetLastError());
    return 0;
  }
  BAML_STATIC_ASSERT(sizeof(symbol) == sizeof(*out), "Windows function pointer size mismatch");
  memcpy(out, &symbol, sizeof(*out));
  return 1;
}

static void baml_close_library(BamlLibrary library) {
  FreeLibrary(library);
}
#else
#  include <dlfcn.h>
typedef void *BamlLibrary;

static BamlLibrary baml_open_library(const char *path) {
  return dlopen(path, RTLD_NOW | RTLD_LOCAL);
}

static int baml_load_getter(BamlLibrary library, BamlGetApiV1Fn *out) {
  dlerror();
  void *symbol = dlsym(library, "baml_get_api_v1");
  const char *error = dlerror();
  if (error != NULL || symbol == NULL) {
    fprintf(stderr, "dlsym(baml_get_api_v1) failed: %s\n", error == NULL ? "not found" : error);
    return 0;
  }
  BAML_STATIC_ASSERT(sizeof(symbol) == sizeof(*out), "POSIX function pointer size mismatch");
  memcpy(out, &symbol, sizeof(*out));
  return 1;
}

static void baml_close_library(BamlLibrary library) {
  dlclose(library);
}
#endif

static int baml_required_functions_exist(const BamlApiV1 *api) {
  return api->version != NULL && api->initialize_runtime_from_bytecode != NULL &&
         api->free_buffer != NULL && api->register_callback != NULL &&
         api->call_function != NULL && api->new_function_call != NULL &&
         api->cancel_function_call != NULL && api->register_host_dispatch_callback != NULL &&
         api->register_host_release_callback != NULL && api->complete_host_call != NULL &&
         api->handle_clone != NULL && api->handle_release != NULL && api->media_from_url != NULL &&
         api->media_from_file != NULL && api->media_from_base64 != NULL && api->media_url != NULL &&
         api->media_file != NULL && api->media_base64 != NULL && api->media_mime_type != NULL &&
         api->register_bridge != NULL;
}

static int baml_test_evolution_checks(const BamlApiV1 *api) {
  BamlApiV1 synthetic = *api;
  synthetic.struct_size = BAML_API_V1_MIN_SIZE - 1;
  if (baml_api_v1_is_compatible(&synthetic)) {
    fprintf(stderr, "truncated V1 table was accepted\n");
    return 0;
  }
  synthetic.struct_size = BAML_API_V1_MIN_SIZE + sizeof(void *);
  if (!baml_api_v1_is_compatible(&synthetic)) {
    fprintf(stderr, "larger append-only V1 table was rejected\n");
    return 0;
  }
  synthetic.abi_version = BAML_API_V1_ABI_VERSION + 1;
  if (baml_api_v1_is_compatible(&synthetic)) {
    fprintf(stderr, "unknown ABI version was accepted as V1\n");
    return 0;
  }
  return 1;
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: abi_smoke <bridge_cffi library>\n");
    return 2;
  }

  BamlLibrary library = baml_open_library(argv[1]);
  if (library == NULL) {
#if defined(_WIN32)
    fprintf(stderr, "LoadLibrary failed: %lu\n", (unsigned long)GetLastError());
#else
    fprintf(stderr, "dlopen failed: %s\n", dlerror());
#endif
    return 3;
  }

  BamlGetApiV1Fn get_api = NULL;
  if (!baml_load_getter(library, &get_api)) {
    baml_close_library(library);
    return 4;
  }
  const BamlApiV1 *api = get_api();
  if (!baml_api_v1_is_compatible(api)) {
    fprintf(stderr, "runtime returned a null, version-mismatched, or truncated V1 table\n");
    baml_close_library(library);
    return 5;
  }
  if (!baml_required_functions_exist(api) || !baml_test_evolution_checks(api)) {
    baml_close_library(library);
    return 6;
  }

  BamlBuffer version = api->version();
  if (version.ptr == NULL || version.len == 0) {
    fprintf(stderr, "version returned an empty buffer\n");
    api->free_buffer(version);
    baml_close_library(library);
    return 7;
  }
  if (fwrite(version.ptr, 1, version.len, stdout) != version.len || fputc('\n', stdout) == EOF) {
    fprintf(stderr, "failed to write version bytes\n");
    api->free_buffer(version);
    baml_close_library(library);
    return 8;
  }
  api->free_buffer(version);
  baml_close_library(library);
  return 0;
}
