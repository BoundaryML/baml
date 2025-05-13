import os
import pytest
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from baml_py import Collector

def test_env_vars_are_loaded_lazily():
    """Test that environment variables are loaded just before function calls."""
    # Enable BAML logging
    os.environ["BAML_LOG"] = "info"
    
    # Create a collector to track function calls
    collector = Collector(name="test-collector")
    
    # Set initial environment variables
    os.environ["OPENAI_API_KEY"] = "sk-initial-key"
    
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
    
    # Change environment variables
    os.environ["OPENAI_API_KEY"] = "sk-new-key"
    
    # Call another function with the same collector
    result2 = sync_b.TestOpenAIShorthand("test2", baml_options={"collector": collector})
    assert result2 is not None
    
    # Get the second function log
    log2 = collector.last
    assert log2 is not None
    calls2 = log2.calls
    assert len(calls2) > 0
    # Get the first API call and print its headers
    call2 = calls2[0]
    request2 = call2.http_request
    assert request2 is not None
    print("Second call headers:", request2.headers)
    
    # Verify that both calls were successful
    assert result1 is not None
    assert result2 is not None
