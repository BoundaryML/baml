import asyncio
import sys
from baml_client import baml

async def test_simple_arrays():
    print("Testing SimpleArrays...")
    result = await baml.TestSimpleArrays("test simple arrays")
    
    # Verify array lengths
    assert len(result.strings) == 3, f"Expected length 3, got {len(result.strings)}"
    assert len(result.integers) == 5, f"Expected length 5, got {len(result.integers)}"
    assert len(result.floats) == 3, f"Expected length 3, got {len(result.floats)}"
    assert len(result.booleans) == 4, f"Expected length 4, got {len(result.booleans)}"
    
    # Verify array types
    assert all(isinstance(x, str) for x in result.strings), "Not all elements are strings"
    assert all(isinstance(x, int) for x in result.integers), "Not all elements are integers"
    assert all(isinstance(x, (int, float)) for x in result.floats), "Not all elements are numbers"
    assert all(isinstance(x, bool) for x in result.booleans), "Not all elements are booleans"
    
    # Verify specific values
    assert result.strings == ["hello", "world", "test"], f"Expected ['hello', 'world', 'test'], got {result.strings}"
    assert result.integers == [1, 2, 3, 4, 5], f"Expected [1, 2, 3, 4, 5], got {result.integers}"
    print("✓ SimpleArrays test passed")

async def test_nested_arrays():
    print("\nTesting NestedArrays...")
    result = await baml.TestNestedArrays("test nested arrays")
    
    # Verify nested array structure
    assert len(result.matrix) == 3, f"Expected matrix length 3, got {len(result.matrix)}"
    assert len(result.stringMatrix) == 2, f"Expected stringMatrix length 2, got {len(result.stringMatrix)}"
    assert len(result.threeDimensional) == 2, f"Expected threeDimensional length 2, got {len(result.threeDimensional)}"
    
    # Verify matrix content
    assert result.matrix[0] == [1, 2, 3], f"Expected [1, 2, 3], got {result.matrix[0]}"
    assert result.matrix[1] == [4, 5, 6], f"Expected [4, 5, 6], got {result.matrix[1]}"
    assert result.matrix[2] == [7, 8, 9], f"Expected [7, 8, 9], got {result.matrix[2]}"
    
    # Verify string matrix
    assert result.stringMatrix[0] == ["a", "b"], f"Expected ['a', 'b'], got {result.stringMatrix[0]}"
    assert result.stringMatrix[1] == ["c", "d"], f"Expected ['c', 'd'], got {result.stringMatrix[1]}"
    
    # Verify 3D structure
    assert len(result.threeDimensional[0]) == 2, "First level should have 2 elements"
    assert len(result.threeDimensional[0][0]) == 2, "Second level should have 2 elements"
    print("✓ NestedArrays test passed")

async def test_object_arrays():
    print("\nTesting ObjectArrays...")
    result = await baml.TestObjectArrays("test object arrays")
    
    # Verify array lengths
    assert len(result.users) == 3, f"Expected 3 users, got {len(result.users)}"
    assert len(result.products) == 2, f"Expected 2 products, got {len(result.products)}"
    assert len(result.tags) == 4, f"Expected 4 tags, got {len(result.tags)}"
    
    # Verify object types and structure
    for user in result.users:
        assert hasattr(user, 'id'), "User should have id"
        assert hasattr(user, 'name'), "User should have name"
        assert hasattr(user, 'email'), "User should have email"
        assert hasattr(user, 'isActive'), "User should have isActive"
        assert isinstance(user.id, int), "User id should be int"
        assert isinstance(user.name, str), "User name should be string"
        assert isinstance(user.email, str), "User email should be string"
        assert isinstance(user.isActive, bool), "User isActive should be boolean"
    
    for product in result.products:
        assert hasattr(product, 'id'), "Product should have id"
        assert hasattr(product, 'name'), "Product should have name"
        assert hasattr(product, 'price'), "Product should have price"
        assert hasattr(product, 'tags'), "Product should have tags"
        assert hasattr(product, 'inStock'), "Product should have inStock"
        assert isinstance(product.tags, list), "Product tags should be array"
    
    for tag in result.tags:
        assert hasattr(tag, 'id'), "Tag should have id"
        assert hasattr(tag, 'name'), "Tag should have name"
        assert hasattr(tag, 'color'), "Tag should have color"
    print("✓ ObjectArrays test passed")

