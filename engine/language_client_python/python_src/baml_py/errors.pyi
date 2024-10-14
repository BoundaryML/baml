class BamlError(Exception):
    """Base class for all BAML-related errors."""

    ...

class BamlInvalidArgumentError(BamlError):
    """Raised when an invalid argument is provided to a function."""

    ...

class BamlClientError(BamlError):
    """Raised for general client errors."""

    ...

class BamlClientHttpError(BamlClientError):
    """Raised for HTTP-related client errors."""

    ...

class BamlValidationError(BamlError):
    """Raised when a validation error occurs."""

    prompt: str
    message: str
    raw_output: str
