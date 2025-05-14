from dotenv import load_dotenv
from ..baml_client.sync_client import b as sync_b
import os


def test_env_vars_in_headers():
    # Load environment variables before any client initialization
    load_dotenv(override=True)    
    
    # Make a request and check the headers
    request = sync_b.request.ExtractReceiptInfo("test@email.com", "curiosity")
    
    # Verify the environment variable is in the headers
    headers = request.headers
    print(headers)
    # assert test_env_var in headers, f"Expected {test_env_var} to be in request headers"
    # assert headers[test_env_var] == test_env_value, f"Expected header value {test_env_value}, got {headers[test_env_var]}"

    # Clean up
    # del os.environ[test_env_var]
