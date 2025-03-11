import type { FunctionResultStream, RuntimeContextManager } from './native';
export declare class BamlStream<PartialOutputType, FinalOutputType> {
    private ffiStream;
    private partialCoerce;
    private finalCoerce;
    private ctxManager;
    private task;
    private eventQueue;
    constructor(ffiStream: FunctionResultStream, partialCoerce: (result: any) => PartialOutputType, finalCoerce: (result: any) => FinalOutputType, ctxManager: RuntimeContextManager);
    private driveToCompletion;
    private driveToCompletionInBg;
    [Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType>;
    getFinalResponse(): Promise<FinalOutputType>;
    /**
     * Converts the BAML stream to a Next.js compatible stream.
     * This is used for server-side streaming in Next.js API routes and Server Actions.
     * The stream emits JSON-encoded messages containing either:
     * - Partial results of type PartialOutputType
     * - Final result of type FinalOutputType
     * - Error information
     */
    toStreamable(): ReadableStream<Uint8Array>;
}
//# sourceMappingURL=stream.d.ts.map