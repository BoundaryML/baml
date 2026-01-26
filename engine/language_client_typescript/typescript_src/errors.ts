// NOTE: Don't take a dependency on ./native here, it will break the browser code

/**
 * Base class for all BAML errors.
 */
export class BamlError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BamlError";
    Object.setPrototypeOf(this, BamlError.prototype);
  }
}

/**
 * Base class for client-related errors (HTTP errors, timeouts, etc.)
 */
export class BamlClientError extends BamlError {
  constructor(message: string) {
    super(message);
    this.name = "BamlClientError";
    Object.setPrototypeOf(this, BamlClientError.prototype);
  }
}

/**
 * Error thrown when invalid arguments are passed to a BAML function.
 */
export class BamlInvalidArgumentError extends BamlError {
  constructor(message: string) {
    super(message);
    this.name = "BamlInvalidArgumentError";
    Object.setPrototypeOf(this, BamlInvalidArgumentError.prototype);
  }

  toJSON(): string {
    return JSON.stringify({
      name: this.name,
      message: this.message,
    });
  }

  static from(error: Error): BamlInvalidArgumentError | undefined {
    if (error.message.includes("BamlInvalidArgumentError")) {
      return new BamlInvalidArgumentError(error.message);
    }
    return undefined;
  }
}

export class BamlClientFinishReasonError extends BamlError {
  prompt: string;
  raw_output: string;
  finish_reason?: string;
  detailed_message: string;

  constructor(
    prompt: string,
    raw_output: string,
    message: string,
    finish_reason: string | undefined,
    detailed_message: string,
  ) {
    super(message);
    this.name = "BamlClientFinishReasonError";
    this.prompt = prompt;
    this.raw_output = raw_output;
    this.finish_reason = finish_reason;
    this.detailed_message = detailed_message;

    Object.setPrototypeOf(this, BamlClientFinishReasonError.prototype);
  }

  toJSON(): string {
    return JSON.stringify(
      {
        name: this.name,
        message: this.message,
        raw_output: this.raw_output,
        prompt: this.prompt,
        finish_reason: this.finish_reason,
        detailed_message: this.detailed_message,
      },
      null,
      2,
    );
  }

  static from(error: Error): BamlClientFinishReasonError | undefined {
    if (error.message.includes("BamlClientFinishReasonError")) {
      try {
        const errorData = JSON.parse(error.message);
        if (errorData.type === "BamlClientFinishReasonError") {
          return new BamlClientFinishReasonError(
            errorData.prompt || "",
            errorData.raw_output || "",
            errorData.message || error.message,
            errorData.finish_reason,
            errorData.detailed_message || "",
          );
        }
      } catch (parseError) {
        console.warn(
          "Failed to parse BamlClientFinishReasonError:",
          parseError,
        );
      }
    }
    return undefined;
  }
}

export class BamlValidationError extends BamlError {
  prompt: string;
  raw_output: string;
  detailed_message: string;

  constructor(
    prompt: string,
    raw_output: string,
    message: string,
    detailed_message: string,
  ) {
    super(message);
    this.name = "BamlValidationError";
    this.prompt = prompt;
    this.raw_output = raw_output;
    this.detailed_message = detailed_message;

    Object.setPrototypeOf(this, BamlValidationError.prototype);
  }

  toJSON(): string {
    return JSON.stringify(
      {
        name: this.name,
        message: this.message,
        raw_output: this.raw_output,
        prompt: this.prompt,
        detailed_message: this.detailed_message,
      },
      null,
      2,
    );
  }

  static from(error: Error): BamlValidationError | undefined {
    if (error.message.includes("BamlValidationError")) {
      try {
        const errorData = JSON.parse(error.message);
        if (errorData.type === "BamlValidationError") {
          return new BamlValidationError(
            errorData.prompt || "",
            errorData.raw_output || "",
            errorData.message || error.message,
            errorData.detailed_message || "",
          );
        }
      } catch (parseError) {
        console.warn("Failed to parse BamlValidationError:", parseError);
      }
    }
    return undefined;
  }
}

export class BamlClientHttpError extends BamlClientError {
  client_name: string;
  status_code: number;
  detailed_message: string;
  /**
   * The raw response body from the LLM API (if available).
   * This contains the exact response from the provider, useful for debugging
   * or extracting structured error information.
   */
  raw_response?: string;

