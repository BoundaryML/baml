export declare class BamlError extends Error {
    constructor(message: string);
}
export declare class BamlInvalidArgumentError extends BamlError {
    constructor(message: string);
}
export declare class BamlClientError extends BamlError {
    constructor(message: string);
}
export declare class BamlCancelledError extends BamlError {
    constructor(message: string);
}
export declare function wrapNativeError(err: unknown): BamlError;
//# sourceMappingURL=errors.d.ts.map