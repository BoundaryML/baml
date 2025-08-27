# BAML Abort Handlers - Working Examples

This document demonstrates abort handler functionality across TypeScript, Python, and Go.

## Overview

Abort handlers allow you to cancel in-flight LLM operations, which is crucial for:
- User-initiated cancellations
- Timeouts
- Resource management
- Preventing unnecessary API costs

## TypeScript Example

```typescript
import { b } from './baml_client';
import { BamlAbortError } from '@boundaryml/baml';

async function example() {
  // Create an AbortController (standard Web API)
  const controller = new AbortController();
  
  // Start an LLM operation
  const promise = b.ExtractName('My name is Alice', {
    abortController: controller,
  });
  
  // Cancel after 100ms
  setTimeout(() => {
    console.log('Aborting operation...');
    controller.abort();
  }, 100);
  
  try {
    const result = await promise;
    console.log('Result:', result);
  } catch (error) {
    if (error instanceof BamlAbortError) {
      console.log('Operation was cancelled');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

// Streaming example
async function streamingExample() {
  const controller = new AbortController();
  
  const stream = b.stream.TestAbortFallbackChain('test input', {
    abortController: controller,
  });
  
  // Cancel after receiving 3 events
  let eventCount = 0;
  
  try {
    for await (const event of stream) {
      console.log('Event:', event);
      eventCount++;
      
      if (eventCount >= 3) {
        controller.abort();
      }
    }
  } catch (error) {
    console.log('Stream cancelled after', eventCount, 'events');
  }
}
```

## Python Example

```python
import asyncio
from baml_client import b
from baml_py import AbortController

async def example():
    # Create an AbortController
    controller = AbortController()
    
    # Start an LLM operation
    task = asyncio.create_task(
        b.ExtractName(
            'My name is Alice',
            baml_options={'abort_controller': controller}
        )
    )
    
    # Cancel after 100ms
    async def cancel_after_delay():
        await asyncio.sleep(0.1)
        print('Aborting operation...')
        controller.abort()
    
    asyncio.create_task(cancel_after_delay())
    
    try:
        result = await task
        print('Result:', result)
    except Exception as e:
        if 'abort' in str(e).lower():
            print('Operation was cancelled')
        else:
            print('Unexpected error:', e)

# Synchronous example
def sync_example():
    from baml_client.sync_client import b as sync_b
    import threading
    import time
    
    controller = AbortController()
    
    # Cancel in background thread
    def cancel_after_delay():
        time.sleep(0.1)
        print('Aborting operation...')
        controller.abort()
    
    thread = threading.Thread(target=cancel_after_delay)
    thread.start()
    
    try:
        result = sync_b.ExtractName(
            'My name is Alice',
            baml_options={'abort_controller': controller}
        )
        print('Result:', result)
    except Exception as e:
        if 'abort' in str(e).lower():
            print('Operation was cancelled')
        else:
            print('Unexpected error:', e)
    
    thread.join()

# Run the async example
if __name__ == '__main__':
    asyncio.run(example())
```

## Go Example

```go
package main

import (
    "context"
    "fmt"
    "time"
    
    "yourproject/baml_client"
)

func example() {
    // Create a context with cancel
    ctx, cancel := context.WithCancel(context.Background())
    
    // Cancel after 100ms
    go func() {
        time.Sleep(100 * time.Millisecond)
        fmt.Println("Aborting operation...")
        cancel()
    }()
    
    // Start an LLM operation
    result, err := baml_client.ExtractName(ctx, "My name is Alice")
    
    if err != nil {
        if err == context.Canceled {
            fmt.Println("Operation was cancelled")
        } else {
            fmt.Printf("Unexpected error: %v\n", err)
        }
    } else {
        fmt.Printf("Result: %s\n", result)
    }
}

// Timeout example
func timeoutExample() {
    // Create context with 2 second timeout
    ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
    defer cancel()
    
    // This will timeout if the operation takes longer than 2 seconds
    result, err := baml_client.ExtractName(ctx, "My name is Alice")
    
    if err != nil {
        if err == context.DeadlineExceeded {
            fmt.Println("Operation timed out")
        } else if err == context.Canceled {
            fmt.Println("Operation was cancelled")
        } else {
            fmt.Printf("Error: %v\n", err)
        }
    } else {
        fmt.Printf("Result: %s\n", result)
    }
}

// Early cancellation check
func earlyCheckExample() {
    ctx, cancel := context.WithCancel(context.Background())
    
    // Cancel immediately
    cancel()
    
    // Operation should fail immediately without making API call
    result, err := baml_client.ExtractName(ctx, "My name is Alice")
    
    if err == context.Canceled {
        fmt.Println("Operation cancelled before starting (as expected)")
    } else {
        fmt.Printf("Unexpected result or error: %v, %v\n", result, err)
    }
}
```

