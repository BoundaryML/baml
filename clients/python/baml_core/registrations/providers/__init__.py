# type: ignore
from .anthropic_provider import AnthropicProvider
import openai
from packaging.version import parse as parse_version

if parse_version(openai.__version__) < parse_version("1.0.0"):
    from .openai_chat_provider import OpenAIChatProvider
    from .openai_completion_provider import OpenAICompletionProvider
else:
    from .openai_chat_provider_1 import OpenAIChatProvider
    from .openai_completion_provider_1 import OpenAICompletionProvider
from .fallback_provider import FallbackProvider


__all__ = [
    "FallbackProvider",
    "AnthropicProvider",
    "OpenAIChatProvider",
    "OpenAICompletionProvider",
]
