import { baml } from '../baml_client';

async function testSimpleNested(): Promise<void> {
  console.log("Testing SimpleNested...");
  const result = await baml.TestSimpleNested("test simple nested");
  
  // Verify user structure
  if (typeof result.user !== 'object' || result.user === null) {
    throw new Error("user should be object");
  }
  if (typeof result.user.id !== 'number' || typeof result.user.name !== 'string') {
    throw new Error("user should have id and name");
  }
  
  // Verify nested profile
  if (typeof result.user.profile !== 'object' || result.user.profile === null) {
    throw new Error("user.profile should be object");
  }
  if (typeof result.user.profile.bio !== 'string' || typeof result.user.profile.avatar !== 'string') {
    throw new Error("profile should have bio and avatar");
  }
  
  // Verify deeply nested social links
  if (typeof result.user.profile.social !== 'object' || result.user.profile.social === null) {
    throw new Error("profile.social should be object");
  }
  
  // Verify deeply nested preferences
  if (typeof result.user.profile.preferences !== 'object' || result.user.profile.preferences === null) {
    throw new Error("profile.preferences should be object");
  }
  if (!['light', 'dark'].includes(result.user.profile.preferences.theme)) {
    throw new Error("preferences.theme should be 'light' or 'dark'");
  }
  
  // Verify notification settings
  if (typeof result.user.profile.preferences.notifications !== 'object' || result.user.profile.preferences.notifications === null) {
    throw new Error("preferences.notifications should be object");
  }
  if (typeof result.user.profile.preferences.notifications.email !== 'boolean') {
    throw new Error("notifications.email should be boolean");
  }
  
  // Verify user settings
  if (typeof result.user.settings !== 'object' || result.user.settings === null) {
    throw new Error("user.settings should be object");
  }
  if (typeof result.user.settings.privacy !== 'object' || result.user.settings.privacy === null) {
    throw new Error("settings.privacy should be object");
  }
  if (typeof result.user.settings.display !== 'object' || result.user.settings.display === null) {
    throw new Error("settings.display should be object");
  }
  
  // Verify address structure
  if (typeof result.address !== 'object' || result.address === null) {
    throw new Error("address should be object");
  }
  if (typeof result.address.street !== 'string' || typeof result.address.city !== 'string') {
    throw new Error("address should have street and city");
  }
  
  // Verify metadata structure
  if (typeof result.metadata !== 'object' || result.metadata === null) {
    throw new Error("metadata should be object");
  }
  if (typeof result.metadata.createdAt !== 'string' || typeof result.metadata.version !== 'number') {
    throw new Error("metadata should have createdAt and version");
  }
  if (!Array.isArray(result.metadata.tags)) {
    throw new Error("metadata.tags should be array");
  }
  if (typeof result.metadata.attributes !== 'object' || result.metadata.attributes === null) {
    throw new Error("metadata.attributes should be object");
  }
  
  console.log("✓ SimpleNested test passed");
}

async function testDeeplyNested(): Promise<void> {
  console.log("\nTesting DeeplyNested...");
  const result = await baml.TestDeeplyNested("test deeply nested");
  
  // Verify 5 levels of nesting
  if (typeof result.level1 !== 'object' || result.level1 === null) {
    throw new Error("level1 should be object");
  }
  if (typeof result.level1.data !== 'string') {
    throw new Error("level1.data should be string");
  }
  
  if (typeof result.level1.level2 !== 'object' || result.level1.level2 === null) {
    throw new Error("level1.level2 should be object");
  }
  if (typeof result.level1.level2.data !== 'string') {
    throw new Error("level2.data should be string");
  }
  
  if (typeof result.level1.level2.level3 !== 'object' || result.level1.level2.level3 === null) {
    throw new Error("level2.level3 should be object");
  }
  if (typeof result.level1.level2.level3.data !== 'string') {
    throw new Error("level3.data should be string");
  }
  
  if (typeof result.level1.level2.level3.level4 !== 'object' || result.level1.level2.level3.level4 === null) {
    throw new Error("level3.level4 should be object");
  }
  if (typeof result.level1.level2.level3.level4.data !== 'string') {
    throw new Error("level4.data should be string");
  }
  
  if (typeof result.level1.level2.level3.level4.level5 !== 'object' || result.level1.level2.level3.level4.level5 === null) {
    throw new Error("level4.level5 should be object");
  }
  if (typeof result.level1.level2.level3.level4.level5.data !== 'string') {
    throw new Error("level5.data should be string");
  }
  if (!Array.isArray(result.level1.level2.level3.level4.level5.items)) {
    throw new Error("level5.items should be array");
  }
  if (typeof result.level1.level2.level3.level4.level5.mapping !== 'object' || result.level1.level2.level3.level4.level5.mapping === null) {
    throw new Error("level5.mapping should be object");
  }
  
  console.log("✓ DeeplyNested test passed");
}

