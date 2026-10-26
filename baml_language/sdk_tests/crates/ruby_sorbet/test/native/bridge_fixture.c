#if defined(_WIN32)
#  define _CRT_SECURE_NO_WARNINGS
#endif
#define BAML_CFFI_BUILD
#include "baml_cffi.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static uint32_t release_count = 0;

static BamlBuffer empty_buffer(void) {
  BamlBuffer buffer = {NULL, 0};
  return buffer;
}

static BamlBuffer probe_version(void) {
  const char *version = getenv("BAML_FAKE_RUNTIME_VERSION");
  BamlBuffer buffer = {(const int8_t *)version, strlen(version)};
  return buffer;
}

static BamlBuffer probe_initialize(const uint8_t *bytecode, size_t length) {
  (void)bytecode;
  (void)length;
  return empty_buffer();
}

static void probe_free(BamlBuffer buffer) { (void)buffer; }
static void probe_register_result(BamlResultCallback callback) { (void)callback; }

static void probe_call(const uint8_t *args, size_t length, uint32_t callback_id) {
  (void)args;
  (void)length;
  (void)callback_id;
}

static uint64_t probe_new_call(void) { return 1; }
static int32_t probe_cancel(uint64_t id) { return id == 0 ? 1 : 0; }

static int32_t probe_release_call(uint64_t id) {
  (void)id;
  release_count += 1;
  return 0;
}

static void probe_register_host_dispatch(BamlHostDispatchCallback callback) { (void)callback; }
static void probe_register_host_release(BamlHostReleaseCallback callback) { (void)callback; }

static void probe_complete_host_call(uint32_t id, int32_t is_error, const int8_t *content, size_t length) {
  (void)id;
  (void)is_error;
  (void)content;
  (void)length;
}

static BamlCffiStatus probe_clone(uint64_t key, uint64_t *out_key) {
  (void)key;
  (void)out_key;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlCffiStatus probe_release(uint64_t key) {
  (void)key;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlCffiStatus probe_media_constructor(
    int32_t kind, const char *value, const char *mime, uint64_t *key, int32_t *type) {
  (void)kind;
  (void)value;
  (void)mime;
  (void)key;
  (void)type;
  return BAML_CFFI_STATUS_UNSUPPORTED_HANDLE_TYPE;
}

static BamlCffiStatus probe_media_accessor(uint64_t key, int32_t type, BamlBuffer *out) {
  (void)key;
  (void)type;
  (void)out;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlBuffer probe_register_bridge(const BamlBridgeInfoV1 *info) {
  (void)info;
  return empty_buffer();
}

static void probe_register_unhandled_spawn_error(BamlUnhandledSpawnErrorCallback callback) { (void)callback; }

static BamlBuffer probe_initialize_with_metadata(const uint8_t *bytecode, size_t length, const char *metadata) {
  (void)metadata;
  return probe_initialize(bytecode, length);
}

static BamlCffiStatus probe_clock(uint64_t id, uint64_t *out) {
  (void)id;
  *out = 0;
  return BAML_CFFI_STATUS_OK;
}

static void probe_dispatch_v2(BamlHostDispatchV2 callback) { (void)callback; }
static void probe_cancel_callback(BamlHostCancel callback) { (void)callback; }
static uint32_t probe_protocol(void) { return 1; }

static BamlCffiStatus probe_trace_selection(uint64_t id, uint64_t key, BamlBuffer *out, uint64_t *reservation) {
  (void)id;
  (void)key;
  (void)out;
  (void)reservation;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlCffiStatus probe_invocation_context(uint64_t key, BamlBuffer *out) {
  (void)key;
  (void)out;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlCffiStatus probe_media_from_file_content(
    int32_t kind, const char *file, const char *base64, const char *mime, uint64_t *key, int32_t *type) {
  (void)file;
  return probe_media_constructor(kind, base64, mime, key, type);
}

static const BamlApiV1 default_api = {
    .abi_version = BAML_API_V1_ABI_VERSION,
    .struct_size = sizeof(BamlApiV1),
    .version = probe_version,
    .initialize_runtime_from_blob = probe_initialize,
    .free_buffer = probe_free,
    .register_callback = probe_register_result,
    .call_function = probe_call,
    .new_function_call = probe_new_call,
    .cancel_function_call = probe_cancel,
    .register_host_dispatch_callback = probe_register_host_dispatch,
    .register_host_release_callback = probe_register_host_release,
    .complete_host_call = probe_complete_host_call,
    .handle_clone = probe_clone,
    .handle_release = probe_release,
    .media_from_url = probe_media_constructor,
    .media_from_file = probe_media_constructor,
    .media_from_base64 = probe_media_constructor,
    .media_url = probe_media_accessor,
    .media_file = probe_media_accessor,
    .media_base64 = probe_media_accessor,
    .media_mime_type = probe_media_accessor,
    .register_bridge = probe_register_bridge,
    .register_unhandled_spawn_error_callback = probe_register_unhandled_spawn_error,
    .shutdown_runtime = empty_buffer,
    .initialize_runtime_from_blob_with_metadata = probe_initialize_with_metadata,
    .invocation_clock_ns = probe_clock,
    .release_function_call = probe_release_call,
    .register_host_dispatch_v2 = probe_dispatch_v2,
    .register_host_cancel_callback = probe_cancel_callback,
    .invocation_protocol_version = probe_protocol,
    .trace_selection = probe_trace_selection,
    .invocation_context = probe_invocation_context,
    .media_name = probe_media_accessor,
    .media_from_file_content = probe_media_from_file_content,
};

BAML_CFFI_API const BamlApiV1 *baml_get_api_v1(void) {
  static BamlApiV1 api;
  const char *mode = getenv("BAML_FAKE_NATIVE_MODE");
  api = default_api;
  if (mode != NULL && strcmp(mode, "truncated") == 0) {
    api.struct_size = offsetof(BamlApiV1, media_from_file_content);
  } else if (mode != NULL && strcmp(mode, "null_function") == 0) {
    api.media_from_file_content = NULL;
  }
  return &api;
}

BAML_CFFI_API uint32_t baml_test_release_count(void) { return release_count; }
