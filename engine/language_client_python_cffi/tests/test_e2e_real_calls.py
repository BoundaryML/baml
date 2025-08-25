"""
End-to-end tests with real async calls through CFFI.
These tests validate the complete pipeline without mocking,
using actual callback routing and async/await patterns with real OpenAI API.
"""

import pytest
import asyncio
import os
from typing import Dict, Any, List
import ctypes

# Import without triggering library load
from baml_py_cffi.serde import cffi_pb2

from baml_py_cffi import create_runtime
from baml_py_cffi.serde.encode import encode_value, encode_function_args
from baml_py_cffi.serde.decode import decode_value
from baml_py_cffi._callbacks import _trigger_callback, _error_callback, _active_callbacks, _callback_lock


class TestEndToEndRealCalls:
    """End-to-end tests with real async calls through CFFI and OpenAI API"""

    @pytest.mark.asyncio
    async def test_full_pipeline_string_function(self):
        """Test complete pipeline: encode -> call -> callback -> decode with real API"""

        # Create runtime with inline BAML using OpenAI
        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function GetGreeting(name: string) -> string {
                    client TestClient
                    prompt #"
                        Say hello to {{ name }}.
                        Respond with ONLY a simple greeting like 'Hello, [name]!'
                        Do not add any other text.
                    "#
                }

                function CalculateSum(a: int, b: int) -> int {
                    client TestClient
                    prompt #"
                        Calculate {{ a }} + {{ b }}.
                        Return ONLY the number result, nothing else.
                    "#
                }
            """
        }

        # Do not log sensitive API keys
        rt = create_runtime(".", baml_src, os.environ.copy())

        # Test string function with real API
        args = {"name": "Alice"}
        encoded_args = self._create_encoded_args("GetGreeting", args)

        # Call through real async pipeline - no mocking
        result = await rt.call_function("GetGreeting", encoded_args)

        # Verify the result
        decoded = self._decode_result(result)
        assert isinstance(decoded, str)
        assert "Alice" in decoded

        # Test int function with real API
        args = {"a": 5, "b": 3}
        encoded_args = self._create_encoded_args("CalculateSum", args)

        result = await rt.call_function("CalculateSum", encoded_args)
        decoded = self._decode_result(result)
        assert isinstance(decoded, int)
        assert decoded == 8

    @pytest.mark.asyncio
    async def test_callback_mechanism_integration(self):
        """Test that the callback mechanism works end-to-end"""

        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function TestCallback(input: string) -> string {
                    client TestClient
                    prompt #"
                        Echo exactly: {{ input }}
                        Return only the input text, nothing else.
                    "#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        # Test real callback through actual API call
        args = {"input": "callback test"}
        encoded_args = self._create_encoded_args("TestCallback", args)

        result = await rt.call_function("TestCallback", encoded_args)
        decoded = self._decode_result(result)

        assert isinstance(decoded, str)
        assert "callback test" in decoded.lower()

    @pytest.mark.asyncio
    async def test_concurrent_real_calls(self):
        """Test concurrent calls through the real async pipeline"""
        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function ProcessItem(id: int, data: string) -> string {
                    client TestClient
                    prompt #"
                        Processing item {{ id }}: {{ data }}.
                        Return exactly: 'Processed item [id]: [data]'
                    "#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        # Create multiple concurrent calls
        tasks = []
        for i in range(3):  # Reduced to avoid rate limits
            args = {"id": i, "data": f"item_{i}"}
            encoded_args = self._create_encoded_args("ProcessItem", args)
            tasks.append(rt.call_function("ProcessItem", encoded_args))

        # Execute all calls concurrently through real async pipeline
        results = await asyncio.gather(*tasks)

        # Verify all results came back
        assert len(results) == 3
        for i, result in enumerate(results):
            decoded = self._decode_result(result)
            assert isinstance(decoded, str)
            assert str(i) in decoded

    @pytest.mark.asyncio
    async def test_error_handling_real_pipeline(self):
        """Test error propagation through the real pipeline"""
        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function ValidFunction(input: string) -> string {
                    client TestClient
                    prompt #"Echo: {{ input }}"#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        args = {"input": "test"}

        # Try to call a non-existent function
        encoded_args = self._create_encoded_args("InvalidFunction", args)

        # This should fail through the real async pipeline
        with pytest.raises(Exception):
            await rt.call_function("InvalidFunction", encoded_args)

    @pytest.mark.asyncio
    async def test_different_data_types(self):
        """Test different data types through real API calls"""

        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function GetBoolean() -> bool {
                    client TestClient
                    prompt #"Return exactly: true"#
                }

                function GetNumber() -> int {
                    client TestClient
                    prompt #"Return exactly: 100"#
                }

                function GetFloat() -> float {
                    client TestClient
                    prompt #"Return exactly: 2.5"#
                }

                function GetArray() -> string[] {
                    client TestClient
                    prompt #"Return exactly this JSON array: [\"red\", \"green\", \"blue\"]"#
                }

                function GetObject() -> map<string, string> {
                    client TestClient
                    prompt #"Return exactly this JSON object: {\"status\": \"ok\", \"count\": \"5\"}"#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        # Test boolean
        result = await rt.call_function("GetBoolean", self._create_encoded_args("GetBoolean", {}))
        decoded = self._decode_result(result)
        assert isinstance(decoded, bool)
        assert decoded == True

        # Test integer
        result = await rt.call_function("GetNumber", self._create_encoded_args("GetNumber", {}))
        decoded = self._decode_result(result)
        assert isinstance(decoded, int)
        assert decoded == 100

        # Test float
        result = await rt.call_function("GetFloat", self._create_encoded_args("GetFloat", {}))
        decoded = self._decode_result(result)
        assert isinstance(decoded, float)
        assert abs(decoded - 2.5) < 0.01

        # Test array
        result = await rt.call_function("GetArray", self._create_encoded_args("GetArray", {}))
        decoded = self._decode_result(result)
        assert isinstance(decoded, list)
        assert len(decoded) == 3
        assert "red" in decoded

        # Test object/map
        result = await rt.call_function("GetObject", self._create_encoded_args("GetObject", {}))
        decoded = self._decode_result(result)
        assert isinstance(decoded, dict)
        assert "status" in decoded
        assert decoded["status"] == "ok"

    @pytest.mark.asyncio
    async def test_complex_prompt_with_real_api(self):
        """Test a more complex prompt with real API"""

        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function AnalyzeText(text: string, criteria: string[]) -> map<string, string> {
                    client TestClient
                    prompt #"
                        Analyze the following text: {{ text }}

                        Check for these criteria: {{ criteria }}

                        Return a JSON object with exactly these keys:
                        - "length": "short", "medium", or "long"
                        - "tone": "formal", "casual", or "neutral"
                        - "has_numbers": "true" or "false"

                        Return ONLY the JSON object, no other text.
                    "#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        args = {
            "text": "Hello! This is test number 42.",
            "criteria": ["length", "tone", "numbers"]
        }
        encoded_args = self._create_encoded_args("AnalyzeText", args)

        result = await rt.call_function("AnalyzeText", encoded_args)
        decoded = self._decode_result(result)

        assert isinstance(decoded, dict)
        assert "length" in decoded
        assert "tone" in decoded
        assert "has_numbers" in decoded
        assert decoded["has_numbers"] == "true"  # The text contains "42"

    @pytest.mark.asyncio
    async def test_streaming_callback_simulation(self):
        """Test callbacks work correctly with actual API responses"""

        baml_src = {
            "main.baml": """
                client<llm> TestClient {
                    provider "openai"
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function StreamTest(input: string) -> string {
                    client TestClient
                    prompt #"
                        Repeat exactly three times with line breaks:
                        {{ input }}
                        {{ input }}
                        {{ input }}
                    "#
                }
            """
        }

        rt = create_runtime(".", baml_src, os.environ.copy())

        args = {"input": "test"}
        encoded_args = self._create_encoded_args("StreamTest", args)

        # Even though we're not streaming, test that the callback mechanism works
        result = await rt.call_function("StreamTest", encoded_args)
        decoded = self._decode_result(result)

        assert isinstance(decoded, str)
        assert decoded.count("test") >= 3

    @pytest.mark.asyncio
    async def test_mixed_type_encoding_decoding(self):
        """Test encoding and decoding of mixed primitive and collection types"""

        # Test various data types through encode/decode cycle
        test_cases = [
            # Primitives
            ("string", "Hello, BAML!"),
            ("int", 42),
            ("float", 3.14159),
            ("bool_true", True),
            ("bool_false", False),
            ("null", None),

            # Collections
            ("empty_list", []),
            ("string_list", ["a", "b", "c"]),
            ("int_list", [1, 2, 3, 4, 5]),
            ("mixed_list", [1, "two", 3.0, True, None]),
            ("empty_map", {}),
            ("string_map", {"key1": "value1", "key2": "value2"}),
            ("mixed_map", {"str": "text", "int": 42, "bool": True, "null": None}),

            # Nested structures
            ("nested_list", [[1, 2], [3, 4], [5, 6]]),
            ("nested_map", {"outer": {"inner": {"deep": "value"}}}),
            ("list_of_maps", [{"a": 1}, {"b": 2}, {"c": 3}]),
            ("map_of_lists", {"nums": [1, 2, 3], "strs": ["a", "b", "c"]}),

            # Complex nested
            ("complex", {
                "users": [
                    {"id": 1, "name": "Alice", "tags": ["admin", "user"]},
                    {"id": 2, "name": "Bob", "tags": ["user"]},
                ],
                "metadata": {
                    "version": "1.0",
                    "count": 2,
                    "features": {
                        "search": True,
                        "export": False,
                        "max_items": None
                    }
                }
            })
        ]

        for name, value in test_cases:
            # Encode
            encoded = encode_value(value)
            serialized = encoded.SerializeToString()

            # Decode
            holder = cffi_pb2.CFFIValueHolder()
            holder.ParseFromString(serialized)
            decoded = decode_value(holder)

            # Verify
            assert decoded == value, f"Failed for {name}: expected {value}, got {decoded}"

    def _create_encoded_args(self, function_name: str, args: dict) -> bytes:
        """Create properly encoded function arguments"""
        return encode_function_args(args)

    def _decode_result(self, result: bytes) -> Any:
        """Decode the result from protobuf"""
        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result)
        return decode_value(holder)
