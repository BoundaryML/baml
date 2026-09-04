#if defined(_WIN32)
#  define _CRT_SECURE_NO_WARNINGS
#endif
#define BAML_CFFI_BUILD
#include "baml_cffi.h"

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#  include <windows.h>
#else
#  include <pthread.h>
#  include <time.h>
#endif

static uint32_t initialize_count = 0;
static volatile uint32_t initialize_started = 0;
static uint32_t free_count = 0;
static uint32_t register_count = 0;
static BamlResultCallback registered_result_callback = NULL;

static BamlBuffer buffer_from_literal(const char *value) {
  BamlBuffer buffer = {(const int8_t *)value, strlen(value)};
  return buffer;
}

static const char *fixture_version(void) {
  const char *version = getenv("BAML_FAKE_RUNTIME_VERSION");
  return version == NULL ? "missing BAML_FAKE_RUNTIME_VERSION" : version;
}

static BamlBuffer probe_version(void) {
  const char *mode = getenv("BAML_FAKE_NATIVE_MODE");
  if (mode != NULL && strcmp(mode, "version-mismatch") == 0) {
    return buffer_from_literal("9.9.9");
  }
  if (mode != NULL && strcmp(mode, "invalid-version-utf8") == 0) {
    static const uint8_t invalid[] = {UINT8_C(0xff)};
    BamlBuffer buffer = {(const int8_t *)invalid, sizeof(invalid)};
    return buffer;
  }
  if (mode != NULL && strcmp(mode, "null-version-pointer") == 0) {
    BamlBuffer buffer = {NULL, 1};
    return buffer;
  }
  return buffer_from_literal(fixture_version());
}

static void delay_initialize_if_requested(void) {
  const char *raw = getenv("BAML_FAKE_INIT_DELAY_MS");
  if (raw == NULL) {
    return;
  }
  unsigned long milliseconds = strtoul(raw, NULL, 10);
#if defined(_WIN32)
  Sleep((DWORD)milliseconds);
#else
  struct timespec duration = {
      .tv_sec = (time_t)(milliseconds / 1000),
      .tv_nsec = (long)((milliseconds % 1000) * 1000000),
  };
  nanosleep(&duration, NULL);
#endif
}

static BamlBuffer probe_initialize(const uint8_t *bytecode, size_t length) {
  static const uint8_t valid[] = "valid-bytecode";
  initialize_count += 1;
  initialize_started = 1;
  delay_initialize_if_requested();
  if (length == sizeof(valid) - 1 && bytecode != NULL &&
      (memcmp(bytecode, valid, sizeof(valid) - 1) == 0 || memcmp(bytecode, "other-bytecode", sizeof(valid) - 1) == 0)) {
    BamlBuffer buffer = {NULL, 0};
    return buffer;
  }
  return buffer_from_literal("invalid test bytecode");
}

static void probe_free(BamlBuffer buffer) {
  (void)buffer;
  free_count += 1;
}

static void probe_register_result(BamlResultCallback callback) {
  registered_result_callback = callback;
}

static void probe_call(
    const uint8_t *encoded_args,
    size_t length,
    uint32_t callback_id) {
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
  const char *expected_version = fixture_version();
  size_t expected_version_len = strlen(expected_version);
  register_count += 1;
  if (getenv("BAML_FAKE_NATIVE_MODE") != NULL &&
      strcmp(getenv("BAML_FAKE_NATIVE_MODE"), "registration-rejected") == 0) {
    return buffer_from_literal("registration rejected by test fixture");
  }
  if (info == NULL || info->struct_size < sizeof(BamlBridgeInfoV1)) {
    return buffer_from_literal("invalid bridge info");
  }
  if (info->language != BAML_BRIDGE_LANGUAGE_RUBY) {
    return buffer_from_literal("expected Ruby bridge language 10");
  }
  if (info->sdk_version_len != expected_version_len ||
      info->sdk_version == NULL ||
      memcmp(info->sdk_version, expected_version, expected_version_len) != 0) {
    return buffer_from_literal("unexpected Ruby toolchain version");
  }
  static const char expected_name[] = "Baml::Bridge";
  if (info->bridge_runtime_name_len != sizeof(expected_name) - 1 ||
      info->bridge_runtime_name == NULL ||
      memcmp(info->bridge_runtime_name, expected_name, sizeof(expected_name) - 1) != 0) {
    return buffer_from_literal("unexpected Ruby bridge runtime name");
  }
  if (info->bridge_runtime_version_len != expected_version_len ||
      info->bridge_runtime_version == NULL ||
      memcmp(info->bridge_runtime_version, expected_version, expected_version_len) != 0) {
    return buffer_from_literal("unexpected Ruby bridge runtime version");
  }
  BamlBuffer buffer = {NULL, 0};
  return buffer;
}

