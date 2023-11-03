# ruff: noqa: E402

import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))


from .__version__ import __version__
from .helpers import baml_init

# this must be imported for the client registration to happen
# even if unused
from .lib import providers, caches  # noqa: F401


__all__ = ["__version__", "baml_init"]
