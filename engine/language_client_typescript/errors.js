"use strict";
// NOTE: Don't take a dependency on ./native here, it will break the browser code
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlClientHttpError = exports.BamlValidationError = exports.BamlClientFinishReasonError = void 0;
exports.isBamlError = isBamlError;
exports.toBamlError = toBamlError;
class BamlClientFinishReasonError extends Error {
    prompt;
    raw_output;
    finish_reason;
    constructor(prompt, raw_output, message, finish_reason) {
        super(message);
        this.name = 'BamlClientFinishReasonError';
        this.prompt = prompt;
        this.raw_output = raw_output;
        this.finish_reason = finish_reason;
        Object.setPrototypeOf(this, BamlClientFinishReasonError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            raw_output: this.raw_output,
            prompt: this.prompt,
            finish_reason: this.finish_reason,
        }, null, 2);
    }
    static from(error) {
        if (error.message.includes('BamlClientFinishReasonError')) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === 'BamlClientFinishReasonError') {
                    return new BamlClientFinishReasonError(errorData.prompt || '', errorData.raw_output || '', errorData.message || error.message, errorData.finish_reason);
                }
            }
            catch (parseError) {
                console.warn('Failed to parse BamlClientFinishReasonError:', parseError);
            }
        }
        return undefined;
    }
}
exports.BamlClientFinishReasonError = BamlClientFinishReasonError;
class BamlValidationError extends Error {
    prompt;
    raw_output;
    constructor(prompt, raw_output, message) {
        super(message);
        this.name = 'BamlValidationError';
        this.prompt = prompt;
        this.raw_output = raw_output;
        Object.setPrototypeOf(this, BamlValidationError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            raw_output: this.raw_output,
            prompt: this.prompt,
        }, null, 2);
    }
    static from(error) {
        if (error.message.includes('BamlValidationError')) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === 'BamlValidationError') {
                    return new BamlValidationError(errorData.prompt || '', errorData.raw_output || '', errorData.message || error.message);
                }
            }
            catch (parseError) {
                console.warn('Failed to parse BamlValidationError:', parseError);
            }
        }
        return undefined;
    }
}
exports.BamlValidationError = BamlValidationError;
class BamlClientHttpError extends Error {
    client_name;
    status_code;
    constructor(client_name, message, status_code) {
        super(message);
        this.name = 'BamlClientHttpError';
        this.client_name = client_name;
        this.status_code = status_code;
        Object.setPrototypeOf(this, BamlClientHttpError.prototype);
    }
    toJSON() {
        return JSON.stringify({
            name: this.name,
            message: this.message,
            status_code: this.status_code,
            client_name: this.client_name,
        });
    }
    static from(error) {
        if (error.message.includes('BamlClientHttpError')) {
            try {
                const errorData = JSON.parse(error.message);
                if (errorData.type === 'BamlClientHttpError') {
                    return new BamlClientHttpError(errorData.client_name || '', errorData.message || error.message, errorData.status_code || -100);
                }
            }
            catch (parseError) {
                console.warn('Failed to parse BamlClientHttpError:', parseError);
            }
        }
        return undefined;
    }
}
exports.BamlClientHttpError = BamlClientHttpError;
function isError(error) {
    if (typeof error === 'string') {
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
// Helper function to safely create a BamlValidationError
function createBamlErrorUnsafe(error) {
    if (!isError(error)) {
        return new Error(String(error));
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
function isBamlError(error) {
    if (error.type === 'BamlClientHttpError' ||
        error.type === 'BamlValidationError' ||
        error.type === 'BamlClientFinishReasonError') {
        return true;
    }
    if (error.name === 'BamlClientHttpError' ||
        error.name === 'BamlValidationError' ||
        error.name === 'BamlClientFinishReasonError') {
        return true;
    }
    return (error instanceof BamlClientHttpError ||
        error instanceof BamlValidationError ||
        error instanceof BamlClientFinishReasonError);
}
function toBamlError(error) {
    try {
        if (isBamlError(error)) {
            return error;
        }
        return createBamlErrorUnsafe(error);
    }
    catch (error) {
        return error;
    }
}
// No need for a separate throwBamlValidationError function in TypeScript
