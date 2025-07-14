import { baml } from '../baml_client';

async function testPrimitiveTypes(): Promise<void> {
  console.log("Testing PrimitiveTypes...");
  const result = await baml.TestPrimitiveTypes("test input");
  
  // Verify primitive values
  if (result.stringField !== "Hello, BAML!") {
    throw new Error(`Expected 'Hello, BAML!', got '${result.stringField}'`);
  }
  if (result.intField !== 42) {
    throw new Error(`Expected 42, got ${result.intField}`);
  }
  if (result.floatField < 3.14 || result.floatField > 3.15) {
    throw new Error(`Expected ~3.14159, got ${result.floatField}`);
  }
  if (result.boolField !== true) {
    throw new Error(`Expected true, got ${result.boolField}`);
  }
  if (result.nullField !== null) {
    throw new Error(`Expected null, got ${result.nullField}`);
  }
  console.log("✓ PrimitiveTypes test passed");
}

async function testPrimitiveArrays(): Promise<void> {
  console.log("\nTesting PrimitiveArrays...");
  const result = await baml.TestPrimitiveArrays("test arrays");
  
  // Verify array contents
  if (result.stringArray.length !== 3) {
    throw new Error(`Expected length 3, got ${result.stringArray.length}`);
  }
  if (result.intArray.length !== 5) {
    throw new Error(`Expected length 5, got ${result.intArray.length}`);
  }
  if (result.floatArray.length !== 4) {
    throw new Error(`Expected length 4, got ${result.floatArray.length}`);
  }
  if (result.boolArray.length !== 4) {
    throw new Error(`Expected length 4, got ${result.boolArray.length}`);
  }
  
  // Verify array types
  if (!result.stringArray.every(x => typeof x === 'string')) {
    throw new Error("Not all elements are strings");
  }
  if (!result.intArray.every(x => typeof x === 'number' && Number.isInteger(x))) {
    throw new Error("Not all elements are integers");
  }
  if (!result.floatArray.every(x => typeof x === 'number')) {
    throw new Error("Not all elements are numbers");
  }
  if (!result.boolArray.every(x => typeof x === 'boolean')) {
    throw new Error("Not all elements are booleans");
  }
  console.log("✓ PrimitiveArrays test passed");
}

async function testPrimitiveMaps(): Promise<void> {
  console.log("\nTesting PrimitiveMaps...");
  const result = await baml.TestPrimitiveMaps("test maps");
  
  // Verify map contents
  if (Object.keys(result.stringMap).length !== 2) {
    throw new Error(`Expected length 2, got ${Object.keys(result.stringMap).length}`);
  }
  if (Object.keys(result.intMap).length !== 3) {
    throw new Error(`Expected length 3, got ${Object.keys(result.intMap).length}`);
  }
  if (Object.keys(result.floatMap).length !== 2) {
    throw new Error(`Expected length 2, got ${Object.keys(result.floatMap).length}`);
  }
  if (Object.keys(result.boolMap).length !== 2) {
    throw new Error(`Expected length 2, got ${Object.keys(result.boolMap).length}`);
  }
  
  // Verify map value types
  if (!Object.values(result.stringMap).every(v => typeof v === 'string')) {
    throw new Error("Not all values are strings");
  }
  if (!Object.values(result.intMap).every(v => typeof v === 'number' && Number.isInteger(v))) {
    throw new Error("Not all values are integers");
  }
  if (!Object.values(result.floatMap).every(v => typeof v === 'number')) {
    throw new Error("Not all values are numbers");
  }
  if (!Object.values(result.boolMap).every(v => typeof v === 'boolean')) {
    throw new Error("Not all values are booleans");
  }
  console.log("✓ PrimitiveMaps test passed");
}

async function testMixedPrimitives(): Promise<void> {
  console.log("\nTesting MixedPrimitives...");
  const result = await baml.TestMixedPrimitives("test mixed");
  
  // Basic validation for mixed types
  if (typeof result.name !== 'string' || result.name === '') {
    throw new Error("Name should be non-empty string");
  }
  if (typeof result.age !== 'number' || !Number.isInteger(result.age) || result.age <= 0) {
    throw new Error("Age should be positive integer");
  }
  if (typeof result.height !== 'number' || result.height <= 0) {
    throw new Error("Height should be positive number");
  }
  if (typeof result.isActive !== 'boolean') {
    throw new Error("isActive should be boolean");
  }
  if (result.metadata !== null) {
    throw new Error("metadata should be null");
  }
  if (!Array.isArray(result.tags) || !result.tags.every(x => typeof x === 'string')) {
    throw new Error("tags should be string array");
  }
  if (!Array.isArray(result.scores) || !result.scores.every(x => typeof x === 'number' && Number.isInteger(x))) {
    throw new Error("scores should be int array");
  }
  if (!Array.isArray(result.measurements) || !result.measurements.every(x => typeof x === 'number')) {
    throw new Error("measurements should be number array");
  }
  if (!Array.isArray(result.flags) || !result.flags.every(x => typeof x === 'boolean')) {
    throw new Error("flags should be bool array");
  }
  console.log("✓ MixedPrimitives test passed");
}

async function testEmptyCollections(): Promise<void> {
  console.log("\nTesting EmptyCollections...");
  const result = await baml.TestEmptyCollections("test empty");
  
  // Verify empty arrays
  if (result.stringArray.length !== 0) {
    throw new Error(`Expected empty array, got length ${result.stringArray.length}`);
  }
  if (result.intArray.length !== 0) {
    throw new Error(`Expected empty array, got length ${result.intArray.length}`);
  }
  if (result.floatArray.length !== 0) {
    throw new Error(`Expected empty array, got length ${result.floatArray.length}`);
  }
  if (result.boolArray.length !== 0) {
    throw new Error(`Expected empty array, got length ${result.boolArray.length}`);
  }
  console.log("✓ EmptyCollections test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testPrimitiveTypes(),
    testPrimitiveArrays(),
    testPrimitiveMaps(),
    testMixedPrimitives(),
    testEmptyCollections()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All primitive type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();