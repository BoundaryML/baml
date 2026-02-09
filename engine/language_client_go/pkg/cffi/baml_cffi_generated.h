/* Generated with cbindgen:0.28.0 */

/* DO NOT MODIFY THIS MANUALLY! This file was generated using cbindgen.
 * To generate this file:
 *   1. Get the latest cbindgen using `cargo install --force cbindgen`
 *   2. Run `cargo build`
 */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef void (*CallbackFn)(uint32_t call_id,
                           int32_t is_done,
                           const int8_t *content,
                           uintptr_t length);

typedef void (*OnTickCallbackFn)(uint32_t call_id);

typedef struct Buffer {
  const int8_t *ptr;
  size_t len;
} Buffer;

void register_callbacks(CallbackFn callback_fn,
                        CallbackFn error_callback_fn,
                        OnTickCallbackFn on_tick_callback_fn);

/**
 * Extern "C" function that returns immediately, scheduling the async call.
 * Once the asynchronous function completes, the provided callback is invoked.
 * Returns Buffer with InvocationResponse (empty on success, error message on failure).
 * Caller must free with free_buffer().
 */
struct Buffer call_function_from_c(const void *runtime,
                                   const char *function_name,
                                   const char *encoded_args,
                                   uintptr_t length,
                                   uint32_t id);

/**
 * Returns Buffer with InvocationResponse (empty on success, error message on failure).
 * Caller must free with free_buffer().
 */
struct Buffer call_function_parse_from_c(const void *runtime,
                                         const char *function_name,
                                         const char *encoded_args,
                                         uintptr_t length,
                                         uint32_t id);

/**
 * Extern "C" function that returns immediately, scheduling the async call.
 * Once the asynchronous function completes, the provided callback is invoked.
 * Returns Buffer with InvocationResponse (empty on success, error message on failure).
 * Caller must free with free_buffer().
 */
struct Buffer call_function_stream_from_c(const void *runtime,
                                          const char *function_name,
                                          const char *encoded_args,
                                          uintptr_t length,
                                          uint32_t id);

/**
 * Extern "C" function that returns immediately, scheduling the async build_request call.
 * Once the asynchronous function completes, the provided callback is invoked with an
 * InvocationResponse containing a BamlObjectHandle (http_request pointer).
 * Returns Buffer with InvocationResponse (empty on success, error message on failure).
 * Caller must free with free_buffer().
 */
struct Buffer build_request_from_c(const void *runtime,
                                   const char *function_name,
                                   const char *encoded_args,
                                   uintptr_t length,
                                   uint32_t id);

/**
 * Cancel a function call by its ID
 * Returns Buffer with InvocationResponse (empty = success).
 * Caller must free with free_buffer().
 */
struct Buffer cancel_function_call(uint32_t id);

struct Buffer call_object_constructor(const char *encoded_args, uintptr_t length);

void free_buffer(struct Buffer buf);

struct Buffer call_object_method(const void *runtime, const char *encoded_args, uintptr_t length);

/**
 * Returns the BAML version as a Buffer containing raw UTF-8 bytes.
 * Caller must free with free_buffer().
 */
struct Buffer version(void);

const void *create_baml_runtime(const char *root_path,
                                const char *src_files_json,
                                const char *env_vars_json);

void destroy_baml_runtime(const void *runtime);

int invoke_runtime_cli(const char *const *args);
