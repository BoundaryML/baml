import asyncio
import sys
from baml_client import baml

async def test_simple_maps():
    print("Testing SimpleMaps...")
    result = await baml.TestSimpleMaps("test simple maps")
    
    # Verify map lengths
    assert len(result.stringToString) == 2, f"Expected length 2, got {len(result.stringToString)}"
    assert len(result.stringToInt) == 3, f"Expected length 3, got {len(result.stringToInt)}"
    assert len(result.stringToFloat) == 2, f"Expected length 2, got {len(result.stringToFloat)}"
    assert len(result.stringToBool) == 2, f"Expected length 2, got {len(result.stringToBool)}"
    assert len(result.intToString) == 3, f"Expected length 3, got {len(result.intToString)}"
    
    # Verify map types and values
    assert all(isinstance(k, str) and isinstance(v, str) for k, v in result.stringToString.items()), "stringToString should be str->str"
    assert all(isinstance(k, str) and isinstance(v, int) for k, v in result.stringToInt.items()), "stringToInt should be str->int"
    assert all(isinstance(k, str) and isinstance(v, (int, float)) for k, v in result.stringToFloat.items()), "stringToFloat should be str->float"
    assert all(isinstance(k, str) and isinstance(v, bool) for k, v in result.stringToBool.items()), "stringToBool should be str->bool"
    assert all(isinstance(k, int) and isinstance(v, str) for k, v in result.intToString.items()), "intToString should be int->str"
    
    # Verify specific values
    assert result.stringToString["key1"] == "value1", f"Expected 'value1', got {result.stringToString.get('key1')}"
    assert result.stringToString["key2"] == "value2", f"Expected 'value2', got {result.stringToString.get('key2')}"
    assert result.stringToInt["one"] == 1, f"Expected 1, got {result.stringToInt.get('one')}"
    assert result.stringToInt["two"] == 2, f"Expected 2, got {result.stringToInt.get('two')}"
    assert result.stringToInt["three"] == 3, f"Expected 3, got {result.stringToInt.get('three')}"
    print("✓ SimpleMaps test passed")

async def test_complex_maps():
    print("\nTesting ComplexMaps...")
    result = await baml.TestComplexMaps("test complex maps")
    
    # Verify map lengths
    assert len(result.userMap) == 2, f"Expected 2 users, got {len(result.userMap)}"
    assert len(result.productMap) == 3, f"Expected 3 products, got {len(result.productMap)}"
    assert len(result.nestedMap) >= 1, f"Expected at least 1 entry, got {len(result.nestedMap)}"
    assert len(result.arrayMap) >= 2, f"Expected at least 2 entries, got {len(result.arrayMap)}"
    assert len(result.mapArray) == 2, f"Expected 2 maps, got {len(result.mapArray)}"
    
    # Verify userMap structure
    for username, user in result.userMap.items():
        assert isinstance(username, str), "Username key should be string"
        assert hasattr(user, 'id'), "User should have id"
        assert hasattr(user, 'name'), "User should have name"
        assert hasattr(user, 'email'), "User should have email"
        assert hasattr(user, 'active'), "User should have active"
    
    # Verify productMap structure
    for product_id, product in result.productMap.items():
        assert isinstance(product_id, int), "Product ID key should be int"
        assert hasattr(product, 'id'), "Product should have id"
        assert hasattr(product, 'name'), "Product should have name"
        assert hasattr(product, 'price'), "Product should have price"
        assert hasattr(product, 'tags'), "Product should have tags"
        assert isinstance(product.tags, list), "Product tags should be array"
    
    # Verify nestedMap structure
    for outer_key, inner_map in result.nestedMap.items():
        assert isinstance(outer_key, str), "Outer key should be string"
        assert isinstance(inner_map, dict), "Inner value should be map"
        for inner_key, inner_value in inner_map.items():
            assert isinstance(inner_key, str), "Inner key should be string"
            assert isinstance(inner_value, str), "Inner value should be string"
    
    # Verify arrayMap structure
    for map_key, array_value in result.arrayMap.items():
        assert isinstance(map_key, str), "Map key should be string"
        assert isinstance(array_value, list), "Map value should be array"
        assert all(isinstance(x, int) for x in array_value), "Array elements should be int"
    
    # Verify mapArray structure
    for map_item in result.mapArray:
        assert isinstance(map_item, dict), "Array item should be map"
        assert all(isinstance(k, str) and isinstance(v, str) for k, v in map_item.items()), "Map should be str->str"
    print("✓ ComplexMaps test passed")

