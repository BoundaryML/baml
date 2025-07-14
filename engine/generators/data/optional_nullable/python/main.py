import asyncio
import sys
from baml_client import baml

async def test_optional_fields():
    print("Testing OptionalFields...")
    result = await baml.TestOptionalFields("test optional fields")
    
    # Verify required fields
    assert result.requiredString == "hello", f"Expected 'hello', got '{result.requiredString}'"
    assert result.requiredInt == 42, f"Expected 42, got {result.requiredInt}"
    assert result.requiredBool is True, f"Expected True, got {result.requiredBool}"
    
    # Verify optional fields - some may be present, some may be None
    if hasattr(result, 'optionalString') and result.optionalString is not None:
        assert isinstance(result.optionalString, str), "Optional string should be string when present"
    
    if hasattr(result, 'optionalInt') and result.optionalInt is not None:
        assert isinstance(result.optionalInt, int), "Optional int should be int when present"
    
    if hasattr(result, 'optionalBool') and result.optionalBool is not None:
        assert isinstance(result.optionalBool, bool), "Optional bool should be bool when present"
    
    if hasattr(result, 'optionalArray') and result.optionalArray is not None:
        assert isinstance(result.optionalArray, list), "Optional array should be list when present"
        assert all(isinstance(x, str) for x in result.optionalArray), "Array elements should be strings"
    
    if hasattr(result, 'optionalMap') and result.optionalMap is not None:
        assert isinstance(result.optionalMap, dict), "Optional map should be dict when present"
        assert all(isinstance(k, str) and isinstance(v, str) for k, v in result.optionalMap.items()), "Map should be str->str"
    
    print("✓ OptionalFields test passed")

async def test_nullable_types():
    print("\nTesting NullableTypes...")
    result = await baml.TestNullableTypes("test nullable types")
    
    # Verify nullable fields - can be value or None
    if result.nullableString is not None:
        assert isinstance(result.nullableString, str), "Nullable string should be string when not null"
    
    if result.nullableInt is not None:
        assert isinstance(result.nullableInt, int), "Nullable int should be int when not null"
    
    if result.nullableFloat is not None:
        assert isinstance(result.nullableFloat, (int, float)), "Nullable float should be number when not null"
    
    if result.nullableBool is not None:
        assert isinstance(result.nullableBool, bool), "Nullable bool should be bool when not null"
    
    if result.nullableArray is not None:
        assert isinstance(result.nullableArray, list), "Nullable array should be list when not null"
        assert all(isinstance(x, str) for x in result.nullableArray), "Array elements should be strings"
    
    if result.nullableObject is not None:
        assert hasattr(result.nullableObject, 'id'), "Nullable object should have id when not null"
        assert hasattr(result.nullableObject, 'name'), "Nullable object should have name when not null"
    
    print("✓ NullableTypes test passed")

async def test_mixed_optional_nullable():
    print("\nTesting MixedOptionalNullable...")
    result = await baml.TestMixedOptionalNullable("test mixed optional nullable")
    
    # Verify required field
    assert isinstance(result.id, int), "ID should be int"
    
    # Verify optional field (may not be present)
    if hasattr(result, 'description') and result.description is not None:
        assert isinstance(result.description, str), "Description should be string when present"
    
    # Verify nullable field (present but can be null)
    if result.metadata is not None:
        assert isinstance(result.metadata, str), "Metadata should be string when not null"
    
    # Verify optional and nullable field
    if hasattr(result, 'notes') and result.notes is not None:
        assert isinstance(result.notes, str), "Notes should be string when present and not null"
    
    # Verify required array (can be empty)
    assert isinstance(result.tags, list), "Tags should be array"
    assert all(isinstance(x, str) for x in result.tags), "Tag elements should be strings"
    
    # Verify optional array
    if hasattr(result, 'categories') and result.categories is not None:
        assert isinstance(result.categories, list), "Categories should be array when present"
        assert all(isinstance(x, str) for x in result.categories), "Category elements should be strings"
    
    # Verify nullable array
    if result.keywords is not None:
        assert isinstance(result.keywords, list), "Keywords should be array when not null"
        assert all(isinstance(x, str) for x in result.keywords), "Keyword elements should be strings"
    
    # Verify required user
    assert hasattr(result.primaryUser, 'id'), "Primary user should have id"
    assert hasattr(result.primaryUser, 'name'), "Primary user should have name"
    
    # Verify optional user
    if hasattr(result, 'secondaryUser') and result.secondaryUser is not None:
        assert hasattr(result.secondaryUser, 'id'), "Secondary user should have id when present"
        assert hasattr(result.secondaryUser, 'name'), "Secondary user should have name when present"
    
    # Verify nullable user
    if result.tertiaryUser is not None:
        assert hasattr(result.tertiaryUser, 'id'), "Tertiary user should have id when not null"
        assert hasattr(result.tertiaryUser, 'name'), "Tertiary user should have name when not null"
    
    print("✓ MixedOptionalNullable test passed")

async def test_all_null():
    print("\nTesting AllNull...")
    result = await baml.TestAllNull("test all null")
    
    # All nullable fields should be null
    assert result.nullableString is None, f"Expected None, got {result.nullableString}"
    assert result.nullableInt is None, f"Expected None, got {result.nullableInt}"
    assert result.nullableFloat is None, f"Expected None, got {result.nullableFloat}"
    assert result.nullableBool is None, f"Expected None, got {result.nullableBool}"
    assert result.nullableArray is None, f"Expected None, got {result.nullableArray}"
    assert result.nullableObject is None, f"Expected None, got {result.nullableObject}"
    
    print("✓ AllNull test passed")

async def test_all_optional_omitted():
    print("\nTesting AllOptionalOmitted...")
    result = await baml.TestAllOptionalOmitted("test all optional omitted")
    
    # Verify required fields are present
    assert isinstance(result.requiredString, str), "Required string should be present"
    assert isinstance(result.requiredInt, int), "Required int should be present"
    assert isinstance(result.requiredBool, bool), "Required bool should be present"
    
    # Optional fields should be omitted (None or not present)
    if hasattr(result, 'optionalString'):
        assert result.optionalString is None, "Optional string should be omitted"
    
    if hasattr(result, 'optionalInt'):
        assert result.optionalInt is None, "Optional int should be omitted"
    
    if hasattr(result, 'optionalBool'):
        assert result.optionalBool is None, "Optional bool should be omitted"
    
    if hasattr(result, 'optionalArray'):
        assert result.optionalArray is None, "Optional array should be omitted"
    
    if hasattr(result, 'optionalMap'):
        assert result.optionalMap is None, "Optional map should be omitted"
    
    print("✓ AllOptionalOmitted test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_optional_fields(),
        test_nullable_types(),
        test_mixed_optional_nullable(),
        test_all_null(),
        test_all_optional_omitted()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All optional/nullable type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())