## Common Use Cases

### 1. User-Initiated Cancellation

```typescript
// TypeScript
const controller = new AbortController();

// UI button click handler
cancelButton.onclick = () => {
  controller.abort();
};

// Start operation
const result = await b.GenerateReport(data, { abortController: controller });
```

### 2. Timeout Implementation

```python
# Python
import asyncio
from baml_py import AbortController

async def with_timeout(func, timeout_seconds=30):
    controller = AbortController()
    
    async def timeout_handler():
        await asyncio.sleep(timeout_seconds)
        controller.abort()
    
    timeout_task = asyncio.create_task(timeout_handler())
    
    try:
        result = await func(baml_options={'abort_controller': controller})
        timeout_task.cancel()
        return result
    except Exception as e:
        timeout_task.cancel()
        if 'abort' in str(e).lower():
            raise TimeoutError(f"Operation timed out after {timeout_seconds}s")
        raise
```

### 3. Resource Cleanup

```go
// Go
func processWithCleanup(ctx context.Context) error {
    ctx, cancel := context.WithCancel(ctx)
    defer cancel() // Ensure cancellation on function exit
    
    // Start multiple operations
    results := make(chan string, 3)
    errors := make(chan error, 3)
    
    go func() {
        r, err := baml_client.Operation1(ctx, input)
        if err != nil {
            errors <- err
        } else {
            results <- r
        }
    }()
    
    go func() {
        r, err := baml_client.Operation2(ctx, input)
        if err != nil {
            errors <- err
        } else {
            results <- r
        }
    }()
    
    // Cancel all operations if any fails
    select {
    case err := <-errors:
        cancel() // Cancel remaining operations
        return err
    case result := <-results:
        // Process result
        return nil
    case <-ctx.Done():
        return ctx.Err()
    }
}
```

## Testing Abort Handlers

### TypeScript Test

```typescript
import { describe, it, expect } from '@jest/globals';

describe('Abort Handlers', () => {
  it('should cancel operation immediately when aborted', async () => {
    const controller = new AbortController();
    const startTime = Date.now();
    
    const promise = b.FnFailRetryExponentialDelay(5, 100, {
      abortController: controller,
    });
    
    // Abort after 50ms
    setTimeout(() => controller.abort(), 50);
    
    await expect(promise).rejects.toThrow(/abort/i);
    
    const elapsed = Date.now() - startTime;
    expect(elapsed).toBeLessThan(200); // Should abort quickly
  });
});
```

### Python Test

```python
import pytest
import asyncio
import time

@pytest.mark.asyncio
async def test_abort_handler():
    controller = AbortController()
    start_time = time.time()
    
    task = asyncio.create_task(
        b.FnFailRetryExponentialDelay(
            retries=5,
            initial_delay_ms=100,
            baml_options={'abort_controller': controller}
        )
    )
    
    # Abort after 50ms
    await asyncio.sleep(0.05)
    controller.abort()
    
    with pytest.raises(Exception) as exc_info:
        await task
    
    assert 'abort' in str(exc_info.value).lower()
    
    elapsed = time.time() - start_time
    assert elapsed < 0.2  # Should abort quickly
```

## Performance Considerations

1. **Early Cancellation Checks**: Operations check for cancellation before starting, avoiding unnecessary API calls
2. **Immediate Propagation**: Cancellation signals propagate immediately through retry and fallback chains
3. **Resource Cleanup**: All resources (connections, memory) are properly cleaned up on cancellation
4. **No Polling**: Uses efficient event-driven mechanisms, not polling

## Best Practices

1. **Always provide abort capability for long-running operations**
   - Users should be able to cancel any operation that might take more than a few seconds

2. **Use timeouts for reliability**
   - Set reasonable timeouts to prevent operations from hanging indefinitely

3. **Handle cancellation gracefully**
   - Distinguish between cancellation and other errors in your error handling

4. **Clean up resources**
   - Ensure any cleanup code runs even when operations are cancelled

5. **Test cancellation paths**
   - Include tests for abort scenarios to ensure they work as expected

## Limitations

- Cancellation is cooperative - the LLM provider must support it
- Some operations may not be cancellable once they reach the provider
- Network requests in flight may complete even after cancellation

## Summary

Abort handlers provide a standardized way to cancel LLM operations across different languages:
- **TypeScript**: Uses standard Web API `AbortController`
- **Python**: Custom `AbortController` class with similar API
- **Go**: Uses standard `context.Context` with cancellation

All implementations provide:
- Immediate cancellation propagation
- Early cancellation checks
- Proper resource cleanup
- Consistent error handling