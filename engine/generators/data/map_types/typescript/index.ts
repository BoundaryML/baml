import { baml } from '../baml_client';

async function testSimpleMaps(): Promise<void> {
  console.log("Testing SimpleMaps...");
  const result = await baml.TestSimpleMaps("test simple maps");
  
  // Verify map structures
  if (typeof result.stringToString !== 'object' || result.stringToString === null) {
    throw new Error("stringToString should be object");
  }
  if (typeof result.stringToInt !== 'object' || result.stringToInt === null) {
    throw new Error("stringToInt should be object");
  }
  if (typeof result.stringToFloat !== 'object' || result.stringToFloat === null) {
    throw new Error("stringToFloat should be object");
  }
  if (typeof result.stringToBool !== 'object' || result.stringToBool === null) {
    throw new Error("stringToBool should be object");
  }
  if (typeof result.intToString !== 'object' || result.intToString === null) {
    throw new Error("intToString should be object");
  }
  
  // Verify map value types
  if (!Object.values(result.stringToString).every(v => typeof v === 'string')) {
    throw new Error("stringToString values should be strings");
  }
  if (!Object.values(result.stringToInt).every(v => typeof v === 'number' && Number.isInteger(v))) {
    throw new Error("stringToInt values should be integers");
  }
  if (!Object.values(result.stringToFloat).every(v => typeof v === 'number')) {
    throw new Error("stringToFloat values should be numbers");
  }
  if (!Object.values(result.stringToBool).every(v => typeof v === 'boolean')) {
    throw new Error("stringToBool values should be booleans");
  }
  if (!Object.values(result.intToString).every(v => typeof v === 'string')) {
    throw new Error("intToString values should be strings");
  }
  
  // Verify maps have content
  if (Object.keys(result.stringToString).length === 0) {
    throw new Error("stringToString should not be empty");
  }
  if (Object.keys(result.stringToInt).length === 0) {
    throw new Error("stringToInt should not be empty");
  }
  
  console.log("✓ SimpleMaps test passed");
}

async function testComplexMaps(): Promise<void> {
  console.log("\nTesting ComplexMaps...");
  const result = await baml.TestComplexMaps("test complex maps");
  
  // Verify complex map structures
  if (typeof result.userMap !== 'object' || result.userMap === null) {
    throw new Error("userMap should be object");
  }
  if (!Object.values(result.userMap).every(user => 
    typeof user === 'object' && 
    typeof user.id === 'number' && 
    typeof user.name === 'string' && 
    typeof user.email === 'string' &&
    typeof user.active === 'boolean'
  )) {
    throw new Error("userMap should contain User objects");
  }
  
  if (typeof result.productMap !== 'object' || result.productMap === null) {
    throw new Error("productMap should be object");
  }
  if (!Object.values(result.productMap).every(product => 
    typeof product === 'object' && 
    typeof product.id === 'number' && 
    typeof product.name === 'string' && 
    typeof product.price === 'number' &&
    Array.isArray(product.tags)
  )) {
    throw new Error("productMap should contain Product objects");
  }
  
  if (typeof result.nestedMap !== 'object' || result.nestedMap === null) {
    throw new Error("nestedMap should be object");
  }
  if (!Object.values(result.nestedMap).every(inner => 
    typeof inner === 'object' && inner !== null
  )) {
    throw new Error("nestedMap should contain nested objects");
  }
  
  if (typeof result.arrayMap !== 'object' || result.arrayMap === null) {
    throw new Error("arrayMap should be object");
  }
  if (!Object.values(result.arrayMap).every(arr => 
    Array.isArray(arr) && arr.every(item => typeof item === 'number' && Number.isInteger(item))
  )) {
    throw new Error("arrayMap should contain integer arrays");
  }
  
  if (!Array.isArray(result.mapArray)) {
    throw new Error("mapArray should be array");
  }
  if (!result.mapArray.every(map => 
    typeof map === 'object' && map !== null
  )) {
    throw new Error("mapArray should contain map objects");
  }
  
  console.log("✓ ComplexMaps test passed");
}

