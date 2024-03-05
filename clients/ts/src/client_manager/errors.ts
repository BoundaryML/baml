enum ProviderErrorCode {
  Unknown = 1,
  ServiceUnavailable = 503,
  InternalError = 500,
  BadRequest = 400,
  Unauthorized = 401,
  Forbidden = 403,
  NotFound = 404,
  RateLimitExceeded = 429,
}

const TerminalErrorCodes = [
  ProviderErrorCode.BadRequest,
  ProviderErrorCode.Unauthorized,
  ProviderErrorCode.Forbidden,
  ProviderErrorCode.NotFound,
];

// toString for ProviderErrorCode
function ProviderErrorCodeToString(code: ProviderErrorCode): string {
  switch (code) {
    case ProviderErrorCode.ServiceUnavailable:
      return "Service Unavailable (503)";
    case ProviderErrorCode.InternalError:
      return "Internal Error (500)";
    case ProviderErrorCode.BadRequest:
      return "Bad Request (400)";
    case ProviderErrorCode.Unauthorized:
      return "Unauthorized (401)";
    case ProviderErrorCode.Forbidden:
      return "Forbidden (403)";
    case ProviderErrorCode.NotFound:
      return "Not Found (404)";
    case ProviderErrorCode.RateLimitExceeded:
      return "Rate Limit Exceeded (429)";
    default:
      return `Unknown (${code})`;
  }
}


class LLMException extends Error {
  code: ProviderErrorCode | number;
  constructor(message: string, code: number) {
    super(message);
    this.code = code;
  }

  toString() {
    return `LLM Failed (${ProviderErrorCodeToString(this.code)}): ${this.message}`;
  }


  static fromError(err: Error | unknown, code: ProviderErrorCode | number): LLMException {
    if (err instanceof LLMException) {
      return err;
    }
    if (err instanceof Error) {
      return new LLMException(err.message, code);
    }
    return new LLMException("Unknown Error", code);
  }
}

export {
  ProviderErrorCode,
  LLMException,
  TerminalErrorCodes,
}