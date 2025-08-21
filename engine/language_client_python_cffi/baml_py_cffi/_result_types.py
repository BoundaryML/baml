from dataclasses import dataclass
from typing import Any, Optional

@dataclass
class ResultCallback:
    """Python equivalent of Go's ResultCallback struct"""
    error: Optional[Exception] = None
    has_stream_data: bool = False
    has_data: bool = False
    stream_data: Any = None
    data: Any = None

@dataclass
class BamlError(Exception):
    """Base BAML error type"""
    message: str
    
    def __str__(self):
        return self.message

@dataclass 
class BamlClientError(BamlError):
    """Client-side BAML error"""
    pass

@dataclass
class BamlClientHttpError(BamlClientError):
    """HTTP-specific client error"""
    pass