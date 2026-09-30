"""Live inference through every provider client in `ns_providers`.

Each leaf client in `leaf_client` (fixtures/llm_providers) gets one plain call
and one streamed call asking for the capital of France, parsed into the
`Capital` class, against the real provider endpoint.

The live cases only run with `BAML_LIVE_PROVIDER_TESTS=1`; the keys live in
the `dev-llm-provider-tests` Infisical environment:

    BAML_LIVE_PROVIDER_TESTS=1 infisical run --env=dev-llm-provider-tests -- \\
        cargo nextest run -p sdk_test_python_pydantic2 llm_providers::pytest

With the flag set, a client whose variables are missing FAILS rather than
skips, so a green live run means every client was actually called. To run a
subset, filter pytest directly (see sdk_tests/README.md):

    (cd sdk_tests/crates/python_pydantic2/llm_providers/generated && \\
        BAML_LIVE_PROVIDER_TESTS=1 infisical run --env=dev-llm-provider-tests -- \\
        uv run pytest -v -k Bedrock)

Without the flag, only the keyless request-rendering tests below run.
"""

import os

import pytest

_LIVE = os.environ.get("BAML_LIVE_PROVIDER_TESTS") == "1"
_INFISICAL_ENV = "dev-llm-provider-tests"

_BEDROCK = ("BEDROCK_AWS_ACCESS_KEY_ID", "BEDROCK_AWS_SECRET_ACCESS_KEY", "BEDROCK_AWS_REGION")
_MANTLE = ("BEDROCK_MANTLE_API_KEY", "BEDROCK_MANTLE_BASE_URL")
_AZURE = ("AZURE_OPENAI_API_KEY", "AZURE_OPENAI_RESPONSES_BASE_URL")
_GEMINI = ("GOOGLE_API_KEY",)
# A service-account JSON document. Vertex Gemini authenticates with it (and
# takes the project from it); the Model Garden clients mint their OAuth token
# from it unless VERTEX_ACCESS_TOKEN is set.
_GCP_SA = "GOOGLE_APPLICATION_CREDENTIALS_CONTENT"

# Keep in sync with `leaf_client` in ns_providers/providers.baml.
CLIENTS: dict[str, tuple[str, ...]] = {
    # AWS Bedrock
    "BedrockHaiku45": _BEDROCK,
    "BedrockHaiku45Thinking1024": _BEDROCK,
    "BedrockHaiku45Thinking4096": _BEDROCK,
    "BedrockSonnet46": _BEDROCK,
    "BedrockSonnet46Low": _BEDROCK,
    "BedrockSonnet5": _BEDROCK,
    "BedrockSonnet5Low": _BEDROCK,
    "BedrockOpus5": _BEDROCK,
    "BedrockNovaMicro": _BEDROCK,
    "BedrockNova2Lite": _BEDROCK,
    "BedrockGlm5": _BEDROCK,
    # Bedrock Mantle
    "BedrockGpt55Medium": _MANTLE,
    "BedrockGpt56LunaLow": _MANTLE,
    "BedrockGpt56TerraMedium": _MANTLE,
    # Azure AI Foundry
    "AzureGpt55Medium": _AZURE,
    "AzureGpt56LunaLow": _AZURE,
    "AzureGpt56LunaMedium": _AZURE,
    "AzureGpt56TerraMedium": _AZURE,
    "AzureGpt6LunaLow": _AZURE,
    # Google AI Studio
    "Gemini31FlashLiteMinimal": _GEMINI,
    "Gemini31FlashLiteLow": _GEMINI,
    "Gemini35FlashLiteLow": _GEMINI,
    "Gemini35FlashLiteMinimal": _GEMINI,
    "Gemini35FlashLiteMedium": _GEMINI,
    "Gemini31ProMedium": _GEMINI,
    # Vertex Gemini
    "VertexGeminiFlashLiteLow": (_GCP_SA,),
    # Vertex Model Garden (MaaS)
    "VertexLlama4ScoutRouter": (_GCP_SA, "VERTEX_LLAMA_OPENAI_BASE_URL"),
    "VertexQwen3NextRouter": (_GCP_SA, "VERTEX_QWEN_OPENAI_BASE_URL"),
}

# `aws.BedrockClient` does not implement `ai.stream.StreamingClient` yet, so a
# streamed call raises `StreamingUnsupported`. Strict, so the xfail flips to a
# failure the day Bedrock streaming lands and this list needs trimming.
_NO_STREAMING = {name for name, env in CLIENTS.items() if env is _BEDROCK}


