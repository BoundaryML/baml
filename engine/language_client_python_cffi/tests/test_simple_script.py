#!/usr/bin/env python3
"""
Simple script to test basic runtime functionality with real OpenAI API.
Can be run directly: python test_simple_script.py

This script demonstrates the end-to-end flow of:
1. Creating a runtime with BAML functions
2. Encoding function arguments using protobuf
3. Making async calls through the runtime to real OpenAI API
4. Decoding results from protobuf
"""

import asyncio
import os
import sys

# Add serde to path to import cffi_pb2 without triggering library load
sys.path.insert(0, 'baml_py_cffi/serde')
import cffi_pb2

# Now import the rest
from baml_py_cffi import create_runtime
from baml_py_cffi.serde.encode import encode_value, encode_function_args
from baml_py_cffi.serde.decode import decode_value


async def test_basic_echo():
    """Test basic runtime call with a simple string function using real API"""

    # Define a simple BAML function
    baml_src = {
        "test.baml": """
            client<llm> TestClient {
                provider "openai"
                options {
                    model "gpt-4o-mini"
                    api_key env.OPENAI_API_KEY
                }
            }

            function Echo(message: string) -> string {
                client TestClient
                prompt #"
                    Echo back exactly: {{ message }}
                    Return ONLY the message text, nothing else.
                "#
            }
        """
    }

    print("Creating runtime...")
    rt = create_runtime(".", baml_src, os.environ.copy())

    print("Encoding arguments...")
    # Encode the function arguments
    encoded_args = encode_function_args({"message": "Hello, BAML!"})

    print("Calling function with real API...")
    # Make the actual async call to OpenAI
    result = await rt.call_function("Echo", encoded_args)

    print("Decoding result...")
    # Decode the result
    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)

    print(f"Result: {decoded}")
    print(f"Type: {type(decoded)}")

    assert isinstance(decoded, str), f"Expected string, got {type(decoded)}"
    assert "Hello, BAML!" in decoded, f"Expected 'Hello, BAML!' in response, got '{decoded}'"
    print("✓ Basic echo test passed!")


async def test_multiple_types():
    """Test handling of different primitive types with real API"""

    baml_src = {
        "test.baml": """
            client<llm> TestClient {
                provider "openai"
                options {
                    model "gpt-4o-mini"
                    api_key env.OPENAI_API_KEY
                }
            }

            function GetInt() -> int {
                client TestClient
                prompt #"Return exactly the number: 42"#
            }

            function GetBool() -> bool {
                client TestClient
                prompt #"Return exactly: true"#
            }

            function GetList() -> string[] {
                client TestClient
                prompt #"Return exactly this JSON array: [\"red\", \"green\", \"blue\"]"#
            }
        """
    }

    print("\nTesting multiple types with real API...")
    rt = create_runtime(".", baml_src, os.environ.copy())


    # Test integer return
    print("- Testing integer return...")
    encoded_args = encode_function_args({})

    result = await rt.call_function("GetInt", encoded_args)

    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)
    assert decoded == 42
    print(f"  ✓ Got integer: {decoded}")

    # Test boolean return
    print("- Testing boolean return...")
    result = await rt.call_function("GetBool", encoded_args)

    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)
    assert decoded == True
    print(f"  ✓ Got boolean: {decoded}")

    # Test list return
    print("- Testing list return...")
    result = await rt.call_function("GetList", encoded_args)

    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)
    assert isinstance(decoded, list)
    assert len(decoded) == 3
    assert "red" in decoded
    print(f"  ✓ Got list: {decoded}")

    print("✓ Multiple types test passed!")


async def test_complex_data():
    """Test encoding and decoding of complex nested data"""

    print("\nTesting complex data structures...")

    # Test complex nested structure
    complex_data = {
        "user": {
            "id": 123,
            "name": "Alice",
            "email": "alice@example.com",
            "active": True,
            "score": 98.5,
            "tags": ["admin", "user", "verified"],
            "metadata": {
                "created": "2024-01-01",
                "updated": "2024-01-15",
                "preferences": {
                    "theme": "dark",
                    "notifications": True,
                    "language": "en"
                }
            },
            "nullable_field": None
        },
        "items": [
            {"id": 1, "name": "Item 1", "price": 10.99},
            {"id": 2, "name": "Item 2", "price": 20.50},
            {"id": 3, "name": "Item 3", "price": 15.00}
        ],
        "stats": {
            "total": 3,
            "average": 15.5,
            "categories": ["electronics", "books", "clothing"]
        }
    }

    print("- Encoding complex structure...")
    encoded = encode_value(complex_data)
    serialized = encoded.SerializeToString()

    print("- Decoding complex structure...")
    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(serialized)
    decoded = decode_value(holder)

    # Verify the structure is preserved
    assert decoded == complex_data
    assert decoded["user"]["id"] == 123
    assert decoded["user"]["name"] == "Alice"
    assert decoded["user"]["active"] is True
    assert decoded["user"]["score"] == 98.5
    assert decoded["user"]["tags"] == ["admin", "user", "verified"]
    assert decoded["user"]["metadata"]["preferences"]["theme"] == "dark"
    assert decoded["user"]["nullable_field"] is None
    assert len(decoded["items"]) == 3
    assert decoded["items"][0]["price"] == 10.99
    assert decoded["stats"]["categories"] == ["electronics", "books", "clothing"]

    print("  ✓ Complex structure encoded and decoded correctly")
    print("✓ Complex data test passed!")


