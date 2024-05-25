import { FunctionResultPy, FunctionResultStreamPy, RuntimeContextManagerPy } from '../native';
export declare class BamlStream<PartialOutputType, FinalOutputType> {
    private ffiStream;
    private partialCoerce;
    private finalCoerce;
    private ctxManager;
    private task;
    private eventQueue;
    constructor(ffiStream: FunctionResultStreamPy, partialCoerce: (result: FunctionResultPy) => PartialOutputType, finalCoerce: (result: FunctionResultPy) => FinalOutputType, ctxManager: RuntimeContextManagerPy);
    private driveToCompletion;
    private driveToCompletionInBg;
    [Symbol.asyncIterator](): AsyncIterableIterator<PartialOutputType>;
    done(): Promise<FinalOutputType>;
}
//# sourceMappingURL=stream.d.ts.map