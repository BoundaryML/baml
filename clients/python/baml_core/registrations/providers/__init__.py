# type: ignore
from .anthropic_provider import AnthropicProvider
import openai
from packaging.version import parse as parse_version
from .fallback_provider import FallbackProvider

print(f"OpenAI version: {openai.__version__}")
if parse_version(openai.__version__) < parse_version("1.0.0"):
    print("Using OpenAI version < 1.0.0")
    from .openai_chat_provider import OpenAIChatProvider
    from .openai_completion_provider import OpenAICompletionProvider
else:
    print("Using OpenAI version >= 1.0.0")
    from .openai_chat_provider_1 import OpenAIChatProvider
    from .openai_completion_provider_1 import OpenAICompletionProvider


__all__ = [
    "FallbackProvider",
    "AnthropicProvider",
    "OpenAIChatProvider",
    "OpenAICompletionProvider",
]
