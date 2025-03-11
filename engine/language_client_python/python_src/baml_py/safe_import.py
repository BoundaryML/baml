from typing import Optional, Type
from .baml_py import get_version

__target_baml_py_version__ = None

__baml_py_version__ = get_version()

class EnsureBamlPyImport:
    def __init__(self, target_version: str | None = None):
        global __target_baml_py_version__
        if target_version is not None:
            __target_baml_py_version__ = target_version

    def __enter__(self):
        return self

    def _is_version_compatible(self, a: str, b: str) -> bool:
        """
        Checks if version a is compatible with version b.
        """
        # Split the version strings into major, minor, and patch components
        try:
            a_major, a_minor, _ = a.split(".", maxsplit=2)
            b_major, b_minor, _ = b.split(".", maxsplit=2)
        except ValueError:
            # If the version strings are not valid integers, return False
            return False

        # Check if the major, minor
        # We don't care about the patch version
        return a_major == b_major and a_minor == b_minor

    def _target_package_name(self) -> str:
        if __target_baml_py_version__ is None:
            return "-U baml-py"
        return f"baml-py=={__target_baml_py_version__}"

    def raise_if_incompatible_version(self, current_version: str):
        if not self._is_version_compatible(current_version, __baml_py_version__):
            self.raise_version_error(f"""
baml-py is likely out of date.
                                     
Version of baml_client generator (see generators.baml): {current_version}
Current version of baml-py: {__baml_py_version__}
""".strip())

    def raise_version_error(self, msg: str):
        target_version = __target_baml_py_version__
        if target_version is None:
            raise ImportError(f"""
{msg}

Please upgrade baml-py to the latest version.

$ pip install {self._target_package_name()}
$ uv add {self._target_package_name()}

If nothing else works, please ask for help:

https://github.com/boundaryml/baml/issues
https://boundaryml.com/discord

""".strip()) from None
        else:
            raise ImportError(f"""
{msg}

Please upgrade baml-py to version "{target_version}".

$ pip install {self._target_package_name()}
$ uv add {self._target_package_name()}

If nothing else works, please ask for help:

https://github.com/boundaryml/baml/issues
https://boundaryml.com/discord
""".strip()) from None
        

    def __exit__(self, exc_type: Optional[Type[Exception]], exc_value: Optional[Exception], traceback):
        if exc_type is not None:
            if isinstance(exc_value, ImportError) and "baml_py" in str(exc_value):
                self.raise_version_error(exc_value.args[0])
            if isinstance(exc_value, AttributeError) and "baml_py" in str(exc_value):
                self.raise_version_error(exc_value.args[0])
            raise exc_value
