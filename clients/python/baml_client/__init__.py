try:
    from ._generated import baml, baml_types  # type: ignore
except ImportError:
    from .stubs import baml, baml_types

__version__ = "0.0.1"
__all__ = ["baml", "baml_types", "__version__"]
