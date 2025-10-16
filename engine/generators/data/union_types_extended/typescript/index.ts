import { baml } from '../baml_client';

async function testPrimitiveUnions(): Promise<void> {
  console.log("Testing PrimitiveUnions...");
  const result = await baml.TestPrimitiveUnions("test primitive unions");
  
  // Verify union types
  if (typeof result.stringOrInt !== 'string' && typeof result.stringOrInt !== 'number') {
    throw new Error("stringOrInt should be string or number");
  }
  if (typeof result.stringOrFloat !== 'string' && typeof result.stringOrFloat !== 'number') {
    throw new Error("stringOrFloat should be string or number");
  }
  if (typeof result.intOrFloat !== 'number') {
    throw new Error("intOrFloat should be number");
  }
  if (typeof result.boolOrString !== 'boolean' && typeof result.boolOrString !== 'string') {
    throw new Error("boolOrString should be boolean or string");
  }
  if (typeof result.anyPrimitive !== 'string' && 
      typeof result.anyPrimitive !== 'number' && 
      typeof result.anyPrimitive !== 'boolean') {
    throw new Error("anyPrimitive should be string, number, or boolean");
  }
  
  console.log("✓ PrimitiveUnions test passed");
}

async function testComplexUnions(): Promise<void> {
  console.log("\nTesting ComplexUnions...");
  const result = await baml.TestComplexUnions("test complex unions");
  
  // Verify complex union types
  if (typeof result.userOrProduct !== 'object' || result.userOrProduct === null) {
    throw new Error("userOrProduct should be object");
  }
  
  // Check if it's a User or Product based on type field
  const userOrProduct = result.userOrProduct as any;
  if (userOrProduct.type !== 'user' && userOrProduct.type !== 'product') {
    throw new Error("userOrProduct should have type 'user' or 'product'");
  }
  
  if (typeof result.userOrProductOrAdmin !== 'object' || result.userOrProductOrAdmin === null) {
    throw new Error("userOrProductOrAdmin should be object");
  }
  
  const userOrProductOrAdmin = result.userOrProductOrAdmin as any;
  if (!['user', 'product', 'admin'].includes(userOrProductOrAdmin.type)) {
    throw new Error("userOrProductOrAdmin should have type 'user', 'product', or 'admin'");
  }
  
  if (typeof result.dataOrError !== 'object' || result.dataOrError === null) {
    throw new Error("dataOrError should be object");
  }
  
  const dataOrError = result.dataOrError as any;
  if (dataOrError.status !== 'success' && dataOrError.status !== 'error') {
    throw new Error("dataOrError should have status 'success' or 'error'");
  }
  
  // resultOrNull can be object or null
  if (result.resultOrNull !== null && (typeof result.resultOrNull !== 'object' || result.resultOrNull === null)) {
    throw new Error("resultOrNull should be object or null");
  }
  
  if (typeof result.multiTypeResult !== 'object' || result.multiTypeResult === null) {
    throw new Error("multiTypeResult should be object");
  }
  
  const multiTypeResult = result.multiTypeResult as any;
  if (!['success', 'warning', 'error'].includes(multiTypeResult.type)) {
    throw new Error("multiTypeResult should have type 'success', 'warning', or 'error'");
  }
  
  console.log("✓ ComplexUnions test passed");
}

async function testDiscriminatedUnions(): Promise<void> {
  console.log("\nTesting DiscriminatedUnions...");
  const result = await baml.TestDiscriminatedUnions("test discriminated unions");
  
  // Verify discriminated unions
  if (typeof result.shape !== 'object' || result.shape === null) {
    throw new Error("shape should be object");
  }
  
  const shape = result.shape as any;
  if (!['circle', 'rectangle', 'triangle'].includes(shape.shape)) {
    throw new Error("shape should have shape property with valid value");
  }
  
  if (typeof result.animal !== 'object' || result.animal === null) {
    throw new Error("animal should be object");
  }
  
  const animal = result.animal as any;
  if (!['dog', 'cat', 'bird'].includes(animal.species)) {
    throw new Error("animal should have species property with valid value");
  }
  
  if (typeof result.response !== 'object' || result.response === null) {
    throw new Error("response should be object");
  }
  
  const response = result.response as any;
  if (!['success', 'error', 'pending'].includes(response.status)) {
    throw new Error("response should have status property with valid value");
  }
  
  console.log("✓ DiscriminatedUnions test passed");
}

async function testUnionArrays(): Promise<void> {
  console.log("\nTesting UnionArrays...");
  const result = await baml.TestUnionArrays("test union arrays");
  
  // Verify union arrays
  if (!Array.isArray(result.mixedArray)) {
    throw new Error("mixedArray should be array");
  }
  if (!result.mixedArray.every(item => typeof item === 'string' || typeof item === 'number')) {
    throw new Error("mixedArray should contain strings or numbers");
  }
  
  if (!Array.isArray(result.nullableItems)) {
    throw new Error("nullableItems should be array");
  }
  if (!result.nullableItems.every(item => typeof item === 'string' || item === null)) {
    throw new Error("nullableItems should contain strings or null");
  }
  
  if (!Array.isArray(result.objectArray)) {
    throw new Error("objectArray should be array");
  }
  if (!result.objectArray.every(item => typeof item === 'object' && item !== null)) {
    throw new Error("objectArray should contain objects");
  }
  
  if (!Array.isArray(result.nestedUnionArray)) {
    throw new Error("nestedUnionArray should be array");
  }
  if (!result.nestedUnionArray.every(item => 
    typeof item === 'string' || (Array.isArray(item) && item.every(subItem => typeof subItem === 'number'))
  )) {
    throw new Error("nestedUnionArray should contain strings or number arrays");
  }
  
  console.log("✓ UnionArrays test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testPrimitiveUnions(),
    testComplexUnions(),
    testDiscriminatedUnions(),
    testUnionArrays()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All extended union type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();