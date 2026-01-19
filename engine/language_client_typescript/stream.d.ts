import type { FunctionResultStream, RuntimeContextManager } from "../native";
export declare class BamlStream<PartialOutputType, FinalOutputType> {
    private ffiStream;
    private partialCoerce;
    private finalCoerce;
    private ctxManager;
    private task;
    private error;
    private eventQueue;
    private abortSignal?;
    constructor(ffiStream: FunctionResultStream, partialCoerce: (result: any) => PartialOutputType, finalCoerce: (result: any) => FinalOutputType, ctxManager: RuntimeContextManager, abortSignal?: AbortSignal);
    private driveToCompletion;
    private driveToCompletionInBg;
    [Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType>;
    getFinalResponse(): Promise<FinalOutputType>;
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
    toStreamable(): ReadableStream<Uint8Array>;
}
//# sourceMappingURL=stream.d.ts.map