async function testNestedMaps(): Promise<void> {
  console.log("\nTesting NestedMaps...");
  const result = await baml.TestNestedMaps("test nested maps");
  
  // Verify nested map structures
  if (typeof result.simple !== 'object' || result.simple === null) {
    throw new Error("simple should be object");
  }
  
  if (typeof result.oneLevelNested !== 'object' || result.oneLevelNested === null) {
    throw new Error("oneLevelNested should be object");
  }
  if (!Object.values(result.oneLevelNested).every(inner => 
    typeof inner === 'object' && inner !== null &&
    Object.values(inner).every(value => typeof value === 'number' && Number.isInteger(value))
  )) {
    throw new Error("oneLevelNested should contain maps with integer values");
  }
  
  if (typeof result.twoLevelNested !== 'object' || result.twoLevelNested === null) {
    throw new Error("twoLevelNested should be object");
  }
  
  if (typeof result.mapOfArrays !== 'object' || result.mapOfArrays === null) {
    throw new Error("mapOfArrays should be object");
  }
  if (!Object.values(result.mapOfArrays).every(arr => 
    Array.isArray(arr) && arr.every(item => typeof item === 'string')
  )) {
    throw new Error("mapOfArrays should contain string arrays");
  }
  
  if (typeof result.mapOfMaps !== 'object' || result.mapOfMaps === null) {
    throw new Error("mapOfMaps should be object");
  }
  if (!Object.values(result.mapOfMaps).every(inner => 
    typeof inner === 'object' && inner !== null
  )) {
    throw new Error("mapOfMaps should contain nested objects");
  }
  
  console.log("✓ NestedMaps test passed");
}

async function testEdgeCaseMaps(): Promise<void> {
  console.log("\nTesting EdgeCaseMaps...");
  const result = await baml.TestEdgeCaseMaps("test edge case maps");
  
  // Verify edge case maps
  if (typeof result.emptyMap !== 'object' || result.emptyMap === null) {
    throw new Error("emptyMap should be object");
  }
  if (Object.keys(result.emptyMap).length !== 0) {
    throw new Error("emptyMap should be empty");
  }
  
  if (typeof result.nullableValues !== 'object' || result.nullableValues === null) {
    throw new Error("nullableValues should be object");
  }
  if (!Object.values(result.nullableValues).every(value => 
    typeof value === 'string' || value === null
  )) {
    throw new Error("nullableValues should contain strings or null");
  }
  
  if (typeof result.unionValues !== 'object' || result.unionValues === null) {
    throw new Error("unionValues should be object");
  }
  if (!Object.values(result.unionValues).every(value => 
    typeof value === 'string' || 
    typeof value === 'number' || 
    typeof value === 'boolean'
  )) {
    throw new Error("unionValues should contain mixed types");
  }
  
  console.log("✓ EdgeCaseMaps test passed");
}

async function testLargeMaps(): Promise<void> {
  console.log("\nTesting LargeMaps...");
  const result = await baml.TestLargeMaps("test large maps");
  
  // Verify large maps have significant content
  if (typeof result.stringToString !== 'object' || Object.keys(result.stringToString).length < 5) {
    throw new Error("stringToString should be large map");
  }
  if (typeof result.stringToInt !== 'object' || Object.keys(result.stringToInt).length < 5) {
    throw new Error("stringToInt should be large map");
  }
  if (typeof result.stringToFloat !== 'object' || Object.keys(result.stringToFloat).length < 5) {
    throw new Error("stringToFloat should be large map");
  }
  if (typeof result.stringToBool !== 'object' || Object.keys(result.stringToBool).length < 5) {
    throw new Error("stringToBool should be large map");
  }
  if (typeof result.intToString !== 'object' || Object.keys(result.intToString).length < 5) {
    throw new Error("intToString should be large map");
  }
  
  console.log("✓ LargeMaps test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testSimpleMaps(),
    testComplexMaps(),
    testNestedMaps(),
    testEdgeCaseMaps(),
    testLargeMaps()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All map type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();