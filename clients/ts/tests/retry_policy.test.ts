import { LLMException, ProviderErrorCode } from "../src/client_manager/errors";
import { ConstantDelayRetryPolicy, ExponentialBackoffRetryPolicy } from "../src/client_manager/retry_policy";

class TestOperationRunner {
  attempts = 0;

  constructor(readonly outcomes: ("success" | number)[]) {}

  async run(): Promise<"success"> {
    const attemptNumber = this.attempts + 1;
    const outcome = this.outcomes[this.attempts];
    this.attempts += 1;

    if (outcome === "success") {
      return "success";
    }

    throw new LLMException(`synthetic exception for testing, attempt ${attemptNumber}`, outcome);
  }
}

describe("constant delay retry policy", () => {
  const max_retries = 3;
  const delay_ms = 1;

  test("first call succeeds", async () => {
    const op = new TestOperationRunner([
      "success",
      ProviderErrorCode.InternalError,
      ProviderErrorCode.RateLimitExceeded,
      ProviderErrorCode.ServiceUnavailable,
    ]);
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ConstantDelayRetryPolicy("constant delay test", max_retries, { delay_ms });

    await expect(retry_policy.run(async () => await op.run())).resolves.toBe("success");
    expect(op.attempts).toBe(1);
  });

  test("third call succeeds", async () => {
    const op = new TestOperationRunner([
      ProviderErrorCode.InternalError,
      ProviderErrorCode.RateLimitExceeded,
      "success",
      ProviderErrorCode.ServiceUnavailable,
    ]);
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ConstantDelayRetryPolicy("constant delay test", max_retries, { delay_ms });

    await expect(retry_policy.run(async () => await op.run())).resolves.toBe("success");
    expect(op.attempts).toBe(3);
  });

  test("max_retries reached", async () => {
    const op = new TestOperationRunner([
      ProviderErrorCode.ServiceUnavailable,
      ProviderErrorCode.InternalError,
      ProviderErrorCode.RateLimitExceeded,
      ProviderErrorCode.ServiceUnavailable,
    ]);
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ConstantDelayRetryPolicy("constant delay test", max_retries, { delay_ms });

    const expectError = expect(retry_policy.run(async () => await op.run())).rejects;
    await expectError.toThrow(LLMException);
    expectError.toThrow("Retry policy exhausted");
    expectError.toThrow(/Service Unavailable.*attempt 1/);
    expectError.toThrow(/Internal Error.*attempt 2/);
    expectError.toThrow(/Rate Limit Exceeded.*attempt 3/);
    expect(op.attempts).toBe(max_retries);
  });
});

describe("exponential backoff retry policy", () => {
  const max_retries = 3;
  const delay_ms = 1;
  const max_delay_ms = 100;
  const multiplier = 2;

  test("first call succeeds", async () => {
    const op = new TestOperationRunner([
      "success",
      ProviderErrorCode.InternalError,
      ProviderErrorCode.RateLimitExceeded,
      ProviderErrorCode.ServiceUnavailable,
    ]);
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ExponentialBackoffRetryPolicy(
      "exponential backoff test", max_retries, { delay_ms, max_delay_ms, multiplier });

    await expect(retry_policy.run(async () => await op.run())).resolves.toBe("success");
    expect(op.attempts).toBe(1);
  });

  test("third call succeeds", async () => {
    const op = new TestOperationRunner([
      ProviderErrorCode.InternalError,
      ProviderErrorCode.RateLimitExceeded,
      "success",
      ProviderErrorCode.ServiceUnavailable,
    ]);
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ExponentialBackoffRetryPolicy(
      "exponential backoff test", max_retries, { delay_ms, max_delay_ms, multiplier });

    await expect(retry_policy.run(async () => await op.run())).resolves.toBe("success");
    expect(op.attempts).toBe(3);
  });

  test("max_retries reached", async () => {
    const op = new TestOperationRunner(Array(10).fill(ProviderErrorCode.ServiceUnavailable));
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ExponentialBackoffRetryPolicy(
      "exponential backoff test", max_retries, { delay_ms, max_delay_ms, multiplier });

    await expect(retry_policy.run(async () => await op.run())).rejects.toThrow("Retry policy exhausted");
    expect(op.attempts).toBe(max_retries);
  });

  test("max_delay_ms reached", async () => {
    const max_retries = 10;
    const max_delay_ms = 50;
    const op = new TestOperationRunner(Array(20).fill(ProviderErrorCode.ServiceUnavailable));
    expect(op.outcomes.length).toBeGreaterThan(max_retries);
    const retry_policy = new ExponentialBackoffRetryPolicy(
      "exponential backoff test", max_retries, { delay_ms, max_delay_ms, multiplier });

    await expect(retry_policy.run(async () => await op.run())).rejects.toThrow("Retry policy exhausted");
    expect(op.attempts).toBeLessThan(max_retries)
    expect(delay_ms * Math.pow(multiplier, op.attempts - 1)).toBeGreaterThan(max_delay_ms);
    // attempt 1, sleep 1ms
    // attempt 2, sleep 2ms
    // attempt 3, sleep 4ms
    // attempt 4, sleep 8ms
    // attempt 5, sleep 16ms
    // attempt 6, sleep 32ms
    // attempt 7, 64ms > max_delay_ms so stop
    expect(op.attempts).toBe(7);
  });
});
