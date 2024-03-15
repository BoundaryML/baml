import { LLMException, TerminalErrorCodes } from "./errors";

const sleep = (delay_ms: number) => new Promise(resolve => setTimeout(resolve, delay_ms));

export abstract class RetryPolicy {
  abstract run<T>(fn: () => Promise<T>): Promise<T>;
}

export class ConstantDelayRetryPolicy {
  private readonly max_retries: number;
  private readonly delay_ms: number;

  constructor(
    readonly policy_name: string,
    max_retries: number,
    { delay_ms = 200 }: { delay_ms?: number },
  ) {
    this.max_retries = Math.max(1, max_retries);
    this.delay_ms = delay_ms;
  }

  async run<T>(fn: () => Promise<T>): Promise<T> {
    const errors = [];
    for (let i = 0; i < this.max_retries; i++) {
      try {
        return await fn();
      } catch (err) {
        if (!(err instanceof LLMException)) {
          throw err;
        }

        if (TerminalErrorCodes.includes(err.code)) {
          throw err;
        }

        errors.push(err);
      }
      if (i === this.max_retries - 1) {
        break;
      }
      await sleep(this.delay_ms);
    }
    throw LLMException.from_retry_errors(
      errors,
      { 
        policy_name: this.policy_name,
        type: "constant_delay",
        max_retries: this.max_retries,
        delay_ms: this.delay_ms
      }
    );
  }
}

export class ExponentialBackoffRetryPolicy {
  private readonly max_retries: number;
  private readonly delay_ms: number;
  private readonly max_delay_ms: number;
  private readonly multiplier: number;

  constructor(
    readonly policy_name: string,
    max_retries: number,
    {
        delay_ms = 200,
        max_delay_ms = 10000,
        multiplier = 1.5,
    } : {
        delay_ms?: number,
        max_delay_ms?: number,
        multiplier?: number,
    }
  ) {
    this.max_retries = Math.max(1, max_retries);
    this.delay_ms = delay_ms;
    this.max_delay_ms = max_delay_ms ;
    this.multiplier = multiplier;
  }

  async run<T>(fn: () => Promise<T>): Promise<T> {
    const errors = [];
    for (let i = 0; i < this.max_retries; i++) {
      const delay_ms = this.delay_ms * Math.pow(this.multiplier, i);
      try {
        return await fn();
      } catch (err) {
        if (!(err instanceof LLMException)) {
          throw err;
        }

        if (TerminalErrorCodes.includes(err.code)) {
          throw err;
        }

        errors.push(err);
      }
      if (delay_ms > this.max_delay_ms) {
        break;
      }
      if (i === this.max_retries - 1) {
        break;
      }
      await sleep(delay_ms);
    }

    throw LLMException.from_retry_errors(
      errors,
      { 
        policy_name: this.policy_name,
        type: "exponential_backoff",
        max_retries: this.max_retries,
        delay_ms: this.delay_ms,
        max_delay_ms: this.max_delay_ms,
        multiplier: this.multiplier,
      }
    );
  }
}