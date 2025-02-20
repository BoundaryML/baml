/**
 * this is our jest configuration for running typescript tests
 * we use ts-jest to handle typescript compilation and testing
 * @type {import('ts-jest').JestConfigWithTsJest}
 */
module.exports = {
  // use the ts-jest preset which handles typescript files
  preset: 'ts-jest',

  // run tests in a node environment rather than jsdom
  testEnvironment: 'node',

  // look for both typescript and javascript files
  moduleFileExtensions: ['ts', 'js'],

  // use ts-jest to transform typescript files before running tests
  transform: {
    '^.+\\.ts$': 'ts-jest',
  },

  // look for test files in __test__ directories that end in .test.ts
  testMatch: ['**/__test__/**/*.test.ts'],
};