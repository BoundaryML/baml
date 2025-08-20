#!/usr/bin/env python
"""Manual test script for Phase 2 verification"""

import os
import sys
import asyncio
import time

# Add the package to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# Set library path
os.environ['BAML_LIBRARY_PATH'] = '/Users/greghale/code/baml-4/engine/target/debug/libbaml_cffi.dylib'

print("Testing Phase 2 implementation...")
print("-" * 50)

# Test 1: Callbacks can be imported
print("Test 1: Callback module imports successfully")
try:
    from baml_py_cffi._callbacks import register_callbacks, CallbackState
    from baml_py_cffi._async_utils import make_async_call
    print("✓ Callback modules imported successfully")
except Exception as e:
    print(f"✗ Failed to import callback modules: {e}")
    sys.exit(1)

# Test 2: Callbacks can be registered
print("\nTest 2: Callbacks can be registered")
try:
    from baml_py_cffi._ffi import _lib
    register_callbacks(_lib)
    print("✓ Callbacks registered successfully")
except Exception as e:
    print(f"✗ Failed to register callbacks: {e}")
    sys.exit(1)

# Test 3: CallbackState works
print("\nTest 3: CallbackState can be created")
try:
    loop = asyncio.new_event_loop()
    future = loop.create_future()
    state = CallbackState(future=future, loop=loop)
    print("✓ CallbackState created successfully")
except Exception as e:
    print(f"✗ Failed to create CallbackState: {e}")
    sys.exit(1)

# Test 4: Test async capabilities
print("\nTest 4: Test async capabilities")
async def test_async():
    """Simple async test"""
    try:
        # Just verify we can create and use async constructs
        future = asyncio.get_event_loop().create_future()
        future.set_result("test result")
        result = await future
        assert result == "test result"
        print("✓ Async capabilities work correctly")
        return True
    except Exception as e:
        print(f"✗ Async test failed: {e}")
        return False

# Run async test
try:
    success = asyncio.run(test_async())
    if not success:
        sys.exit(1)
except Exception as e:
    print(f"✗ Failed to run async test: {e}")
    sys.exit(1)

print("\n" + "=" * 50)
print("✓ All Phase 2 tests passed!")
print("=" * 50)