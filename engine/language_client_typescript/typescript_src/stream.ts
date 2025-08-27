import { toBamlError, BamlAbortError } from "./errors";
import type {
  FunctionResult,
  FunctionResultStream,
  RuntimeContextManager,
} from "../native";

export class BamlStream<PartialOutputType, FinalOutputType> {
  private task: Promise<FunctionResult> | null = null;
  private error: Error | null = null;

  private eventQueue: (FunctionResult | null)[] = [];
  private abortSignal?: AbortSignal;

  constructor(
    private ffiStream: FunctionResultStream,
    private partialCoerce: (result: any) => PartialOutputType,
    private finalCoerce: (result: any) => FinalOutputType,
    private ctxManager: RuntimeContextManager,
    abortSignal?: AbortSignal
  ) {
    this.abortSignal = abortSignal;

    // Listen for abort to clean up
    if (abortSignal) {
      abortSignal.addEventListener("abort", () => {
        this.eventQueue.push(null); // Signal end of stream
      });
    }
  }

  private async driveToCompletion(): Promise<FunctionResult> {
    try {
      // Check for early abort
      if (this.abortSignal?.aborted) {
        throw new BamlAbortError(
          "Operation was aborted",
          this.abortSignal.reason
        );
      }

      this.ffiStream.onEvent(
        (err: Error | null, data: FunctionResult | null) => {
          if (err) {
            this.error = err;
            return;
          }

          this.eventQueue.push(data);
        }
      );

      const retval = await this.ffiStream.done(this.ctxManager);

      // Check if we have an error to throw
      if (this.error) {
        throw this.error;
      }

      return retval;
    } catch (error) {
      if (error instanceof BamlAbortError) {
        this.error = error;
        this.eventQueue.push(null);
      }
      throw error;
    } finally {
      this.eventQueue.push(null);
      this.ffiStream.onEvent(undefined);
    }
  }

  private driveToCompletionInBg(): Promise<FunctionResult> {
    if (this.task === null) {
      this.task = this.driveToCompletion();
    }

    return this.task;
  }

  async *[Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType> {
    this.driveToCompletionInBg();

    while (true) {
      // Check if we have an error to throw
      if (this.error) {
        throw this.error;
      }

      const event = this.eventQueue.shift();

      if (event === undefined) {
        await new Promise((resolve) => setTimeout(resolve, 100));
        continue;
      }

      if (event === null) {
        // Check one more time for any error before ending
        if (this.error) {
          throw this.error;
        }
        break;
      }

      if (event.isOk()) {
        yield this.partialCoerce(event.parsed(true));
      }
    }
  }

  async getFinalResponse(): Promise<FinalOutputType> {
    const final = await this.driveToCompletionInBg();

    return this.finalCoerce(final.parsed(false));
  }

  /**
   * Converts the BAML stream to a Next.js compatible stream.
   * This is used for server-side streaming in Next.js API routes and Server Actions.
   * The stream emits JSON-encoded messages containing either:
   * - Partial results of type PartialOutputType
   * - Final result of type FinalOutputType
   * - Error information
   */
  toStreamable(): ReadableStream<Uint8Array> {
    const stream = this;
    const encoder = new TextEncoder();

    return new ReadableStream({
      async start(controller) {
        try {
          // Stream partials
          for await (const partial of stream) {
            controller.enqueue(encoder.encode(JSON.stringify({ partial })));
          }

          try {
            const final = await stream.getFinalResponse();
            controller.enqueue(encoder.encode(JSON.stringify({ final })));
            controller.close();
            return;
          } catch (err: unknown) {
            const bamlError = toBamlError(
              err instanceof Error ? err : new Error(String(err))
            );
            controller.enqueue(
              encoder.encode(JSON.stringify({ error: bamlError }))
            );
            controller.close();
            return;
          }
        } catch (streamErr: unknown) {
          const errorPayload = {
            type: "StreamError",
            message:
              streamErr instanceof Error
                ? streamErr.message
                : "Error in stream processing",
            prompt: "",
            raw_output: "",
          };

          controller.enqueue(
            encoder.encode(JSON.stringify({ error: errorPayload }))
          );
          controller.close();
        }
      },
    });
  }
}
