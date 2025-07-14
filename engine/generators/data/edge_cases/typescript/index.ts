import { baml } from '../baml_client';

async function testEmptyCollections(): Promise<void> {
  console.log("Testing EmptyCollections...");
  const result = await baml.TestEmptyCollections("test empty collections");
  
  // Verify all collections are empty
  if (!Array.isArray(result.emptyStringArray) || result.emptyStringArray.length !== 0) {
    throw new Error("emptyStringArray should be empty array");
  }
  if (!Array.isArray(result.emptyIntArray) || result.emptyIntArray.length !== 0) {
    throw new Error("emptyIntArray should be empty array");
  }
  if (!Array.isArray(result.emptyObjectArray) || result.emptyObjectArray.length !== 0) {
    throw new Error("emptyObjectArray should be empty array");
  }
  if (typeof result.emptyMap !== 'object' || result.emptyMap === null || Object.keys(result.emptyMap).length !== 0) {
    throw new Error("emptyMap should be empty object");
  }
  if (!Array.isArray(result.emptyNestedArray) || result.emptyNestedArray.length !== 0) {
    throw new Error("emptyNestedArray should be empty array");
  }
  
  console.log("✓ EmptyCollections test passed");
}

async function testLargeStructure(): Promise<void> {
  console.log("\nTesting LargeStructure...");
  const result = await baml.TestLargeStructure("test large structure");
  
  // Verify all string fields
  for (let i = 1; i <= 5; i++) {
    const field = `field${i}` as keyof typeof result;
    if (typeof result[field] !== 'string') {
      throw new Error(`${field} should be string`);
    }
  }
  
  // Verify all int fields
  for (let i = 6; i <= 10; i++) {
    const field = `field${i}` as keyof typeof result;
    if (typeof result[field] !== 'number' || !Number.isInteger(result[field] as number)) {
      throw new Error(`${field} should be integer`);
    }
  }
  
  // Verify all float fields
  for (let i = 11; i <= 15; i++) {
    const field = `field${i}` as keyof typeof result;
    if (typeof result[field] !== 'number') {
      throw new Error(`${field} should be number`);
    }
  }
  
  // Verify all bool fields
  for (let i = 16; i <= 20; i++) {
    const field = `field${i}` as keyof typeof result;
    if (typeof result[field] !== 'boolean') {
      throw new Error(`${field} should be boolean`);
    }
  }
  
  // Verify arrays
  if (!Array.isArray(result.array1) || !result.array1.every(x => typeof x === 'string')) {
    throw new Error("array1 should be string array");
  }
  if (!Array.isArray(result.array2) || !result.array2.every(x => typeof x === 'number' && Number.isInteger(x))) {
    throw new Error("array2 should be int array");
  }
  if (!Array.isArray(result.array3) || !result.array3.every(x => typeof x === 'number')) {
    throw new Error("array3 should be float array");
  }
  if (!Array.isArray(result.array4) || !result.array4.every(x => typeof x === 'boolean')) {
    throw new Error("array4 should be bool array");
  }
  if (!Array.isArray(result.array5) || !result.array5.every(x => typeof x === 'object' && x !== null)) {
    throw new Error("array5 should be User array");
  }
  
  // Verify maps
  if (typeof result.map1 !== 'object' || result.map1 === null) {
    throw new Error("map1 should be object");
  }
  if (typeof result.map2 !== 'object' || result.map2 === null) {
    throw new Error("map2 should be object");
  }
  if (typeof result.map3 !== 'object' || result.map3 === null) {
    throw new Error("map3 should be object");
  }
  if (typeof result.map4 !== 'object' || result.map4 === null) {
    throw new Error("map4 should be object");
  }
  if (typeof result.map5 !== 'object' || result.map5 === null) {
    throw new Error("map5 should be object");
  }
  
  console.log("✓ LargeStructure test passed");
}

async function testDeepRecursion(): Promise<void> {
  console.log("\nTesting DeepRecursion...");
  const result = await baml.TestDeepRecursion(5);
  
  // Verify recursive structure
  let current = result;
  let depth = 0;
  
  while (current !== null && current !== undefined) {
    if (typeof current !== 'object') {
      throw new Error("Each level should be object");
    }
    if (typeof current.value !== 'string') {
      throw new Error("Each level should have string value");
    }
    
    depth++;
    current = current.next as any;
    
    if (depth > 10) {
      // Prevent infinite loop
      break;
    }
  }
  
  if (depth === 0) {
    throw new Error("Should have at least one level");
  }
  
  console.log("✓ DeepRecursion test passed");
}

