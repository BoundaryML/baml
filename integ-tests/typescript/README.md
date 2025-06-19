# BAML TypeScript Integration Tests

This directory contains integration tests for the BAML TypeScript client. These tests verify the functionality of the TypeScript client library and ensure it works correctly with various BAML features.

## Prerequisites

- Node.js (Latest LTS recommended)
- pnpm package manager
- BAML CLI installed
- Infisical CLI installed

## Setup

1. First, build the TypeScript runtime:

```bash
cd engine/language_client_typescript
pnpm build:debug
```

2. Install dependencies:

```bash
pnpm install
```

3. Generate the BAML client code:

```bash
pnpm generate
```

## Running Tests

### Run all tests

```bash
pnpm integ-tests
```

### Run specific tests

You can run specific tests by using the `-t` flag followed by the test name pattern:

```bash
pnpm integ-tests -t "works with fallbacks"
```

### Environment Variables

- Tests can be run with environment variables using `infisical` (default)

```bash
pnpm integ-tests
```

- Alternatively, you can use a .env file:

```bash
pnpm integ-tests:dotenv
```

### CI Environment

For CI environments, use:

```bash
pnpm integ-tests:ci
```

## Test Reports

After running tests, a HTML test report is generated as `test-report.html` in the project root. This report includes:

- Test results summary
- Console logs
- Failure messages and stack traces

## Project Structure

- `tests/` - Test files
- `baml_client/` - Generated BAML client code
- `src/` - Source files for test utilities and helpers
- `jest.config.js` - Jest test configuration
- `tsconfig.json` - TypeScript configuration

## Debugging Tests

### VS Code Setup

1. Install the [Jest Runner](https://marketplace.visualstudio.com/items?itemName=firsttris.vscode-jest-runner) extension for VS Code

   - This provides inline test running and debugging capabilities
   - Adds "Run Test" and "Debug Test" buttons above each test

2. Create a launch configuration in `.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "node",
      "request": "launch",
      "name": "Debug Tests",
      "runtimeExecutable": "infisical",
      "runtimeArgs": ["run", "--env=test", "--"],
      "program": "${workspaceFolder}/node_modules/.bin/jest",
      "args": ["--runInBand", "--testTimeout", "30000"],
      "console": "integratedTerminal",
      "windows": {
        "program": "${workspaceFolder}/node_modules/jest/bin/jest"
      }
    }
  ]
}
```

3. Set breakpoints in your test files
4. Use the VS Code debugger to run and debug tests, or use the Jest Runner inline buttons

### Debug Logs

- Add `console.log()` statements in your tests
- Set the environment variable `BAML_LOG=trace` to see detailed BAML client logs:

```bash
BAML_LOG=trace pnpm integ-tests
```

## Troubleshooting

### Common Issues

1. **Missing API Keys**

   - Ensure all required API keys are set in your environment
   - Check that `.env` file exists if using `integ-tests:dotenv`
   - Verify Infisical is properly configured if using `integ-tests`

2. **Build Issues**

   - If you get TypeScript errors, try:
     ```bash
     pnpm build:debug
     pnpm generate
     ```
   - Clear the `node_modules` and rebuild if needed:
     ```bash
     rm -rf node_modules
     pnpm install
     ```

3. **Test Timeouts**

   - Default timeout is 30 seconds. For longer running tests, use:
     ```bash
     pnpm integ-tests -- --testTimeout 60000
     ```

4. **BAML Client Generation Issues**

   - Ensure BAML CLI is up to date
   - Check that BAML source files in `../baml_src` are valid
   - Try regenerating the client:
     ```bash
     rm -rf baml_client
     pnpm generate
     ```

5. **Jest Configuration Issues**
   - If tests aren't being found, check:
     - File naming follows `*.test.ts` pattern
     - Test files are in the `tests/` directory
     - Jest configuration in `jest.config.js` is correct

### Test Reports

- Check the test report in `test-report.html` for detailed error information
- Review the console output for error messages and stack traces
- Set `--verbose=true` for more detailed test output:
  ```bash
  pnpm integ-tests -- --verbose=true
  ```

## Adding New Tests

### 1. Define BAML Files

First, add your test definitions in the BAML source files (see [BAML Source README](../baml_src/README.md)):

1. Add clients in `baml_src/clients.baml`
2. Add functions and tests in `baml_src/test-files/providers/`

### 2. Generate TypeScript Client

```bash
pnpm generate
```

This will create new TypeScript client code in `baml_client/`.

### 3. Create Test File

Create a new test file in `tests/` directory:

```typescript
import { TestAnthropicCompletion } from "../baml_client/functions";

describe("Anthropic Tests", () => {
  it("should complete basic prompt", async () => {
    const result = await TestAnthropicCompletion({
      input: "What is the capital of France?",
    });
    expect(result).toBe("Paris");
  });

  it("should handle errors", async () => {
    await expect(
      TestAnthropicCompletion({
        input: "Test input",
      }),
    ).rejects.toThrow();
  });
});
```

### 4. Run Your Tests

```bash
# Run all tests
pnpm integ-tests

# Run specific test file
pnpm integ-tests -t "Anthropic Tests"
```

### Test File Organization

- Group related tests in the same file
- Name test files with `.test.ts` extension
- Place tests in the `tests/` directory
- Use descriptive test names

### Best Practices

1. **Test Setup**

   - Import functions from `baml_client/`
   - Use Jest's `describe` and `it` blocks
   - Add proper error handling tests

2. **Assertions**

   - Use Jest's expect assertions
   - Test both success and error cases
   - Add timeout for long-running tests

3. **Environment**
   - Ensure all required env vars are set
   - Use test-specific API keys
   - Handle rate limiting appropriately
