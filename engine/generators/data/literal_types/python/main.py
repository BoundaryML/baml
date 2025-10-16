import asyncio
import sys
from baml_client import baml

async def test_string_literals():
    print("Testing StringLiterals...")
    result = await baml.TestStringLiterals("test string literals")
    
    # Verify string literal values
    assert result.status == "active", f"Expected 'active', got '{result.status}'"
    assert result.environment == "prod", f"Expected 'prod', got '{result.environment}'"
    assert result.method == "POST", f"Expected 'POST', got '{result.method}'"
    print("✓ StringLiterals test passed")

async def test_integer_literals():
    print("\nTesting IntegerLiterals...")
    result = await baml.TestIntegerLiterals("test integer literals")
    
    # Verify integer literal values
    assert result.priority == 3, f"Expected 3, got {result.priority}"
    assert result.httpStatus == 201, f"Expected 201, got {result.httpStatus}"
    assert result.maxRetries == 3, f"Expected 3, got {result.maxRetries}"
    print("✓ IntegerLiterals test passed")

async def test_boolean_literals():
    print("\nTesting BooleanLiterals...")
    result = await baml.TestBooleanLiterals("test boolean literals")
    
    # Verify boolean literal values
    assert result.alwaysTrue is True, f"Expected True, got {result.alwaysTrue}"
    assert result.alwaysFalse is False, f"Expected False, got {result.alwaysFalse}"
    assert result.eitherBool is True, f"Expected True, got {result.eitherBool}"
    print("✓ BooleanLiterals test passed")

async def test_mixed_literals():
    print("\nTesting MixedLiterals...")
    result = await baml.TestMixedLiterals("test mixed literals")
    
    # Verify mixed literal values
    assert result.id == 12345, f"Expected 12345, got {result.id}"
    assert result.type == "admin", f"Expected 'admin', got '{result.type}'"
    assert result.level == 2, f"Expected 2, got {result.level}"
    assert result.isActive is True, f"Expected True, got {result.isActive}"
    assert result.apiVersion == "v2", f"Expected 'v2', got '{result.apiVersion}'"
    print("✓ MixedLiterals test passed")

async def test_complex_literals():
    print("\nTesting ComplexLiterals...")
    result = await baml.TestComplexLiterals("test complex literals")
    
    # Verify complex literal values
    assert result.state == "published", f"Expected 'published', got '{result.state}'"
    assert result.retryCount == 5, f"Expected 5, got {result.retryCount}"
    assert result.response == "success", f"Expected 'success', got '{result.response}'"
    
    # Verify arrays with literals
    assert len(result.flags) == 3, f"Expected length 3, got {len(result.flags)}"
    assert result.flags == [True, False, True], f"Expected [True, False, True], got {result.flags}"
    
    assert len(result.codes) == 3, f"Expected length 3, got {len(result.codes)}"
    assert result.codes == [200, 404, 200], f"Expected [200, 404, 200], got {result.codes}"
    print("✓ ComplexLiterals test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_string_literals(),
        test_integer_literals(),
        test_boolean_literals(),
        test_mixed_literals(),
        test_complex_literals()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All literal type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())