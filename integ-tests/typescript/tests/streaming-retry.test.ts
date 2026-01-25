import { b, ClientRegistry } from "./test-setup";
import * as http from "http";

/**
 * Test for streaming retry bug:
 * When a streaming request fails and BAML retries, the chunks from the retried
 * stream should be yielded to the user.
 *
 * Bug report: Discord
 * - Using plain string streaming with Anthropic provider
 * - Ephemeral errors like "bad MAC" cause retries
 * - b.stream will RETRY the request, but will NOT yield the text chunks
 *   of the restarted stream. It yields nothing (silent fail).
 */

/**
 * Mock OpenAI-compatible streaming server that:
 * - Fails on the first N requests with a 500 error
 * - Succeeds on subsequent requests with a streaming response
 */
function createMockRetryServer(failCount: number = 1): {
  server: http.Server;
  getRequestCount: () => number;
} {
  let requestCount = 0;

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

    requestCount++;
    console.log(`Mock server: Request #${requestCount}`);

    // Fail the first N requests
    if (requestCount <= failCount) {
      console.log(`Mock server: Returning 500 error for request #${requestCount}`);
      res.writeHead(500, {
        "Content-Type": "application/json",
        "Access-Control-Allow-Origin": "*",
      });
      res.end(JSON.stringify({
        error: {
          message: "Internal Server Error (simulated failure for retry test)",
          type: "server_error",
          code: "internal_error"
        }
      }));
      return;
    }

    // Success case: return streaming response
    console.log(`Mock server: Returning streaming response for request #${requestCount}`);
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
      "Access-Control-Allow-Origin": "*",
    });

    // Send first chunk with role
    res.write(
      'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}\n\n',
    );

    // Send content chunks with small delays
    const chunks = ["Hello", " from", " retry", " success", "!"];
    for (let i = 0; i < chunks.length; i++) {
      await new Promise((resolve) => setTimeout(resolve, 50));
      res.write(
        `data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"${chunks[i]}"},"finish_reason":null}]}\n\n`,
      );
    }

    // Send final chunk with stop reason
    await new Promise((resolve) => setTimeout(resolve, 50));
    res.write(
      'data: {"id":"chatcmpl-mock","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n',
    );
    res.write("data: [DONE]\n\n");
    res.end();
  });

  return {
    server,
    getRequestCount: () => requestCount,
  };
}

describe("Streaming Retry Bug", () => {
  it("should receive chunks from retried stream after first attempt fails", async () => {
    // Create mock server that fails once then succeeds
    const { server, getRequestCount } = createMockRetryServer(1);

    await new Promise<void>((resolve) => {
      server.listen(0, () => resolve());
    });

    const address = server.address();
    if (!address || typeof address === "string") {
      throw new Error("Failed to get server address");
    }
    const port = address.port;
    const baseUrl = `http://localhost:${port}/v1`;

    try {
      // Create a client registry with our mock server and retry policy
      // Note: "Constant" retry policy is defined in retry.baml with max_retries: 3
      const registry = new ClientRegistry();
      registry.addLlmClient("MockRetryClient", "openai", {
        base_url: baseUrl,
        api_key: "mock-key",
        model: "gpt-4",
      }, "Constant"); // Use the Constant retry policy from retry.baml
      registry.setPrimary("MockRetryClient");

      // Use streaming with the mock client
      const stream = b.stream.TestStreamingTimeout("test retry streaming", {
        clientRegistry: registry,
      });

      // Collect all chunks
      const chunks: string[] = [];
      let chunkCount = 0;

      console.log("Starting to iterate over stream...");
      for await (const partial of stream) {
        chunkCount++;
        console.log(`Received chunk ${chunkCount}:`, partial);
        if (partial) {
          chunks.push(String(partial));
        }
      }

      console.log(`Total chunks received: ${chunkCount}`);
      console.log(`Total requests made: ${getRequestCount()}`);

      // Get final response
      const final = await stream.getFinalResponse();
      console.log("Final response:", final);

      // Verify we made 2 requests (1 failed + 1 succeeded)
      expect(getRequestCount()).toBe(2);

      // Verify we received chunks from the retry
      expect(chunkCount).toBeGreaterThan(0);

      // Verify the final response contains expected content
      expect(final).toContain("retry");
      expect(final).toContain("success");

    } finally {
      await new Promise<void>((resolve) => {
        server.close(() => resolve());
      });
    }
  });

  it("should receive chunks from retried stream after multiple failures", async () => {
    // Create mock server that fails twice then succeeds
    const { server, getRequestCount } = createMockRetryServer(2);

    await new Promise<void>((resolve) => {
      server.listen(0, () => resolve());
    });

    const address = server.address();
    if (!address || typeof address === "string") {
      throw new Error("Failed to get server address");
    }
    const port = address.port;
    const baseUrl = `http://localhost:${port}/v1`;

    try {
      const registry = new ClientRegistry();
      registry.addLlmClient("MockRetryClient", "openai", {
        base_url: baseUrl,
        api_key: "mock-key",
        model: "gpt-4",
      }, "Constant");
      registry.setPrimary("MockRetryClient");

      const stream = b.stream.TestStreamingTimeout("test multiple retry streaming", {
        clientRegistry: registry,
      });

      const chunks: string[] = [];
      let chunkCount = 0;

      for await (const partial of stream) {
        chunkCount++;
        if (partial) {
          chunks.push(String(partial));
        }
      }

      const final = await stream.getFinalResponse();

      // Verify we made 3 requests (2 failed + 1 succeeded)
      expect(getRequestCount()).toBe(3);

      // Verify we received chunks from the successful retry
      expect(chunkCount).toBeGreaterThan(0);
      expect(final).toContain("retry");

    } finally {
      await new Promise<void>((resolve) => {
        server.close(() => resolve());
      });
    }
  });

  it("should throw error only after all retries are exhausted", async () => {
    // Create mock server that always fails (more failures than retries)
    const { server, getRequestCount } = createMockRetryServer(10);

    await new Promise<void>((resolve) => {
      server.listen(0, () => resolve());
    });

    const address = server.address();
    if (!address || typeof address === "string") {
      throw new Error("Failed to get server address");
    }
    const port = address.port;
    const baseUrl = `http://localhost:${port}/v1`;

    try {
      const registry = new ClientRegistry();
      registry.addLlmClient("MockRetryClient", "openai", {
        base_url: baseUrl,
        api_key: "mock-key",
        model: "gpt-4",
      }, "Constant"); // max_retries: 3, so 4 total attempts
      registry.setPrimary("MockRetryClient");

      const stream = b.stream.TestStreamingTimeout("test exhausted retries", {
        clientRegistry: registry,
      });

      // Should throw after all retries exhausted
      await expect(async () => {
        for await (const partial of stream) {
          // Should not receive any successful chunks
        }
      }).rejects.toThrow();

      // Verify all retry attempts were made (1 initial + 3 retries = 4)
      expect(getRequestCount()).toBe(4);

    } finally {
      await new Promise<void>((resolve) => {
        server.close(() => resolve());
      });
    }
  });
});
