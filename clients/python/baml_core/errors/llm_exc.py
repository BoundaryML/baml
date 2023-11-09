class ProviderErrorCode(int):
    UNKNOWN = 1
    SERVICE_UNAVAILABLE = 503
    INTERNAL_ERROR = 500
    BAD_REQUEST = 400
    UNAUTHORIZED = 401
    FORBIDDEN = 403
    NOT_FOUND = 404
    RATE_LIMITED = 429


TerminalErrorCode = (
    ProviderErrorCode.BAD_REQUEST,
    ProviderErrorCode.UNAUTHORIZED,
    ProviderErrorCode.FORBIDDEN,
    ProviderErrorCode.NOT_FOUND,
)


class LLMException(Exception):
    code: int
    message: str

    def __init__(self, *, code: int, message: str) -> None:
        self.code = code
        self.message = message
        super().__init__(message)

    def __str__(self) -> str:
        return f"LLM Failed: Code {self.__code_str()}: {self.message}"

    def __repr__(self) -> str:
        return f"LLMException(code={self.__code_str()}, message={self.message!r})"

    def __code_str(self) -> str:
        if self.code == ProviderErrorCode.INTERNAL_ERROR:
            return "Internal Error (500)"
        elif self.code == ProviderErrorCode.BAD_REQUEST:
            return "Bad Request (400)"
        elif self.code == ProviderErrorCode.UNAUTHORIZED:
            return "Unauthorized (401)"
        elif self.code == ProviderErrorCode.FORBIDDEN:
            return "Forbidden (403)"
        elif self.code == ProviderErrorCode.NOT_FOUND:
            return "Not Found (404)"
        elif self.code == ProviderErrorCode.RATE_LIMITED:
            return "Rate Limited (429)"
        elif self.code == ProviderErrorCode.SERVICE_UNAVAILABLE:
            return "Service Unavailable (503)"
        return f"Unknown ({self.code})"
