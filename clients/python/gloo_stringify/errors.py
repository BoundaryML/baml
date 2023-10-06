class StringifyError(Exception):
    """Raised when an error occurs while stringifying an object."""
    
    def __init__(self, message: str) -> None:
        super().__init__(message)
    