static void probe_register_unhandled_spawn_error(
    BamlUnhandledSpawnErrorCallback callback) {
  (void)callback;
}

static BamlBuffer probe_shutdown_runtime(void) {
  BamlBuffer buffer = {NULL, 0};
  return buffer;
}

static BamlBuffer probe_initialize_with_metadata(
    const uint8_t *bytecode,
    size_t length,
    const char *baml_toml) {
  (void)baml_toml;
  return probe_initialize(bytecode, length);
}


// Test-only identity/registry; production uses SHA-256 and canonical program bytes.
static BamlBuffer probe_program_key(const uint8_t *bytes, size_t length, uint64_t *key) {
  uint64_t hash = UINT64_C(14695981039346656037);
  for (size_t i = 0; i < length; ++i) { hash ^= bytes[i]; hash *= UINT64_C(1099511628211); }
  *key = hash | (UINT64_C(1) << 63);
  return (BamlBuffer){NULL, 0};
}
static BamlBuffer probe_register_program(uint64_t key, const uint8_t *bytes, size_t length, const char *metadata) {
  (void)metadata;
  static struct { uint64_t key; uint8_t bytes[64]; size_t length; } programs[16];
  static size_t count;
  for (size_t i = 0; i < count; ++i) {
    if (programs[i].key == key) {
      if (programs[i].length == length && memcmp(programs[i].bytes, bytes, length) == 0)
        return (BamlBuffer){NULL, 0};
      return buffer_from_literal("Conflicting BAML program registration");
    }
  }
  if (length > 64 || count == 16) return buffer_from_literal("test registry capacity exceeded");
  BamlBuffer status = probe_initialize(bytes, length);
  if (status.len != 0) return status;
  programs[count].key = key; programs[count].length = length;
  memcpy(programs[count].bytes, bytes, length); ++count;
  return status;
}
static BamlBuffer probe_create_runtime(const uint8_t *bytes, size_t length, uint64_t *key) {
  static uint64_t next = 1;
  BamlBuffer status = probe_initialize(bytes, length);
  if (status.len == 0) *key = next++;
  return status;
}
static BamlBuffer probe_unregister_runtime(uint64_t key) { (void)key; return (BamlBuffer){NULL, 0}; }
static void probe_call_keyed(uint64_t key, const uint8_t *bytes, size_t length, uint32_t callback) {
  (void)key; probe_call(bytes, length, callback);
}

static const BamlApiV1 default_api = {
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
    .register_unhandled_spawn_error_callback = probe_register_unhandled_spawn_error,
    .shutdown_runtime = probe_shutdown_runtime,
    .initialize_runtime_from_bytecode_with_metadata = probe_initialize_with_metadata,
    .register_program = probe_register_program,
    .create_runtime = probe_create_runtime,
    .unregister_runtime = probe_unregister_runtime,
    .call_function_for_runtime = probe_call_keyed,
    .program_key = probe_program_key,
};

static BamlApiV1 probe_api;

