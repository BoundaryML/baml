import { b } from 'baml_client';
import { NextRequest } from 'next/server';

export const runtime = 'nodejs';

export async function POST(request: NextRequest) {
  try {
    // Parse the request body
    const { params, useAbortSignal } = await request.json();
    
    // Create a stream
    let stream;
    
    if (useAbortSignal) {
      // Method 2: Create a stream with an AbortController
      const controller = new AbortController();
      
      // Set up a timeout to abort after 30 seconds if needed
      const timeoutId = setTimeout(() => {
        controller.abort();
      }, 30000);
      
      // Create the stream with the signal
      stream = b.stream.YourFunction(params, { signal: controller.signal });
      
      // Clear the timeout when the request is aborted
      request.signal.addEventListener('abort', () => {
        clearTimeout(timeoutId);
        controller.abort();
      }, { once: true });
    } else {
      // Method 1: Create a stream without an AbortController
      stream = b.stream.YourFunction(params);
      
      // Set up abort handling for client disconnects
      request.signal.addEventListener('abort', () => {
        stream.abort();
      }, { once: true });
    }
    
    // Convert to a web-compatible stream
    const readableStream = stream.toStreamable();
    
    // Return the stream as a response
    return new Response(readableStream, {
      headers: {
        'Content-Type': 'application/json',
        'Transfer-Encoding': 'chunked',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
      },
    });
  } catch (error) {
    console.error('Error in stream API route:', error);
    return new Response(
      JSON.stringify({ 
        error: {
          message: error instanceof Error ? error.message : 'Unknown error',
          type: error instanceof Error ? error.name : 'UnknownError',
        }
      }),
      { 
        status: 500,
        headers: { 'Content-Type': 'application/json' }
      }
    );
  }
}

// Client-side code to consume this API:
/*
// Method 1: Using the built-in abort handling
async function fetchStream() {
  const controller = new AbortController();
  
  try {
    const response = await fetch('/api/stream', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ 
        params: { param1: 'value1' },
        useAbortSignal: false 
      }),
      signal: controller.signal, // This will abort the fetch request
    });
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    
    while (true) {
      const { done, value } = await reader.read();
      
      if (done) {
        break;
      }
      
      // Process the chunk
      const chunk = decoder.decode(value);
      const messages = chunk
        .split('\n')
        .filter(Boolean)
        .map(line => JSON.parse(line));
      
      for (const message of messages) {
        if (message.partial) {
          // Handle partial result
          console.log('Partial:', message.partial);
        } else if (message.final) {
          // Handle final result
          console.log('Final:', message.final);
        } else if (message.error) {
          // Handle error
          console.error('Error:', message.error);
        }
      }
    }
  } catch (error) {
    if (error.name === 'AbortError') {
      console.log('Fetch aborted');
    } else {
      console.error('Fetch error:', error);
    }
  }
}

// Method 2: Using the server-side abort signal
async function fetchStreamWithServerAbort() {
  const controller = new AbortController();
  
  try {
    const response = await fetch('/api/stream', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ 
        params: { param1: 'value1' },
        useAbortSignal: true  // Use server-side abort signal
      }),
      signal: controller.signal,
    });
    
    // Process response...
  } catch (error) {
    // Handle error...
  }
}

// To abort:
controller.abort();
*/
