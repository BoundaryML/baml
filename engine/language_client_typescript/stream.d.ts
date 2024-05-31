import { FunctionResult, FunctionResultStream, RuntimeContextManager } from './native';
export declare class BamlStream<PartialOutputType, FinalOutputType> {
    private ffiStream;
    private partialCoerce;
    private finalCoerce;
    private ctxManager;
    private task;
    private eventQueue;
    constructor(ffiStream: FunctionResultStream, partialCoerce: (result: FunctionResult) => PartialOutputType, finalCoerce: (result: FunctionResult) => FinalOutputType, ctxManager: RuntimeContextManager);
    private driveToCompletion;
    private driveToCompletionInBg;
    [Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType>;
    getFinalResponse(): Promise<FinalOutputType>;
}
//# sourceMappingURL=stream.d.ts.map