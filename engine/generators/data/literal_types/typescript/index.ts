import { baml } from '../baml_client';

async function testStringLiterals(): Promise<void> {
  console.log("Testing StringLiterals...");
  const result = await baml.TestStringLiterals("test string literals");
  
  // Verify literal values
  if (result.status !== "active") {
    throw new Error(`Expected 'active', got '${result.status}'`);
  }
  if (result.environment !== "prod") {
    throw new Error(`Expected 'prod', got '${result.environment}'`);
  }
  if (result.method !== "POST") {
    throw new Error(`Expected 'POST', got '${result.method}'`);
  }
  console.log("✓ StringLiterals test passed");
}

async function testIntegerLiterals(): Promise<void> {
  console.log("\nTesting IntegerLiterals...");
  const result = await baml.TestIntegerLiterals("test integer literals");
  
  // Verify literal values
  if (result.priority !== 3) {
    throw new Error(`Expected 3, got ${result.priority}`);
  }
  if (result.httpStatus !== 201) {
    throw new Error(`Expected 201, got ${result.httpStatus}`);
  }
  if (result.maxRetries !== 3) {
    throw new Error(`Expected 3, got ${result.maxRetries}`);
  }
  console.log("✓ IntegerLiterals test passed");
}

async function testBooleanLiterals(): Promise<void> {
  console.log("\nTesting BooleanLiterals...");
  const result = await baml.TestBooleanLiterals("test boolean literals");
  
  // Verify literal values
  if (result.alwaysTrue !== true) {
    throw new Error(`Expected true, got ${result.alwaysTrue}`);
  }
  if (result.alwaysFalse !== false) {
    throw new Error(`Expected false, got ${result.alwaysFalse}`);
  }
  if (typeof result.eitherBool !== 'boolean') {
    throw new Error(`Expected boolean, got ${typeof result.eitherBool}`);
  }
  console.log("✓ BooleanLiterals test passed");
}

async function testMixedLiterals(): Promise<void> {
  console.log("\nTesting MixedLiterals...");
  const result = await baml.TestMixedLiterals("test mixed literals");
  
  // Verify mixed literal types
  if (typeof result.id !== 'number' || !Number.isInteger(result.id)) {
    throw new Error(`Expected integer, got ${typeof result.id}`);
  }
  if (!['user', 'admin', 'guest'].includes(result.type)) {
    throw new Error(`Expected user/admin/guest, got ${result.type}`);
  }
  if (![1, 2, 3].includes(result.level)) {
    throw new Error(`Expected 1/2/3, got ${result.level}`);
  }
  if (typeof result.isActive !== 'boolean') {
    throw new Error(`Expected boolean, got ${typeof result.isActive}`);
  }
  if (!['v1', 'v2', 'v3'].includes(result.apiVersion)) {
    throw new Error(`Expected v1/v2/v3, got ${result.apiVersion}`);
  }
  console.log("✓ MixedLiterals test passed");
}

async function testComplexLiterals(): Promise<void> {
  console.log("\nTesting ComplexLiterals...");
  const result = await baml.TestComplexLiterals("test complex literals");
  
  // Verify complex literal structures
  if (!['draft', 'published', 'archived', 'deleted'].includes(result.state)) {
    throw new Error(`Invalid state: ${result.state}`);
  }
  if (![0, 1, 2, 3, 5, 8, 13].includes(result.retryCount)) {
    throw new Error(`Invalid retryCount: ${result.retryCount}`);
  }
  if (!['success', 'error', 'timeout'].includes(result.response)) {
    throw new Error(`Invalid response: ${result.response}`);
  }
  if (!Array.isArray(result.flags) || !result.flags.every(f => typeof f === 'boolean')) {
    throw new Error("flags should be boolean array");
  }
  if (!Array.isArray(result.codes) || !result.codes.every(c => [200, 404, 500].includes(c))) {
    throw new Error("codes should be array of 200/404/500");
  }
  console.log("✓ ComplexLiterals test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testStringLiterals(),
    testIntegerLiterals(),
    testBooleanLiterals(),
    testMixedLiterals(),
    testComplexLiterals()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All literal type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();