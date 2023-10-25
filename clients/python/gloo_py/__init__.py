from gloo_internal.context_manager import CodeVariant, LLMVariant
from gloo_internal.env import ENV
from gloo_internal.llm_clients import register_llm_client, llm_client_factory
from gloo_internal.tracer import trace, update_trace_tags

# For backwards compatibility
from gloo_internal.llm_client import LLMClient, OpenAILLMClient


__version__ = "1.3.1"

__all__ = [
    "CodeVariant",
    "LLMVariant",
    "ENV",
    "LLMClient",
    "OpenAILLMClient",
    "register_llm_client",
    "llm_client_factory",
    "trace",
    "update_trace_tags",
]
