#include "abi_assertions.h"

#include <stdio.h>

#define PRINT_SIZE(type) printf("size." #type "=%zu\n", sizeof(type))
#define PRINT_ALIGN(type) printf("align." #type "=%zu\n", (size_t)BAML_ALIGNOF(type))
#define PRINT_OFFSET(type, field) printf("offset." #type "." #field "=%zu\n", offsetof(type, field))

static void BAML_CFFI_CALL expected_result_callback(
    uint32_t call_id, const int8_t *content, size_t length) {
  (void)call_id;
  (void)content;
  (void)length;
}

static void BAML_CFFI_CALL expected_host_dispatch_callback(
    uint64_t host_value_key, uint32_t call_id, const uint8_t *args, size_t length) {
  (void)host_value_key;
  (void)call_id;
  (void)args;
  (void)length;
}

static void BAML_CFFI_CALL expected_host_release_callback(uint64_t host_value_key) {
  (void)host_value_key;
}

static void BAML_CFFI_CALL expected_unhandled_spawn_error_callback(
    const int8_t *content, size_t length, int32_t cancelled) {
  (void)content;
  (void)length;
  (void)cancelled;
}

int main(void) {
  BamlResultCallback result_callback = expected_result_callback;
  BamlHostDispatchCallback dispatch_callback = expected_host_dispatch_callback;
  BamlHostReleaseCallback release_callback = expected_host_release_callback;
  BamlUnhandledSpawnErrorCallback unhandled_spawn_error_callback =
      expected_unhandled_spawn_error_callback;
  (void)result_callback;
  (void)dispatch_callback;
  (void)release_callback;
  (void)unhandled_spawn_error_callback;
  PRINT_SIZE(BamlCffiStatus);
  PRINT_ALIGN(BamlCffiStatus);
  PRINT_SIZE(BamlBridgeLanguage);
  PRINT_ALIGN(BamlBridgeLanguage);
  PRINT_SIZE(BamlCffiMediaKind);
  PRINT_ALIGN(BamlCffiMediaKind);
  PRINT_SIZE(BamlCffiHandleType);
  PRINT_ALIGN(BamlCffiHandleType);
  PRINT_SIZE(BamlBuffer);
  PRINT_ALIGN(BamlBuffer);
  PRINT_OFFSET(BamlBuffer, ptr);
  PRINT_OFFSET(BamlBuffer, len);
  PRINT_SIZE(BamlBridgeInfoV1);
  PRINT_ALIGN(BamlBridgeInfoV1);
  PRINT_OFFSET(BamlBridgeInfoV1, struct_size);
  PRINT_OFFSET(BamlBridgeInfoV1, language);
  PRINT_OFFSET(BamlBridgeInfoV1, sdk_version);
  PRINT_OFFSET(BamlBridgeInfoV1, sdk_version_len);
  PRINT_OFFSET(BamlBridgeInfoV1, bridge_runtime_name);
  PRINT_OFFSET(BamlBridgeInfoV1, bridge_runtime_name_len);
  PRINT_OFFSET(BamlBridgeInfoV1, bridge_runtime_version);
  PRINT_OFFSET(BamlBridgeInfoV1, bridge_runtime_version_len);
  PRINT_SIZE(BamlApiV1);
  PRINT_ALIGN(BamlApiV1);
  PRINT_OFFSET(BamlApiV1, abi_version);
  PRINT_OFFSET(BamlApiV1, struct_size);
  PRINT_OFFSET(BamlApiV1, version);
  PRINT_OFFSET(BamlApiV1, initialize_runtime_from_bytecode);
  PRINT_OFFSET(BamlApiV1, free_buffer);
  PRINT_OFFSET(BamlApiV1, register_callback);
  PRINT_OFFSET(BamlApiV1, call_function);
  PRINT_OFFSET(BamlApiV1, new_function_call);
  PRINT_OFFSET(BamlApiV1, cancel_function_call);
  PRINT_OFFSET(BamlApiV1, register_host_dispatch_callback);
  PRINT_OFFSET(BamlApiV1, register_host_release_callback);
  PRINT_OFFSET(BamlApiV1, complete_host_call);
  PRINT_OFFSET(BamlApiV1, handle_clone);
  PRINT_OFFSET(BamlApiV1, handle_release);
  PRINT_OFFSET(BamlApiV1, media_from_url);
  PRINT_OFFSET(BamlApiV1, media_from_file);
  PRINT_OFFSET(BamlApiV1, media_from_base64);
  PRINT_OFFSET(BamlApiV1, media_url);
  PRINT_OFFSET(BamlApiV1, media_file);
  PRINT_OFFSET(BamlApiV1, media_base64);
  PRINT_OFFSET(BamlApiV1, media_mime_type);
  PRINT_OFFSET(BamlApiV1, register_bridge);
  PRINT_OFFSET(BamlApiV1, register_unhandled_spawn_error_callback);
  PRINT_OFFSET(BamlApiV1, shutdown_runtime);
  PRINT_OFFSET(BamlApiV1, initialize_runtime_from_bytecode_with_metadata);
  return 0;
}
