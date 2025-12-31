import pytest
import baml_py
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b


@pytest.mark.asyncio
async def test_client_option_routes_to_correct_client():
    """Test that the client option routes requests to the specified client."""
    # ExtractResume normally uses GPT4 (openai), but we override to Claude
    request = await b.request.ExtractResume(
        "test resume",
        baml_options={"client": "Claude"}
    )

    # Should route to Anthropic API
    assert "anthropic" in request.url.lower()


@pytest.mark.asyncio
async def test_client_option_takes_precedence_over_client_registry():
    """Test that client option takes precedence over client_registry.set_primary()."""
    cr = baml_py.ClientRegistry()
    cr.set_primary("GPT4")  # This should be overridden

    request = await b.request.ExtractResume(
        "test resume",
        baml_options={"client": "Claude", "client_registry": cr}
    )

    # client option should win - should route to Anthropic, not OpenAI
    assert "anthropic" in request.url.lower()


@pytest.mark.asyncio
async def test_client_registry_still_works():
    """Test that client_registry without client option still works."""
    cr = baml_py.ClientRegistry()
    cr.set_primary("Claude")

    request = await b.request.ExtractResume(
        "test resume",
        baml_options={"client_registry": cr}
    )

    # Should route to Anthropic API
    assert "anthropic" in request.url.lower()


@pytest.mark.asyncio
async def test_with_options_client():
    """Test that with_options(client=...) works correctly."""
    my_b = b.with_options(client="Claude")

    request = await my_b.request.ExtractResume("test resume")

    # Should route to Anthropic API
    assert "anthropic" in request.url.lower()


def test_client_option_sync():
    """Test that the client option works with sync client."""
    request = sync_b.request.ExtractResume(
        "test resume",
        baml_options={"client": "Claude"}
    )

    # Should route to Anthropic API
    assert "anthropic" in request.url.lower()


def test_with_options_client_sync():
    """Test that with_options(client=...) works with sync client."""
    my_b = sync_b.with_options(client="Claude")

    request = my_b.request.ExtractResume("test resume")

    # Should route to Anthropic API
    assert "anthropic" in request.url.lower()
