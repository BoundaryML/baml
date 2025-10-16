import pytest
import asyncio
from baml_py import AbortController

def test_abort_controller_creation():
    """Test that AbortController can be created"""
    controller = AbortController()
    assert controller is not None
    assert controller.aborted is False

def test_abort_controller_abort():
    """Test that AbortController can be aborted"""
    controller = AbortController()
    controller.abort()
    assert controller.aborted is True

def test_abort_controller_multiple_aborts():
    """Test that AbortController can be aborted multiple times"""
    controller = AbortController()
    assert controller.aborted is False
    controller.abort()
    assert controller.aborted is True
    # Should be idempotent
    controller.abort()
    assert controller.aborted is True

@pytest.mark.asyncio
async def test_abort_controller_async():
    """Test AbortController in async context"""
    controller = AbortController()
    
    async def abort_after_delay():
        await asyncio.sleep(0.1)
        controller.abort()
    
    task = asyncio.create_task(abort_after_delay())
    
    # Check status before abort
    assert controller.aborted is False
    
    # Wait for abort
    await task
    
    # Check status after abort
    assert controller.aborted is True

def test_multiple_controllers():
    """Test that multiple controllers are independent"""
    controller1 = AbortController()
    controller2 = AbortController()
    
    controller1.abort()
    
    assert controller1.aborted is True
    assert controller2.aborted is False
    
    controller2.abort()
    
    assert controller1.aborted is True
    assert controller2.aborted is True

# Test with actual BAML client if available
try:
    from baml_client import b
    
    @pytest.mark.asyncio
    async def test_with_baml_client():
        """Test AbortController with BAML client"""
        controller = AbortController()
        
        # Abort immediately
        controller.abort()
        
        # Try to call a function - should fail
        with pytest.raises(Exception) as exc_info:
            await b.ExtractName(
                text="My name is Alice",
                baml_options={"abort_controller": controller}
            )
        
        assert "abort" in str(exc_info.value).lower()
    
    @pytest.mark.asyncio
    async def test_normal_operation():
        """Test that operations work normally without abort controller"""
        result = await b.ExtractName(text="My name is Alice")
        assert isinstance(result, str)
        assert "alice" in result.lower()
        
except ImportError:
    print("BAML client not available, skipping client tests")