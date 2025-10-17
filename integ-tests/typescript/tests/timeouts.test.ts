import { b, b_sync, BamlClientHttpError, BamlTimeoutError } from "./test-setup";

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

  it("should skip streaming timeout test (not yet implemented)", async () => {
    // This test would be for Phase 4, but adding placeholder
    // Skip for now as streaming timeouts are not yet implemented
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
