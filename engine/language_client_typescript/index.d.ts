export { BamlRuntime, FunctionResult, FunctionResultStream, BamlImage as Image, ClientBuilder, BamlAudio as Audio, invoke_runtime_cli, ClientRegistry, BamlLogEvent, } from './native';
export { BamlStream } from './stream';
export { BamlCtxManager } from './async_context_vars';
export declare class BamlValidationError extends Error {
    prompt: string;
    raw_output: string;
    constructor(prompt: string, raw_output: string, message: string);
    static from(error: Error): BamlValidationError | Error;
    toJSON(): string;
}
export declare function createBamlValidationError(error: Error): BamlValidationError | Error;
//# sourceMappingURL=index.d.ts.map