  constructor(
    client_name: string,
    message: string,
    status_code: number,
    detailed_message: string,
    raw_response?: string,
  ) {
    super(message);
    this.name = "BamlClientHttpError";
    this.client_name = client_name;
    this.status_code = status_code;
    this.detailed_message = detailed_message;
    this.raw_response = raw_response;

    Object.setPrototypeOf(this, BamlClientHttpError.prototype);
  }

  toJSON(): string {
    return JSON.stringify({
      name: this.name,
      message: this.message,
      status_code: this.status_code,
      client_name: this.client_name,
      detailed_message: this.detailed_message,
      raw_response: this.raw_response,
    });
  }

  static from(error: Error): BamlClientHttpError | undefined {
    if (error.message.includes("BamlClientHttpError")) {
      try {
        const errorData = JSON.parse(error.message);
        if (errorData.type === "BamlClientHttpError") {
          return new BamlClientHttpError(
            errorData.client_name || "",
            errorData.message || error.message,
            errorData.status_code || -100,
            errorData.detailed_message || "",
            errorData.raw_response || undefined,
          );
        }
      } catch (parseError) {
        console.warn("Failed to parse BamlClientHttpError:", parseError);
      }
    }
    return undefined;
  }
}

export class BamlAbortError extends BamlError {
  public readonly reason?: any;
  detailed_message: string;

  constructor(message: string, reason?: any, detailed_message: string = "") {
    super(message);
    this.name = "BamlAbortError";
    this.reason = reason;
    this.detailed_message = detailed_message;

    Object.setPrototypeOf(this, BamlAbortError.prototype);
  }

  toJSON(): string {
    return JSON.stringify(
      {
        name: this.name,
        message: this.message,
        reason: this.reason,
        detailed_message: this.detailed_message,
      },
      null,
      2,
    );
  }

  static from(error: Error): BamlAbortError | undefined {
    if (
      error.message.includes("BamlAbortError") ||
      error.message.includes("Operation was aborted") ||
      error.message.includes("Operation cancelled")
    ) {
      return new BamlAbortError(error.message, undefined, "");
    }
    return undefined;
  }
}

export class BamlTimeoutError extends BamlClientHttpError {
  constructor(client_name: string, message: string) {
    super(client_name, message, 408, ""); // HTTP 408 Request Timeout
    this.name = "BamlTimeoutError";

    Object.setPrototypeOf(this, BamlTimeoutError.prototype);
  }

  static from(error: Error): BamlTimeoutError | undefined {
    if (
      error.message.includes("BamlTimeoutError") ||
      error.message.includes("timed out")
    ) {
      try {
        const errorData = JSON.parse(error.message);
        if (errorData.type === "BamlTimeoutError") {
          return new BamlTimeoutError(
            errorData.client_name || "",
            errorData.message || error.message,
          );
        }
      } catch (parseError) {
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

export type BamlErrors =
  | BamlClientHttpError
  | BamlValidationError
  | BamlClientFinishReasonError
  | BamlAbortError
  | BamlTimeoutError
  | BamlInvalidArgumentError;

function isError(error: unknown): error is Error {
  if (typeof error === "string") {
    return false;
  }

  if ((error as any).message) {
    return true;
  }

  if (error instanceof Error) {
    return true;
  }

  return false;
}

// Helper function to safely create a BamlError from an unknown error
function createBamlErrorUnsafe(error: unknown): BamlError | Error {
  if (!isError(error)) {
    return new Error(String(error));
  }

  const bamlInvalidArgumentError = BamlInvalidArgumentError.from(error);
  if (bamlInvalidArgumentError) {
    return bamlInvalidArgumentError;
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

  // If the error message contains "BamlError", wrap it in a generic BamlError
  if (error.message.includes("BamlError")) {
    return new BamlError(error.message);
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
export function isBamlError(error: unknown): error is BamlError {
  return error instanceof BamlError;
}

export function toBamlError(error: unknown): BamlError | Error {
  try {
    if (isBamlError(error)) {
      return error;
    }

    return createBamlErrorUnsafe(error);
  } catch (e) {
    return e as Error;
  }
}

// No need for a separate throwBamlValidationError function in TypeScript
