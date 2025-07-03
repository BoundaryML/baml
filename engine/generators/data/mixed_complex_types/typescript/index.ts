import { baml } from '../baml_client';

async function testKitchenSink(): Promise<void> {
  console.log("Testing KitchenSink...");
  const result = await baml.TestKitchenSink("test kitchen sink");
  
  // Verify primitives
  if (typeof result.id !== 'number' || !Number.isInteger(result.id)) {
    throw new Error("id should be integer");
  }
  if (typeof result.name !== 'string') {
    throw new Error("name should be string");
  }
  if (typeof result.score !== 'number') {
    throw new Error("score should be number");
  }
  if (typeof result.active !== 'boolean') {
    throw new Error("active should be boolean");
  }
  if (result.nothing !== null) {
    throw new Error("nothing should be null");
  }
  
  // Verify literals
  if (!['draft', 'published', 'archived'].includes(result.status)) {
    throw new Error("status should be valid literal");
  }
  if (![1, 2, 3, 4, 5].includes(result.priority)) {
    throw new Error("priority should be valid literal");
  }
  
  // Verify arrays
  if (!Array.isArray(result.tags)) {
    throw new Error("tags should be array");
  }
  if (!Array.isArray(result.numbers)) {
    throw new Error("numbers should be array");
  }
  if (!Array.isArray(result.matrix)) {
    throw new Error("matrix should be array");
  }
  if (!result.matrix.every(row => Array.isArray(row))) {
    throw new Error("matrix should be array of arrays");
  }
  
  // Verify maps
  if (typeof result.metadata !== 'object' || result.metadata === null) {
    throw new Error("metadata should be object");
  }
  if (typeof result.scores !== 'object' || result.scores === null) {
    throw new Error("scores should be object");
  }
  
  // Verify optional/nullable
  if (result.description !== undefined && typeof result.description !== 'string') {
    throw new Error("description should be string or undefined");
  }
  if (result.notes !== null && typeof result.notes !== 'string') {
    throw new Error("notes should be string or null");
  }
  
  // Verify unions
  const dataType = typeof result.data;
  if (dataType !== 'string' && dataType !== 'number' && (dataType !== 'object' || result.data === null)) {
    throw new Error("data should be string, number, or object");
  }
  
  if (typeof result.result !== 'object' || result.result === null) {
    throw new Error("result should be object");
  }
  const resultObj = result.result as any;
  if (!['success', 'error'].includes(resultObj.type)) {
    throw new Error("result should have type 'success' or 'error'");
  }
  
  // Verify complex nested
  if (typeof result.user !== 'object' || result.user === null) {
    throw new Error("user should be object");
  }
  if (typeof result.user.id !== 'number') {
    throw new Error("user.id should be number");
  }
  if (typeof result.user.profile !== 'object' || result.user.profile === null) {
    throw new Error("user.profile should be object");
  }
  if (typeof result.user.settings !== 'object' || result.user.settings === null) {
    throw new Error("user.settings should be object");
  }
  
  if (!Array.isArray(result.items)) {
    throw new Error("items should be array");
  }
  if (!result.items.every(item => 
    typeof item === 'object' && item !== null &&
    typeof item.id === 'number' &&
    typeof item.name === 'string' &&
    Array.isArray(item.variants)
  )) {
    throw new Error("items should have proper structure");
  }
  
  if (typeof result.config !== 'object' || result.config === null) {
    throw new Error("config should be object");
  }
  if (typeof result.config.version !== 'string') {
    throw new Error("config.version should be string");
  }
  if (!Array.isArray(result.config.features)) {
    throw new Error("config.features should be array");
  }
  if (typeof result.config.environments !== 'object' || result.config.environments === null) {
    throw new Error("config.environments should be object");
  }
  if (!Array.isArray(result.config.rules)) {
    throw new Error("config.rules should be array");
  }
  
  console.log("✓ KitchenSink test passed");
}

