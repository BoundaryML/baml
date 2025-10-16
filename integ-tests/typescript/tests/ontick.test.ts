// Using Jest - no need to import describe, it, expect, beforeEach as they are globals
import { b } from "../baml_client";
import { b as syncB } from "../baml_client/sync_client";
import { flush } from "../baml_client/tracing";
import type { FunctionLog, LlmStreamCall } from "@boundaryml/baml";

type TickReason = "Unknown";

function getOnTick(): {
  onTick: (reason: TickReason, log: FunctionLog | null) => void;
  tickEvents: Array<[TickReason, string | null]>;
  tickCount: number[];
} {
  let tickCount = [0];
  const tickEvents: Array<[TickReason, string | null]> = [];
  let lastThinking = "";

  const onTick = (reason: TickReason, log: FunctionLog | null) => {
    tickCount[0] = tickCount[0] + 1;

    // Extract thinking content if available
    if (log?.calls) {
      const lastCall = log.calls[log.calls.length - 1];
      // Check if it's a stream call
      if (lastCall && "sseResponses" in lastCall) {
        const streamCall = lastCall as LlmStreamCall;
        const responses = streamCall.sseResponses();
        if (responses) {
          for (const response of responses) {
            try {
              const data = JSON.parse(response.text);
              if (data.delta?.thinking) {
                lastThinking += data.delta.thinking;
              }
            } catch {
              // Ignore parse errors
            }
          }
        }
      }
    }

    tickEvents.push([reason, lastThinking]);
  };

  return { onTick, tickEvents, tickCount };
}

describe("Experimental OnTick", () => {
  beforeEach(() => {
    flush();
  });

  it("should fire onTick callbacks for async non-streaming function", async () => {
    const { onTick, tickEvents, tickCount } = getOnTick();

    const result = await b.TestAnthropicShorthand("Hello world", { onTick });

    expect(result).toBeDefined();
    expect(tickCount[0]).toBeGreaterThan(0);
    console.log(`Total ticks: ${tickCount}`);
  });

  it("should fire onTick callbacks for streaming function", async () => {
    const { onTick, tickEvents, tickCount } = getOnTick();

    const stream = b.stream.TestAnthropicShorthand("Hello world", { onTick });

    const messages = [];
    for await (const msg of stream) {
      messages.push(msg);
    }

    const finalResult = await stream.getFinalResponse();

    expect(finalResult).toBeDefined();
    expect(tickCount[0]).toBeGreaterThan(0);
    expect(messages.length).toBeGreaterThan(0);
    console.log(`Total ticks: ${tickCount[0]}, messages: ${messages.length}`);
  });

  it("should throw error for sync functions with onTick", () => {
    const { onTick } = getOnTick();

    expect(() => {
      syncB.TestAnthropicShorthand("Hello world", { onTick });
    }).toThrow("onTick is not supported for synchronous functions");
  });

  it("should handle onTick callback errors", async () => {
    // Function should complete despite callback error
    // expect error

    let tickCount = 0;
    const onTick = (_: TickReason, __: FunctionLog | null) => {
      tickCount++;
      if (tickCount === 5) {
        throw new Error("Intentional error in onTick");
      }
    };

    const result = await b.TestAnthropicShorthand("Hello world", { onTick });

    expect(result).toBeDefined();
    expect(tickCount).toBeGreaterThanOrEqual(5);
  });

  it("should not significantly impact performance", async () => {
    // Run without onTick
    const startNoTick = Date.now();
    const resultNoTick = await b.TestAnthropicShorthand("Hello world");
    const timeNoTick = Date.now() - startNoTick;

    // Run with onTick
    const { onTick, tickCount } = getOnTick();
    const startWithTick = Date.now();
    const resultWithTick = await b.TestAnthropicShorthand("Hello world", {
      onTick,
    });
    const timeWithTick = Date.now() - startWithTick;

    expect(resultNoTick).toBeDefined();
    expect(resultWithTick).toBeDefined();

    // Allow 50% overhead
    expect(timeWithTick).toBeLessThan(timeNoTick * 1.5);

    console.log(`Time without onTick: ${timeNoTick}ms`);
    console.log(`Time with onTick: ${timeWithTick}ms`);
    console.log(
      `Overhead: ${(((timeWithTick - timeNoTick) / timeNoTick) * 100).toFixed(
        1
      )}%`
    );
    console.log(`Total ticks: ${tickCount[0]}`);
  });
});
