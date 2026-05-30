from __future__ import annotations

from baml_sdk.baml.fs import exists
from baml_sdk.baml.math import trunc


# `baml.math.trunc(value: float) -> int` is a `$rust_function` →
# `FunctionKind::Native`. Calling it as an entry point should truncate toward
# zero and return `3`, not reject with `NotInvokableAsEntry`.
def test_native_trunc_callable_as_entry_point():
    assert trunc(3.7) == 3


# `baml.fs.exists(path: string) -> bool` is a `$rust_io_function` →
# `FunctionKind::SysOp`. Calling it as an entry point should run the
# filesystem sysop and return a bool. `.` exists in the generated fixture
# directory on the test host.
def test_sysop_fs_exists_callable_as_entry_point():
    assert exists(".") is True
