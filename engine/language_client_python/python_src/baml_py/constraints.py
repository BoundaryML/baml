from typing import Generic, Optional, TypeVar
from pydantic import BaseModel

T = TypeVar('T')
K = TypeVar('K')

class Check(BaseModel):
    name: str
    expression: str
    status: str

class Checked(BaseModel, Generic[T,K]):
    value: T
    checks: K
