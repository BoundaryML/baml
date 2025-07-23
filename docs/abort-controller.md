# Using AbortController with BAML Streams

BAML supports aborting stream processing using the standard `AbortController` API. This allows you to cancel ongoing stream operations, which is useful for scenarios like:

- User navigating away from a page
- Timeout conditions
- User-initiated cancellation
- Resource management

## Basic Usage

BAML provides two ways to abort a stream:

### Method 1: Using the built-in abort method

```typescript
import { b } from 'baml_client';

// Start a streaming operation
const stream = b.stream.YourFunction(params);

// Later, when you want to abort:
stream.abort();
```

### Method 2: Passing an AbortSignal at creation time

```typescript
import { b } from 'baml_client';

// Create an AbortController
const controller = new AbortController();

// Pass the signal when creating the stream
const stream = b.stream.YourFunction(params, { signal: controller.signal });

// Later, when you want to abort:
controller.abort();
```

### Method 3: Using the stream's signal

```typescript
import { b } from 'baml_client';

// Start a streaming operation
const stream = b.stream.YourFunction(params);

// Get the AbortSignal from the stream
const { signal } = stream;

// Use the signal with other APIs that accept AbortSignal
fetchSomething(url, { signal });

// Or listen for abort events
signal.addEventListener('abort', () => {
  console.log('Stream was aborted');
});
```

## Pre-emptive Abort

You can abort a stream before it even starts processing:

```typescript
// Create an AbortController and abort it immediately
const controller = new AbortController();
controller.abort();

// The stream will be aborted as soon as it's created
const stream = b.stream.YourFunction(params, { signal: controller.signal });

// This will throw an AbortError immediately
try {
  for await (const partial of stream) {
    console.log(partial);
  }
} catch (error) {
  console.log('Stream was aborted before processing started');
}
```

## React Example

```tsx
import { useState, useEffect } from 'react';
import { b } from 'baml_client';

function StreamingComponent() {
  const [results, setResults] = useState([]);
  const [isStreaming, setIsStreaming] = useState(false);
  
  useEffect(() => {
    // Create an AbortController for this effect
    const controller = new AbortController();
    let stream;
    
    async function startStream() {
      setIsStreaming(true);
      
      // Start the stream with the abort signal
      stream = b.stream.YourFunction(params, { signal: controller.signal });
      
      try {
        // Process stream results
        for await (const partial of stream) {
          setResults(prev => [...prev, partial]);
        }
        
        // Get final result
        const final = await stream.getFinalResponse();
        setResults(prev => [...prev, final]);
      } catch (error) {
        if (error.name !== 'AbortError') {
          console.error('Stream error:', error);
        }
      } finally {
        setIsStreaming(false);
      }
    }
    
    startStream();
    
    // Cleanup function to abort the stream when component unmounts
    return () => {
      controller.abort();
    };
  }, []);
  
  return (
    <div>
      <button 
        onClick={() => stream?.abort()} 
        disabled={!isStreaming}
      >
        Cancel Stream
      </button>
      
      <div>
        {results.map((result, i) => (
          <div key={i}>{JSON.stringify(result)}</div>
        ))}
      </div>
    </div>
  );
}
```

## Next.js Server Component Example

```tsx
import { b } from 'baml_client';

export async function POST(request: Request) {
  const { params } = await request.json();
  
  // Create a stream
  const stream = b.stream.YourFunction(params);
  
  // Convert to a web-compatible stream
  const readableStream = stream.toStreamable();
  
  // The stream will be automatically aborted if the client disconnects
  return new Response(readableStream);
}
```

## Error Handling

When a stream is aborted, any pending operations will throw an `AbortError`. You should handle this error appropriately in your code:

```typescript
try {
  for await (const partial of stream) {
    // Process partial results
  }
  
  const final = await stream.getFinalResponse();
  // Process final result
} catch (error) {
  if (error.name === 'AbortError') {
    console.log('Stream was aborted');
  } else {
    console.error('Stream error:', error);
  }
}
```

## Checking Abort Status

You can check if a stream has been aborted:

```typescript
if (stream.isAborted) {
  console.log('Stream has been aborted');
}
```

## Performance Considerations

Aborting a stream helps free up resources both on the client and server side. It's good practice to abort streams when they're no longer needed, especially for long-running operations.

## TypeScript Types

The BAML library exports the necessary types for working with abort controllers:

```typescript
import { AbortError, type AbortSignal, type AbortController } from '@boundaryml/baml';

// Or use the standard DOM types
// import type { AbortSignal, AbortController } from 'dom';
```
