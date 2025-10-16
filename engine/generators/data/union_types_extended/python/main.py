import asyncio
import sys
from baml_client import baml

async def test_primitive_unions():
    print("Testing PrimitiveUnions...")
    result = await baml.TestPrimitiveUnions("test primitive unions")
    
    # Verify union types - check that values are of expected types
    assert isinstance(result.stringOrInt, (str, int)), "stringOrInt should be string or int"
    assert isinstance(result.stringOrFloat, (str, int, float)), "stringOrFloat should be string or float"
    assert isinstance(result.intOrFloat, (int, float)), "intOrFloat should be int or float"
    assert isinstance(result.boolOrString, (bool, str)), "boolOrString should be bool or string"
    assert isinstance(result.anyPrimitive, (str, int, float, bool)), "anyPrimitive should be any primitive"
    
    # Verify specific expected values from prompt
    assert result.stringOrInt == 42, f"Expected 42, got {result.stringOrInt}"
    assert result.stringOrFloat == "hello", f"Expected 'hello', got {result.stringOrFloat}"
    assert isinstance(result.intOrFloat, (int, float)) and result.intOrFloat >= 3.1, f"Expected ~3.14, got {result.intOrFloat}"
    assert result.boolOrString is True, f"Expected True, got {result.boolOrString}"
    assert result.anyPrimitive == "mixed", f"Expected 'mixed', got {result.anyPrimitive}"
    
    print("✓ PrimitiveUnions test passed")

async def test_complex_unions():
    print("\nTesting ComplexUnions...")
    result = await baml.TestComplexUnions("test complex unions")
    
    # Verify userOrProduct union
    assert hasattr(result.userOrProduct, 'id'), "userOrProduct should have id"
    assert hasattr(result.userOrProduct, 'name'), "userOrProduct should have name"
    
    # Check if it's a User or Product based on type field
    if hasattr(result.userOrProduct, 'type'):
        if result.userOrProduct.type == "user":
            assert hasattr(result.userOrProduct, 'type'), "User should have type field"
        elif result.userOrProduct.type == "product":
            assert hasattr(result.userOrProduct, 'price'), "Product should have price field"
    
    # Verify userOrProductOrAdmin union
    assert hasattr(result.userOrProductOrAdmin, 'id'), "userOrProductOrAdmin should have id"
    assert hasattr(result.userOrProductOrAdmin, 'name'), "userOrProductOrAdmin should have name"
    
    # Verify dataOrError union
    assert hasattr(result.dataOrError, 'status'), "dataOrError should have status"
    if result.dataOrError.status == "success":
        assert hasattr(result.dataOrError, 'data'), "DataResponse should have data"
        assert hasattr(result.dataOrError, 'timestamp'), "DataResponse should have timestamp"
    elif result.dataOrError.status == "error":
        assert hasattr(result.dataOrError, 'error'), "ErrorResponse should have error"
        assert hasattr(result.dataOrError, 'code'), "ErrorResponse should have code"
    
    # Verify resultOrNull union
    if result.resultOrNull is not None:
        assert hasattr(result.resultOrNull, 'value'), "Result should have value when not null"
        assert hasattr(result.resultOrNull, 'metadata'), "Result should have metadata when not null"
    
    # Verify multiTypeResult union
    assert hasattr(result.multiTypeResult, 'type'), "multiTypeResult should have type"
    assert hasattr(result.multiTypeResult, 'message'), "multiTypeResult should have message"
    
    print("✓ ComplexUnions test passed")

async def test_discriminated_unions():
    print("\nTesting DiscriminatedUnions...")
    result = await baml.TestDiscriminatedUnions("test discriminated unions")
    
    # Verify shape union - should be Circle
    assert hasattr(result.shape, 'shape'), "Shape should have shape discriminator"
    assert result.shape.shape == "circle", f"Expected 'circle', got {result.shape.shape}"
    assert hasattr(result.shape, 'radius'), "Circle should have radius"
    assert result.shape.radius == 5.0, f"Expected 5.0, got {result.shape.radius}"
    
    # Verify animal union - should be Dog
    assert hasattr(result.animal, 'species'), "Animal should have species discriminator"
    assert result.animal.species == "dog", f"Expected 'dog', got {result.animal.species}"
    assert hasattr(result.animal, 'breed'), "Dog should have breed"
    assert hasattr(result.animal, 'goodBoy'), "Dog should have goodBoy"
    assert result.animal.goodBoy is True, f"Expected True, got {result.animal.goodBoy}"
    
    # Verify response union - should be ApiError
    assert hasattr(result.response, 'status'), "Response should have status discriminator"
    assert result.response.status == "error", f"Expected 'error', got {result.response.status}"
    assert hasattr(result.response, 'message'), "ApiError should have message"
    assert hasattr(result.response, 'code'), "ApiError should have code"
    assert result.response.code == 404, f"Expected 404, got {result.response.code}"
    
    print("✓ DiscriminatedUnions test passed")

async def test_union_arrays():
    print("\nTesting UnionArrays...")
    result = await baml.TestUnionArrays("test union arrays")
    
    # Verify mixedArray
    assert len(result.mixedArray) == 4, f"Expected length 4, got {len(result.mixedArray)}"
    assert result.mixedArray[0] == "hello", f"Expected 'hello', got {result.mixedArray[0]}"
    assert result.mixedArray[1] == 1, f"Expected 1, got {result.mixedArray[1]}"
    assert result.mixedArray[2] == "world", f"Expected 'world', got {result.mixedArray[2]}"
    assert result.mixedArray[3] == 2, f"Expected 2, got {result.mixedArray[3]}"
    
    # Verify nullableItems
    assert len(result.nullableItems) == 4, f"Expected length 4, got {len(result.nullableItems)}"
    assert result.nullableItems[0] == "present", f"Expected 'present', got {result.nullableItems[0]}"
    assert result.nullableItems[1] is None, f"Expected None, got {result.nullableItems[1]}"
    assert result.nullableItems[2] == "also present", f"Expected 'also present', got {result.nullableItems[2]}"
    assert result.nullableItems[3] is None, f"Expected None, got {result.nullableItems[3]}"
    
    # Verify objectArray contains mix of User and Product
    assert len(result.objectArray) >= 2, "Should have at least 2 objects"
    for obj in result.objectArray:
        assert hasattr(obj, 'id'), "Object should have id"
        assert hasattr(obj, 'name'), "Object should have name"
        # Check if it's User or Product based on type field
        if hasattr(obj, 'type'):
            if obj.type == "user":
                assert hasattr(obj, 'type'), "User should have type field"
            elif obj.type == "product":
                assert hasattr(obj, 'price'), "Product should have price field"
    
    # Verify nestedUnionArray
    assert len(result.nestedUnionArray) == 4, f"Expected length 4, got {len(result.nestedUnionArray)}"
    assert result.nestedUnionArray[0] == "string", f"Expected 'string', got {result.nestedUnionArray[0]}"
    assert isinstance(result.nestedUnionArray[1], list), "Second element should be array"
    assert result.nestedUnionArray[1] == [1, 2, 3], f"Expected [1, 2, 3], got {result.nestedUnionArray[1]}"
    assert result.nestedUnionArray[2] == "another", f"Expected 'another', got {result.nestedUnionArray[2]}"
    assert isinstance(result.nestedUnionArray[3], list), "Fourth element should be array"
    assert result.nestedUnionArray[3] == [4, 5], f"Expected [4, 5], got {result.nestedUnionArray[3]}"
    
    print("✓ UnionArrays test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_primitive_unions(),
        test_complex_unions(),
        test_discriminated_unions(),
        test_union_arrays()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All union type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())