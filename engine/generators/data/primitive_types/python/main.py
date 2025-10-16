import asyncio
import sys
from baml_client import baml

async def test_primitive_types():
    print("Testing PrimitiveTypes...")
    result = await baml.TestPrimitiveTypes("test input")
    
    # Verify primitive values
    assert result.stringField == "Hello, BAML!", f"Expected 'Hello, BAML!', got '{result.stringField}'"
    assert result.intField == 42, f"Expected 42, got {result.intField}"
    assert 3.14 <= result.floatField <= 3.15, f"Expected ~3.14159, got {result.floatField}"
    assert result.boolField is True, f"Expected True, got {result.boolField}"
    assert result.nullField is None, f"Expected None, got {result.nullField}"
    print("✓ PrimitiveTypes test passed")

async def test_primitive_arrays():
    print("\nTesting PrimitiveArrays...")
    result = await baml.TestPrimitiveArrays("test arrays")
    
    # Verify array contents
    assert len(result.stringArray) == 3, f"Expected length 3, got {len(result.stringArray)}"
    assert len(result.intArray) == 5, f"Expected length 5, got {len(result.intArray)}"
    assert len(result.floatArray) == 4, f"Expected length 4, got {len(result.floatArray)}"
    assert len(result.boolArray) == 4, f"Expected length 4, got {len(result.boolArray)}"
    
    # Verify array types
    assert all(isinstance(x, str) for x in result.stringArray), "Not all elements are strings"
    assert all(isinstance(x, int) for x in result.intArray), "Not all elements are integers"
    assert all(isinstance(x, (int, float)) for x in result.floatArray), "Not all elements are numbers"
    assert all(isinstance(x, bool) for x in result.boolArray), "Not all elements are booleans"
    print("✓ PrimitiveArrays test passed")

async def test_primitive_maps():
    print("\nTesting PrimitiveMaps...")
    result = await baml.TestPrimitiveMaps("test maps")
    
    # Verify map contents
    assert len(result.stringMap) == 2, f"Expected length 2, got {len(result.stringMap)}"
    assert len(result.intMap) == 3, f"Expected length 3, got {len(result.intMap)}"
    assert len(result.floatMap) == 2, f"Expected length 2, got {len(result.floatMap)}"
    assert len(result.boolMap) == 2, f"Expected length 2, got {len(result.boolMap)}"
    
    # Verify map value types
    assert all(isinstance(v, str) for v in result.stringMap.values()), "Not all values are strings"
    assert all(isinstance(v, int) for v in result.intMap.values()), "Not all values are integers"
    assert all(isinstance(v, (int, float)) for v in result.floatMap.values()), "Not all values are numbers"
    assert all(isinstance(v, bool) for v in result.boolMap.values()), "Not all values are booleans"
    print("✓ PrimitiveMaps test passed")

async def test_mixed_primitives():
    print("\nTesting MixedPrimitives...")
    result = await baml.TestMixedPrimitives("test mixed")
    
    # Basic validation for mixed types
    assert isinstance(result.name, str) and result.name != "", "Name should be non-empty string"
    assert isinstance(result.age, int) and result.age > 0, "Age should be positive integer"
    assert isinstance(result.height, (int, float)) and result.height > 0, "Height should be positive number"
    assert isinstance(result.isActive, bool), "isActive should be boolean"
    assert result.metadata is None, "metadata should be None"
    assert isinstance(result.tags, list) and all(isinstance(x, str) for x in result.tags), "tags should be string array"
    assert isinstance(result.scores, list) and all(isinstance(x, int) for x in result.scores), "scores should be int array"
    assert isinstance(result.measurements, list) and all(isinstance(x, (int, float)) for x in result.measurements), "measurements should be number array"
    assert isinstance(result.flags, list) and all(isinstance(x, bool) for x in result.flags), "flags should be bool array"
    print("✓ MixedPrimitives test passed")

async def test_empty_collections():
    print("\nTesting EmptyCollections...")
    result = await baml.TestEmptyCollections("test empty")
    
    # Verify empty arrays
    assert len(result.stringArray) == 0, f"Expected empty array, got length {len(result.stringArray)}"
    assert len(result.intArray) == 0, f"Expected empty array, got length {len(result.intArray)}"
    assert len(result.floatArray) == 0, f"Expected empty array, got length {len(result.floatArray)}"
    assert len(result.boolArray) == 0, f"Expected empty array, got length {len(result.boolArray)}"
    print("✓ EmptyCollections test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_primitive_types(),
        test_primitive_arrays(),
        test_primitive_maps(),
        test_mixed_primitives(),
        test_empty_collections()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All primitive type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())