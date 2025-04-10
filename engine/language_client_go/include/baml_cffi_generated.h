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

typedef void (*CallbackFn)(uint32_t call_id, bool is_done, const int8_t *content, uintptr_t length);

const char *version(void);

const void *create_baml_runtime(const char *root_path,
                                const char *src_files_json,
                                const char *env_vars_json);

void destroy_baml_runtime(const void *runtime);

int invoke_runtime_cli(const char *const *args);

void register_callbacks(CallbackFn callback_fn, CallbackFn error_callback_fn);

/**
 * Extern "C" function that returns immediately, scheduling the async call.
 * Once the asynchronous function completes, the provided callback is invoked.
 */
const void *call_function_from_c(const void *runtime,
                                 const char *function_name,
                                 const char *encoded_args,
                                 uintptr_t length,
                                 uint32_t id);

/**
 * Extern "C" function that returns immediately, scheduling the async call.
 * Once the asynchronous function completes, the provided callback is invoked.
 */
const void *call_function_stream_from_c(const void *runtime,
                                        const char *function_name,
                                        const char *encoded_args,
                                        uintptr_t length,
                                        uint32_t id);
