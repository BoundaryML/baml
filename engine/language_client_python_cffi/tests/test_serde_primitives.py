import pytest
from baml_py_cffi.serde.encode import encode_value
from baml_py_cffi.serde.decode import decode_value


class TestPrimitiveSerde:
    """Test serialization/deserialization of primitive types."""

    def test_string_roundtrip(self):
        """Test string encoding and decoding."""
        original = "hello"
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_int_roundtrip(self):
        """Test integer encoding and decoding."""
        original = 42
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_float_roundtrip(self):
        """Test float encoding and decoding."""
        original = 3.14
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_bool_true_roundtrip(self):
        """Test boolean True encoding and decoding."""
        original = True
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_bool_false_roundtrip(self):
        """Test boolean False encoding and decoding."""
        original = False
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_none_roundtrip(self):
        """Test None encoding and decoding."""
        original = None
        encoded = encode_value(original)
        decoded = decode_value(encoded)
        assert decoded == original

    def test_various_strings(self):
        """Test various string values."""
        test_cases = [
            "",  # Empty string
            " ",  # Single space
            "with spaces",
            "with\nnewlines",
            "with\ttabs",
            "unicode: ñ, 你好, 🎉",
            "special: !@#$%^&*()",
        ]

        for original in test_cases:
            encoded = encode_value(original)
            decoded = decode_value(encoded)
            assert decoded == original, f"Failed for string: {repr(original)}"

    def test_various_numbers(self):
        """Test various numeric values."""
        test_cases = [
            0,
            -1,
            1,
            -42,
            2**31 - 1,  # Max 32-bit int
            -(2**31),  # Min 32-bit int
            0.0,
            -0.0,
            1.5,
            -3.14159,
            float("inf"),
            float("-inf"),
        ]

        for original in test_cases:
            encoded = encode_value(original)
            decoded = decode_value(encoded)
            if isinstance(original, float) and original != original:  # NaN check
                assert decoded != decoded  # NaN != NaN
            else:
                assert decoded == original, f"Failed for number: {original}"

    def test_lists_and_maps_are_supported(self):
        """Test that lists and maps are now supported (phase 4.2)."""
        # Lists should work fine now
        list_val = [1, 2, 3]
        encoded = encode_value(list_val)
        decoded = decode_value(encoded)
        assert decoded == list_val
        
        # Dicts should work fine now
        dict_val = {"key": "value"}
        encoded = encode_value(dict_val)
        decoded = decode_value(encoded)
        assert decoded == dict_val
        
    def test_unsupported_type_raises(self):
        """Test that truly unsupported types raise ValueError."""
        # Custom objects not supported
        with pytest.raises(ValueError, match="Unsupported type"):
            encode_value(object())
            
        # Sets not supported
        with pytest.raises(ValueError, match="Unsupported type"):
            encode_value({1, 2, 3})
            
        # Tuples not supported (yet)
        with pytest.raises(ValueError, match="Unsupported type"):
            encode_value((1, 2, 3))
            
        # Custom classes not supported (without type map)
        class CustomClass:
            pass
        with pytest.raises(ValueError, match="Unsupported type"):
            encode_value(CustomClass())