async def test_mixed_arrays():
    print("\nTesting MixedArrays...")
    result = await baml.TestMixedArrays("test mixed arrays")
    
    # Verify mixed primitive array
    assert len(result.primitiveArray) == 4, f"Expected length 4, got {len(result.primitiveArray)}"
    assert result.primitiveArray[0] == "hello", f"Expected 'hello', got {result.primitiveArray[0]}"
    assert result.primitiveArray[1] == 42, f"Expected 42, got {result.primitiveArray[1]}"
    assert isinstance(result.primitiveArray[2], (int, float)), "Third element should be number"
    assert isinstance(result.primitiveArray[3], bool), "Fourth element should be boolean"
    
    # Verify nullable array
    assert len(result.nullableArray) == 4, f"Expected length 4, got {len(result.nullableArray)}"
    assert result.nullableArray[0] == "hello", f"Expected 'hello', got {result.nullableArray[0]}"
    assert result.nullableArray[1] is None, f"Expected None, got {result.nullableArray[1]}"
    assert result.nullableArray[2] == "world", f"Expected 'world', got {result.nullableArray[2]}"
    assert result.nullableArray[3] is None, f"Expected None, got {result.nullableArray[3]}"
    
    # Verify optional items
    assert len(result.optionalItems) >= 2, "Should have at least 2 items"
    
    # Verify array of arrays
    assert len(result.arrayOfArrays) >= 2, "Should have at least 2 sub-arrays"
    assert isinstance(result.arrayOfArrays[0], list), "First element should be array"
    assert isinstance(result.arrayOfArrays[1], list), "Second element should be array"
    
    # Verify complex mixed array
    assert len(result.complexMixed) >= 3, "Should have at least 3 objects"
    print("✓ MixedArrays test passed")

async def test_empty_arrays():
    print("\nTesting EmptyArrays...")
    result = await baml.TestEmptyArrays("test empty arrays")
    
    # Verify all arrays are empty
    assert len(result.strings) == 0, f"Expected empty array, got length {len(result.strings)}"
    assert len(result.integers) == 0, f"Expected empty array, got length {len(result.integers)}"
    assert len(result.floats) == 0, f"Expected empty array, got length {len(result.floats)}"
    assert len(result.booleans) == 0, f"Expected empty array, got length {len(result.booleans)}"
    print("✓ EmptyArrays test passed")

async def test_large_arrays():
    print("\nTesting LargeArrays...")
    result = await baml.TestLargeArrays("test large arrays")
    
    # Verify large array sizes
    assert len(result.strings) >= 30, f"Expected at least 30 strings, got {len(result.strings)}"
    assert len(result.integers) >= 50, f"Expected at least 50 integers, got {len(result.integers)}"
    assert len(result.floats) >= 20, f"Expected at least 20 floats, got {len(result.floats)}"
    assert len(result.booleans) >= 15, f"Expected at least 15 booleans, got {len(result.booleans)}"
    
    # Verify types
    assert all(isinstance(x, str) for x in result.strings), "Not all elements are strings"
    assert all(isinstance(x, int) for x in result.integers), "Not all elements are integers"
    assert all(isinstance(x, (int, float)) for x in result.floats), "Not all elements are numbers"
    assert all(isinstance(x, bool) for x in result.booleans), "Not all elements are booleans"
    print("✓ LargeArrays test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_simple_arrays(),
        test_nested_arrays(),
        test_object_arrays(),
        test_mixed_arrays(),
        test_empty_arrays(),
        test_large_arrays()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All array type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())