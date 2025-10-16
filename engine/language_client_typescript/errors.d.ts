export declare class BamlClientFinishReasonError extends Error {
    prompt: string;
    raw_output: string;
    finish_reason?: string;
    detailed_message: string;
    constructor(prompt: string, raw_output: string, message: string, finish_reason: string | undefined, detailed_message: string);
    toJSON(): string;
    static from(error: Error): BamlClientFinishReasonError | undefined;
}
export declare class BamlValidationError extends Error {
    prompt: string;
    raw_output: string;
    detailed_message: string;
    constructor(prompt: string, raw_output: string, message: string, detailed_message: string);
    toJSON(): string;
    static from(error: Error): BamlValidationError | undefined;
}
export declare class BamlClientHttpError extends Error {
    client_name: string;
    status_code: number;
    detailed_message: string;
    constructor(client_name: string, message: string, status_code: number, detailed_message: string);
    toJSON(): string;
    static from(error: Error): BamlClientHttpError | undefined;
}
export declare class BamlAbortError extends Error {
    readonly reason?: any;
    detailed_message: string;
    constructor(message: string, reason?: any, detailed_message?: string);
    toJSON(): string;
    static from(error: Error): BamlAbortError | undefined;
}
export type BamlErrors = BamlClientHttpError | BamlValidationError | BamlClientFinishReasonError | BamlAbortError;
export declare function isBamlError(error: unknown): error is BamlErrors;
export declare function toBamlError(error: unknown): BamlErrors | Error;
//# sourceMappingURL=errors.d.ts.map