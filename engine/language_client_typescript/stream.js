"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlStream = void 0;
const errors_1 = require("./errors");
class BamlStream {
    ffiStream;
    partialCoerce;
    finalCoerce;
    ctxManager;
    task = null;
    error = null;
    eventQueue = [];
    abortSignal;
    constructor(ffiStream, partialCoerce, finalCoerce, ctxManager, abortSignal) {
        this.ffiStream = ffiStream;
        this.partialCoerce = partialCoerce;
        this.finalCoerce = finalCoerce;
        this.ctxManager = ctxManager;
        this.abortSignal = abortSignal;
        // Listen for abort to clean up
        if (abortSignal) {
            abortSignal.addEventListener("abort", () => {
                this.eventQueue.push(null); // Signal end of stream
            });
        }
    }
    async driveToCompletion() {
        try {
            // Check for early abort
            if (this.abortSignal?.aborted) {
                throw new errors_1.BamlAbortError("Operation was aborted", this.abortSignal.reason);
            }
            this.ffiStream.onEvent((err, data) => {
                if (err) {
                    this.error = err;
                    return;
                }
                this.eventQueue.push(data);
            });
            const retval = await this.ffiStream.done(this.ctxManager);
            // Check if we have an error to throw
            if (this.error) {
                throw this.error;
            }
            return retval;
        }
        catch (error) {
            if (error instanceof errors_1.BamlAbortError) {
                this.error = error;
                this.eventQueue.push(null);
            }
            throw error;
        }
        finally {
            this.eventQueue.push(null);
            this.ffiStream.onEvent(undefined);
        }
    }
    driveToCompletionInBg() {
        if (this.task === null) {
            this.task = this.driveToCompletion();
        }
        return this.task;
    }
    async *[Symbol.asyncIterator]() {
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
            else {
                // Event contains an error (e.g., timeout, LLM failure)
                // Try to parse it to get the proper error, which will throw
                try {
                    event.parsed(true);
                }
                catch (error) {
                    throw (0, errors_1.toBamlError)(error);
                }
            }
        }
    }
    async getFinalResponse() {
        const final = await this.driveToCompletionInBg();
        return this.finalCoerce(final.parsed(false));
    }
    /**
     * Converts the BAML stream to a Next.js compatible stream.
     * This is used for server-side streaming in Next.js API routes and Server Actions.
     * The stream emits newline-delimited JSON (NDJSON) messages containing either:
     * - Partial results of type PartialOutputType
     * - Final result of type FinalOutputType
     * - Error information
     *
     * Each message is a JSON object followed by a newline character.
     * This format handles TCP chunking correctly - messages can be split across
     * chunks or multiple messages can arrive in a single chunk.
     */
    toStreamable() {
        const stream = this;
        const encoder = new TextEncoder();
        return new ReadableStream({
            async start(controller) {
                try {
                    // Stream partials - each message ends with newline for NDJSON format
                    for await (const partial of stream) {
                        controller.enqueue(encoder.encode(JSON.stringify({ partial }) + "\n"));
                    }
                    try {
                        const final = await stream.getFinalResponse();
                        controller.enqueue(encoder.encode(JSON.stringify({ final }) + "\n"));
                        controller.close();
                        return;
                    }
                    catch (err) {
                        const bamlError = (0, errors_1.toBamlError)(err instanceof Error ? err : new Error(String(err)));
                        controller.enqueue(encoder.encode(JSON.stringify({ error: bamlError }) + "\n"));
                        controller.close();
                        return;
                    }
                }
                catch (streamErr) {
                    const errorPayload = {
                        type: "StreamError",
                        message: streamErr instanceof Error
                            ? streamErr.message
                            : "Error in stream processing",
                        prompt: "",
                        raw_output: "",
                    };
                    controller.enqueue(encoder.encode(JSON.stringify({ error: errorPayload }) + "\n"));
                    controller.close();
                }
            },
        });
    }
}
exports.BamlStream = BamlStream;
