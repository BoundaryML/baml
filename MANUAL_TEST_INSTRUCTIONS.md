# Manual Test Instructions for Go Abort Handlers

## Prerequisites
1. Ensure you have built the CFFI library:
   ```bash
   cd engine/language_client_cffi && cargo build
   ```

2. Ensure the Go client is built:
   ```bash
   cd engine/language_client_go && go build ./...
   ```

## Running Manual Tests

### Quick Test
Run the automated manual test suite:
```bash
cd integ-tests/go
go run manual_abort_test.go
```

This will test:
- ✅ Context cancellation propagates to Rust runtime
- ✅ Streaming operations stop when cancelled  
- ✅ Timeout-based cancellation
- ✅ Goroutine leak detection

### Interactive Testing

1. **Test Context Cancellation**:
   ```go
   ctx, cancel := context.WithCancel(context.Background())
   go func() {
       time.Sleep(100 * time.Millisecond)
       cancel()
   }()
   _, err := baml_client.TestRetryConstant(ctx)
   // Should return "context canceled" error
   ```

2. **Test Streaming Cancellation**:
   ```go
   ctx, cancel := context.WithCancel(context.Background())
   stream, _ := baml_client.Stream.TestFallbackClient(ctx)
   
   go func() {
       time.Sleep(50 * time.Millisecond)
       cancel()
   }()
   
   for event := range stream {
       // Should stop receiving events after cancellation
   }
   ```

3. **Test Timeout**:
   ```go
   ctx, _ := context.WithTimeout(context.Background(), 200*time.Millisecond)
   _, err := baml_client.TestRetryExponential(ctx)
   // Should timeout and return "deadline exceeded" error
   ```

## Expected Behavior

### ✅ Success Indicators:
- Functions return immediately when context is cancelled
- Error messages contain "context canceled" or "deadline exceeded"
- Streaming stops producing events after cancellation
- No goroutines leak after cancellation
- Cancellation happens quickly (within ~50-200ms depending on test)

### ❌ Failure Indicators:
- Functions continue running after context cancellation
- Functions block until all retries complete
- Goroutine count increases after tests
- Cancellation takes longer than expected (>500ms)
- Unexpected panics or crashes

## Debugging

If tests fail, check:

1. **CFFI Library**: Ensure `cancel_function_call` is exported:
   ```bash
   nm engine/target/debug/libbaml_cffi.dylib | grep cancel_function_call
   ```

2. **Go Imports**: Verify the Go client imports are correct:
   ```bash
   cd integ-tests/go
   go mod tidy
   ```

3. **Enable Debug Logging**:
   ```bash
   DEBUG=1 go run manual_abort_test.go
   ```

## Notes

- The abort handler implementation works at the CFFI layer, not in the Rust runtime itself
- Cancellation is cooperative - it checks at specific points during execution
- Some delay is expected between cancellation request and actual termination
- The implementation uses `stream-cancel` crate's Tripwire mechanism for efficient cancellation