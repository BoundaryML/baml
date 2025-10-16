import { baml } from '../baml_client';

async function testOptionalFields(): Promise<void> {
  console.log("Testing OptionalFields...");
  const result = await baml.TestOptionalFields("test optional fields");
  
  // Verify required fields are present
  if (typeof result.requiredString !== 'string') {
    throw new Error("requiredString should be string");
  }
  if (typeof result.requiredInt !== 'number' || !Number.isInteger(result.requiredInt)) {
    throw new Error("requiredInt should be integer");
  }
  if (typeof result.requiredBool !== 'boolean') {
    throw new Error("requiredBool should be boolean");
  }
  
  // Verify optional fields (can be present or undefined)
  if (result.optionalString !== undefined && typeof result.optionalString !== 'string') {
    throw new Error("optionalString should be string or undefined");
  }
  if (result.optionalInt !== undefined && (typeof result.optionalInt !== 'number' || !Number.isInteger(result.optionalInt))) {
    throw new Error("optionalInt should be integer or undefined");
  }
  if (result.optionalBool !== undefined && typeof result.optionalBool !== 'boolean') {
    throw new Error("optionalBool should be boolean or undefined");
  }
  if (result.optionalArray !== undefined && (!Array.isArray(result.optionalArray) || !result.optionalArray.every(s => typeof s === 'string'))) {
    throw new Error("optionalArray should be string array or undefined");
  }
  if (result.optionalMap !== undefined && (typeof result.optionalMap !== 'object' || result.optionalMap === null)) {
    throw new Error("optionalMap should be object or undefined");
  }
  
  console.log("✓ OptionalFields test passed");
}

async function testNullableTypes(): Promise<void> {
  console.log("\nTesting NullableTypes...");
  const result = await baml.TestNullableTypes("test nullable types");
  
  // Verify nullable fields (can be value or null)
  if (result.nullableString !== null && typeof result.nullableString !== 'string') {
    throw new Error("nullableString should be string or null");
  }
  if (result.nullableInt !== null && (typeof result.nullableInt !== 'number' || !Number.isInteger(result.nullableInt))) {
    throw new Error("nullableInt should be integer or null");
  }
  if (result.nullableFloat !== null && typeof result.nullableFloat !== 'number') {
    throw new Error("nullableFloat should be number or null");
  }
  if (result.nullableBool !== null && typeof result.nullableBool !== 'boolean') {
    throw new Error("nullableBool should be boolean or null");
  }
  if (result.nullableArray !== null && (!Array.isArray(result.nullableArray) || !result.nullableArray.every(s => typeof s === 'string'))) {
    throw new Error("nullableArray should be string array or null");
  }
  if (result.nullableObject !== null && (typeof result.nullableObject !== 'object' || result.nullableObject === null || typeof result.nullableObject.id !== 'number')) {
    throw new Error("nullableObject should be User object or null");
  }
  
  console.log("✓ NullableTypes test passed");
}

async function testMixedOptionalNullable(): Promise<void> {
  console.log("\nTesting MixedOptionalNullable...");
  const result = await baml.TestMixedOptionalNullable("test mixed optional nullable");
  
  // Verify required field
  if (typeof result.id !== 'number' || !Number.isInteger(result.id)) {
    throw new Error("id should be integer");
  }
  
  // Verify optional field
  if (result.description !== undefined && typeof result.description !== 'string') {
    throw new Error("description should be string or undefined");
  }
  
  // Verify nullable field
  if (result.metadata !== null && typeof result.metadata !== 'string') {
    throw new Error("metadata should be string or null");
  }
  
  // Verify required array
  if (!Array.isArray(result.tags)) {
    throw new Error("tags should be array");
  }
  
  // Verify optional array
  if (result.categories !== undefined && !Array.isArray(result.categories)) {
    throw new Error("categories should be array or undefined");
  }
  
  // Verify nullable array
  if (result.keywords !== null && !Array.isArray(result.keywords)) {
    throw new Error("keywords should be array or null");
  }
  
  // Verify required user
  if (typeof result.primaryUser !== 'object' || result.primaryUser === null) {
    throw new Error("primaryUser should be User object");
  }
  
  // Verify optional user
  if (result.secondaryUser !== undefined && (typeof result.secondaryUser !== 'object' || result.secondaryUser === null)) {
    throw new Error("secondaryUser should be User object or undefined");
  }
  
  // Verify nullable user
  if (result.tertiaryUser !== null && (typeof result.tertiaryUser !== 'object' || result.tertiaryUser === null)) {
    throw new Error("tertiaryUser should be User object or null");
  }
  
  console.log("✓ MixedOptionalNullable test passed");
}

async function testAllNull(): Promise<void> {
  console.log("\nTesting AllNull...");
  const result = await baml.TestAllNull("test all null");
  
  // Verify all fields are null
  if (result.nullableString !== null) {
    throw new Error("nullableString should be null");
  }
  if (result.nullableInt !== null) {
    throw new Error("nullableInt should be null");
  }
  if (result.nullableFloat !== null) {
    throw new Error("nullableFloat should be null");
  }
  if (result.nullableBool !== null) {
    throw new Error("nullableBool should be null");
  }
  if (result.nullableArray !== null) {
    throw new Error("nullableArray should be null");
  }
  if (result.nullableObject !== null) {
    throw new Error("nullableObject should be null");
  }
  
  console.log("✓ AllNull test passed");
}

async function testAllOptionalOmitted(): Promise<void> {
  console.log("\nTesting AllOptionalOmitted...");
  const result = await baml.TestAllOptionalOmitted("test all optional omitted");
  
  // Verify required fields are present
  if (typeof result.requiredString !== 'string') {
    throw new Error("requiredString should be string");
  }
  if (typeof result.requiredInt !== 'number' || !Number.isInteger(result.requiredInt)) {
    throw new Error("requiredInt should be integer");
  }
  if (typeof result.requiredBool !== 'boolean') {
    throw new Error("requiredBool should be boolean");
  }
  
  // Note: Optional fields may be omitted (undefined) or present
  // We don't enforce they must be undefined since the LLM might include them
  
  console.log("✓ AllOptionalOmitted test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testOptionalFields(),
    testNullableTypes(),
    testMixedOptionalNullable(),
    testAllNull(),
    testAllOptionalOmitted()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All optional/nullable type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();