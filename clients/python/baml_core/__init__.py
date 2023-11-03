import dotenv

dotenv.load_dotenv(dotenv_path=dotenv.find_dotenv(usecwd=True))


from .__version__ import __version__
from .helpers import baml_init


__all__ = ["__version__", "baml_init"]
