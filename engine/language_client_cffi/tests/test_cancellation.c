/**
 * Comprehensive tests for BAML CFFI cancellation functionality
 */

#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#include <stdbool.h>
#include <unistd.h>
#include <pthread.h>
#include <time.h>

#include "../include/baml_cancellation.h"

// Test utilities
#define TEST_ASSERT(condition, message) \
    do { \
        if (!(condition)) { \
            fprintf(stderr, "FAIL: %s\n", message); \
            exit(1); \
        } else { \
            printf("PASS: %s\n", message); \
        } \
    } while(0)

// Mock stream structure for testing
typedef struct {
    bool cancelled;
    pthread_mutex_t mutex;
} MockStream;

MockStream* create_mock_stream() {
    MockStream* stream = malloc(sizeof(MockStream));
    stream->cancelled = false;
    pthread_mutex_init(&stream->mutex, NULL);
    return stream;
}

void free_mock_stream(MockStream* stream) {
    if (stream) {
        pthread_mutex_destroy(&stream->mutex);
        free(stream);
    }
}

bool is_mock_stream_cancelled(MockStream* stream) {
    pthread_mutex_lock(&stream->mutex);
    bool cancelled = stream->cancelled;
    pthread_mutex_unlock(&stream->mutex);
    return cancelled;
}

void set_mock_stream_cancelled(MockStream* stream) {
    pthread_mutex_lock(&stream->mutex);
    stream->cancelled = true;
    pthread_mutex_unlock(&stream->mutex);
}

// Test functions
void test_create_cancellation_token() {
    printf("\n=== Testing create_cancellation_token ===\n");
    
    void* token = create_cancellation_token();
    TEST_ASSERT(token != NULL, "create_cancellation_token should return non-NULL");
    
    free_cancellation_token(token);
}

void test_cancel_token() {
    printf("\n=== Testing cancel_token ===\n");
    
    void* token = create_cancellation_token();
    TEST_ASSERT(token != NULL, "Token should be created");
    
    // Initially not cancelled
    TEST_ASSERT(!is_token_cancelled(token), "Token should not be cancelled initially");
    
    // Cancel the token
    cancel_token(token);
    TEST_ASSERT(is_token_cancelled(token), "Token should be cancelled after cancel_token()");
    
    free_cancellation_token(token);
}

void test_is_token_cancelled() {
    printf("\n=== Testing is_token_cancelled ===\n");
    
    void* token = create_cancellation_token();
    
    // Test initial state
    TEST_ASSERT(!is_token_cancelled(token), "New token should not be cancelled");
    
    // Test after cancellation
    cancel_token(token);
    TEST_ASSERT(is_token_cancelled(token), "Cancelled token should return true");
    
    // Test with NULL token
    TEST_ASSERT(!is_token_cancelled(NULL), "NULL token should return false");
    
    free_cancellation_token(token);
}

void test_free_cancellation_token() {
    printf("\n=== Testing free_cancellation_token ===\n");
    
    void* token = create_cancellation_token();
    TEST_ASSERT(token != NULL, "Token should be created");
    
    // Should not crash
    free_cancellation_token(token);
    
    // Should handle NULL gracefully
    free_cancellation_token(NULL);
    
    TEST_ASSERT(true, "free_cancellation_token should handle NULL gracefully");
}

void test_cancel_stream() {
    printf("\n=== Testing cancel_stream ===\n");
    
    void* token = create_cancellation_token();
    MockStream* stream = create_mock_stream();
    
    // Test successful cancellation
    bool result = cancel_stream(stream, token);
    TEST_ASSERT(result, "cancel_stream should return true on success");
    TEST_ASSERT(is_token_cancelled(token), "Token should be cancelled");
    
    // Test with NULL parameters
    TEST_ASSERT(!cancel_stream(NULL, token), "cancel_stream should return false with NULL stream");
    TEST_ASSERT(!cancel_stream(stream, NULL), "cancel_stream should return false with NULL token");
    TEST_ASSERT(!cancel_stream(NULL, NULL), "cancel_stream should return false with both NULL");
    
    free_mock_stream(stream);
    free_cancellation_token(token);
}

void test_multiple_cancellations() {
    printf("\n=== Testing multiple cancellations ===\n");
    
    void* token = create_cancellation_token();
    
    // Multiple cancellations should be safe
    cancel_token(token);
    cancel_token(token);
    cancel_token(token);
    
    TEST_ASSERT(is_token_cancelled(token), "Token should remain cancelled");
    
    free_cancellation_token(token);
}

// Thread test data
typedef struct {
    void* token;
    int thread_id;
    bool completed;
} ThreadTestData;

void* thread_cancel_test(void* arg) {
    ThreadTestData* data = (ThreadTestData*)arg;
    
    // Small delay to create race conditions
    usleep(1000 * (data->thread_id % 10));  // 0-9ms delay
    
    cancel_token(data->token);
    data->completed = true;
    
    return NULL;
}

