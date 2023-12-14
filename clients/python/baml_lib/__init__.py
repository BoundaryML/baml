# ruff: noqa: E402

import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))


from baml_version import __version__
from .helpers import baml_init
from ._impl.deserializer import DeserializerException


__all__ = ["__version__", "baml_init", "DeserializerException"]
