export { BamlRuntime, FunctionResult, FunctionResultStream, BamlImage as Image, ClientBuilder, BamlAudio as Audio, invoke_runtime_cli, ClientRegistry, BamlLogEvent, } from "./native";
export { BamlStream } from "./stream";
export { BamlCtxManager } from "./async_context_vars";
export declare class BamlClientFinishReasonError extends Error {
    prompt: string;
    raw_output: string;
    constructor(prompt: string, raw_output: string, message: string);
    toJSON(): string;
    static from(error: Error): BamlClientFinishReasonError | undefined;
}
export declare class BamlValidationError extends Error {
    prompt: string;
    raw_output: string;
    constructor(prompt: string, raw_output: string, message: string);
    toJSON(): string;
    static from(error: Error): BamlValidationError | undefined;
}
export declare class BamlClientHttpError extends Error {
    client_name: string;
    status_code: number;
    constructor(client_name: string, message: string, status_code: number);
    toJSON(): string;
    static from(error: Error): BamlClientHttpError | undefined;
}
export declare function toBamlError(error: any): any;
//# sourceMappingURL=index.d.ts.map