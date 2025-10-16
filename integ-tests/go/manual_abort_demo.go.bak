package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"runtime"
	"time"

	baml_client "example.com/integ-tests/baml_client"
)

func main() {
	fmt.Println("=== Manual Abort Handler Test ===\n")
	
	// Test 1: Context cancellation propagates to Rust runtime
	fmt.Println("Test 1: Context Cancellation")
	testContextCancellation()
	
	// Test 2: Streaming operations stop when context is cancelled
	fmt.Println("\nTest 2: Streaming Cancellation")
	testStreamingCancellation()
	
	// Test 3: Timeout cancellation
	fmt.Println("\nTest 3: Timeout Cancellation")
	testTimeoutCancellation()
	
	// Test 4: Check for goroutine leaks
	fmt.Println("\nTest 4: Goroutine Leak Check")
	testGoroutineLeaks()
	
	fmt.Println("\n=== All manual tests completed ===")
}

func testContextCancellation() {
	ctx, cancel := context.WithCancel(context.Background())
	
	// Start a goroutine that will cancel after 100ms
	go func() {
		time.Sleep(100 * time.Millisecond)
		fmt.Println("  Cancelling context...")
		cancel()
	}()
	
	// Try to run a function that would normally take longer due to retries
	fmt.Println("  Starting TestRetryConstant...")
	start := time.Now()
	_, err := baml_client.TestRetryConstant(ctx)
	duration := time.Since(start)
	
	if err != nil {
		fmt.Printf("  ✓ Got expected error: %v\n", err)
		fmt.Printf("  ✓ Cancelled after %v\n", duration)
	} else {
		fmt.Println("  ✗ ERROR: Function should have been cancelled")
	}
}

func testStreamingCancellation() {
	ctx, cancel := context.WithCancel(context.Background())
	
	fmt.Println("  Starting streaming TestFallbackClient...")
	stream, err := baml_client.Stream.TestFallbackClient(ctx)
	if err != nil {
		fmt.Printf("  ✗ Failed to start stream: %v\n", err)
		return
	}
	
	// Cancel after 50ms
	go func() {
		time.Sleep(50 * time.Millisecond)
		fmt.Println("  Cancelling stream...")
		cancel()
	}()
	
	count := 0
	start := time.Now()
	for range stream {
		count++
		if count > 100 {
			fmt.Println("  ✗ ERROR: Stream should have been cancelled by now")
			break
		}
	}
	duration := time.Since(start)
	
	fmt.Printf("  ✓ Stream cancelled after %d events in %v\n", count, duration)
}

func testTimeoutCancellation() {
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	
	fmt.Println("  Starting TestRetryExponential with 200ms timeout...")
	start := time.Now()
	_, err := baml_client.TestRetryExponential(ctx)
	duration := time.Since(start)
	
	if err != nil {
		fmt.Printf("  ✓ Got timeout error: %v\n", err)
		fmt.Printf("  ✓ Timed out after %v\n", duration)
		if duration > 250*time.Millisecond {
			fmt.Println("  ⚠ Warning: Took longer than expected to timeout")
		}
	} else {
		fmt.Println("  ✗ ERROR: Function should have timed out")
	}
}

func testGoroutineLeaks() {
	// Get initial goroutine count
	runtime.GC()
	time.Sleep(100 * time.Millisecond)
	initialCount := runtime.NumGoroutine()
	fmt.Printf("  Initial goroutines: %d\n", initialCount)
	
	// Run several cancelled operations
	for i := 0; i < 5; i++ {
		ctx, cancel := context.WithCancel(context.Background())
		go func() {
			time.Sleep(10 * time.Millisecond)
			cancel()
		}()
		
		// Ignore errors, we're just checking for leaks
		_, _ = baml_client.TestRetryConstant(ctx)
	}
	
	// Wait for goroutines to clean up
	time.Sleep(500 * time.Millisecond)
	runtime.GC()
	time.Sleep(100 * time.Millisecond)
	
	finalCount := runtime.NumGoroutine()
	fmt.Printf("  Final goroutines: %d\n", finalCount)
	
	if finalCount > initialCount+2 {
		fmt.Printf("  ⚠ Warning: Possible goroutine leak (increased by %d)\n", finalCount-initialCount)
	} else {
		fmt.Println("  ✓ No significant goroutine leaks detected")
	}
}

func init() {
	// Set log level if needed
	if os.Getenv("DEBUG") == "1" {
		log.SetFlags(log.LstdFlags | log.Lshortfile)
	}
}