#ifndef BAML_CFFI_H
#define BAML_CFFI_H

/* Generated with cbindgen:0.29.4 */

/* DO NOT MODIFY THIS MANUALLY! This file was generated using cbindgen.
 * To regenerate:
 *   BLESS=1 cargo test -p bridge_cffi --test header_is_current
 */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Status returned by the handle C ABI.
 */
enum BamlCffiStatus
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint32_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  BamlCffiStatus_Ok = 0,
  BamlCffiStatus_InvalidHandle = 1,
  BamlCffiStatus_TypeMismatch = 2,
  BamlCffiStatus_UnsupportedHandleType = 3,
  BamlCffiStatus_InternalError = 4,
  BamlCffiStatus_UnexpectedNullptr = 5,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum BamlCffiStatus BamlCffiStatus;
#else
typedef uint32_t BamlCffiStatus;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

/**
 * Callback signature: (call_id, content, length).
 *
 * `content` is always a protobuf-encoded `BamlOutboundResult` envelope —
 * carrying the ok value, a thrown error, a panic, or a synthesized pre-call
 * host-boundary failure. There is no separate error channel.
 */
typedef void (*CallbackFn)(uint32_t call_id, const int8_t *content, uintptr_t length);

/**
 * Buffer type for returning data across FFI boundary.
 * Caller must free with `free_buffer()`.
 *
 * This matches the Buffer struct expected by baml-sys.
 */
typedef struct Buffer {
  const int8_t *ptr;
  uintptr_t len;
} Buffer;

/**
 * C-compatible dispatch callback installed by the host bridge.
 *
 * Called by `call_host_value` when BAML code invokes a `HostValue`. The
 * bridge decodes `args`, invokes the host callable, and resolves the
 * in-flight call via `complete_host_call`.
 *
 * ## Contract (upheld by the bridges)
 *
 * * **Complete exactly once.** Every dispatched `call_id` must be resolved by
 *   exactly one `complete_host_call` (success or error), on every exit path —
 *   including host-side exceptions and panics. There is no engine-side timeout
 *   (see "In-flight lifetime" above), so a call that is never completed and
 *   never cancelled hangs the issuing BAML call indefinitely.
 * * **No synchronous re-entrancy.** A host callable must not synchronously
 *   re-enter the engine (e.g. issue a *blocking* BAML call from inside the
 *   callback). Dispatch is fire-and-return and the engine awaits completion,
 *   so a blocking re-entrant call would deadlock the thread that must service
 *   this dispatch. The bridges fast-fail the narrow case of passing a callable
 *   to the synchronous call path; broader sync re-entrancy is unsupported.
 */
typedef void (*HostDispatchFn)(uint64_t host_value_key,
                               uint32_t call_id,
                               const uint8_t *args,
                               uintptr_t length);

/**
 * Drop-on-last-clone notification fired to the host language.
 */
typedef void (*HostReleaseFn)(uint64_t host_value_key);

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Call a BAML function asynchronously.
 *
 * Returns immediately after spawning the async task.
 * Result/error is delivered via the registered callback as a
 * `BamlOutboundResult` envelope — including pre-call host-boundary failures,
 * which are synthesized into the envelope via [`error_to_outbound`].
 */
void call_function(const char *function_name,
                   const uint8_t *encoded_args,
                   uintptr_t length,
                   uint32_t id);

/**
 * Allocate a new process-unique function-call ID.
 */
uint64_t new_function_call(void);

/**
 * Cancel an in-flight function call.
 *
 * Returns 0 on success, 1 if the call ID is unknown or already completed.
 */
int32_t cancel_function_call(uint64_t id);

void register_callback(CallbackFn callback_fn);

/**
 * Clone a handle, creating a new owned key pointing to the same underlying value.
 *
 * # Safety
 * `out_key` must be either null or valid for writing one `u64`.
 */
BamlCffiStatus baml_handle_clone(uint64_t key, uint64_t *out_key);

/**
 * Release one owned handle key.
 *
 * # Safety
 * The caller must pass a key previously returned by this CFFI handle API or
 * accept an `InvalidHandle` status.
 */
BamlCffiStatus baml_handle_release(uint64_t key);

/**
 * # Safety
 * `out_key` and `out_handle_type` must be either null or valid for writing
 * one value of their pointee type.
 */
BamlCffiStatus __testonly_seed_function_ref(uint64_t global_index,
                                            uint64_t *out_key,
                                            int32_t *out_handle_type);

/**
 * # Safety
 * `out_key` and `out_handle_type` must be either null or valid for writing
 * one value of their pointee type.
 */
BamlCffiStatus __testonly_seed_generic_media(uint64_t *out_key, int32_t *out_handle_type);

/**
 * # Safety
 * `url` and `mime_type_or_null`, when non-null, must point to valid
 * NUL-terminated C strings. `out_key` and `out_handle_type` must be either
 * null or valid for writing one value of their pointee type.
 */
BamlCffiStatus baml_media_from_url(int32_t media_kind,
                                   const char *url,
                                   const char *mime_type_or_null,
                                   uint64_t *out_key,
                                   int32_t *out_handle_type);

