/**
 * BAML CFFI Cancellation Support
 * 
 * This header provides C FFI functions for cancelling BAML streams,
 * which will stop ongoing HTTP requests to LLM providers.
 */

#ifndef BAML_CANCELLATION_H
#define BAML_CANCELLATION_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Create a new cancellation token.
 * 
 * @return Pointer to the cancellation token, or NULL on failure
 */
void* create_cancellation_token(void);

/**
 * Cancel a cancellation token.
 * This will signal all associated operations to stop.
 * 
 * @param token_ptr Pointer to the cancellation token
 */
void cancel_token(const void* token_ptr);

/**
 * Check if a cancellation token has been cancelled.
 * 
 * @param token_ptr Pointer to the cancellation token
 * @return true if cancelled, false otherwise
 */
bool is_token_cancelled(const void* token_ptr);

/**
 * Free a cancellation token and its resources.
 * 
 * @param token_ptr Pointer to the cancellation token
 */
void free_cancellation_token(const void* token_ptr);

/**
 * Cancel a stream using its cancellation token.
 * This will stop ongoing HTTP requests and clean up resources.
 * 
 * @param stream_ptr Pointer to the stream
 * @param token_ptr Pointer to the cancellation token
 * @return true if cancellation was successful, false otherwise
 */
bool cancel_stream(const void* stream_ptr, const void* token_ptr);

#ifdef __cplusplus
}
#endif

#endif /* BAML_CANCELLATION_H */