async function testSpecialCharacters(): Promise<void> {
  console.log("\nTesting SpecialCharacters...");
  const result = await baml.TestSpecialCharacters("test special characters");
  
  // Verify all fields are strings
  if (typeof result.normalText !== 'string') {
    throw new Error("normalText should be string");
  }
  if (typeof result.withNewlines !== 'string') {
    throw new Error("withNewlines should be string");
  }
  if (typeof result.withTabs !== 'string') {
    throw new Error("withTabs should be string");
  }
  if (typeof result.withQuotes !== 'string') {
    throw new Error("withQuotes should be string");
  }
  if (typeof result.withBackslashes !== 'string') {
    throw new Error("withBackslashes should be string");
  }
  if (typeof result.withUnicode !== 'string') {
    throw new Error("withUnicode should be string");
  }
  if (typeof result.withEmoji !== 'string') {
    throw new Error("withEmoji should be string");
  }
  if (typeof result.withMixedSpecial !== 'string') {
    throw new Error("withMixedSpecial should be string");
  }
  
  // Verify strings are non-empty
  if (result.normalText.length === 0) {
    throw new Error("normalText should not be empty");
  }
  
  console.log("✓ SpecialCharacters test passed");
}

async function testNumberEdgeCases(): Promise<void> {
  console.log("\nTesting NumberEdgeCases...");
  const result = await baml.TestNumberEdgeCases("test number edge cases");
  
  // Verify integer fields
  if (typeof result.zero !== 'number' || !Number.isInteger(result.zero)) {
    throw new Error("zero should be integer");
  }
  if (typeof result.negativeInt !== 'number' || !Number.isInteger(result.negativeInt)) {
    throw new Error("negativeInt should be integer");
  }
  if (typeof result.largeInt !== 'number' || !Number.isInteger(result.largeInt)) {
    throw new Error("largeInt should be integer");
  }
  if (typeof result.veryLargeInt !== 'number' || !Number.isInteger(result.veryLargeInt)) {
    throw new Error("veryLargeInt should be integer");
  }
  
  // Verify float fields
  if (typeof result.smallFloat !== 'number') {
    throw new Error("smallFloat should be number");
  }
  if (typeof result.largeFloat !== 'number') {
    throw new Error("largeFloat should be number");
  }
  if (typeof result.negativeFloat !== 'number') {
    throw new Error("negativeFloat should be number");
  }
  if (typeof result.scientificNotation !== 'number') {
    throw new Error("scientificNotation should be number");
  }
  
  // Verify optional float fields (can be number or undefined)
  if (result.infinity !== undefined && typeof result.infinity !== 'number') {
    throw new Error("infinity should be number or undefined");
  }
  if (result.notANumber !== undefined && typeof result.notANumber !== 'number') {
    throw new Error("notANumber should be number or undefined");
  }
  
  console.log("✓ NumberEdgeCases test passed");
}

async function testCircularReference(): Promise<void> {
  console.log("\nTesting CircularReference...");
  const result = await baml.TestCircularReference("test circular reference");
  
  // Verify basic structure
  if (typeof result !== 'object' || result === null) {
    throw new Error("result should be object");
  }
  if (typeof result.id !== 'number' || typeof result.name !== 'string') {
    throw new Error("result should have id and name");
  }
  
  // Verify children array
  if (!Array.isArray(result.children)) {
    throw new Error("children should be array");
  }
  
  // Verify children structure
  if (result.children.length > 0) {
    if (!result.children.every(child => 
      typeof child === 'object' && child !== null &&
      typeof child.id === 'number' &&
      typeof child.name === 'string' &&
      Array.isArray(child.children)
    )) {
      throw new Error("children should have proper structure");
    }
  }
  
  // Verify relatedItems array
  if (!Array.isArray(result.relatedItems)) {
    throw new Error("relatedItems should be array");
  }
  
  console.log("✓ CircularReference test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testEmptyCollections(),
    testLargeStructure(),
    testDeepRecursion(),
    testSpecialCharacters(),
    testNumberEdgeCases(),
    testCircularReference()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All edge case tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();