async def test_nested_maps():
    print("\nTesting NestedMaps...")
    result = await baml.TestNestedMaps("test nested maps")
    
    # Verify simple map
    assert len(result.simple) >= 2, "Simple map should have at least 2 entries"
    assert all(isinstance(k, str) and isinstance(v, str) for k, v in result.simple.items()), "Simple should be str->str"
    
    # Verify one level nested
    assert len(result.oneLevelNested) >= 2, "One level nested should have at least 2 entries"
    for outer_key, inner_map in result.oneLevelNested.items():
        assert isinstance(outer_key, str), "Outer key should be string"
        assert isinstance(inner_map, dict), "Inner value should be map"
        assert all(isinstance(k, str) and isinstance(v, int) for k, v in inner_map.items()), "Inner map should be str->int"
    
    # Verify two level nested
    assert len(result.twoLevelNested) >= 2, "Two level nested should have at least 2 entries"
    for level1_key, level1_map in result.twoLevelNested.items():
        assert isinstance(level1_key, str), "Level 1 key should be string"
        assert isinstance(level1_map, dict), "Level 1 value should be map"
        for level2_key, level2_map in level1_map.items():
            assert isinstance(level2_key, str), "Level 2 key should be string"
            assert isinstance(level2_map, dict), "Level 2 value should be map"
            assert all(isinstance(k, str) and isinstance(v, bool) for k, v in level2_map.items()), "Level 2 map should be str->bool"
    
    # Verify map of arrays
    assert len(result.mapOfArrays) >= 2, "Map of arrays should have at least 2 entries"
    for map_key, array_value in result.mapOfArrays.items():
        assert isinstance(map_key, str), "Map key should be string"
        assert isinstance(array_value, list), "Map value should be array"
        assert all(isinstance(x, str) for x in array_value), "Array elements should be string"
    
    # Verify map of maps
    assert len(result.mapOfMaps) >= 2, "Map of maps should have at least 2 entries"
    for outer_key, inner_map in result.mapOfMaps.items():
        assert isinstance(outer_key, str), "Outer key should be string"
        assert isinstance(inner_map, dict), "Inner value should be map"
        assert all(isinstance(k, int) and isinstance(v, (int, float)) for k, v in inner_map.items()), "Inner map should be int->float"
    print("✓ NestedMaps test passed")

async def test_edge_case_maps():
    print("\nTesting EdgeCaseMaps...")
    result = await baml.TestEdgeCaseMaps("test edge case maps")
    
    # Verify empty map
    assert len(result.emptyMap) == 0, f"Expected empty map, got length {len(result.emptyMap)}"
    
    # Verify nullable values
    assert len(result.nullableValues) == 2, f"Expected 2 entries, got {len(result.nullableValues)}"
    assert result.nullableValues["present"] == "value", f"Expected 'value', got {result.nullableValues.get('present')}"
    assert result.nullableValues["absent"] is None, f"Expected None, got {result.nullableValues.get('absent')}"
    
    # Verify optional values
    assert len(result.optionalValues) >= 1, "Should have at least 1 entry"
    
    # Verify union values
    assert len(result.unionValues) == 3, f"Expected 3 entries, got {len(result.unionValues)}"
    assert result.unionValues["string"] == "hello", f"Expected 'hello', got {result.unionValues.get('string')}"
    assert result.unionValues["number"] == 42, f"Expected 42, got {result.unionValues.get('number')}"
    assert result.unionValues["boolean"] is True, f"Expected True, got {result.unionValues.get('boolean')}"
    print("✓ EdgeCaseMaps test passed")

async def test_large_maps():
    print("\nTesting LargeMaps...")
    result = await baml.TestLargeMaps("test large maps")
    
    # Verify large map sizes
    assert len(result.stringToString) >= 20, f"Expected at least 20 entries, got {len(result.stringToString)}"
    assert len(result.stringToInt) >= 20, f"Expected at least 20 entries, got {len(result.stringToInt)}"
    assert len(result.stringToFloat) >= 20, f"Expected at least 20 entries, got {len(result.stringToFloat)}"
    assert len(result.stringToBool) >= 20, f"Expected at least 20 entries, got {len(result.stringToBool)}"
    assert len(result.intToString) >= 20, f"Expected at least 20 entries, got {len(result.intToString)}"
    
    # Verify types
    assert all(isinstance(k, str) and isinstance(v, str) for k, v in result.stringToString.items()), "stringToString should be str->str"
    assert all(isinstance(k, str) and isinstance(v, int) for k, v in result.stringToInt.items()), "stringToInt should be str->int"
    assert all(isinstance(k, str) and isinstance(v, (int, float)) for k, v in result.stringToFloat.items()), "stringToFloat should be str->float"
    assert all(isinstance(k, str) and isinstance(v, bool) for k, v in result.stringToBool.items()), "stringToBool should be str->bool"
    assert all(isinstance(k, int) and isinstance(v, str) for k, v in result.intToString.items()), "intToString should be int->str"
    print("✓ LargeMaps test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_simple_maps(),
        test_complex_maps(),
        test_nested_maps(),
        test_edge_case_maps(),
        test_large_maps()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All map type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())