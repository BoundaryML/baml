from ..baml_client.sync_client import b as sync_b
import os


def test_env_vars_in_headers():
    # Load environment variables before any client initialization
    os.environ["OPENAI_API_KEY"] = "test"
    
    # Make a request and check the headers
    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    
    # Verify the environment variable is in the headers
    headers = request.headers
    print(headers)

    os.environ["OPENAI_API_KEY"] = "test2"

    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    
    # Verify the environment variable is in the headers
    headers = request.headers
    print(headers)