# type: ignore
from .anthropic_provider import AnthropicProvider
from .anthropic_chat_provider import AnthropicChatProvider
import openai
from packaging.version import parse as parse_version
from .fallback_provider import FallbackProvider
from .round_robin_provider import RoundRobinProvider

if parse_version(version=openai.__version__) < parse_version("1.0.0"):
    from .openai_chat_provider import OpenAIChatProvider
    from .openai_completion_provider import OpenAICompletionProvider
else:
    from .openai_chat_provider_1 import OpenAIChatProvider
    from .openai_completion_provider_1 import OpenAICompletionProvider

try:
    from .ollama_chat_provider import OllamaChatProvider
    from .ollama_completion_provider import OllamaCompletionProvider
except ImportError:
    OllamaChatProvider = None
    OllamaCompletionProvider = None

__all__ = [
    "AnthropicProvider",
    "AnthropicChatProvider",
    "FallbackProvider",
    "OpenAIChatProvider",
    "OpenAICompletionProvider",
    "RoundRobinProvider",
    "OllamaChatProvider",
    "OllamaCompletionProvider",
]
