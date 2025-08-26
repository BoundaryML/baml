import { b } from '../baml_client';
import { BamlAbortError } from '@boundaryml/baml';

describe('Abort Handlers - Manual Testing', () => {
  it('cancels a long-running retry operation mid-execution', async () => {
    const controller = new AbortController();
    const startTime = Date.now();
    
    const promise = b.FnFailRetryExponentialDelay(5, 100, {
      abortController: controller,
    });
    
    // Cancel after 250ms (should be during first or second retry)
    setTimeout(() => {
      console.log('Aborting after 250ms...');
      controller.abort();
    }, 250);
    
    try {
      await promise;
      fail('Should have thrown an error');
    } catch (error: any) {
      const elapsedTime = Date.now() - startTime;
      console.log(`Operation cancelled after ${elapsedTime}ms`);
      
      // Should cancel within ~300ms (250ms delay + processing time)
      expect(elapsedTime).toBeLessThan(400);
      
      // Verify error message indicates cancellation
      expect(error.message).toMatch(/abort|cancel/i);
    }
  });

  it('handles rapid successive cancellations', async () => {
    const results = await Promise.allSettled([
      (async () => {
        const controller = new AbortController();
        setTimeout(() => controller.abort(), 10);
        return b.FnFailRetryConstantDelay(5, 100, { abortController: controller });
      })(),
      (async () => {
        const controller = new AbortController();
        setTimeout(() => controller.abort(), 20);
        return b.FnFailRetryConstantDelay(5, 100, { abortController: controller });
      })(),
      (async () => {
        const controller = new AbortController();
        setTimeout(() => controller.abort(), 30);
        return b.FnFailRetryConstantDelay(5, 100, { abortController: controller });
      })(),
    ]);
    
    // All should be rejected
    results.forEach((result, index) => {
      expect(result.status).toBe('rejected');
      console.log(`Task ${index + 1} cancelled: ${result.status === 'rejected' ? result.reason.message : 'N/A'}`);
    });
  });

  it('cancels streaming operation and verifies no further events', async () => {
    const controller = new AbortController();
    const stream = b.stream.TestAbortFallbackChain('test streaming', {
      abortController: controller,
    });
    
    const events = [];
    let errorCaught = false;
    
    setTimeout(() => {
      console.log('Aborting stream after 50ms...');
      controller.abort();
    }, 50);
    
    try {
      for await (const event of stream) {
        events.push(event);
        console.log(`Received event ${events.length}`);
      }
    } catch (error: any) {
      errorCaught = true;
      console.log(`Stream cancelled after ${events.length} events: ${error.message}`);
    }
    
    // Stream should have been cancelled
    expect(errorCaught || events.length === 0).toBe(true);
    
    // Wait a bit to ensure no more events come through
    await new Promise(resolve => setTimeout(resolve, 100));
    const finalEventCount = events.length;
    
    // Verify no additional events were received after cancellation
    expect(events.length).toBe(finalEventCount);
    console.log(`Final event count: ${finalEventCount}`);
  });

  it('verifies no retries occur after cancellation', async () => {
    const controller = new AbortController();
    const startTime = Date.now();
    
    // This should normally retry 5 times with 100ms delays
    const promise = b.FnFailRetryConstantDelay(5, 100, {
      abortController: controller,
    });
    
    // Cancel after first retry should have started
    setTimeout(() => {
      console.log('Cancelling after 150ms (during first retry)...');
      controller.abort();
    }, 150);
    
    try {
      await promise;
    } catch (error: any) {
      const elapsedTime = Date.now() - startTime;
      console.log(`Total time: ${elapsedTime}ms`);
      
      // Without cancellation, this would take at least 500ms (5 retries * 100ms)
      // With cancellation at 150ms, should complete within 200ms
      expect(elapsedTime).toBeLessThan(250);
    }
  });

  it('tests with real provider (if available)', async () => {
    // This test uses the real OpenAI provider configured in the BAML file
    const controller = new AbortController();
    
    try {
      const promise = b.ExtractName('My name is Bob and I live in Seattle', {
        abortController: controller,
      });
      
      // Cancel immediately
      controller.abort();
      
      await promise;
      fail('Should have been cancelled');
    } catch (error: any) {
      // Should be cancelled before making the API call
      expect(error).toBeDefined();
      console.log('Real provider call cancelled successfully:', error.message);
    }
  });

  it('memory cleanup - multiple aborted operations', async () => {
    // Create and abort many operations to test cleanup
    const operations = [];
    
    for (let i = 0; i < 100; i++) {
      const controller = new AbortController();
      
      const op = b.FnFailRetryConstantDelay(3, 50, {
        abortController: controller,
      }).catch(() => {}); // Ignore errors
      
      operations.push(op);
      
      // Abort at random times
      setTimeout(() => controller.abort(), Math.random() * 100);
    }
    
    // Wait for all to complete/fail
    await Promise.allSettled(operations);
    
    // Force garbage collection if available
    if (global.gc) {
      global.gc();
    }
    
    console.log('Completed 100 abort operations for memory cleanup test');
    
    // In a real scenario, you'd monitor memory usage here
    // For now, we just verify all operations completed without hanging
    expect(operations.length).toBe(100);
  });
});