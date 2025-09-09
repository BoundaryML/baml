import { traceAsync } from "../baml_client/tracing";
import { b, b_sync } from "./test-setup";
import { BamlRuntime, Collector, FunctionLog, Usage } from "@boundaryml/baml";

async function gc() {
  global.gc!();
  // allows node to run finalizers
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("Collector Tests", () => {
  beforeEach(() => {
    // Ensure collector is empty before each test
    expect(Collector.__functionSpanCount()).toBe(0);
  });

  afterEach(async () => {
    // Ensure garbage collection and verify all spans are cleaned up
    await gc();
    expect(Collector.__functionSpanCount()).toBe(0);
  });

  it("should collect logs for non-streaming calls", async () => {
    console.log("### function_call_count", Collector.__functionSpanCount());
    // Should be garbage collected
    expect(Collector.__functionSpanCount()).toBe(0);

    const collector = new Collector("my-collector");
    const functionLogs = collector.logs;
    expect(functionLogs.length).toBe(0);

    await b.TestOpenAIGPT4oMini("hi there", { collector });

    const updatedLogs = collector.logs;
    expect(updatedLogs.length).toBe(1);

    const log = collector.last;
    console.log("### log", log?.toString());
    expect(log).not.toBeNull();
    expect(log?.functionName).toBe("TestOpenAIGPT4oMini");
    expect(log?.logType).toBe("call");

    // Verify timing fields
    expect(log?.timing.startTimeUtcMs).toBeGreaterThan(0);
    expect(log?.timing.durationMs).toBeGreaterThan(0);

    // Verify usage fields
    expect(log?.usage.inputTokens).toBeGreaterThan(0);
    expect(log?.usage.outputTokens).toBeGreaterThan(0);
    expect(log?.usage.cachedInputTokens).toBeUndefined();

    // Verify calls
    const calls = log?.calls || [];

    expect(calls.length).toBe(1);

    const call = calls[0];
    expect(call.provider).toBe("openai");
    expect(call.clientName).toBe("GPT4oMini");
    expect(call.selected).toBe(true);

    // Verify request/response
    const request = call.httpRequest;
    expect(request).not.toBeNull();

    const body = request?.body.json();

    expect(typeof body).toBe("object");
    expect(body.messages).toBeDefined();
    expect(body.messages[0].content).not.toBeNull();
    expect(body.model).toBe("gpt-4o-mini");

    // Verify http response
    const response = call.httpResponse;
    const responseBody = response?.body.json();
    expect(response).not.toBeNull();
    expect(response?.status).toBe(200);
    expect(responseBody).not.toBeNull();
    expect(responseBody?.choices).toBeDefined();
    expect(responseBody?.choices.length).toBeGreaterThan(0);
    expect(responseBody?.choices[0].message.content).not.toBeNull();

    // Verify call timing
    const callTiming = call.timing;
    expect(callTiming.startTimeUtcMs).toBeGreaterThan(0);
    expect(callTiming.durationMs).toBeGreaterThan(0);

    // Verify call usage
    const callUsage = call.usage;
    expect(callUsage?.inputTokens).toBeGreaterThan(0);
    expect(callUsage?.outputTokens).toBeGreaterThan(0);
    expect(callUsage?.cachedInputTokens).toBeUndefined();

    // Usage matches log usage
    expect(callUsage?.inputTokens).toBe(log?.usage.inputTokens);
    expect(callUsage?.outputTokens).toBe(log?.usage.outputTokens);
    expect(callUsage?.cachedInputTokens).toBe(log?.usage.cachedInputTokens);

    // Verify raw response exists
    expect(log?.rawLlmResponse).not.toBeNull();

    // Collector usage should match log usage
    expect(collector.usage.inputTokens).toBe(log?.usage.inputTokens);
    expect(collector.usage.outputTokens).toBe(log?.usage.outputTokens);

    // Verify metadata
    // expect(typeof log?.metadata).toBe('object');

    // Force garbage collection
    await gc();
    console.log("----- gc.collect() -----");
    // Still not collected because it's in use
    expect(Collector.__functionSpanCount()).toBeGreaterThan(0);
  });

  it("should handle streaming calls correctly", async () => {
    const collector = new Collector("my-collector");
    const functionLogs = collector.logs;
    expect(functionLogs.length).toBe(0);

    const stream = b.stream.TestOpenAIGPT4oMini("hi there", { collector });

    const chunks = [];
    for await (const chunk of stream) {
      chunks.push(chunk);
      console.log(`### chunk: ${chunk}`);
    }

    const res = await stream.getFinalResponse();
    console.log(`### res: ${res}`);

    const updatedLogs = collector.logs;
    expect(updatedLogs.length).toBe(1);

    const log = collector.last;
    expect(log).not.toBeNull();
    expect(log?.functionName).toBe("TestOpenAIGPT4oMini");
    expect(log?.logType).toBe("stream");

    // Verify timing fields
    expect(log?.timing.startTimeUtcMs).toBeGreaterThan(0);
    expect(log?.timing.durationMs).toBeGreaterThan(0);

    // Verify usage fields
    expect(log?.usage.inputTokens).toBeGreaterThan(0);
    expect(log?.usage.outputTokens).toBeGreaterThan(0);
    expect(log?.usage.cachedInputTokens).toBeUndefined();

    // Verify calls
    const calls = log?.calls || [];
    expect(calls.length).toBe(1);

    const call = calls[0];
    expect(call.provider).toBe("openai");
    expect(call.clientName).toBe("GPT4oMini");
    expect(call.selected).toBe(true);

    // Verify request
    const request = call.httpRequest;
    expect(request).not.toBeNull();
    expect(typeof request?.body).toBe("object");
    expect((request?.body.json()).messages).toBeDefined();

    // For streaming, httpResponse might be null since it's streaming
    const response = call.httpResponse;
    expect(response).toBeNull();

    // Verify call timing
    const callTiming = call.timing;
    expect(callTiming.startTimeUtcMs).toBeGreaterThan(0);
    expect(callTiming.durationMs).toBeGreaterThan(0);

    // Verify call usage
    const callUsage = call.usage;
    expect(callUsage?.inputTokens).toBeGreaterThan(0);
    expect(callUsage?.outputTokens).toBeGreaterThan(0);

    // Verify raw response exists
    expect(log?.rawLlmResponse).not.toBeNull();

    await gc();
    console.log("----- gc.collect() -----");
    // Still not collected because it's in use
    expect(Collector.__functionSpanCount()).toBeGreaterThan(0);
  });

  it("should verify LLMStreamCall properties for streaming calls", async () => {
    const collector = new Collector("openai-stream-chunks");

    // Track chunks as they arrive
    const chunksReceived: string[] = [];
    const stream = b.stream.TestOpenAIGPT4oMini("Count from 1 to 5", { collector });

    for await (const chunk of stream) {
      chunksReceived.push(chunk);
      console.log(`Received chunk: ${chunk}`);
    }

    // Get final response
    const finalResponse = await stream.getFinalResponse();

    // Verify we received multiple chunks
    expect(chunksReceived.length).toBeGreaterThan(1);

    // Verify final response is complete
    expect(finalResponse.length).toBeGreaterThan(0);

    // Verify collector captured the stream
    const logs = collector.logs;
    expect(logs.length).toBe(1);

    const log = logs[0];
    expect(log.functionName).toBe("TestOpenAIGPT4oMini");
    expect(log.logType).toBe("stream");

    // Verify timing for streaming
    expect(log.timing.startTimeUtcMs).toBeGreaterThan(0);
    expect(log.timing.durationMs).toBeGreaterThan(0);

    // Verify usage is captured for streaming
    expect(log.usage.inputTokens).toBeGreaterThan(0);
    expect(log.usage.outputTokens).toBeGreaterThan(0);

    // Verify call details
    const call = log.calls[0];
    // Check if it's an LLMStreamCall by checking for sseResponses method
    expect('sseResponses' in call).toBe(true);
    
    expect(call.provider).toBe("openai");
    expect(call.clientName).toBe("GPT4oMini");
    
    // Cast to any to access sseResponses since TypeScript doesn't know about the union type
    const sseChunks = (call as any).sseResponses();
    expect(sseChunks).not.toBeNull();
    if (sseChunks) {
      expect(sseChunks.length).toBeGreaterThanOrEqual(chunksReceived.length);
      for (const chunk of sseChunks) {
        console.log(`Chunk: ${JSON.stringify(chunk.json())}`);
      }
    }

    // For streaming, http response should be null (as noted in existing test)
    expect(call.httpResponse).toBeNull();

    // But request should exist
    expect(call.httpRequest).not.toBeNull();
    const requestBody = call.httpRequest?.body.json();
    expect(requestBody?.stream).toBe(true); // Verify streaming was requested
  });

  it("should track cumulative usage across multiple calls", async () => {
    const collector = new Collector("my-collector");

    // First call
    await b.TestOpenAIGPT4oMini("First call", { collector });
    const functionLogs = collector.logs;
    expect(functionLogs.length).toBe(1);

    // Capture usage after first call
    const firstCallUsage = functionLogs[0].usage;
    expect(collector.usage.inputTokens).toBe(firstCallUsage.inputTokens);
    expect(collector.usage.outputTokens).toBe(firstCallUsage.outputTokens);
    expect(collector.usage.cachedInputTokens).toBe(firstCallUsage.cachedInputTokens);

    // Second call
    await b.TestOpenAIGPT4oMini("Second call", { collector });
    const updatedLogs = collector.logs;
    expect(updatedLogs.length).toBe(2);

    // Capture usage after second call and verify it's the sum of both calls
    const secondCallUsage = updatedLogs[1].usage;
    const totalInput =
      (firstCallUsage?.inputTokens ?? 0) + (secondCallUsage?.inputTokens ?? 0);
    const totalOutput =
      (firstCallUsage?.outputTokens ?? 0) +
      (secondCallUsage?.outputTokens ?? 0);
    const totalCachedInput =
      (firstCallUsage?.cachedInputTokens ?? 0) + (secondCallUsage?.cachedInputTokens ?? 0);
    expect(collector.usage.inputTokens).toBe(totalInput);
    expect(collector.usage.outputTokens).toBe(totalOutput);
    expect(collector.usage.cachedInputTokens).toBe(totalCachedInput);
  });

  it("should support multiple collectors", async () => {
    const coll1 = new Collector("collector-1");
    const coll2 = new Collector("collector-2");

    // Pass in both collectors for the first call
    await b.TestOpenAIGPT4oMini("First call", { collector: [coll1, coll2] });

    // Check usage/logs after the first call
    const logs1 = coll1.logs;
    const logs2 = coll2.logs;
    expect(logs1.length).toBe(1);
    expect(logs2.length).toBe(1);

    const usageFirstCallColl1 = logs1[0].usage;
    const usageFirstCallColl2 = logs2[0].usage;

    // Verify both collectors have the exact same usage for the first call
    expect(usageFirstCallColl1.inputTokens).toBe(
      usageFirstCallColl2.inputTokens,
    );
    expect(usageFirstCallColl1.outputTokens).toBe(
      usageFirstCallColl2.outputTokens,
    );
    expect(usageFirstCallColl1.cachedInputTokens).toBe(
      usageFirstCallColl2.cachedInputTokens,
    );

    // Also check that the collector-level usage matches the single call usage for each collector
    expect(coll1.usage.inputTokens).toBe(usageFirstCallColl1.inputTokens);
    expect(coll1.usage.outputTokens).toBe(usageFirstCallColl1.outputTokens);
    expect(coll1.usage.cachedInputTokens).toBe(usageFirstCallColl1.cachedInputTokens);
    expect(coll2.usage.inputTokens).toBe(usageFirstCallColl2.inputTokens);
    expect(coll2.usage.outputTokens).toBe(usageFirstCallColl2.outputTokens);

    // Second call uses only coll1
    await b.TestOpenAIGPT4oMini("Second call", { collector: coll1 });

    // Re-check logs/usage
    const updatedLogs1 = coll1.logs;
    const updatedLogs2 = coll2.logs;
    expect(updatedLogs1.length).toBe(2);
    expect(updatedLogs2.length).toBe(1);

    // Verify coll1 usage is now the sum of both calls
    const usageSecondCallColl1 = updatedLogs1[1].usage;
    const totalInput =
      (usageFirstCallColl1?.inputTokens ?? 0) +
      (usageSecondCallColl1?.inputTokens ?? 0);
    const totalOutput =
      (usageFirstCallColl1?.outputTokens ?? 0) +
      (usageSecondCallColl1?.outputTokens ?? 0);
    const totalCachedInput =
      (usageFirstCallColl1?.cachedInputTokens ?? 0) +
      (usageSecondCallColl1?.cachedInputTokens ?? 0);
    expect(coll1.usage.inputTokens).toBe(totalInput);
    expect(coll1.usage.outputTokens).toBe(totalOutput);
    expect(coll1.usage.cachedInputTokens).toBe(totalCachedInput);

    // Verify coll2 usage remains unchanged (it did not participate in the second call)
    expect(coll2.usage.inputTokens).toBe(usageFirstCallColl2.inputTokens);
    expect(coll2.usage.outputTokens).toBe(usageFirstCallColl2.outputTokens);
  });

  it("should handle parallel async calls correctly", async () => {
    const collector = new Collector("parallel-collector");

    // Execute two calls in parallel
    await Promise.all([
      b.TestOpenAIGPT4oMini("call #1", { collector }),
      b.TestOpenAIGPT4oMini("call #2", { collector }),
    ]);
    console.log("------------------------- ended parallel calls");

    // Verify the collector has two function logs
    const logs = collector.logs;
    expect(logs.length).toBe(2);

    // Ensure each call is recorded properly
    console.log("------------------------- logs iteration", logs);
    for (const log of logs) {
      expect(log.functionName).toBe("TestOpenAIGPT4oMini");
      expect(log.logType).toBe("call");
    }

    // Check usage for each call
    const usageCall1 = logs[0].usage;
    const usageCall2 = logs[1].usage;
    expect(usageCall1).not.toBeNull();
    expect(usageCall2).not.toBeNull();

    // Verify that total collector usage equals the sum of the two logs
    const totalInput =
      (usageCall1?.inputTokens ?? 0) + (usageCall2?.inputTokens ?? 0);
    const totalOutput =
      (usageCall1?.outputTokens ?? 0) + (usageCall2?.outputTokens ?? 0);
    const totalCachedInput =
      (usageCall1?.cachedInputTokens ?? 0) + (usageCall2?.cachedInputTokens ?? 0);
    expect(collector.usage.inputTokens).toBe(totalInput);
    expect(collector.usage.outputTokens).toBe(totalOutput);
  });

  it("should handle sync calls correctly", async () => {
    const collector = new Collector("sync-collector");
    const result = b_sync.TestOpenAIGPT4oMini("sync call", { collector });

    const logs = collector.logs;
    expect(logs.length).toBe(1);
    expect(logs[0].functionName).toBe("TestOpenAIGPT4oMini");
    expect(logs[0].logType).toBe("call");
    expect(logs[0].usage).not.toBeNull();
  });

  it("should handle multiple async calls with nested gathers", async () => {
    const collector = new Collector("my-collector");
    console.log("blabla");

    async function gatherBatch2() {
      await traceAsync("traceAsyncparent", () =>
        b.TestOpenAIGPT4oMini("hi there", { collector }),
      )();
    }

    async function gatherBatch1() {
      return await Promise.all([gatherBatch2()]);
    }

    await gatherBatch1();

    // expect(collector.usage.inputTokens).toBeGreaterThan(0);
    // expect(collector.usage.outputTokens).toBeGreaterThan(0);
  });

  it("should track cached input tokens for Anthropic caching", async () => {
    const collector = new Collector("caching-collector");

    // Create substantial content (2048+ tokens) to ensure caching triggers
    // Each repetition is ~100 tokens, so 25 repetitions = ~2500 tokens
    const largeContent = `
    In the ancient kingdom of Eldoria, there lived a brave knight named Sir Galahad who was known throughout the land for his unwavering courage, exceptional wisdom, and boundless compassion for all living creatures. His story began in the small village of Millbrook, where he was born to humble farmers who taught him the values of hard work, honesty, and kindness from a very young age.

    As a child, Galahad showed remarkable intelligence and an innate sense of justice. He would often help settle disputes between the village children and was always the first to defend those who were weaker or being bullied. His parents noticed these qualities and, though they were not wealthy, they saved every copper coin they could to provide him with the best education possible.

    When Galahad turned sixteen, a traveling knight named Sir Roderick visited their village. He immediately recognized the young man's potential and offered to take him as a squire. This was the opportunity of a lifetime, and though it broke their hearts to see him leave, Galahad's parents knew it was his destiny to serve a greater purpose.

    Under Sir Roderick's tutelage, Galahad learned not only the arts of combat and horsemanship but also the deeper principles of chivalry, honor, and service to others. He spent years training in various castles and courts, always demonstrating exceptional skill and character that earned him the respect of nobles and commoners alike.
    `.repeat(10);
    
    // First call - establishes cache (using cache_control in the BAML template)
    await b.TestCaching(largeContent, "What are the key virtues of Sir Galahad?", { collector });
    
    const firstLog = collector.logs[0];
    expect(firstLog).not.toBeNull();
    expect(firstLog.functionName).toBe("TestCaching");
    
    // Note first request may not have cached tokens
    // expect(firstLog.usage.cachedInputTokens).toBeDefined();
    // expect(firstLog.calls[0].usage?.cachedInputTokens).toBeDefined();
    
    // First call establishes cache, might have some cached tokens from cache creation
    const firstCachedTokens = firstLog.usage.cachedInputTokens || 0;
    
    // Second call with same large content - should use cache and show cached tokens > 0
    await b.TestCaching(largeContent, "What is Sir Galahad's background and origin?", { collector });
    
    const secondLog = collector.logs[1];
    expect(secondLog).not.toBeNull();
    expect(secondLog.functionName).toBe("TestCaching");
    
    // Verify cached tokens are tracked and should be > 0 for the second call
    expect(secondLog.usage.cachedInputTokens).toBeDefined();
    expect(secondLog.calls[0].usage?.cachedInputTokens).toBeDefined();
    
    // Third call to really ensure caching is working
    await b.TestCaching(largeContent, "How did Sir Galahad become a knight?", { collector });
    
    const thirdLog = collector.logs[2];
    expect(thirdLog).not.toBeNull();
    
    // At least one of the later calls should have cached tokens > 0
    const hasCachedTokens = 
      (secondLog.usage.cachedInputTokens || 0) > 0 || 
      (thirdLog.usage.cachedInputTokens || 0) > 0;
    
    expect(hasCachedTokens).toBe(true);
    
    // Verify collector aggregates cached tokens correctly
    const totalCachedTokens = 
      (collector.logs[0].usage.cachedInputTokens || 0) +
      (collector.logs[1].usage.cachedInputTokens || 0) +
      (collector.logs[2].usage.cachedInputTokens || 0);
    expect(collector.usage.cachedInputTokens).toBe(totalCachedTokens);
    
    console.log("Cached tokens - First call:", firstLog.usage.cachedInputTokens);
    console.log("Cached tokens - Second call:", secondLog.usage.cachedInputTokens);
    console.log("Cached tokens - Third call:", thirdLog.usage.cachedInputTokens);
    console.log("Total cached tokens:", collector.usage.cachedInputTokens);
    console.log("Large content length:", largeContent.length, "characters");
  });
});
