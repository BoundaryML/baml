import { b } from '../baml_client';
import { BamlAbortError } from '@boundaryml/baml';

describe('Abort Handlers', () => {
  it('manual cancellation', async () => {
    const controller = new AbortController();
    
    const promise = b.FnFailRetryExponentialDelay(5, 100, {
      abortController: controller,
    });
    
    setTimeout(() => controller.abort(), 100);
    
    await expect(promise).rejects.toThrow();
    // Could be BamlAbortError or another error if cancelled fast enough
  });
  
  it('streaming cancellation', async () => {
    const controller = new AbortController();
    
    const stream = b.stream.TestAbortFallbackChain('test', {
      abortController: controller,
    });
    
    setTimeout(() => controller.abort(), 50);
    
    const values = [];
    try {
      for await (const value of stream) {
        values.push(value);
      }
    } catch (e) {
      // Expected - stream should be cancelled
    }
    
    // Should have stopped early due to cancellation
    expect(values.length).toBeLessThan(10);
  });
  
  it('timeout using AbortSignal.timeout', async () => {
    const controller = new AbortController();
    // Simulate timeout by aborting after 200ms
    setTimeout(() => controller.abort('timeout'), 200);
    
    const promise = b.FnFailRetryConstantDelay(5, 100, {
      abortController: controller,
    });
    
    await expect(promise).rejects.toThrow();
  });

  it('early abort check', async () => {
    const controller = new AbortController();
    controller.abort('early abort');
    
    await expect(b.ExtractName('John Doe', {
      abortController: controller,
    })).rejects.toThrow(BamlAbortError);
  });

  it('normal operation without abort', async () => {
    const result = await b.ExtractName('My name is Alice');
    expect(typeof result).toBe('string');
    expect(result.toLowerCase()).toContain('alice');
  });
});