#define NULL_FIELD(field_name)                                  \
  if (strcmp(field, #field_name) == 0) {                        \
    probe_api.field_name = NULL;                                \
    return;                                                     \
  }

static void null_requested_field(const char *field) {
  if (field == NULL) {
    return;
  }
  NULL_FIELD(version)
  NULL_FIELD(initialize_runtime_from_bytecode)
  NULL_FIELD(free_buffer)
  NULL_FIELD(register_callback)
  NULL_FIELD(call_function)
  NULL_FIELD(new_function_call)
  NULL_FIELD(cancel_function_call)
  NULL_FIELD(register_host_dispatch_callback)
  NULL_FIELD(register_host_release_callback)
  NULL_FIELD(complete_host_call)
  NULL_FIELD(handle_clone)
  NULL_FIELD(handle_release)
  NULL_FIELD(media_from_url)
  NULL_FIELD(media_from_file)
  NULL_FIELD(media_from_base64)
  NULL_FIELD(media_url)
  NULL_FIELD(media_file)
  NULL_FIELD(media_base64)
  NULL_FIELD(media_mime_type)
  NULL_FIELD(register_bridge)
  NULL_FIELD(register_unhandled_spawn_error_callback)
  NULL_FIELD(shutdown_runtime)
  NULL_FIELD(initialize_runtime_from_bytecode_with_metadata)
  NULL_FIELD(register_program)
  NULL_FIELD(create_runtime)
  NULL_FIELD(unregister_runtime)
  NULL_FIELD(call_function_for_runtime)
  NULL_FIELD(program_key)
}

BAML_CFFI_API const BamlApiV1 *baml_get_api_v1(void) {
  const char *mode = getenv("BAML_FAKE_NATIVE_MODE");
  probe_api = default_api;

  if (mode != NULL && strcmp(mode, "null-table") == 0) {
    return NULL;
  }
  if (mode != NULL && strcmp(mode, "wrong-abi") == 0) {
    probe_api.abi_version = 999;
  } else if (mode != NULL && strcmp(mode, "truncated") == 0) {
    probe_api.struct_size = offsetof(BamlApiV1, register_bridge);
  }
  null_requested_field(getenv("BAML_FAKE_NULL_FIELD"));
  return &probe_api;
}

BAML_CFFI_API uint32_t baml_test_initialize_count(void) {
  return initialize_count;
}

BAML_CFFI_API uint32_t baml_test_initialize_started(void) {
  return initialize_started;
}

BAML_CFFI_API uint32_t baml_test_free_count(void) {
  return free_count;
}

BAML_CFFI_API uint32_t baml_test_register_count(void) {
  return register_count;
}

typedef struct CallbackInvocation {
  uint32_t call_id;
  const uint8_t *content;
  size_t length;
} CallbackInvocation;

#if defined(_WIN32)
static DWORD WINAPI invoke_registered_callback(LPVOID raw_invocation) {
  const CallbackInvocation *invocation = (const CallbackInvocation *)raw_invocation;
  registered_result_callback(
      invocation->call_id,
      (const int8_t *)invocation->content,
      invocation->length);
  return 0;
}
#else
static void *invoke_registered_callback(void *raw_invocation) {
  const CallbackInvocation *invocation = (const CallbackInvocation *)raw_invocation;
  registered_result_callback(
      invocation->call_id,
      (const int8_t *)invocation->content,
      invocation->length);
  return NULL;
}
#endif

BAML_CFFI_API int32_t baml_test_invoke_registered_callback_on_thread(
    uint32_t call_id,
    const uint8_t *content,
    size_t length) {
  if (registered_result_callback == NULL) {
    return 1;
  }
  CallbackInvocation invocation = {call_id, content, length};
#if defined(_WIN32)
  HANDLE thread = CreateThread(NULL, 0, invoke_registered_callback, &invocation, 0, NULL);
  if (thread == NULL) {
    return 2;
  }
  WaitForSingleObject(thread, INFINITE);
  CloseHandle(thread);
#else
  pthread_t thread;
  if (pthread_create(&thread, NULL, invoke_registered_callback, &invocation) != 0) {
    return 2;
  }
  if (pthread_join(thread, NULL) != 0) {
    return 3;
  }
#endif
  return 0;
}
