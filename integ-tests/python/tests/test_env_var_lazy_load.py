import pytest
from ..baml_client.sync_client import b as sync_b
from ..baml_client.async_client import b as async_b
from baml_py.baml_py import get_log_level


@pytest.mark.parametrize("test_input,expected_key", [
    ("test", "test"),
    ("test2", "test2"),
])
def test_env_vars_in_headers(monkeypatch, test_input, expected_key):
    """Test that environment variable changes are reflected in request headers."""
    # Set the API key using monkeypatch
    monkeypatch.setenv("OPENAI_API_KEY", test_input)
    
    # Make a request and check the headers
    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    headers = request.headers
    
    # Verify the API key is in the headers  
    assert expected_key in str(headers), f"API key '{expected_key}' not found in headers"
    print(f"Headers with key '{expected_key}':", headers)


def test_env_var_changes_are_reflected(monkeypatch):
    """Test that changing environment variables between requests updates the headers."""
    # Initial request with first key
    monkeypatch.setenv("OPENAI_API_KEY", "test")
    try:
        request1 = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    except Exception:
        pass
    assert "test" in str(request1.headers), "Initial API key not found in headers"
    
    # Change key and make second request
    monkeypatch.setenv("OPENAI_API_KEY", "test2")
    try:
        request2 = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    except Exception:
        pass
    assert "test2" in str(request2.headers), "Updated API key not found in headers"
    
    # Verify headers are different
    assert request1.headers != request2.headers, "Headers should be different after API key change"

@pytest.mark.asyncio
async def test_env_var_changes_are_reflected_in_log_level(monkeypatch):
    """Test that changing environment variables between requests updates the log level."""
    monkeypatch.setenv("BAML_LOG", "WARN")
    await async_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    assert get_log_level() == "WARN"