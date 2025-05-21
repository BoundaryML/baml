import { b, b_sync } from './test-setup'; // or wherever your b is defined
import { Collector } from '@boundaryml/baml';

/**
 * Helper function to force garbage collection.
 * Allows Node to run finalizers, ensuring we accurately track
 * whether function spans remain in memory.
 */
async function gc() {
  global.gc?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("With Options Tests", () => {
  beforeEach(() => {
    // Ensure collector is empty before each test
    expect(Collector.__functionSpanCount()).toBe(0);
  });

  afterEach(async () => {
    // Force garbage collection and verify all spans are cleaned up
    await gc();
    expect(Collector.__functionSpanCount()).toBe(0);
  });

  it("should test with options logger async call", async () => {
    console.log("### function_span_count", Collector.__functionSpanCount());
    // Should be garbage collected
    expect(Collector.__functionSpanCount()).toBe(0);

    // Create a collector
    const collector = new Collector("my-collector");
    let functionLogs = collector.logs;
    expect(functionLogs.length).toBe(0);

    // Create a new instance with the collector
    const myB = b.withOptions({ collector });

    // Make the call
    await myB.TestOpenAIGPT4oMini("hi there");

    // Verify logs
    functionLogs = collector.logs;
    expect(functionLogs.length).toBe(1);

    const log = collector.last;
    expect(log).not.toBeNull();
    expect(log?.functionName).toBe("TestOpenAIGPT4oMini");
    expect(log?.logType).toBe("call");

    // Verify usage fields
    expect(log?.usage.inputTokens).toBeGreaterThan(0);
    expect(log?.usage.outputTokens).toBeGreaterThan(0);

    // Verify calls
    const calls = log?.calls ?? [];
    expect(calls.length).toBe(1);

    // Make a second call on the default b object (no collector)
    await b.TestOpenAIGPT4oMini("hi there");
    // Should not be logged since collector not passed
    expect(collector.logs.length).toBe(1);

    // Force garbage collection to check function spans
    await gc();
    // Still not collected because it's in use
    expect(Collector.__functionSpanCount()).toBeGreaterThan(0);
  });

  it("should test with options logger sync", async () => {
    // Create a collector
    const collector = new Collector("my-collector");
    // Create a new instance with the collector using a sync client
    
    const myB = b_sync.withOptions({ collector });

    // Make the sync call
    myB.TestOpenAIGPT4oMini("hi there");

    // Verify logs
    expect(collector.logs.length).toBe(1);
  });

  it("should test with options logger async stream", async () => {
    // Create a collector
    const collector = new Collector("my-collector");
    const myB = b.withOptions({ collector });
    expect(collector.logs.length).toBe(0);

    // Call the streaming function
    const stream = myB.stream.TestOpenAIGPT4oMini("hi there");
    for await (const chunk of stream) {
      // We don't need to do anything with the chunk in this test
    }

    // Verify a single log entry was created
    expect(collector.logs.length).toBe(1);
  });
});