/**
 * # Safety
 * `path` and `mime_type_or_null`, when non-null, must point to valid
 * NUL-terminated C strings. `out_key` and `out_handle_type` must be either
 * null or valid for writing one value of their pointee type.
 */
BamlCffiStatus baml_media_from_file(int32_t media_kind,
                                    const char *path,
                                    const char *mime_type_or_null,
                                    uint64_t *out_key,
                                    int32_t *out_handle_type);

/**
 * # Safety
 * `base64` and `mime_type_or_null`, when non-null, must point to valid
 * NUL-terminated C strings. `out_key` and `out_handle_type` must be either
 * null or valid for writing one value of their pointee type.
 */
BamlCffiStatus baml_media_from_base64(int32_t media_kind,
                                      const char *base64,
                                      const char *mime_type_or_null,
                                      uint64_t *out_key,
                                      int32_t *out_handle_type);

/**
 * # Safety
 * `out` must be either null or valid for writing one `Buffer`.
 */
BamlCffiStatus baml_media_url(uint64_t key, int32_t handle_type, struct Buffer *out);

/**
 * # Safety
 * `out` must be either null or valid for writing one `Buffer`.
 */
BamlCffiStatus baml_media_file(uint64_t key, int32_t handle_type, struct Buffer *out);

/**
 * # Safety
 * `out` must be either null or valid for writing one `Buffer`.
 */
BamlCffiStatus baml_media_base64(uint64_t key, int32_t handle_type, struct Buffer *out);

/**
 * # Safety
 * `out` must be either null or valid for writing one `Buffer`.
 */
BamlCffiStatus baml_media_mime_type(uint64_t key, int32_t handle_type, struct Buffer *out);

/**
 * Register the host dispatch callback. First call wins; subsequent calls
 * are silently ignored (consistent with `register_callback` semantics).
 *
 * Delegates to `sys_native::host_dispatch::set_dispatch_fn` so the
 * `call_host_value` sysop (implemented in `sys_native`) can read it.
 *
 * # Safety
 *
 * `cb` must remain valid for the lifetime of the process.
 */
void register_host_dispatch_callback(HostDispatchFn cb);

/**
 * Register the host release callback. First call wins; subsequent calls
 * log a diagnostic and are ignored.
 *
 * The callback fires when the last Rust clone of a `HostValueArc` is
 * dropped. The bridge uses this notification to remove its internal
 * `key → host-language reference` entry.
 *
 * # Safety
 *
 * `cb` must remain valid for the lifetime of the process.
 */
void register_host_release_callback(HostReleaseFn cb);

/**
 * Called by the host to complete an in-flight host-value invocation.
 *
 * - `call_id`  — the id forwarded by `HostDispatchFn`.
 * - `is_error` — 0 for success, non-zero for error.
 * - `content`  — pointer to the protobuf payload (may be null if `length == 0`).
 * - `length`   — byte length of `content`.
 *
 * **Success** (`is_error == 0`): `content` is a protobuf-encoded
 * `InboundValue` (host→engine direction; engine re-validates against the
 * declared return type).
 *
 * **Error** (`is_error != 0`): `content` is a protobuf-encoded
 * `InboundValue` carrying the thrown value (typically an `Instance` of
 * `baml.errors.HostCallable` with the host exception's metadata, or a
 * codegenned BAML error class). The engine's `materialize_host_throw`
 * runs the declared-throws contract check against the decoded value.
 *
 * # Safety
 *
 * `content` must be valid for `length` bytes for the duration of this call.
 */
void complete_host_call(uint32_t call_id,
                        int32_t is_error,
                        const int8_t *content,
                        uintptr_t length);

/**
 * Free a buffer returned by FFI functions.
 */
void free_buffer(struct Buffer buf);

/**
 * Flush the event sink. No-op: tracing/event production has been removed.
 */
void flush_events(void);

/**
 * Returns the BAML version as a Buffer containing raw UTF-8 bytes.
 * Caller must free with free_buffer().
 */
struct Buffer version(void);

/**
 * Create/initialize the BAML runtime (global BexEngine).
 *
 * # Arguments
 * * `root_path` - Root path for BAML files (C string)
 * * `src_files_json` - JSON-encoded HashMap<String, String> of file contents
 *
 * # Returns
 * Non-null pointer on success (value is opaque, not used), null on failure.
 */
const void *create_baml_runtime(const char *root_path, const char *src_files_json);

/**
 * Destroy the BAML runtime.
 * This is a no-op since the global engine persists for the process lifetime.
 */
void destroy_baml_runtime(const void *_runtime);

/**
 * Invoke the BAML CLI.
 * Currently returns 1 (error) as CLI is not implemented for bridge.
 */
int invoke_runtime_cli(const char *const *_args);

/**
 * Create/initialize the BAML runtime (global BexEngine) from serialized
 * BAML bytecode — the borsh-encoded `Program` payload that `baml pack`
 * embeds and that generated SDKs inline. The bytecode-first counterpart of
 * [`create_baml_runtime`], enabling SDKs to boot without shipping `.baml`
 * sources.
 *
 * # Returns
 * Non-null pointer on success (value is opaque, not used), null on failure.
 */
const void *create_baml_runtime_from_bytecode(const uint8_t *bytecode, uintptr_t length);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* BAML_CFFI_H */
