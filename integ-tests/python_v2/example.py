from __future__ import annotations
#import abc
import pydantic
import typing as t


T = t.TypeVar("T")
U = t.TypeVar("U")


class StaffMember:
  first_name: str
  last_name: str

class Course:
  title: str
  professor: StaffMember
  credits: int

class Department:
  name: str
  dean: StaffMember
  classes: t.List[Course]
  

class Partial_StaffMember:
  first_name: str
  last_name: str

class Partial_Course:
  title: str
  professor: Partial_StaffMember
  credits: int

class Partial_Department:
  name: str
  dean: Partial_StaffMember
  classes: t.List[Partial_Course]

  def value(self): return self

  def is_started(self): return True

  def is_completed(self): return False

# status: oneof NOT_STARTED -> STARTED / STREAMING -> COMPLETED
# assumes you never go back and update an old value...
class CompletionMeta:
  def is_started(self): return True
  def is_completed(self): return True

class CompletionMeta_StaffMember:
  first_name: CompletionMeta
  last_name: CompletionMeta

class CompletionMeta_List:
  def is_started(self): return True
  def is_completed(self): return True

  def __getitem__(self, idx: int) -> CompletionMeta: ...


class CompletionMeta_Department:
  name: CompletionMeta
  dean: CompletionMeta_StaffMember
  classes: CompletionMeta_List
  classes_meta: t.List[CompletionMeta]



  

def on_event(u: Partial_Department, meta: CompletionMeta_Department):
  u.value().classes[0].professor.first_name
  



#class Unset:
#    pass
#
#
#class Streamable(t.Generic[T]):
#    def __init__(self, is_completed: bool, value: T | Unset) -> None:
#        self.completed = is_completed
#        self._value = value
#
#    def started(self) -> bool:
#        return isinstance(self._value, Unset)
#
#    def value(self) -> t.Optional[T]:
#        if isinstance(self._value, Unset):
#            return None
#        return self._value
#
#
#StreamableStr = Streamable[str]
#StreamableInt = Streamable[int]
#StreamableFloat = Streamable[float]
#StreamableBool = Streamable[bool]
#StreamableList = Streamable[t.List[T]]
#
#
#def foo(f: StreamableList[Streamable[str]]) -> None:
#    if x := f.value():
#        print(x)
#
#
#class SomeClass2(pydantic.BaseModel):
#    foo: int
#
#
#class StreamableCls2(pydantic.BaseModel):
#    foo: StreamablePrimitive[int, int]
#
#
#class StreamableClassWrapper:
#    def __init__(self, value: StreamableCls2, started: bool, completed: bool) -> None:
#        self._value = value
#        self.started = started
#        self.completed = completed
#
#    def is_completed(self) -> bool:
#        return self.completed
#
#    def is_started(self) -> bool:
#        return self.started
#
#    def value(self) -> t.Optional[StreamableCls2]:
#        return self._value
#
#    def as_completed(self) -> SomeClass2:
#        assert self.started and self.completed
#        return t.cast(SomeClass2, self._value)
#
#
#StreamableStr = StreamablePrimitive[str, str]
#
#
#async def foo() -> None:
#    c = StreamableCls2(None, False, False)
#
#    c.as_completed()
#
#    x = c.value().foo.value()