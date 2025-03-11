from typing import Optional, Type

__target_baml_py_version__ = None

class EnsureBamlPyImport:
    def __init__(self, target_version: str | None = None):
        global __target_baml_py_version__
        if target_version is not None:
            __target_baml_py_version__ = target_version

    def __enter__(self):
        return self
    

    def _target_package_name(self) -> str:
        if __target_baml_py_version__ is None:
            return "-U baml-py"
        return f"baml-py=={__target_baml_py_version__}"
    
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
