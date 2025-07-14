import { baml } from '../baml_client';

async function testSimpleArrays(): Promise<void> {
  console.log("Testing SimpleArrays...");
  const result = await baml.TestSimpleArrays("test simple arrays");
  
  // Verify array structures
  if (!Array.isArray(result.strings)) {
    throw new Error("strings should be array");
  }
  if (!Array.isArray(result.integers)) {
    throw new Error("integers should be array");
  }
  if (!Array.isArray(result.floats)) {
    throw new Error("floats should be array");
  }
  if (!Array.isArray(result.booleans)) {
    throw new Error("booleans should be array");
  }
  
  // Verify array types
  if (!result.strings.every(s => typeof s === 'string')) {
    throw new Error("All strings array items should be strings");
  }
  if (!result.integers.every(i => typeof i === 'number' && Number.isInteger(i))) {
    throw new Error("All integers array items should be integers");
  }
  if (!result.floats.every(f => typeof f === 'number')) {
    throw new Error("All floats array items should be numbers");
  }
  if (!result.booleans.every(b => typeof b === 'boolean')) {
    throw new Error("All booleans array items should be booleans");
  }
  
  // Verify array has content
  if (result.strings.length === 0) {
    throw new Error("strings array should not be empty");
  }
  if (result.integers.length === 0) {
    throw new Error("integers array should not be empty");
  }
  if (result.floats.length === 0) {
    throw new Error("floats array should not be empty");
  }
  if (result.booleans.length === 0) {
    throw new Error("booleans array should not be empty");
  }
  
  console.log("✓ SimpleArrays test passed");
}

async function testNestedArrays(): Promise<void> {
  console.log("\nTesting NestedArrays...");
  const result = await baml.TestNestedArrays("test nested arrays");
  
  // Verify nested array structures
  if (!Array.isArray(result.matrix)) {
    throw new Error("matrix should be array");
  }
  if (!result.matrix.every(row => Array.isArray(row) && row.every(item => typeof item === 'number' && Number.isInteger(item)))) {
    throw new Error("matrix should be array of integer arrays");
  }
  
  if (!Array.isArray(result.stringMatrix)) {
    throw new Error("stringMatrix should be array");
  }
  if (!result.stringMatrix.every(row => Array.isArray(row) && row.every(item => typeof item === 'string'))) {
    throw new Error("stringMatrix should be array of string arrays");
  }
  
  if (!Array.isArray(result.threeDimensional)) {
    throw new Error("threeDimensional should be array");
  }
  if (!result.threeDimensional.every(layer => 
    Array.isArray(layer) && layer.every(row => 
      Array.isArray(row) && row.every(item => typeof item === 'number')
    )
  )) {
    throw new Error("threeDimensional should be array of array of number arrays");
  }
  
  console.log("✓ NestedArrays test passed");
}

async function testObjectArrays(): Promise<void> {
  console.log("\nTesting ObjectArrays...");
  const result = await baml.TestObjectArrays("test object arrays");
  
  // Verify object arrays
  if (!Array.isArray(result.users)) {
    throw new Error("users should be array");
  }
  if (!result.users.every(user => 
    typeof user === 'object' && 
    typeof user.id === 'number' && 
    typeof user.name === 'string' && 
    typeof user.email === 'string' &&
    typeof user.isActive === 'boolean'
  )) {
    throw new Error("users should be array of User objects");
  }
  
  if (!Array.isArray(result.products)) {
    throw new Error("products should be array");
  }
  if (!result.products.every(product => 
    typeof product === 'object' && 
    typeof product.id === 'number' && 
    typeof product.name === 'string' && 
    typeof product.price === 'number' &&
    Array.isArray(product.tags) &&
    typeof product.inStock === 'boolean'
  )) {
    throw new Error("products should be array of Product objects");
  }
  
  if (!Array.isArray(result.tags)) {
    throw new Error("tags should be array");
  }
  if (!result.tags.every(tag => 
    typeof tag === 'object' && 
    typeof tag.id === 'number' && 
    typeof tag.name === 'string' && 
    typeof tag.color === 'string'
  )) {
    throw new Error("tags should be array of Tag objects");
  }
  
  console.log("✓ ObjectArrays test passed");
}

async function testMixedArrays(): Promise<void> {
  console.log("\nTesting MixedArrays...");
  const result = await baml.TestMixedArrays("test mixed arrays");
  
  // Verify mixed array structures
  if (!Array.isArray(result.primitiveArray)) {
    throw new Error("primitiveArray should be array");
  }
  if (!result.primitiveArray.every(item => 
    typeof item === 'string' || 
    typeof item === 'number' || 
    typeof item === 'boolean' ||
    typeof item === 'object'
  )) {
    throw new Error("primitiveArray should contain mixed primitive types");
  }
  
  if (!Array.isArray(result.nullableArray)) {
    throw new Error("nullableArray should be array");
  }
  if (!result.nullableArray.every(item => typeof item === 'string' || item === null)) {
    throw new Error("nullableArray should contain strings or null");
  }
  
  if (!Array.isArray(result.arrayOfArrays)) {
    throw new Error("arrayOfArrays should be array");
  }
  if (!result.arrayOfArrays.every(arr => 
    Array.isArray(arr) && arr.every(item => typeof item === 'string')
  )) {
    throw new Error("arrayOfArrays should be array of string arrays");
  }
  
  if (!Array.isArray(result.complexMixed)) {
    throw new Error("complexMixed should be array");
  }
  
  console.log("✓ MixedArrays test passed");
}

async function testEmptyArrays(): Promise<void> {
  console.log("\nTesting EmptyArrays...");
  const result = await baml.TestEmptyArrays("test empty arrays");
  
  // Verify empty arrays
  if (!Array.isArray(result.strings) || result.strings.length !== 0) {
    throw new Error("strings should be empty array");
  }
  if (!Array.isArray(result.integers) || result.integers.length !== 0) {
    throw new Error("integers should be empty array");
  }
  if (!Array.isArray(result.floats) || result.floats.length !== 0) {
    throw new Error("floats should be empty array");
  }
  if (!Array.isArray(result.booleans) || result.booleans.length !== 0) {
    throw new Error("booleans should be empty array");
  }
  
  console.log("✓ EmptyArrays test passed");
}

async function testLargeArrays(): Promise<void> {
  console.log("\nTesting LargeArrays...");
  const result = await baml.TestLargeArrays("test large arrays");
  
  // Verify large arrays have significant content
  if (!Array.isArray(result.strings) || result.strings.length < 10) {
    throw new Error("strings should be large array");
  }
  if (!Array.isArray(result.integers) || result.integers.length < 10) {
    throw new Error("integers should be large array");
  }
  if (!Array.isArray(result.floats) || result.floats.length < 10) {
    throw new Error("floats should be large array");
  }
  if (!Array.isArray(result.booleans) || result.booleans.length < 10) {
    throw new Error("booleans should be large array");
  }
  
  console.log("✓ LargeArrays test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testSimpleArrays(),
    testNestedArrays(),
    testObjectArrays(),
    testMixedArrays(),
    testEmptyArrays(),
    testLargeArrays()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All array type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();