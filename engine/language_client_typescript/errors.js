"use strict";
// NOTE: Don't take a dependency on ./native here, it will break the browser code
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlTimeoutError = exports.BamlAbortError = exports.BamlClientHttpError = exports.BamlValidationError = exports.BamlClientFinishReasonError = exports.BamlClientError = exports.BamlError = void 0;
exports.isBamlError = isBamlError;
exports.toBamlError = toBamlError;
/**
 * Base class for all BAML errors.
 */
class BamlError extends Error {
    constructor(message) {
        super(message);
        this.name = "BamlError";
        Object.setPrototypeOf(this, BamlError.prototype);
    }
}
exports.BamlError = BamlError;
/**
 * Base class for client-related errors (HTTP errors, timeouts, etc.)
 */
class BamlClientError extends BamlError {
    constructor(message) {
        super(message);
        this.name = "BamlClientError";
        Object.setPrototypeOf(this, BamlClientError.prototype);
    }
}
exports.BamlClientError = BamlClientError;
class BamlClientFinishReasonError extends BamlError {
    prompt;
    raw_output;
    finish_reason;
    detailed_message;
    constructor(prompt, raw_output, message, finish_reason, detailed_message) {
        super(message);
        this.name = "BamlClientFinishReasonError";
        this.prompt = prompt;
        this.raw_output = raw_output;
        this.finish_reason = finish_reason;
        this.detailed_message = detailed_message;
        Object.setPrototypeOf(this, BamlClientFinishReasonError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            raw_output: this.raw_output,
            prompt: this.prompt,
            finish_reason: this.finish_reason,
            detailed_message: this.detailed_message,
        }, null, 2);
    }
    static from(error) {
        if (error.message.includes("BamlClientFinishReasonError")) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === "BamlClientFinishReasonError") {
                    return new BamlClientFinishReasonError(errorData.prompt || "", errorData.raw_output || "", errorData.message || error.message, errorData.finish_reason, errorData.detailed_message || "");
                }
            }
            catch (parseError) {
                console.warn("Failed to parse BamlClientFinishReasonError:", parseError);
            }
        }
        return undefined;
    }
}
exports.BamlClientFinishReasonError = BamlClientFinishReasonError;
class BamlValidationError extends BamlError {
    prompt;
    raw_output;
    detailed_message;
    constructor(prompt, raw_output, message, detailed_message) {
        super(message);
        this.name = "BamlValidationError";
        this.prompt = prompt;
        this.raw_output = raw_output;
        this.detailed_message = detailed_message;
        Object.setPrototypeOf(this, BamlValidationError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            raw_output: this.raw_output,
            prompt: this.prompt,
            detailed_message: this.detailed_message,
        }, null, 2);
    }
    static from(error) {
        if (error.message.includes("BamlValidationError")) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === "BamlValidationError") {
                    return new BamlValidationError(errorData.prompt || "", errorData.raw_output || "", errorData.message || error.message, errorData.detailed_message || "");
                }
            }
            catch (parseError) {
                console.warn("Failed to parse BamlValidationError:", parseError);
            }
        }
        return undefined;
    }
}
exports.BamlValidationError = BamlValidationError;
class BamlClientHttpError extends BamlClientError {
    client_name;
    status_code;
    detailed_message;
    /**
     * The raw response body from the LLM API (if available).
     * This contains the exact response from the provider, useful for debugging
     * or extracting structured error information.
     */
    raw_response;
    constructor(client_name, message, status_code, detailed_message, raw_response) {
        super(message);
        this.name = "BamlClientHttpError";
        this.client_name = client_name;
        this.status_code = status_code;
        this.detailed_message = detailed_message;
        this.raw_response = raw_response;
        Object.setPrototypeOf(this, BamlClientHttpError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            status_code: this.status_code,
            client_name: this.client_name,
            detailed_message: this.detailed_message,
            raw_response: this.raw_response,
        });
    }
    static from(error) {
        if (error.message.includes("BamlClientHttpError")) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === "BamlClientHttpError") {
                    return new BamlClientHttpError(errorData.client_name || "", errorData.message || error.message, errorData.status_code || -100, errorData.detailed_message || "", errorData.raw_response || undefined);
                }
            }
            catch (parseError) {
                console.warn("Failed to parse BamlClientHttpError:", parseError);
            }
        }
        return undefined;
    }
}
exports.BamlClientHttpError = BamlClientHttpError;
class BamlAbortError extends BamlError {
    reason;
    detailed_message;
    constructor(message, reason, detailed_message = "") {
        super(message);
        this.name = "BamlAbortError";
        this.reason = reason;
        this.detailed_message = detailed_message;
        Object.setPrototypeOf(this, BamlAbortError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            reason: this.reason,
            detailed_message: this.detailed_message,
        }, null, 2);
    }
    static from(error) {
        if (error.message.includes("BamlAbortError") ||
            error.message.includes("Operation was aborted") ||
            error.message.includes("Operation cancelled")) {
            return new BamlAbortError(error.message, undefined, "");
        }
        return undefined;
    }
}
exports.BamlAbortError = BamlAbortError;
class BamlTimeoutError extends BamlClientHttpError {
    constructor(client_name, message) {
        super(client_name, message, 408, ""); // HTTP 408 Request Timeout
        this.name = "BamlTimeoutError";
        Object.setPrototypeOf(this, BamlTimeoutError.prototype);
    }
    static from(error) {
        if (error.message.includes("BamlTimeoutError") ||
            error.message.includes("timed out")) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === "BamlTimeoutError") {
                    return new BamlTimeoutError(errorData.client_name || "", errorData.message || error.message);
                }
            }
            catch (parseError) {
                // If parsing fails, check for timeout in message
                if (error.message.includes("timed out")) {
                    return new BamlTimeoutError("", error.message);
                }
                console.warn("Failed to parse BamlTimeoutError:", parseError);
            }
        }
        return undefined;
    }
}
exports.BamlTimeoutError = BamlTimeoutError;
function isError(error) {
    if (typeof error === "string") {
        return false;
    }
    if (error.message) {
        return true;
    }
    if (error instanceof Error) {
        return true;
    }
    return false;
}
// Helper function to safely create a BamlError from an unknown error
function createBamlErrorUnsafe(error) {
    if (!isError(error)) {
        return new Error(String(error));
    }
    const bamlAbortError = BamlAbortError.from(error);
    if (bamlAbortError) {
        return bamlAbortError;
    }
    const bamlTimeoutError = BamlTimeoutError.from(error);
    if (bamlTimeoutError) {
        return bamlTimeoutError;
    }
    const bamlClientHttpError = BamlClientHttpError.from(error);
    if (bamlClientHttpError) {
        return bamlClientHttpError;
    }
    const bamlValidationError = BamlValidationError.from(error);
    if (bamlValidationError) {
        return bamlValidationError;
    }
    const bamlClientFinishReasonError = BamlClientFinishReasonError.from(error);
    if (bamlClientFinishReasonError) {
        return bamlClientFinishReasonError;
    }
    // otherwise return the original error
    return error;
}
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
function isBamlError(error) {
    return error instanceof BamlError;
}
function toBamlError(error) {
    try {
        if (isBamlError(error)) {
            return error;
        }
        if (isError(error)) {
            const converted = createBamlErrorUnsafe(error);
            // Only return if we successfully converted to a BamlError
            if (converted instanceof BamlError) {
                return converted;
            }
        }
        // Return null if not convertible
        return null;
    }
    catch {
        return null;
    }
}
// No need for a separate throwBamlValidationError function in TypeScript
