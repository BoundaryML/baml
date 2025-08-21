import pytest
import asyncio
from baml_py_cffi import create_runtime, BamlRuntime, version


class TestRuntime:
    """Test basic runtime operations"""

    def test_version(self):
        """Test that version function works"""
        v = version()
        assert isinstance(v, str)
        assert len(v) > 0
        print(f"BAML Library version: {v}")

    def test_runtime_creation(self):
        """Test that runtime can be created"""
        rt = create_runtime(".", {}, {})
        assert rt is not None
        assert isinstance(rt, BamlRuntime)

        # Test that it has the expected attributes
        assert hasattr(rt, "_ptr")
        assert rt._ptr is not None

    def test_multiple_runtime_creation(self):
        """Test that multiple runtimes can be created without interference"""
        rt1 = create_runtime(".", {}, {})
        rt2 = create_runtime(".", {}, {})

        assert rt1 is not rt2
        assert rt1._ptr != rt2._ptr

    def test_runtime_with_src_files(self):
        """Test runtime creation with source files"""
        src_files = {"test.baml": "// Test BAML file\n"}
        env_vars = {"TEST_VAR": "test_value"}

        rt = create_runtime(".", src_files, env_vars)
        assert rt is not None
        assert isinstance(rt, BamlRuntime)

    @pytest.mark.asyncio
    async def test_call_function_method_exists(self):
        """Test that call_function method exists and is callable"""
        rt = create_runtime(".", {}, {})
        assert hasattr(rt, "call_function")
        assert callable(rt.call_function)

        # We can't test actual function calls without a proper BAML function
        # but we can test that the method signature is correct
        # The actual call would fail since we don't have a real function
        # This is just to verify the method exists and has the right structure


if __name__ == "__main__":
    # Run tests manually
    test = TestRuntime()
    test.test_version()
    test.test_runtime_creation()
    test.test_multiple_runtime_creation()
    test.test_runtime_with_src_files()
    asyncio.run(test.test_call_function_method_exists())
    print("All tests passed!")
