export declare class BamlClientFinishReasonError extends Error {
    prompt: string;
    raw_output: string;
    finish_reason?: string;
    constructor(prompt: string, raw_output: string, message: string, finish_reason: string | undefined);
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
export type BamlErrors = BamlClientHttpError | BamlValidationError | BamlClientFinishReasonError;
export declare function isBamlError(error: unknown): error is BamlErrors;
export declare function toBamlError(error: unknown): BamlErrors | Error;
//# sourceMappingURL=errors.d.ts.map