def _params(streaming: bool) -> list:
    params = []
    for name in CLIENTS:
        marks = []
        if not _LIVE:
            marks.append(pytest.mark.skip(reason="set BAML_LIVE_PROVIDER_TESTS=1"))
        if streaming and name in _NO_STREAMING:
            marks.append(
                pytest.mark.xfail(reason="aws.BedrockClient cannot stream", strict=True)
            )
        params.append(pytest.param(name, id=name, marks=marks))
    return params


def _require_env(name: str) -> None:
    """Fail, not skip: a live run with a missing key is a broken setup."""
    missing = [var for var in CLIENTS[name] if not os.environ.get(var)]
    # The Model Garden clients take a pre-minted token in place of the
    # service account, as the v0 app does.
    if _GCP_SA in missing and name.endswith("Router") and os.environ.get("VERTEX_ACCESS_TOKEN"):
        missing.remove(_GCP_SA)
    if missing:
        pytest.fail(
            f"{name} needs {', '.join(missing)}; add them to the "
            f"`{_INFISICAL_ENV}` Infisical environment"
        )


def _assert_paris(result) -> None:
    from baml_sdk.providers import Capital

    assert isinstance(result, Capital)
    assert "paris" in result.city.lower(), result
    assert "france" in result.country.lower(), result


# SDK_PARITY_LINT(skip): live provider matrix is Python-only
@pytest.mark.parametrize("name", _params(streaming=False))
def test_provider_call(name: str):
    from baml_sdk.providers import provider_answer

    _require_env(name)
    _assert_paris(provider_answer(name, "France"))


# SDK_PARITY_LINT(skip): live provider matrix is Python-only
@pytest.mark.parametrize("name", _params(streaming=True))
def test_provider_stream(name: str):
    from baml_sdk.providers import provider_stream_answer

    _require_env(name)
    _assert_paris(provider_stream_answer(name, "France"))


# SDK_PARITY_LINT(skip): live provider matrix is Python-only
def test_unknown_client_is_rejected():
    """A name missing from `leaf_client` throws instead of silently
    picking some other client."""
    from baml_sdk.providers import provider_answer

    with pytest.raises(Exception, match="unknown provider client"):
        provider_answer("NotAClient", "France")


# ---------------------------------------------------------------------------
# Keyless: render each client's request with placeholder credentials.
# ---------------------------------------------------------------------------

_DUMMY_ENV = {
    "BEDROCK_AWS_ACCESS_KEY_ID": "AKIDEXAMPLE",
    "BEDROCK_AWS_SECRET_ACCESS_KEY": "dummy-secret",
    "BEDROCK_AWS_REGION": "us-west-2",
    "BEDROCK_MANTLE_API_KEY": "dummy-mantle-key",
    "BEDROCK_MANTLE_BASE_URL": "https://mantle.example.test/v1",
    "AZURE_OPENAI_API_KEY": "dummy-azure-key",
    "AZURE_OPENAI_RESPONSES_BASE_URL": "https://azure.example.test/openai/v1",
    "GOOGLE_API_KEY": "dummy-google-key",
    "GOOGLE_CLOUD_PROJECT": "dummy-project",
    "VERTEX_ACCESS_TOKEN": "dummy-vertex-token",
    "VERTEX_LLAMA_OPENAI_BASE_URL": "https://vertex.example.test/llama",
    "VERTEX_QWEN_OPENAI_BASE_URL": "https://vertex.example.test/qwen",
}

_BEDROCK_URL = "https://bedrock-runtime.us-west-2.amazonaws.com/model/"

_MANTLE_URL = "https://mantle.example.test/v1/responses"
_AZURE_URL = "https://azure.example.test/openai/v1/responses"
_GEMINI_URL = "https://generativelanguage.googleapis.com/"

