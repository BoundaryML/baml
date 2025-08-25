"""
Test actual BAML runtime method calls with primitive returns.
These tests validate the end-to-end pipeline of encoding arguments,
calling functions through the CFFI layer, and decoding results.
Uses real OpenAI API calls through infisical.
"""

import pytest
import asyncio
import json
import os
from typing import Dict, Any

# Import without triggering library load
import sys
sys.path.insert(0, 'baml_py_cffi/serde')
import cffi_pb2

# Now import the rest
from baml_py_cffi import create_runtime
from baml_py_cffi.serde.encode import encode_value, encode_function_args
from baml_py_cffi.serde.decode import decode_value


class TestRuntimeMethodCalls:
    """Test actual BAML runtime method calls with primitive returns using real API"""

    @pytest.fixture
    def baml_files(self):
        """BAML function definitions for testing"""
        return {
            "test_functions.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function TestEchoString(input: string) -> string {
                    client TestClient
                    prompt #"
                        Return EXACTLY this text with no changes: {{ input }}
                        Do not add any punctuation, quotes, or formatting.
                        Just return the exact text.
                    "#
                }

                function TestReturnInt() -> int {
                    client TestClient
                    prompt #"
                        Return exactly the number 42 as an integer.
                        Just the number, nothing else.
                    "#
                }

                function TestReturnBool() -> bool {
                    client TestClient
                    prompt #"
                        Return the boolean value true.
                        Just return: true
                    "#
                }

                function TestConcatStrings(first: string, second: string) -> string {
                    client TestClient
                    prompt #"
                        Concatenate these two strings with a space between them:
                        First: {{ first }}
                        Second: {{ second }}
                        Return only the concatenated result, nothing else.
                    "#
                }

                function TestReturnFloat() -> float {
                    client TestClient
                    prompt #"
                        Return exactly the number 3.14 as a float.
                        Just the number, nothing else.
                    "#
                }

                function TestReturnNull() -> string? {
                    client TestClient
                    prompt #"
                        Return null.
                        Just return: null
                    "#
                }

                function TestListOfStrings() -> string[] {
                    client TestClient
                    prompt #"
                        Return exactly this JSON array of strings:
                        ["apple", "banana", "orange"]
                    "#
                }

                function TestMapOfPrimitives() -> map<string, string> {
                    client TestClient
                    prompt #"
                        Return exactly this JSON object:
                        {"name": "John", "age": "30", "city": "NYC"}
                    "#
                }
            """
        }

    @pytest.mark.asyncio
    async def test_simple_string_return(self, baml_files):
        """Test calling a function that returns a primitive string"""
        # Create runtime with test BAML files and real API key
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Prepare arguments - encode the input string
        args = {"input": "Hello World"}
        encoded_args = self._encode_function_args("TestEchoString", args)

        # Call the function through the real async pipeline
        result = await rt.call_function("TestEchoString", encoded_args)

        # Decode and verify result
        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, str)
        assert "Hello World" in decoded

    @pytest.mark.asyncio
    async def test_multiple_string_parameters(self, baml_files):
        """Test function with multiple string parameters"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        args = {"first": "Hello", "second": "World"}
        encoded_args = self._encode_function_args("TestConcatStrings", args)

        result = await rt.call_function("TestConcatStrings", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, str)
        assert "Hello" in decoded and "World" in decoded

    @pytest.mark.asyncio
    async def test_primitive_type_returns(self, baml_files):
        """Test functions returning different primitive types"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Test int return
        encoded_args = self._encode_function_args("TestReturnInt", {})

        result = await rt.call_function("TestReturnInt", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, int)
        assert decoded == 42

        # Test bool return
        encoded_args = self._encode_function_args("TestReturnBool", {})

        result = await rt.call_function("TestReturnBool", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, bool)
        assert decoded == True

        # Test float return
        encoded_args = self._encode_function_args("TestReturnFloat", {})

        result = await rt.call_function("TestReturnFloat", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, float)
        assert abs(decoded - 3.14) < 0.01  # Allow small floating point differences

    @pytest.mark.asyncio
    async def test_collection_returns(self, baml_files):
        """Test functions returning collections"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Test list of strings
        encoded_args = self._encode_function_args("TestListOfStrings", {})

        result = await rt.call_function("TestListOfStrings", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, list)
        assert len(decoded) == 3
        assert "apple" in decoded
        assert "banana" in decoded
        assert "orange" in decoded

        # Test map of primitives
        encoded_args = self._encode_function_args("TestMapOfPrimitives", {})

        result = await rt.call_function("TestMapOfPrimitives", encoded_args)

        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        decoded = decode_value(holder)
        assert isinstance(decoded, dict)
        assert "name" in decoded
        assert "age" in decoded
        assert decoded["name"] == "John"
        assert decoded["age"] == "30"

    @pytest.mark.asyncio
    async def test_concurrent_calls(self, baml_files):
        """Test multiple concurrent function calls"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Create multiple function calls
        calls = []
        for i in range(3):  # Reduced from 5 to 3 to avoid rate limits
            args = {"input": f"test_{i}"}
            encoded_args = self._encode_function_args("TestEchoString", args)
            calls.append(rt.call_function("TestEchoString", encoded_args))

        # Execute all calls concurrently
        results = await asyncio.gather(*calls)

        # Verify all results
        assert len(results) == 3
        for i, result in enumerate(results):
            holder = cffi_pb2.CFFIValueHolder()
            holder.ParseFromString(result)
            decoded = decode_value(holder)
            assert isinstance(decoded, str)
            assert f"test_{i}" in decoded

    @pytest.mark.asyncio
    async def test_error_propagation(self, baml_files):
        """Test that errors are properly propagated through the async pipeline"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Try to call with invalid function name
        args = {"input": "test"}
        encoded_args = self._encode_function_args("NonExistentFunction", args)

        # This should raise an error
        with pytest.raises(Exception):  # The actual error type may vary
            await rt.call_function("NonExistentFunction", encoded_args)

    @pytest.mark.asyncio
    async def test_complex_nested_data(self, baml_files):
        """Test encoding and decoding of complex nested data structures"""
        rt = create_runtime(".", baml_files, os.environ.copy())

        # Test nested lists and maps through encode/decode cycle
        complex_data = {
            "users": [
                {"name": "Alice", "age": 30, "active": True},
                {"name": "Bob", "age": 25, "active": False}
            ],
            "metadata": {
                "count": 2,
                "version": "1.0",
                "tags": ["user", "data", "test"]
            },
            "nullable_field": None
        }

        # Encode the complex data
        encoded = encode_value(complex_data)
        serialized = encoded.SerializeToString()

        # Decode it back
        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(serialized)
        decoded = decode_value(holder)

        # Verify the structure is preserved
        assert decoded == complex_data
        assert decoded["users"][0]["name"] == "Alice"
        assert decoded["users"][0]["age"] == 30
        assert decoded["users"][0]["active"] is True
        assert decoded["metadata"]["tags"] == ["user", "data", "test"]
        assert decoded["nullable_field"] is None

    def _encode_function_args(self, function_name: str, args: dict) -> bytes:
        """Helper to encode function arguments"""
        return encode_function_args(args)
