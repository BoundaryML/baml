import { b, b_sync, BamlClientHttpError, BamlTimeoutError } from "./test-setup";
import { ClientRegistry } from "@boundaryml/baml";
import * as http from "http";

// Mock OpenAI-compatible streaming server that sends many chunks with delays
// This will send 200 chunks with 10ms between each, taking ~2 seconds total.
// We will use this to test prompt return from BAML clients specifying an
// idle_timeout_ms.
function createMockStreamingServer(): http.Server {
  const server = http.createServer(async (req, res) => {
    // Handle OPTIONS for CORS
    if (req.method === "OPTIONS") {
      res.writeHead(200, {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "POST, OPTIONS",
        "Access-Control-Allow-Headers": "Content-Type, Authorization",
      });
      res.end();
      return;
    }

    // Only handle POST to /v1/chat/completions
    if (req.method !== "POST" || req.url !== "/v1/chat/completions") {
      res.writeHead(404);
      res.end();
      return;
    }

    // Set up SSE headers
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
      "Access-Control-Allow-Origin": "*",
    });

    // Send first chunk immediately with role
    res.write(
      'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":"Chunk 0 "},"finish_reason":null}]}\n\n',
    );

    // Send 199 more chunks with 10ms delay between each
    // BUT chunk 3 has a 500ms delay to trigger the idle timeout
    // Total time would be 198 * 10ms + 500ms = 2480ms (~2.5 seconds)
    for (let i = 1; i < 200; i++) {
      // Special case: 500ms delay before chunk 3 to trigger idle timeout
      if (i === 3) {
        await new Promise((resolve) => setTimeout(resolve, 500));
      } else {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }

      res.write(
        `data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Chunk ${i} "},"finish_reason":null}]}\n\n`,
      );
    }

    // Send final chunk
    res.write(
      'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n',
    );
    res.write("data: [DONE]\n\n");
    res.end();
  });

  return server;
}