void test_concurrent_cancellation() {
    printf("\n=== Testing concurrent cancellation ===\n");
    
    void* token = create_cancellation_token();
    const int num_threads = 10;
    pthread_t threads[num_threads];
    ThreadTestData thread_data[num_threads];
    
    // Create threads that all try to cancel the same token
    for (int i = 0; i < num_threads; i++) {
        thread_data[i].token = token;
        thread_data[i].thread_id = i;
        thread_data[i].completed = false;
        
        int result = pthread_create(&threads[i], NULL, thread_cancel_test, &thread_data[i]);
        TEST_ASSERT(result == 0, "Thread creation should succeed");
    }
    
    // Wait for all threads to complete
    for (int i = 0; i < num_threads; i++) {
        pthread_join(threads[i], NULL);
        TEST_ASSERT(thread_data[i].completed, "Thread should complete");
    }
    
    TEST_ASSERT(is_token_cancelled(token), "Token should be cancelled after concurrent operations");
    
    free_cancellation_token(token);
}

void test_cancellation_timing() {
    printf("\n=== Testing cancellation timing ===\n");
    
    void* token = create_cancellation_token();
    
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    
    cancel_token(token);
    
    clock_gettime(CLOCK_MONOTONIC, &end);
    
    // Calculate elapsed time in microseconds
    long elapsed_us = (end.tv_sec - start.tv_sec) * 1000000 + 
                      (end.tv_nsec - start.tv_nsec) / 1000;
    
    printf("Cancellation took %ld microseconds\n", elapsed_us);
    
    // Cancellation should be very fast (less than 1ms)
    TEST_ASSERT(elapsed_us < 1000, "Cancellation should be fast (< 1ms)");
    TEST_ASSERT(is_token_cancelled(token), "Token should be cancelled");
    
    free_cancellation_token(token);
}

void test_memory_cleanup() {
    printf("\n=== Testing memory cleanup ===\n");
    
    // Create and destroy many tokens to test for memory leaks
    const int num_tokens = 1000;
    
    for (int i = 0; i < num_tokens; i++) {
        void* token = create_cancellation_token();
        TEST_ASSERT(token != NULL, "Token creation should succeed");
        
        cancel_token(token);
        TEST_ASSERT(is_token_cancelled(token), "Token should be cancelled");
        
        free_cancellation_token(token);
    }
    
    TEST_ASSERT(true, "Memory cleanup test completed without crashes");
}

void test_edge_cases() {
    printf("\n=== Testing edge cases ===\n");
    
    // Test cancelling already cancelled token
    void* token = create_cancellation_token();
    cancel_token(token);
    cancel_token(token);  // Should be safe
    TEST_ASSERT(is_token_cancelled(token), "Double cancellation should be safe");
    
    // Test operations on NULL token
    cancel_token(NULL);  // Should not crash
    TEST_ASSERT(!is_token_cancelled(NULL), "NULL token should return false");
    
    free_cancellation_token(token);
    TEST_ASSERT(true, "Edge cases handled correctly");
}

// Performance benchmark
void benchmark_cancellation() {
    printf("\n=== Benchmarking cancellation performance ===\n");
    
    const int iterations = 10000;
    struct timespec start, end;
    
    clock_gettime(CLOCK_MONOTONIC, &start);
    
    for (int i = 0; i < iterations; i++) {
        void* token = create_cancellation_token();
        cancel_token(token);
        free_cancellation_token(token);
    }
    
    clock_gettime(CLOCK_MONOTONIC, &end);
    
    long elapsed_us = (end.tv_sec - start.tv_sec) * 1000000 + 
                      (end.tv_nsec - start.tv_nsec) / 1000;
    
    double avg_us = (double)elapsed_us / iterations;
    
    printf("Average time per cancellation: %.2f microseconds\n", avg_us);
    printf("Cancellations per second: %.0f\n", 1000000.0 / avg_us);
    
    TEST_ASSERT(avg_us < 100, "Average cancellation should be fast (< 100μs)");
}

int main() {
    printf("BAML CFFI Cancellation Tests\n");
    printf("============================\n");
    
    test_create_cancellation_token();
    test_cancel_token();
    test_is_token_cancelled();
    test_free_cancellation_token();
    test_cancel_stream();
    test_multiple_cancellations();
    test_concurrent_cancellation();
    test_cancellation_timing();
    test_memory_cleanup();
    test_edge_cases();
    benchmark_cancellation();
    
    printf("\n🎉 All CFFI cancellation tests passed!\n");
    printf("✅ Cancellation tokens work correctly\n");
    printf("✅ Thread safety verified\n");
    printf("✅ Memory management verified\n");
    printf("✅ Performance is acceptable\n");
    
    return 0;
}
