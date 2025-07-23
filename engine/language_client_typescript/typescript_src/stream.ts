import { toBamlError, BamlAbortError } from "./errors";
import type {
  FunctionResult,
  FunctionResultStream,
  RuntimeContextManager,
} from "../native";

export class BamlStream<PartialOutputType, FinalOutputType> {
  private task: Promise<FunctionResult> | null = null;
  private eventQueue: (FunctionResult | null)[] = [];
  private abortController: AbortController = new AbortController();
  private externalSignal: AbortSignal | null = null;
  private aborted = false;

  constructor(
    private ffiStream: FunctionResultStream,
    private partialCoerce: (result: any) => PartialOutputType,
    private finalCoerce: (result: any) => FinalOutputType,
    private ctxManager: RuntimeContextManager,
    options?: { signal?: AbortSignal }
  ) {
    // If an external signal is provided, link it to our internal controller
    if (options?.signal) {
      this.externalSignal = options.signal;
      
      // If the signal is already aborted, abort immediately
      if (this.externalSignal.aborted) {
        this.abort();
      } else {
        // Otherwise listen for abort events
        this.externalSignal.addEventListener('abort', () => {
          this.abort();
        });
      }
    }
  }

  /**
   * Aborts the stream processing.
   * This will stop any ongoing stream processing and clean up resources.
   */
  abort(): void {
    if (!this.aborted) {
      this.aborted = true;
      this.abortController.abort();
      this.eventQueue.push(null); // Signal end of stream
      this.ffiStream.onEvent(undefined); // Remove event handler
    }
  }

  private async driveToCompletion(): Promise<FunctionResult> {
    try {
      this.ffiStream.onEvent(
        (err: Error | null, data: FunctionResult | null) => {
          if (err) {
            return;
          } else {
            // Check if aborted before adding to queue
            if (!this.aborted) {
              this.eventQueue.push(data);
            }
          }
        }
      );
      
      // Set up abort signal handling
      this.abortController.signal.addEventListener('abort', () => {
        this.abort();
      });
      
      const retval = await this.ffiStream.done(this.ctxManager);
      return retval;
    } catch (error) {
      throw error;
    } finally {
      if (!this.eventQueue.includes(null)) {
        this.eventQueue.push(null);
      }
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
    // Check if already aborted before starting
    if (this.aborted) {
      throw new AbortError('Stream was aborted');
    }
    
    this.driveToCompletionInBg();

    while (true) {
      // Check if aborted during iteration
      if (this.aborted) {
        throw new AbortError('Stream was aborted');
      }
      
      const event = this.eventQueue.shift();

      if (event === undefined) {
        await new Promise((resolve) => setTimeout(resolve, 100));
        continue;
      }

      if (event === null) {
        break;
      }

      if (event.isOk()) {
        yield this.partialCoerce(event.parsed(true));
      }
    }
  }

  async getFinalResponse(): Promise<FinalOutputType> {
    // Check if aborted
    if (this.aborted) {
      throw new AbortError('Stream was aborted');
    }
    
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
          // Set up abort handling for the ReadableStream
          stream.signal.addEventListener('abort', () => {
            controller.enqueue(
              encoder.encode(JSON.stringify({ 
                error: { 
                  type: 'AbortError',
                  message: 'Stream was aborted by client',
                  prompt: '',
                  raw_output: '',
                } 
              }))
            );
            controller.close();
          });
          
          // Stream partials
          try {
            for await (const partial of stream) {
              controller.enqueue(encoder.encode(JSON.stringify({ partial })));
            }
          } catch (iterError) {
            if (iterError instanceof AbortError) {
              // Handle abort during iteration
              controller.enqueue(
                encoder.encode(JSON.stringify({ 
                  error: { 
                    type: 'AbortError',
                    message: 'Stream was aborted by client',
                    prompt: '',
                    raw_output: '',
                  } 
                }))
              );
              controller.close();
              return;
            }
            throw iterError; // Re-throw other errors
          }

          // If aborted, don't try to get final response
          if (stream.aborted) {
            return;
          }

          try {
            const final = await stream.getFinalResponse();
            controller.enqueue(encoder.encode(JSON.stringify({ final })));
            controller.close();
            return;
          } catch (err: unknown) {
            // Don't send error if aborted
            if (stream.aborted) {
              return;
            }
            
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
          // Don't send error if aborted
          if (stream.aborted) {
            return;
          }
          
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
      
      cancel() {
        // Handle stream cancellation by aborting the underlying stream
        stream.abort();
      }
    });
  }
  
  /**
   * Returns the AbortSignal associated with this stream.
   * This can be used to abort the stream from outside.
   */
  get signal(): AbortSignal {
    return this.abortController.signal;
  }
  
  /**
   * Returns whether the stream has been aborted.
   */
  get isAborted(): boolean {
    return this.aborted;
  }
}

/**
 * Custom error class for abort errors
 */
export class AbortError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AbortError';
  }
}
