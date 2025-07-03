import asyncio
import sys
from baml_client import baml

async def test_empty_collections():
    print("Testing EmptyCollections...")
    result = await baml.TestEmptyCollections("test empty collections")
    
    # Verify all collections are empty
    assert len(result.emptyStringArray) == 0, f"Expected empty string array, got length {len(result.emptyStringArray)}"
    assert len(result.emptyIntArray) == 0, f"Expected empty int array, got length {len(result.emptyIntArray)}"
    assert len(result.emptyObjectArray) == 0, f"Expected empty object array, got length {len(result.emptyObjectArray)}"
    assert len(result.emptyMap) == 0, f"Expected empty map, got length {len(result.emptyMap)}"
    assert len(result.emptyNestedArray) == 0, f"Expected empty nested array, got length {len(result.emptyNestedArray)}"
    
    # Verify types
    assert isinstance(result.emptyStringArray, list), "Empty string array should be list"
    assert isinstance(result.emptyIntArray, list), "Empty int array should be list"
    assert isinstance(result.emptyObjectArray, list), "Empty object array should be list"
    assert isinstance(result.emptyMap, dict), "Empty map should be dict"
    assert isinstance(result.emptyNestedArray, list), "Empty nested array should be list"
    
    print("✓ EmptyCollections test passed")

async def test_large_structure():
    print("\nTesting LargeStructure...")
    result = await baml.TestLargeStructure("test large structure")
    
    # Verify all 20 basic fields are present
    for i in range(1, 6):
        field_name = f"field{i}"
        assert hasattr(result, field_name), f"Should have {field_name}"
        assert isinstance(getattr(result, field_name), str), f"{field_name} should be string"
    
    for i in range(6, 11):
        field_name = f"field{i}"
        assert hasattr(result, field_name), f"Should have {field_name}"
        assert isinstance(getattr(result, field_name), int), f"{field_name} should be int"
    
    for i in range(11, 16):
        field_name = f"field{i}"
        assert hasattr(result, field_name), f"Should have {field_name}"
        assert isinstance(getattr(result, field_name), (int, float)), f"{field_name} should be float"
    
    for i in range(16, 21):
        field_name = f"field{i}"
        assert hasattr(result, field_name), f"Should have {field_name}"
        assert isinstance(getattr(result, field_name), bool), f"{field_name} should be bool"
    
    # Verify arrays
    arrays = ['array1', 'array2', 'array3', 'array4', 'array5']
    expected_types = [str, int, (int, float), bool, object]
    
    for i, (array_name, expected_type) in enumerate(zip(arrays, expected_types)):
        assert hasattr(result, array_name), f"Should have {array_name}"
        array_value = getattr(result, array_name)
        assert isinstance(array_value, list), f"{array_name} should be list"
        assert 3 <= len(array_value) <= 5, f"{array_name} should have 3-5 items, got {len(array_value)}"
        
        if expected_type != object:
            assert all(isinstance(x, expected_type) for x in array_value), f"All {array_name} elements should be {expected_type}"
        else:
            # For array5 (User objects), check they have required fields
            for user in array_value:
                assert hasattr(user, 'id'), "User should have id"
                assert hasattr(user, 'name'), "User should have name"
    
    # Verify maps
    maps = ['map1', 'map2', 'map3', 'map4', 'map5']
    expected_value_types = [str, int, (int, float), bool, object]
    
    for i, (map_name, expected_value_type) in enumerate(zip(maps, expected_value_types)):
        assert hasattr(result, map_name), f"Should have {map_name}"
        map_value = getattr(result, map_name)
        assert isinstance(map_value, dict), f"{map_name} should be dict"
        assert 2 <= len(map_value) <= 3, f"{map_name} should have 2-3 entries, got {len(map_value)}"
        
        # All keys should be strings
        assert all(isinstance(k, str) for k in map_value.keys()), f"All {map_name} keys should be strings"
        
        if expected_value_type != object:
            assert all(isinstance(v, expected_value_type) for v in map_value.values()), f"All {map_name} values should be {expected_value_type}"
        else:
            # For map5 (User objects), check they have required fields
            for user in map_value.values():
                assert hasattr(user, 'id'), "User should have id"
                assert hasattr(user, 'name'), "User should have name"
    
    print("✓ LargeStructure test passed")

