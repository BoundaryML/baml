import os
import pytest
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from baml_py import Collector

def test_env_vars_are_loaded_lazily():
    """Test that environment variables are loaded just before function calls."""
    # Enable BAML logging
    os.environ["BAML_LOG"] = "debug"
    
    # Create a collector to track function calls
    collector = Collector(name="test-collector")
    
    # Set initial environment variables
    # os.environ["OPENAI_API_KEY"] = "test1-key"
    
    # Call a function with the collector
    result1 = sync_b.TestOpenAIShorthand("test1", baml_options={"collector": collector})
    assert result1 is not None
    
    # Get the first function log
    log1 = collector.last
    assert log1 is not None
    calls1 = log1.calls
    assert len(calls1) > 0
    # Get the first API call and print its headers
    call1 = calls1[0]
    request1 = call1.http_request
    assert request1 is not None
    print("First call headers:", request1.headers)
    
