import { FireBamlEvent } from "../ffi_layer";
import { BaseProvider } from "./base_provider";
import { LLMResponse } from "./llm_base_provider";

type PartialValue<T> = { delta: string } & ({
  is_parseable: true,
  parsed: Partial<T>,
} | {
  is_parseable: false,
  parsed: null,
});

class LLMResponseStream<T> implements AsyncIterable<PartialValue<T>> {
  #stream: AsyncIterable<LLMResponse>;
  #accumulated_content: string = "";
  #lastReceived: LLMResponse | null = null;
  #lastError: any = null;

  constructor(
    private stream: AsyncIterable<LLMResponse>,
    private readonly partialDeserialize: (partial: string) => Partial<T> | null,
    private readonly deserialize: (final: string) => T,
  ) {
    this.#stream = stream;
  }

  [Symbol.asyncIterator](): AsyncIterator<PartialValue<T>> {
    const iterator = this.stream[Symbol.asyncIterator]();

    // TODO: begin tracing
    return {
      next: async (): Promise<IteratorResult<PartialValue<T>>> => {
        try {
          // TODO: what happens if an error occurs during any single stream event?
          const { value, done } = await iterator.next();

          if (!done) {
            this.#lastReceived = value;
            this.#accumulated_content += value.generated;

            const parsed = this.partialDeserialize(this.#accumulated_content);
            if (parsed === null) {
              return {
                value: {
                  delta: value.generated,
                  is_parseable: false,
                  parsed,
                },
                done: false,
              };
            } else {
              return {
                value: {
                  delta: value.generated,
                  is_parseable: true,
                  parsed,
                },
                done: false,
              };
            }
          }

          // TODO: end tracing
          return { value: undefined, done: true };
        } catch (error) {
          this.#lastError = error;
          throw error;
        }
      },
      return: async () => {
        return { value: undefined, done: true };
      },
    };
  }

  async getFinalResponse(): Promise<T> {
    // If an error was thrown while consuming the stream, re-throw it.
    if (this.#lastError !== null) {
      throw this.#lastError
    }
    // Consume the rest of the stream.
    for await (const result of this) {}
    if (this.#lastReceived === null) {
      throw new Error("Never received a response from the LLM")
    }
    return this.deserialize(this.#accumulated_content);
  }
}

export { LLMResponseStream };
