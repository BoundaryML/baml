#define BAML_CFFI_BUILD
#include "baml_cffi.h"

#include <stdlib.h>
#include <string.h>

static BamlBuffer probe_version(void) {
  const char *mode = getenv("BAML_FAKE_NATIVE_MODE");
  const char *version =
      mode != NULL && strcmp(mode, "version-mismatch") == 0 ? "9.9.9" : "0.15.0";
  BamlBuffer buffer = {(const int8_t *)version, strlen(version)};
  return buffer;
}

static BamlBuffer probe_initialize(const uint8_t *bytecode, size_t length) {
  (void)bytecode;
  (void)length;
  BamlBuffer buffer = {NULL, 0};
  return buffer;
}

static void probe_free(BamlBuffer buffer) {
  (void)buffer;
}

static void probe_register_result(BamlResultCallback callback) {
  (void)callback;
}

static void probe_call(
    const char *function_name,
    const uint8_t *encoded_args,
    size_t length,
    uint32_t callback_id) {
  (void)function_name;
  (void)encoded_args;
  (void)length;
  (void)callback_id;
}

static uint64_t probe_new_call(void) {
  return 1;
}

static int32_t probe_cancel(uint64_t id) {
  return id == 0 ? 1 : 0;
}

static void probe_register_host_dispatch(BamlHostDispatchCallback callback) {
  (void)callback;
}

static void probe_register_host_release(BamlHostReleaseCallback callback) {
  (void)callback;
}

static void probe_complete_host_call(
    uint32_t call_id,
    int32_t is_error,
    const int8_t *content,
    size_t length) {
  (void)call_id;
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
    int32_t media_kind,
    const char *value,
    const char *mime_type_or_null,
    uint64_t *out_key,
    int32_t *out_handle_type) {
  (void)media_kind;
  (void)value;
  (void)mime_type_or_null;
  (void)out_key;
  (void)out_handle_type;
  return BAML_CFFI_STATUS_UNSUPPORTED_HANDLE_TYPE;
}

static BamlCffiStatus probe_media_accessor(
    uint64_t key,
    int32_t handle_type,
    BamlBuffer *out) {
  (void)key;
  (void)handle_type;
  (void)out;
  return BAML_CFFI_STATUS_INVALID_HANDLE;
}

static BamlBuffer probe_register_bridge(const BamlBridgeInfoV1 *info) {
  (void)info;
  BamlBuffer buffer = {NULL, 0};
  return buffer;
}

static BamlApiV1 probe_api = {
    .abi_version = BAML_API_V1_ABI_VERSION,
    .struct_size = sizeof(BamlApiV1),
    .version = probe_version,
    .initialize_runtime_from_bytecode = probe_initialize,
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
};

const BamlApiV1 *baml_get_api_v1(void) {
  const char *mode = getenv("BAML_FAKE_NATIVE_MODE");
  probe_api.abi_version = BAML_API_V1_ABI_VERSION;
  probe_api.struct_size = sizeof(BamlApiV1);
  probe_api.register_bridge = probe_register_bridge;

  if (mode != NULL && strcmp(mode, "null-table") == 0) {
    return NULL;
  }
  if (mode != NULL && strcmp(mode, "wrong-abi") == 0) {
    probe_api.abi_version = 999;
  } else if (mode != NULL && strcmp(mode, "truncated") == 0) {
    probe_api.struct_size = offsetof(BamlApiV1, register_bridge);
  } else if (mode != NULL && strcmp(mode, "missing-field") == 0) {
    probe_api.register_bridge = NULL;
  }

  return &probe_api;
}
