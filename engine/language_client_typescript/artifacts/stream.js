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
    eventQueue = [];
    constructor(ffiStream, partialCoerce, finalCoerce, ctxManager) {
        this.ffiStream = ffiStream;
        this.partialCoerce = partialCoerce;
        this.finalCoerce = finalCoerce;
        this.ctxManager = ctxManager;
    }
    async driveToCompletion() {
        try {
            this.ffiStream.onEvent((err, data) => {
                if (err) {
                    return;
                }
                else {
                    this.eventQueue.push(data);
                }
            });
            const retval = await this.ffiStream.done(this.ctxManager);
            return retval;
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
    async getFinalResponse() {
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
    toStreamable() {
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
                    }
                    catch (err) {
                        const bamlError = (0, errors_1.toBamlError)(err instanceof Error ? err : new Error(String(err)));
                        controller.enqueue(encoder.encode(JSON.stringify({ error: bamlError })));
                        controller.close();
                        return;
                    }
                }
                catch (streamErr) {
                    const errorPayload = {
                        type: 'StreamError',
                        message: streamErr instanceof Error
                            ? streamErr.message
                            : 'Error in stream processing',
                        prompt: '',
                        raw_output: '',
                    };
                    controller.enqueue(encoder.encode(JSON.stringify({ error: errorPayload })));
                    controller.close();
                }
            },
        });
    }
}
exports.BamlStream = BamlStream;
