import pytest
from ..baml_client.sync_client import b as sync_b
import os


@pytest.fixture
def api_key():
    """Fixture to manage API key environment variable."""
    original_key = os.environ.get("OPENAI_API_KEY")
    yield
    # Restore original key after test
    if original_key is not None:
        os.environ["OPENAI_API_KEY"] = original_key
    else:
        os.environ.pop("OPENAI_API_KEY", None)


@pytest.mark.parametrize("test_input,expected_key", [
    ("test", "test"),
    ("test2", "test2"),
])
def test_env_vars_in_headers(api_key, test_input, expected_key):
    """Test that environment variable changes are reflected in request headers."""
    # Set the API key
    os.environ["OPENAI_API_KEY"] = test_input
    
    # Make a request and check the headers
    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    headers = request.headers
    
    # Verify the API key is in the headers
    assert expected_key in str(headers), f"API key '{expected_key}' not found in headers"
    print(f"Headers with key '{expected_key}':", headers)


def test_env_var_changes_are_reflected(api_key):
    """Test that changing environment variables between requests updates the headers."""
    # Initial request with first key
    os.environ["OPENAI_API_KEY"] = "test"
    request1 = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    assert "test" in str(request1.headers), "Initial API key not found in headers"
    
    # Change key and make second request
    os.environ["OPENAI_API_KEY"] = "test2"
    request2 = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    assert "test2" in str(request2.headers), "Updated API key not found in headers"
    
    # Verify headers are different
    assert request1.headers != request2.headers, "Headers should be different after API key change"