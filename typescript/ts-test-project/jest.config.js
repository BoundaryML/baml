/** @type {import('ts-jest').JestConfigWithTsJest} */
export default {
  preset: 'ts-jest',
  testEnvironment: 'node',
  runner: 'jest-runner-baml',
  silent: false,
  verbose: true,
}
