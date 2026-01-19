/**
 * Base class for all BAML errors.
 */
export declare class BamlError extends Error {
    constructor(message: string);
}
/**
 * Base class for client-related errors (HTTP errors, timeouts, etc.)
 */
export declare class BamlClientError extends BamlError {
    constructor(message: string);
}
export declare class BamlClientFinishReasonError extends BamlError {
    prompt: string;
    raw_output: string;
    finish_reason?: string;
    detailed_message: string;
    constructor(prompt: string, raw_output: string, message: string, finish_reason: string | undefined, detailed_message: string);
    toJSON(): string;
    static from(error: Error): BamlClientFinishReasonError | undefined;
}
export declare class BamlValidationError extends BamlError {
    prompt: string;
    raw_output: string;
    detailed_message: string;
    constructor(prompt: string, raw_output: string, message: string, detailed_message: string);
    toJSON(): string;
    static from(error: Error): BamlValidationError | undefined;
}
export declare class BamlClientHttpError extends BamlClientError {
    client_name: string;
    status_code: number;
    detailed_message: string;
    /**
     * The raw response body from the LLM API (if available).
     * This contains the exact response from the provider, useful for debugging
     * or extracting structured error information.
     */
    raw_response?: string;
    constructor(client_name: string, message: string, status_code: number, detailed_message: string, raw_response?: string);
    toJSON(): string;
    static from(error: Error): BamlClientHttpError | undefined;
}
export declare class BamlAbortError extends BamlError {
    readonly reason?: any;
    detailed_message: string;
    constructor(message: string, reason?: any, detailed_message?: string);
    toJSON(): string;
    static from(error: Error): BamlAbortError | undefined;
}
export declare class BamlTimeoutError extends BamlClientHttpError {
    constructor(client_name: string, message: string);
    static from(error: Error): BamlTimeoutError | undefined;
}
export type BamlErrors = BamlClientHttpError | BamlValidationError | BamlClientFinishReasonError | BamlAbortError | BamlTimeoutError;
/**
 * Check if an error is an instance of BamlError.
 *
 * Note: This only returns true for actual BamlError instances (using instanceof).
 * If you have a raw error from NAPI-RS that hasn't been converted yet, use
 * toBamlError() first to convert it, then check with isBamlError().
 *
 * @example
 * ```typescript
 * try {
 *   await b.MyFunction();
 * } catch (e) {
 *   const error = toBamlError(e);
 *   if (error) {
 *     // error is now typed as BamlError
 *   }
 * }
 * ```
 */
export declare function isBamlError(error: unknown): error is BamlError;
export declare function toBamlError(error: unknown): BamlError | null;
//# sourceMappingURL=errors.d.ts.map