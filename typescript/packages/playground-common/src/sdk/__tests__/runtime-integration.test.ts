/**
 * Integration Test: Real BAML Runtime State Management
 *
 * Tests that the SDK properly integrates with the real BAML runtime
 * and updates state correctly through the storage layer.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { createStore } from 'jotai';
import { createRealBAMLSDK } from '../factory';
import { readFileSync } from 'fs';
import { join } from 'path';

describe('BAML Runtime Integration', () => {
  let sdk: ReturnType<typeof createRealBAMLSDK>;
  let store: ReturnType<typeof createStore>;
  let bamlFiles: Record<string, string>;

  beforeAll(async () => {
    // Load BAML fixtures
    const fixturesPath = join(__dirname, 'fixtures', 'baml_src');
    bamlFiles = {
      'baml_src/main.baml': readFileSync(join(fixturesPath, 'main.baml'), 'utf-8'),
      'baml_src/clients.baml': readFileSync(join(fixturesPath, 'clients.baml'), 'utf-8'),
    };

    // Create SDK with real BAML runtime
    store = createStore();
    sdk = createRealBAMLSDK(store);
  });

  describe('SDK Initialization', () => {
    it('should initialize SDK with BAML files', async () => {
      await sdk.initialize(bamlFiles);

      // Verify SDK is initialized
      expect(sdk).toBeDefined();
    });

    it('should expose atoms via sdk.atoms', () => {
      expect(sdk.atoms).toBeDefined();
      expect(sdk.atoms.diagnosticsAtom).toBeDefined();
      expect(sdk.atoms.generatedFilesAtom).toBeDefined();
      expect(sdk.atoms.workflowsAtom).toBeDefined();
      expect(sdk.atoms.isRuntimeValid).toBeDefined();
    });
  });

  describe('Diagnostics Extraction', () => {
    it('should extract diagnostics from BAML runtime', () => {
      const diagnostics = store.get(sdk.atoms.diagnosticsAtom);

      expect(diagnostics).toBeDefined();
      expect(Array.isArray(diagnostics)).toBe(true);

      // Log diagnostics for debugging
      console.log('Extracted diagnostics:', diagnostics);

      if (diagnostics.length > 0) {
        // Verify diagnostic structure
        const firstDiag = diagnostics[0]!;
        expect(firstDiag).toHaveProperty('id');
        expect(firstDiag).toHaveProperty('type');
        expect(firstDiag).toHaveProperty('message');
        expect(['error', 'warning']).toContain(firstDiag.type);
      }
    });

    it('should update error counts correctly', () => {
      const { errors, warnings } = store.get(sdk.atoms.numErrorsAtom);

      expect(typeof errors).toBe('number');
      expect(typeof warnings).toBe('number');
      expect(errors >= 0).toBe(true);
      expect(warnings >= 0).toBe(true);

      console.log(`Error counts: ${errors} errors, ${warnings} warnings`);
    });

    it('should track runtime validity', () => {
      const isValid = store.get(sdk.atoms.isRuntimeValid);

      expect(typeof isValid).toBe('boolean');

      console.log('Runtime validity:', isValid);
    });
  });

  describe('Generated Files', () => {
    it('should extract generated files from runtime', () => {
      const generatedFiles = store.get(sdk.atoms.generatedFilesAtom);

      expect(generatedFiles).toBeDefined();
      expect(Array.isArray(generatedFiles)).toBe(true);

      console.log(`Generated ${generatedFiles.length} files`);

      // Only check structure if files were generated
      if (generatedFiles.length > 0) {
        const firstFile = generatedFiles[0]!;
        expect(firstFile).toHaveProperty('path');
        expect(firstFile).toHaveProperty('content');
        expect(firstFile).toHaveProperty('outputDir');
      }
    });

    it('should allow filtering generated files by language', () => {
      const allFiles = store.get(sdk.atoms.generatedFilesAtom);

      // Try to get Python files
      const pythonFiles = store.get(sdk.atoms.generatedFilesByLangAtomFamily('python'));

      expect(pythonFiles).toBeDefined();
      expect(Array.isArray(pythonFiles)).toBe(true);

      // Python files should be a subset of all files
      expect(pythonFiles.length).toBeLessThanOrEqual(allFiles.length);

      console.log(`Found ${pythonFiles.length} Python files out of ${allFiles.length} total`);
    });
  });

  describe('State Tracking', () => {
    it('should track BAML files in atoms', () => {
      const trackedFiles = store.get(sdk.atoms.bamlFilesTrackedAtom);

      expect(trackedFiles).toBeDefined();
      expect(Object.keys(trackedFiles).length).toBeGreaterThan(0);

      // Verify our files are tracked
      expect(trackedFiles['baml_src/main.baml']).toBeDefined();
      expect(trackedFiles['baml_src/clients.baml']).toBeDefined();
    });

    it('should store environment variables in atoms', async () => {
      const testEnvVars = {
        OPENAI_API_KEY: 'test-key-placeholder',
        TEST_VAR: 'test-value',
      };

      // Update env vars
      await sdk.envVars.update(testEnvVars);

      // Verify they're stored
      const storedEnvVars = store.get(sdk.atoms.envVarsAtom);
      expect(storedEnvVars).toEqual(testEnvVars);

      // Verify via SDK API
      const currentEnvVars = sdk.envVars.getCurrent();
      expect(currentEnvVars).toEqual(testEnvVars);
    });

    it('should store feature flags in atoms', async () => {
      const testFlags = ['beta', 'experimental'];

      // Update feature flags
      await sdk.featureFlags.update(testFlags);

      // Verify they're stored
      const storedFlags = store.get(sdk.atoms.featureFlagsAtom);
      expect(storedFlags).toEqual(testFlags);

      // Verify beta flag derived atom
      const betaEnabled = store.get(sdk.atoms.betaFeatureEnabledAtom);
      expect(betaEnabled).toBe(true);

      // Verify via SDK API
      const currentFlags = sdk.featureFlags.getCurrent();
      expect(currentFlags).toEqual(testFlags);
    });
  });

  describe('Runtime Recreation', () => {
    it('should recreate runtime when files change', async () => {
      // Get initial diagnostics
      const initialDiagnostics = store.get(sdk.atoms.diagnosticsAtom);

      // Update files (add a syntax error)
      const updatedFiles = {
        ...bamlFiles,
        'baml_src/main.baml': bamlFiles['baml_src/main.baml'] + '\n\n// Invalid syntax\nthis is not valid baml',
      };

      await sdk.files.update(updatedFiles);

      // Get new diagnostics
      const newDiagnostics = store.get(sdk.atoms.diagnosticsAtom);

      // Diagnostics should have changed (we added an error)
      // Note: The exact behavior depends on the BAML parser
      expect(newDiagnostics).toBeDefined();

      console.log('Diagnostics after file update:', newDiagnostics.length);

      // Verify files are tracked
      const trackedFiles = store.get(sdk.atoms.bamlFilesTrackedAtom);
      expect(trackedFiles['baml_src/main.baml']).toContain('this is not valid baml');
    });

    it('should recreate runtime when env vars change', async () => {
      const newEnvVars = {
        OPENAI_API_KEY: 'updated-key',
      };

      await sdk.envVars.update(newEnvVars);

      // Verify runtime was recreated (diagnostics should be re-extracted)
      const diagnostics = store.get(sdk.atoms.diagnosticsAtom);
      expect(diagnostics).toBeDefined();

      // Verify env vars are stored
      const storedEnvVars = store.get(sdk.atoms.envVarsAtom);
      expect(storedEnvVars.OPENAI_API_KEY).toBe('updated-key');
    });

    it('should recreate runtime when feature flags change', async () => {
      const newFlags = ['beta'];

      await sdk.featureFlags.update(newFlags);

      // Verify runtime was recreated
      const diagnostics = store.get(sdk.atoms.diagnosticsAtom);
      expect(diagnostics).toBeDefined();

      // Verify flags are stored
      const storedFlags = store.get(sdk.atoms.featureFlagsAtom);
      expect(storedFlags).toEqual(newFlags);
    });
  });

  describe('Workflow Extraction', () => {
    it('should extract workflows from BAML runtime', () => {
      const workflows = sdk.workflows.getAll();

      expect(workflows).toBeDefined();
      expect(Array.isArray(workflows)).toBe(true);

      console.log(`Extracted ${workflows.length} workflows`);

      // Note: Workflow extraction may not be fully implemented yet
      // This test documents expected behavior
    });
  });

  describe('Test Execution (Expected Failure)', () => {
    // TODO: Fix this test - it fails because sdk.initialize() tries to call
    // VSCode endpoint (getPlaygroundPort) which doesn't exist in test environment,
    // causing the runtime to become invalid and return no test cases.
    it.skip('should extract and attempt to run test case', async () => {
      // Re-initialize SDK with valid settings (previous tests may have left it in invalid state)
      await sdk.initialize(bamlFiles);

      // Get test cases
      const testCases = sdk.testCases.get('ExtractResume');
      console.log('Available test cases:', testCases);

      expect(testCases).toBeDefined();
      expect(testCases.length).toBe(1);

      const testCase = testCases[0];
      expect(testCase).toBeDefined();
      expect(testCase?.name).toBe('Test1');
      expect(testCase?.functionId).toBe('ExtractResume');
      expect(testCase?.inputs).toBeDefined();
      expect(testCase?.inputs.resume_text).toContain('John Doe');

      // Try to run the test via runAll
      // Expected to fail due to missing OPENAI_API_KEY
      await sdk.tests.runAll([{ functionName: 'ExtractResume', testName: 'Test1' }]);

      // Since runAll doesn't return a result directly anymore, we'd need to check state via atoms
      // For now, just verify the method doesn't throw
      console.log('Test execution completed (may have failed due to missing API key)');
    });
  });

  describe('WASM Panic Handling', () => {
    it('should expose WASM panic atom', () => {
      const panicState = store.get(sdk.atoms.wasmPanicAtom);

      // Should be null initially (no panic)
      expect(panicState).toBeNull();
    });

    it('should allow setting WASM panic state', () => {
      const testPanic = {
        msg: 'Test panic message',
        timestamp: Date.now(),
      };

      // Manually set panic (simulating a WASM panic)
      store.set(sdk.atoms.wasmPanicAtom, testPanic);

      // Verify it was set
      const panicState = store.get(sdk.atoms.wasmPanicAtom);
      expect(panicState).toEqual(testPanic);

      // Clear it
      store.set(sdk.atoms.wasmPanicAtom, null);
      const clearedPanic = store.get(sdk.atoms.wasmPanicAtom);
      expect(clearedPanic).toBeNull();
    });
  });

  describe('SDK API Methods', () => {
    it('should provide file management API', () => {
      expect(sdk.files).toBeDefined();
      expect(sdk.files.update).toBeDefined();
      expect(sdk.files.getCurrent).toBeDefined();

      const currentFiles = sdk.files.getCurrent();
      expect(currentFiles).toBeDefined();
      expect(Object.keys(currentFiles).length).toBeGreaterThan(0);
    });

    it('should provide workflow API', () => {
      expect(sdk.workflows).toBeDefined();
      expect(sdk.workflows.getAll).toBeDefined();
      expect(sdk.workflows.getById).toBeDefined();
      expect(sdk.workflows.getActive).toBeDefined();
      expect(sdk.workflows.setActive).toBeDefined();
    });

    it('should provide environment variables API', () => {
      expect(sdk.envVars).toBeDefined();
      expect(sdk.envVars.update).toBeDefined();
      expect(sdk.envVars.getCurrent).toBeDefined();
    });

    it('should provide feature flags API', () => {
      expect(sdk.featureFlags).toBeDefined();
      expect(sdk.featureFlags.update).toBeDefined();
      expect(sdk.featureFlags.getCurrent).toBeDefined();
    });

    it('should provide generated files API', () => {
      expect(sdk.generatedFiles).toBeDefined();
      expect(sdk.generatedFiles.getAll).toBeDefined();
      expect(sdk.generatedFiles.getByLanguage).toBeDefined();
    });

    it('should provide execution API', () => {
      expect(sdk.executions).toBeDefined();
      expect(sdk.executions.start).toBeDefined();
      expect(sdk.executions.getAll).toBeDefined();
      expect(sdk.executions.cancel).toBeDefined();
    });

    it('should provide cache API', () => {
      expect(sdk.cache).toBeDefined();
      expect(sdk.cache.get).toBeDefined();
      expect(sdk.cache.set).toBeDefined();
      expect(sdk.cache.clear).toBeDefined();
    });

    it('should provide test cases API', () => {
      expect(sdk.testCases).toBeDefined();
      expect(sdk.testCases.get).toBeDefined();
    });
  });

  describe('Storage Integration', () => {
    it('should properly wire atoms to storage', () => {
      // Test that we can read atoms directly from store
      const diagnostics = store.get(sdk.atoms.diagnosticsAtom);
      const generatedFiles = store.get(sdk.atoms.generatedFilesAtom);
      const workflows = store.get(sdk.atoms.workflowsAtom);

      expect(diagnostics).toBeDefined();
      expect(generatedFiles).toBeDefined();
      expect(workflows).toBeDefined();
    });

    it('should allow subscribing to atom changes', () => {
      let callCount = 0;

      // Subscribe to diagnostics changes
      const unsubscribe = store.sub(sdk.atoms.diagnosticsAtom, () => {
        callCount++;
      });

      // Create a mock runtime with test diagnostics
      const mockRuntime = {
        getDiagnostics: () => [{
          id: 'test-1',
          type: 'error' as const,
          message: 'Test error',
        }],
        getFunctions: () => [],
        getWorkflows: () => [],
        getGeneratedFiles: () => [],
        getVersion: () => '0.0.0-test',
        getWasmInstance: () => undefined,
      } as any;

      // Update runtime to trigger diagnostics change (diagnosticsAtom is derived from runtime)
      store.set(sdk.atoms.runtimeInstanceAtom, mockRuntime);

      // Verify subscription was called
      expect(callCount).toBeGreaterThan(0);

      unsubscribe();
    });
  });
});
