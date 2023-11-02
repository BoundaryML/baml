import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))

from . import _impl, otel, lib
from .__version__ import __version__

__all__ = ["__version__", "otel", "lib", "_impl"]