async def test_deep_recursion():
    print("\nTesting DeepRecursion...")
    depth = 5
    result = await baml.TestDeepRecursion(depth)
    
    # Navigate through the recursive structure
    current = result
    level = 1
    
    while current is not None and level <= depth:
        assert hasattr(current, 'value'), f"Level {level} should have value"
        assert hasattr(current, 'next'), f"Level {level} should have next"
        assert isinstance(current.value, str), f"Level {level} value should be string"
        
        # Check if the value contains level information
        assert str(level) in current.value or "Level" in current.value, f"Level {level} value should contain level info: {current.value}"
        
        current = current.next
        level += 1
    
    # At the end, current should be None (no more nesting)
    assert current is None, "Final level should have next=None"
    
    print("✓ DeepRecursion test passed")

async def test_special_characters():
    print("\nTesting SpecialCharacters...")
    result = await baml.TestSpecialCharacters("test special characters")
    
    # Verify special character handling
    assert result.normalText == "Hello World", f"Expected 'Hello World', got '{result.normalText}'"
    assert "\n" in result.withNewlines, "Should contain newlines"
    assert "\t" in result.withTabs, "Should contain tabs"
    assert '"' in result.withQuotes, "Should contain quotes"
    assert "\\" in result.withBackslashes, "Should contain backslashes"
    
    # Verify unicode and emoji handling
    assert len(result.withUnicode) > 0, "Unicode string should not be empty"
    assert len(result.withEmoji) > 0, "Emoji string should not be empty"
    assert len(result.withMixedSpecial) > 0, "Mixed special string should not be empty"
    
    # Verify all are strings
    assert isinstance(result.normalText, str), "Normal text should be string"
    assert isinstance(result.withNewlines, str), "Newlines text should be string"
    assert isinstance(result.withTabs, str), "Tabs text should be string"
    assert isinstance(result.withQuotes, str), "Quotes text should be string"
    assert isinstance(result.withBackslashes, str), "Backslashes text should be string"
    assert isinstance(result.withUnicode, str), "Unicode text should be string"
    assert isinstance(result.withEmoji, str), "Emoji text should be string"
    assert isinstance(result.withMixedSpecial, str), "Mixed special text should be string"
    
    print("✓ SpecialCharacters test passed")

async def test_number_edge_cases():
    print("\nTesting NumberEdgeCases...")
    result = await baml.TestNumberEdgeCases("test number edge cases")
    
    # Verify integer edge cases
    assert result.zero == 0, f"Expected 0, got {result.zero}"
    assert result.negativeInt < 0, f"Expected negative int, got {result.negativeInt}"
    assert result.largeInt > 1000, f"Expected large int, got {result.largeInt}"
    assert result.veryLargeInt > 100000, f"Expected very large int, got {result.veryLargeInt}"
    
    # Verify float edge cases
    assert isinstance(result.smallFloat, (int, float)), "Small float should be number"
    assert isinstance(result.largeFloat, (int, float)), "Large float should be number"
    assert isinstance(result.negativeFloat, (int, float)), "Negative float should be number"
    assert isinstance(result.scientificNotation, (int, float)), "Scientific notation should be number"
    assert result.negativeFloat < 0, f"Expected negative float, got {result.negativeFloat}"
    
    # Verify optional special values (can be None)
    if result.infinity is not None:
        assert isinstance(result.infinity, (int, float)), "Infinity should be number when present"
    
    if result.notANumber is not None:
        assert isinstance(result.notANumber, (int, float)), "NaN should be number when present"
    
    print("✓ NumberEdgeCases test passed")

async def test_circular_reference():
    print("\nTesting CircularReference...")
    result = await baml.TestCircularReference("test circular reference")
    
    # Verify root node
    assert result.id == 1, f"Expected root id 1, got {result.id}"
    assert isinstance(result.name, str), "Root name should be string"
    assert isinstance(result.children, list), "Children should be list"
    assert len(result.children) == 2, f"Expected 2 children, got {len(result.children)}"
    assert result.parent is None, "Root should have no parent"
    
    # Verify children
    for i, child in enumerate(result.children):
        expected_id = i + 2  # Should be 2 and 3
        assert child.id == expected_id, f"Expected child id {expected_id}, got {child.id}"
        assert isinstance(child.name, str), f"Child {expected_id} name should be string"
        assert isinstance(child.children, list), f"Child {expected_id} children should be list"
        
        # Check parent reference
        if child.parent is not None:
            assert child.parent.id == 1, f"Child {expected_id} parent should reference root"
    
    # Verify related items
    assert isinstance(result.relatedItems, list), "Related items should be list"
    for item in result.relatedItems:
        assert hasattr(item, 'id'), "Related item should have id"
        assert hasattr(item, 'name'), "Related item should have name"
    
    print("✓ CircularReference test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_empty_collections(),
        test_large_structure(),
        test_deep_recursion(),
        test_special_characters(),
        test_number_edge_cases(),
        test_circular_reference()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All edge case tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())