async function testComplexNested(): Promise<void> {
  console.log("\nTesting ComplexNested...");
  const result = await baml.TestComplexNested("test complex nested");
  
  // Verify company structure
  if (typeof result.company !== 'object' || result.company === null) {
    throw new Error("company should be object");
  }
  if (typeof result.company.id !== 'number' || typeof result.company.name !== 'string') {
    throw new Error("company should have id and name");
  }
  
  // Verify company address
  if (typeof result.company.address !== 'object' || result.company.address === null) {
    throw new Error("company.address should be object");
  }
  
  // Verify departments
  if (!Array.isArray(result.company.departments)) {
    throw new Error("company.departments should be array");
  }
  if (result.company.departments.length === 0) {
    throw new Error("company.departments should not be empty");
  }
  if (!result.company.departments.every(dept => 
    typeof dept === 'object' && dept !== null &&
    typeof dept.id === 'number' &&
    typeof dept.name === 'string' &&
    Array.isArray(dept.members) &&
    Array.isArray(dept.projects)
  )) {
    throw new Error("departments should have proper structure");
  }
  
  // Verify employees array
  if (!Array.isArray(result.employees)) {
    throw new Error("employees should be array");
  }
  if (result.employees.length === 0) {
    throw new Error("employees should not be empty");
  }
  if (!result.employees.every(emp => 
    typeof emp === 'object' && emp !== null &&
    typeof emp.id === 'number' &&
    typeof emp.name === 'string' &&
    typeof emp.email === 'string' &&
    Array.isArray(emp.skills)
  )) {
    throw new Error("employees should have proper structure");
  }
  
  // Verify projects array
  if (!Array.isArray(result.projects)) {
    throw new Error("projects should be array");
  }
  if (result.projects.length === 0) {
    throw new Error("projects should not be empty");
  }
  if (!result.projects.every(proj => 
    typeof proj === 'object' && proj !== null &&
    typeof proj.id === 'number' &&
    typeof proj.name === 'string' &&
    ['planning', 'active', 'completed', 'cancelled'].includes(proj.status) &&
    Array.isArray(proj.team) &&
    Array.isArray(proj.milestones) &&
    typeof proj.budget === 'object'
  )) {
    throw new Error("projects should have proper structure");
  }
  
  // Verify nested milestones and tasks
  result.projects.forEach(project => {
    if (!project.milestones.every(milestone => 
      typeof milestone === 'object' && milestone !== null &&
      typeof milestone.id === 'number' &&
      typeof milestone.name === 'string' &&
      Array.isArray(milestone.tasks)
    )) {
      throw new Error("milestones should have proper structure");
    }
  });
  
  console.log("✓ ComplexNested test passed");
}

async function testRecursiveStructure(): Promise<void> {
  console.log("\nTesting RecursiveStructure...");
  const result = await baml.TestRecursiveStructure("test recursive structure");
  
  // Verify root structure
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
  
  // Verify recursive structure
  if (result.children.length > 0) {
    if (!result.children.every(child => 
      typeof child === 'object' && child !== null &&
      typeof child.id === 'number' &&
      typeof child.name === 'string' &&
      Array.isArray(child.children)
    )) {
      throw new Error("children should have recursive structure");
    }
  }
  
  // Verify metadata map
  if (typeof result.metadata !== 'object' || result.metadata === null) {
    throw new Error("metadata should be object");
  }
  
  console.log("✓ RecursiveStructure test passed");
}

async function main(): Promise<void> {
  // Run all tests in parallel
  const tests = [
    testSimpleNested(),
    testDeeplyNested(),
    testComplexNested(),
    testRecursiveStructure()
  ];
  
  try {
    await Promise.all(tests);
    console.log("\n✅ All nested structure tests passed!");
  } catch (error) {
    console.error(`\n❌ Test failed: ${error}`);
    process.exit(1);
  }
}

// Run the tests
main();