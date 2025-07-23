import { b } from 'baml_client';
import { AbortError } from '@boundaryml/baml';

/**
 * Example 1: Basic abort usage
 */
async function basicAbortExample() {
  console.log('Starting basic abort example...');
  
  // Start a streaming operation
  const stream = b.stream.YourFunction({ param1: 'value1' });
  
  // Set a timeout to abort after 5 seconds
  setTimeout(() => {
    console.log('Aborting stream after 5 seconds');
    stream.abort();
  }, 5000);
  
  try {
    // Process stream results
    console.log('Processing stream results...');
    for await (const partial of stream) {
      console.log('Partial result:', partial);
    }
    
    // This should not be reached if aborted
    const final = await stream.getFinalResponse();
    console.log('Final result:', final);
  } catch (error) {
    if (error instanceof AbortError) {
      console.log('Stream was aborted as expected');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

/**
 * Example 2: Using external AbortController
 */
async function externalAbortControllerExample() {
  console.log('Starting external AbortController example...');
  
  // Create an AbortController
  const controller = new AbortController();
  
  // Pass the signal when creating the stream
  const stream = b.stream.YourFunction({ param1: 'value1' }, { signal: controller.signal });
  
  // Set a timeout to abort after 3 seconds
  setTimeout(() => {
    console.log('Aborting stream using external controller after 3 seconds');
    controller.abort();
  }, 3000);
  
  try {
    // Process stream results
    console.log('Processing stream results...');
    for await (const partial of stream) {
      console.log('Partial result:', partial);
    }
    
    // This should not be reached if aborted
    const final = await stream.getFinalResponse();
    console.log('Final result:', final);
  } catch (error) {
    if (error instanceof AbortError) {
      console.log('Stream was aborted as expected');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

/**
 * Example 3: Pre-emptive abort
 */
async function preemptiveAbortExample() {
  console.log('Starting pre-emptive abort example...');
  
  // Create an AbortController and abort it immediately
  const controller = new AbortController();
  controller.abort();
  
  // The stream will be aborted as soon as it's created
  const stream = b.stream.YourFunction({ param1: 'value1' }, { signal: controller.signal });
  
  try {
    // This should throw an AbortError immediately
    console.log('Attempting to process stream that was aborted before starting...');
    for await (const partial of stream) {
      console.log('This should not be reached');
    }
  } catch (error) {
    if (error instanceof AbortError) {
      console.log('Stream was aborted before processing started, as expected');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

/**
 * Example 4: Using the stream's signal
 */
async function streamSignalExample() {
  console.log('Starting stream signal example...');
  
  // Start a streaming operation
  const stream = b.stream.YourFunction({ param1: 'value1' });
  
  // Get the AbortSignal from the stream
  const { signal } = stream;
  
  // Listen for abort events
  signal.addEventListener('abort', () => {
    console.log('Stream abort detected via signal');
  }, { once: true });
  
  // Set a timeout to abort after 4 seconds
  setTimeout(() => {
    console.log('Aborting stream after 4 seconds');
    stream.abort();
  }, 4000);
  
  try {
    // Process stream results
    console.log('Processing stream results...');
    for await (const partial of stream) {
      console.log('Partial result:', partial);
    }
    
    // This should not be reached if aborted
    const final = await stream.getFinalResponse();
    console.log('Final result:', final);
  } catch (error) {
    if (error instanceof AbortError) {
      console.log('Stream was aborted as expected');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

/**
 * Example 5: Checking abort status
 */
async function checkAbortStatusExample() {
  console.log('Starting abort status check example...');
  
  // Start a streaming operation
  const stream = b.stream.YourFunction({ param1: 'value1' });
  
  console.log('Is stream aborted initially?', stream.isAborted);
  
  // Set a timeout to abort after 2 seconds
  setTimeout(() => {
    console.log('Aborting stream after 2 seconds');
    stream.abort();
    console.log('Is stream aborted after calling abort()?', stream.isAborted);
  }, 2000);
  
  try {
    // Process stream results
    console.log('Processing stream results...');
    for await (const partial of stream) {
      console.log('Partial result:', partial);
    }
  } catch (error) {
    if (error instanceof AbortError) {
      console.log('Stream was aborted as expected');
    } else {
      console.error('Unexpected error:', error);
    }
  }
}

// Run the examples
async function runExamples() {
  await basicAbortExample();
  console.log('\n-----------------------------------\n');
  
  await externalAbortControllerExample();
  console.log('\n-----------------------------------\n');
  
  await preemptiveAbortExample();
  console.log('\n-----------------------------------\n');
  
  await streamSignalExample();
  console.log('\n-----------------------------------\n');
  
  await checkAbortStatusExample();
}

runExamples().catch(console.error);