async function testUltraComplex(): Promise<void> {
  console.log("\nTesting UltraComplex...");
  const result = await baml.TestUltraComplex("test ultra complex");
  
  // Verify tree structure
  if (typeof result.tree !== 'object' || result.tree === null) {
    throw new Error("tree should be object");
  }
  if (typeof result.tree.id !== 'number') {
    throw new Error("tree.id should be number");
  }
  if (!['leaf', 'branch'].includes(result.tree.type)) {
    throw new Error("tree.type should be 'leaf' or 'branch'");
  }
  
  // Verify widgets array
  if (!Array.isArray(result.widgets)) {
    throw new Error("widgets should be array");
  }
  if (!result.widgets.every(widget => 
    typeof widget === 'object' && widget !== null &&
    ['button', 'text', 'image', 'container'].includes(widget.type)
  )) {
    throw new Error("widgets should have valid types");
  }
  
  // Verify complex data (optional)
  if (result.data !== undefined) {
    if (typeof result.data !== 'object' || result.data === null) {
      throw new Error("data should be object if present");
    }
    if (typeof result.data.primary !== 'object' || result.data.primary === null) {
      throw new Error("data.primary should be object");
    }
  }
  
  // Verify response structure
  if (typeof result.response !== 'object' || result.response === null) {
    throw new Error("response should be object");
  }
  if (!['success', 'error'].includes(result.response.status)) {
    throw new Error("response.status should be 'success' or 'error'");
  }
  if (typeof result.response.metadata !== 'object' || result.response.metadata === null) {
    throw new Error("response.metadata should be object");
  }
  
  // Verify assets array
  if (!Array.isArray(result.assets)) {
    throw new Error("assets should be array");
  }
  if (!result.assets.every(asset => 
    typeof asset === 'object' && asset !== null &&
    typeof asset.id === 'number' &&
    ['image', 'audio', 'document'].includes(asset.type) &&
    typeof asset.metadata === 'object' &&
    Array.isArray(asset.tags)
  )) {
    throw new Error("assets should have proper structure");
  }
  
  console.log("✓ UltraComplex test passed");
}

async function testRecursiveComplexity(): Promise<void> {
  console.log("\nTesting RecursiveComplexity...");
  const result = await baml.TestRecursiveComplexity("test recursive complexity");
  
  // Verify node structure
  if (typeof result !== 'object' || result === null) {
    throw new Error("result should be object");
  }
  if (typeof result.id !== 'number') {
    throw new Error("result.id should be number");
  }
  if (!['leaf', 'branch'].includes(result.type)) {
    throw new Error("result.type should be 'leaf' or 'branch'");
  }
  
  // Verify value field (union type)
  const valueType = typeof result.value;
  const isValidValue = 
    valueType === 'string' ||
    valueType === 'number' ||
    Array.isArray(result.value) ||
    (valueType === 'object' && result.value !== null);
  
  if (!isValidValue) {
    throw new Error("value should be string, number, array, or object");
  }
  
  // If value is array of nodes, verify structure
  if (Array.isArray(result.value)) {
    if (!result.value.every(item => 
      typeof item === 'object' && item !== null &&
      typeof item.id === 'number' &&
      ['leaf', 'branch'].includes(item.type)
    )) {
      throw new Error("value array should contain valid Node objects");
    }
  }
  
  // If value is map of nodes, verify structure
  if (typeof result.value === 'object' && result.value !== null && !Array.isArray(result.value)) {
    const values = Object.values(result.value);
    if (values.length > 0 && !values.every(item => 
      typeof item === 'object' && item !== null &&
      typeof (item as any).id === 'number'
    )) {
      // This might be a string/int map instead of node map, which is also valid
    }
  }
  
  // Verify optional metadata
  if (result.metadata !== undefined) {
    if (typeof result.metadata !== 'object' || result.metadata === null) {
      throw new Error("metadata should be object if present");
    }
    if (typeof result.metadata.created !== 'string') {
      throw new Error("metadata.created should be string");
    }
    if (!Array.isArray(result.metadata.tags)) {
      throw new Error("metadata.tags should be array");
    }
    if (typeof result.metadata.attributes !== 'object' || result.metadata.attributes === null) {
      throw new Error("metadata.attributes should be object");
    }
  }
  
  console.log("✓ RecursiveComplexity test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testKitchenSink(),
    testUltraComplex(),
    testRecursiveComplexity()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All mixed complex type tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();