async def test_real_api_with_parameters():
    """Test a function with parameters using real API"""

    print("\nTesting function with parameters using real API...")

    baml_src = {
        "test.baml": """
            client<llm> TestClient {
                provider "openai"
                options {
                    model "gpt-4o-mini"
                    api_key env.OPENAI_API_KEY
                }
            }

            function FormatName(first: string, last: string) -> string {
                client TestClient
                prompt #"
                    Format this name as 'Last, First':
                    First name: {{ first }}
                    Last name: {{ last }}

                    Return ONLY the formatted name in the form 'Last, First'.
                "#
            }

            function Calculate(x: int, y: int, operation: string) -> int {
                client TestClient
                prompt #"
                    Calculate {{ x }} {{ operation }} {{ y }}.
                    Return ONLY the numerical result.
                "#
            }
        """
    }

    rt = create_runtime(".", baml_src, os.environ.copy())

    # Test FormatName
    print("- Testing FormatName function...")
    encoded_args = encode_function_args({"first": "John", "last": "Doe"})

    result = await rt.call_function("FormatName", encoded_args)

    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)

    assert isinstance(decoded, str)
    assert "Doe" in decoded and "John" in decoded
    print(f"  ✓ Got formatted name: {decoded}")

    # Test Calculate
    print("- Testing Calculate function...")
    encoded_args = encode_function_args({"x": 10, "y": 5, "operation": "plus"})

    result = await rt.call_function("Calculate", encoded_args)

    holder = cffi_pb2.CFFIValueHolder()
    holder.ParseFromString(result)
    decoded = decode_value(holder)

    assert isinstance(decoded, int)
    assert decoded == 15
    print(f"  ✓ Got calculation result: {decoded}")

    print("✓ Real API with parameters test passed!")


async def test_error_handling():
    """Test error handling in the pipeline"""

    print("\nTesting error handling...")

    baml_src = {
        "test.baml": """
            client<llm> TestClient {
                provider "openai"
                options {
                    model "gpt-4o-mini"
                    api_key env.OPENAI_API_KEY
                }
            }

            function TestFunction(input: string) -> string {
                client TestClient
                prompt #"Echo: {{ input }}"#
            }
        """
    }

    rt = create_runtime(".", baml_src, os.environ.copy())

    # Test with non-existent function
    print("- Testing error with non-existent function...")
    encoded_args = encode_function_args({"input": "test"})

    try:
        await rt.call_function("NonExistentFunction", encoded_args)
        assert False, "Should have raised an error"
    except Exception as e:
        print(f"  ✓ Error caught as expected: {type(e).__name__}")

    print("✓ Error handling test passed!")


async def main():
    """Run all tests"""

    print("=" * 60)
    print("BAML Python CFFI Simple Script Test with Real OpenAI API")
    print("=" * 60)

    # Check for API key
    if not os.environ.get("OPENAI_API_KEY"):
        print("Warning: OPENAI_API_KEY not set, tests will fail")
        print("Run with: infisical run --env=test -- python test_simple_script.py")
        sys.exit(1)

    try:
        await test_basic_echo()
        await test_multiple_types()
        await test_complex_data()
        await test_real_api_with_parameters()
        await test_error_handling()

        print("\n" + "=" * 60)
        print("✅ ALL TESTS PASSED WITH REAL API!")
        print("=" * 60)

    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    # Check if we're running with the right environment
    if os.environ.get("BAML_LIBRARY_PATH"):
        print(f"Using BAML library: {os.environ['BAML_LIBRARY_PATH']}")
    else:
        print("Warning: BAML_LIBRARY_PATH not set, library loading may fail")

    asyncio.run(main())
