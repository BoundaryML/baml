#ifndef RUST_CFFI_H
#define RUST_CFFI_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>


/*
 * Extern "C" functions exported from the Rust CFFI layer.
 */

// Creates and returns a pointer to a Baml runtime instance.
const void* create_baml_runtime(const char *root_path, const char *src_files_json, const char *env_vars_json);

// Destroys a previously created Baml runtime instance.
void destroy_baml_runtime(const void *runtime);

/*
 * Calls a function in the Baml runtime.
 *
 * Parameters:
 *  - runtime: a pointer to the runtime (as returned by create_baml_runtime).
 *  - function_name: the name of the function to call (null-terminated string).
 *  - kwargs: pointer to a CKwargs structure containing keyword arguments.
 *  - callback: a function to be called with the result.
 *
 * The callback receives a pointer to a C string (JSON) that must later be freed with free_string.
 */
void call_function_from_c(const void *runtime,
                          const char *function_name,
                          // as JSON string of kwargs
                          const char *kwargs,
                          uint32_t callback_id);

void call_function_stream_from_c(const void *runtime,
                          const char *function_name,
                          // as JSON string of kwargs
                          const char *kwargs,
                          uint32_t callback_id);

typedef void (*callback_fcn)(uint32_t, bool, const int8_t *, int);

// Registers a callback function to be called when a function is called.
void register_callbacks(callback_fcn result_callback, callback_fcn error_callback);

// Invokes the runtime CLI. `args` is a null-terminated array of null-terminated C strings.
void invoke_runtime_cli(const char * const* args);

#ifdef __cplusplus
}
#endif

#endif // RUST_CFFI_H
