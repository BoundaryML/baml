import { FireBamlEvent } from "../ffi_layer";
import { BaseProvider } from "./base_provider";
import { LLMResponse } from "./llm_base_provider";

class LLMResponseStream<T> implements AsyncIterable<Partial<T>> {
  #stream: AsyncIterable<LLMResponse>;
  #lastReceived: LLMResponse | null = null;
  
  #connectedPromise: Promise<void>;
  #resolveConnectedPromise: () => void = () => {};
  #rejectConnectedPromise: (error: Error) => void = () => {};

  #endPromise: Promise<void>;
  #resolveEndPromise: () => void = () => {};
  #rejectEndPromise: (error: any) => void = () => {};

  constructor(
    private stream: AsyncIterable<LLMResponse>,
    private readonly partialDeserialize: (partial: LLMResponse) => Partial<T>,
    private readonly deserialize: (final: LLMResponse) => T,
  ) {
    this.#stream = stream;

    this.#connectedPromise = new Promise<void>((resolve, reject) => {
      this.#resolveConnectedPromise = resolve;
      this.#rejectConnectedPromise = reject;
    });

    this.#endPromise = new Promise<void>((resolve, reject) => {
      this.#resolveEndPromise = resolve;
      this.#rejectEndPromise = reject;
    });

    // Don't let these promises cause unhandled rejection errors.
    // we will manually cause an unhandled rejection error later
    // if the user hasn't registered any error listener or called
    // any promise-returning method.
    this.#connectedPromise.catch(() => {});
    this.#endPromise.catch(() => {});
  }

  [Symbol.asyncIterator](): AsyncIterator<Partial<T>> {
    const iterator = this.stream[Symbol.asyncIterator]();

    // TODO: begin tracing
    return {
      next: async (): Promise<IteratorResult<Partial<T>>> => {
        try {
          // TODO: what happens if an error occurs during any single stream event?
          const { value, done } = await iterator.next();

          if (!done) {
            this.#lastReceived = value;
            return { value: this.partialDeserialize(value), done: false };
          }

          // TODO: end tracing
          this.#resolveEndPromise();
          return { value: undefined, done: true };
        } catch (error) {
          this.#rejectEndPromise(error);
          throw error;
        }
      },
      return: async () => {
        return { value: undefined, done: true };
      },
    };
  }

  async getFinalResponse(): Promise<T> {
    await this.#endPromise;
    if (this.#lastReceived === null) {
      throw new Error("Never received a response from the LLM")
    }
    return this.deserialize(this.#lastReceived);
  }
}

export { LLMResponseStream };