# name -> (URL prefix, fragments the compact JSON body must contain).
_EXPECTED_REQUESTS: dict[str, tuple[str, tuple[str, ...]]] = {
    "BedrockSonnet5": (_BEDROCK_URL, ('"type":"adaptive"', '"effort":"medium"', '"maxTokens":30000')),
    "BedrockSonnet5Low": (_BEDROCK_URL, ('"type":"adaptive"', '"effort":"low"')),
    "BedrockGlm5": (_BEDROCK_URL, ('"maxTokens":30000', '"temperature":0.0')),
    "BedrockOpus5": (_BEDROCK_URL, ('"type":"adaptive"', '"effort":"medium"', '"maxTokens":4096')),
    "BedrockSonnet46": (_BEDROCK_URL, ('"type":"adaptive"', '"effort":"medium"')),
    "BedrockSonnet46Low": (_BEDROCK_URL, ('"type":"adaptive"', '"effort":"low"')),
    "BedrockHaiku45": (_BEDROCK_URL, ('"maxTokens":64000', '"temperature":0.0')),
    "BedrockNovaMicro": (_BEDROCK_URL, ('"maxTokens":256',)),
    "BedrockNova2Lite": (_BEDROCK_URL, ('"reasoningConfig":{"type":"disabled"}',)),
    "BedrockHaiku45Thinking1024": (_BEDROCK_URL, ('"budget_tokens":1024', '"temperature":1.0')),
    "BedrockHaiku45Thinking4096": (_BEDROCK_URL, ('"budget_tokens":4096', '"temperature":1.0')),
    "BedrockGpt55Medium": (
        _MANTLE_URL,
        ('"effort":"medium"', '"prompt_cache_key":"hybrd-agent-loop-bedrock-gpt55-v1"'),
    ),
    "BedrockGpt56LunaLow": (_MANTLE_URL, ('"effort":"low"', '"prompt_cache_options":{"mode":"explicit"}')),
    "BedrockGpt56TerraMedium": (
        _MANTLE_URL,
        ('"effort":"medium"', '"prompt_cache_options":{"mode":"implicit"}'),
    ),
    "AzureGpt55Medium": (_AZURE_URL, ('"model":"gpt-5.5"', '"effort":"medium"', '"store":false')),
    "AzureGpt56LunaLow": (_AZURE_URL, ('"model":"gpt-5.6-luna"', '"effort":"low"')),
    "AzureGpt56LunaMedium": (_AZURE_URL, ('"model":"gpt-5.6-luna"', '"effort":"medium"')),
    "AzureGpt56TerraMedium": (_AZURE_URL, ('"model":"gpt-5.6-terra"', '"effort":"medium"')),
    "AzureGpt6LunaLow": (_AZURE_URL, ('"model":"gpt-6-luna"', '"effort":"low"')),
    "Gemini31FlashLiteMinimal": (_GEMINI_URL, ('"thinkingLevel":"minimal"',)),
    "Gemini31FlashLiteLow": (_GEMINI_URL, ('"thinkingLevel":"low"',)),
    "Gemini35FlashLiteMinimal": (_GEMINI_URL, ('"thinkingLevel":"minimal"', '"temperature":0.0')),
    "Gemini35FlashLiteLow": (_GEMINI_URL, ('"thinkingLevel":"low"', '"temperature":0.0')),
    "Gemini35FlashLiteMedium": (_GEMINI_URL, ('"thinkingLevel":"medium"', '"temperature":0.0')),
    "Gemini31ProMedium": (_GEMINI_URL, ('"thinkingLevel":"medium"',)),
    "VertexGeminiFlashLiteLow": ("https://aiplatform.googleapis.com/", ('"thinkingLevel":"low"',)),
    "VertexLlama4ScoutRouter": (
        "https://vertex.example.test/llama/chat/completions",
        ('"model":"meta/llama-4-scout-17b-16e-instruct-maas"', '"temperature":0.0'),
    ),
    "VertexQwen3NextRouter": (
        "https://vertex.example.test/qwen/chat/completions",
        ('"model":"qwen/qwen3-next-80b-a3b-instruct-maas"', '"temperature":0.0'),
    ),
}


# SDK_PARITY_LINT(skip): live provider matrix is Python-only
def test_request_expectations_cover_every_client():
    assert set(_EXPECTED_REQUESTS) == set(CLIENTS)


# SDK_PARITY_LINT(skip): live provider matrix is Python-only
@pytest.mark.parametrize("name", list(CLIENTS))
def test_provider_request(name: str, monkeypatch: pytest.MonkeyPatch):
    import json

    from baml_sdk.providers import provider_request

    for var, value in _DUMMY_ENV.items():
        monkeypatch.setenv(var, value)

    url_prefix, fragments = _EXPECTED_REQUESTS[name]
    request = provider_request(name, "France")
    assert request.method == "POST"
    assert request.url.startswith(url_prefix), request.url
    body = json.dumps(json.loads(request.body), separators=(",", ":"))
    assert "France" in body
    for fragment in fragments:
        assert fragment in body, body
    if name.startswith("Azure"):
        assert "api-key" in [h.lower() for h in request.header_names]