describe("Timeout Tests", () => {
  it("should raise BamlTimeoutError for connect timeout", async () => {
    await expect(async () => {
      await b.TestTimeoutError("test input");
    }).rejects.toThrow("timed out");

    try {
      await b.TestTimeoutError("test input");
      fail("Expected TestTimeoutError to throw");
    } catch (error: any) {
      expect(error.message.toLowerCase()).toContain("time");
      // Verify it's the right error type
      expect(error).toBeInstanceOf(BamlTimeoutError);
      expect(error).toBeInstanceOf(BamlClientHttpError); // Should inherit from BamlClientHttpError
    }
  });

  it("should raise BamlTimeoutError for request timeout", async () => {
    const startTime = Date.now();

    try {
      await b.TestRequestTimeout("climate change and its effects");
      fail("Expected TestRequestTimeout to throw");
    } catch (error: any) {
      const elapsed = (Date.now() - startTime) / 1000;
      // Should fail quickly (within ~200ms accounting for overhead)
      expect(elapsed).toBeLessThan(0.2);

      expect(error.message.toLowerCase()).toContain("timeout");
      expect(error).toBeInstanceOf(BamlTimeoutError);
    }
  });

  it("should prioritize abort signal over timeout", async () => {
    const controller = new AbortController();

    // Schedule abort after 25ms
    setTimeout(() => {
      controller.abort();
    }, 25);

    // Use a client with 100ms timeout
    try {
      await b.TestRequestTimeout("test input", {
        signal: controller.signal,
      });
      fail("Expected to throw an error");
    } catch (error: any) {
      // Should get abort error, not timeout error
      const errorStr = error.message.toLowerCase();
      expect(errorStr.includes("abort") || errorStr.includes("cancel")).toBe(
        true,
      );
      // Should NOT be a timeout error
      expect(error).not.toBeInstanceOf(BamlTimeoutError);
    }
  });

  it("should handle timeout in synchronous context", () => {
    try {
      b_sync.TestTimeoutError("test input");
      fail("Expected TestTimeoutError to throw");
    } catch (error: any) {
      expect(error.message.toLowerCase()).toContain("timeout");
      expect(error).toBeInstanceOf(BamlTimeoutError);
    }
  });

  it("should raise BamlTimeoutError for streaming timeout", async () => {
    // TestStreamingTimeout has time_to_first_token_timeout_ms: 1 and idle_timeout_ms: 1
    // With 1ms timeouts, it should definitely timeout
    const startTime = Date.now();

    await expect(async () => {
      const stream = b.stream.TestStreamingTimeout(
        "Write a very long essay about the history of computing",
      );

      // Collect stream results - this should timeout
      for await (const chunk of stream) {
        // Should not reach here due to timeout
      }
    }).rejects.toThrow();

    // Now verify it's the right kind of error and timing
    const startTime2 = Date.now();
    try {
      const stream = b.stream.TestStreamingTimeout(
        "Write a very long essay about the history of computing",
      );
      for await (const chunk of stream) {
        // Should not reach here
      }
      throw new Error("Should have thrown timeout error");
    } catch (error: any) {
      const elapsed = (Date.now() - startTime2) / 1000;

      if (error.message === "Should have thrown timeout error") {
        throw error;
      }

      // Note: Most of this time is connection establishment (~1s).
      // The actual streaming timeout detection happens in ~2ms (see BAML logs).
      // Streaming timeouts only apply AFTER the connection is established.
      // Verify it times out within 3s (connection + timeout detection)
      expect(elapsed).toBeLessThan(3.0);

      // Verify it's a timeout error
      expect(error.message.toLowerCase()).toContain("timeout");
      expect(error).toBeInstanceOf(BamlTimeoutError);
    }
  });

  it("should timeout on idle with mock server", async () => {
    // Start mock server
    const mockServer = createMockStreamingServer();
    await new Promise<void>((resolve) => {
      mockServer.listen(0, () => resolve()); // Use random port
    });

    const address = mockServer.address();
    if (!address || typeof address === "string") {
      throw new Error("Failed to get server address");
    }
    const port = address.port;
    const baseUrl = `http://localhost:${port}/v1`;

    try {
      // Create a client registry with our mock server
      const registry = new ClientRegistry();
      registry.addLlmClient("MockIdleClient", "openai", {
        base_url: baseUrl,
        api_key: "mock-key",
        model: "gpt-4",
        http: {
          idle_timeout_ms: 200, // 200ms idle timeout - should trigger during the 500ms delay before chunk 3
        },
      });
      registry.setPrimary("MockIdleClient");

      const startTime = Date.now();

      try {
        // Use the TestStreamingTimeout function with our mock client
        const stream = b.stream.TestStreamingTimeout("test with mock server", {
          clientRegistry: registry,
        });

        let chunkCount = 0;
        for await (const chunk of stream) {
          chunkCount++;
          // Don't log every chunk to avoid spam with 200 chunks
          if (chunkCount <= 5 || chunkCount % 50 === 0) {
            console.log(
              `Received chunk ${chunkCount}: ${JSON.stringify(chunk).substring(0, 50)}`,
            );
          }
        }

        throw new Error("Should have thrown timeout error");
      } catch (error: any) {
        const elapsed = (Date.now() - startTime) / 1000;

        if (error.message === "Should have thrown timeout error") {
          throw error;
        }

        console.log(`Mock server timeout took ${elapsed.toFixed(3)} seconds`);

        // The mock server would take ~2.5 seconds to send all 200 chunks without timeout
        // (chunks 0-2: ~20ms, 500ms delay, chunks 3-199: ~1970ms = ~2.5s total)
        // With the 200ms idle timeout, it should short-circuit during the 500ms delay.
        // Expected time: ~20ms (chunks 0-2) + 200ms (timeout) = ~220ms
        // Allow up to 1 second for safety to ensure it's much less than the full 2.5s
        expect(elapsed).toBeLessThan(1.0);

        // Verify it's a timeout error
        expect(error.message.toLowerCase()).toContain("timeout");
        expect(error).toBeInstanceOf(BamlTimeoutError);
      }
    } finally {
      // Clean up server
      await new Promise<void>((resolve) => {
        mockServer.close(() => resolve());
      });
    }
  });

  it("should succeed with fallback when first client times out", async () => {
    // The first client in fallback should timeout, but the second should succeed
    const result = await b.TestTimeoutFallback("hello world");

    // Should have succeeded with the second client
    expect(result).toBeDefined();
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(10); // Should have gotten a reasonable response
  });

  it("should treat zero timeout as infinite", async () => {
    // This should succeed despite having 0 timeout (infinite)
    const result = await b.TestZeroTimeout("test infinite timeout");

    // Should succeed (no exception raised, got a valid response)
    expect(result).toBeDefined();
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(10); // Should have gotten a reasonable response
  });

  it("should include client name in timeout error message", async () => {
    try {
      await b.TestTimeoutError("test");
      fail("Expected TestTimeoutError to throw");
    } catch (error: any) {
      const errorStr = error.message;
      // Should mention the client name somewhere in the error
      expect(
        errorStr.includes("TestTimeoutClient") ||
          errorStr.toLowerCase().includes("client"),
      ).toBe(true);
    }
  });
});
