/**
 * BAML SDK - Core Class
 *
 * Orchestrates runtime and storage
 * Follows wasmAtom pattern: creates new runtime instances on file changes
 */

import type { SDKStorage } from './storage/SDKStorage';
import type { BamlRuntimeInterface, BamlRuntimeFactory } from './runtime/BamlRuntimeInterface';
import type { FunctionWithCallGraph, TestResponseData, WatchNotification, TestCaseMetadata } from './interface';
import type {
  ExecutionSnapshot,
  CacheEntry,
  TestCaseInput,
  BAMLFile,
} from './types';

// Import all atoms to expose via sdk.atoms
import * as coreAtoms from './atoms/core.atoms';
import * as testAtoms from './atoms/test.atoms';

// Import navigation
import { createNavigationCoordinator, type NavigationCoordinator } from './navigation/coordinator';
import type { NavigationInput, NavigationContext } from './navigation/types';

// Import vscode integration for telemetry and flashing regions
import { vscode } from '../shared/baml-project-panel/vscode';

/**
 * BAML SDK - orchestrates runtime and storage
 * Follows wasmAtom pattern: creates new runtime instances on file changes
 */
export class BAMLSDK {
  private runtime: BamlRuntimeInterface | null = null;
  private storage: SDKStorage;
  private activeExecutions = new Map<string, AbortController>();
  private runtimeFactory: BamlRuntimeFactory;
  private currentFiles: Record<string, string> = {};
  private coordinator: NavigationCoordinator | null = null;

  /**
   * Expose all atoms directly via sdk.atoms
   * Components can access state via: sdk.atoms.workflows, sdk.atoms.diagnostics, etc.
   * Test-related atoms are namespaced under sdk.atoms.test
   */
  atoms = {
    ...coreAtoms,
    test: testAtoms,
  };

  constructor(runtimeFactory: BamlRuntimeFactory, storage: SDKStorage) {
    this.runtimeFactory = runtimeFactory;
    this.storage = storage;
  }

  /**
   * Initialize SDK with initial files
   * Creates the first runtime instance
   */
  async initialize(
    initialFiles: Record<string, string>,
    options?: {
      envVars?: Record<string, string>;
      featureFlags?: string[];
    }
  ) {
    if (Object.keys(initialFiles).length === 0) {
      throw new Error('Cannot initialize SDK with empty files');
    }

    // Load VSCode settings (in VSCode environment only)
    await this.loadVSCodeSettings();

    // Store initial state in atoms
    this.currentFiles = initialFiles;
    this.storage.setBAMLFiles(initialFiles);

    if (options?.envVars) {
      this.storage.setEnvVars(options.envVars);
    }
    if (options?.featureFlags) {
      this.storage.setFeatureFlags(options.featureFlags);
    }
  }

  /**
   * Load VSCode settings and playground port
   * Called during initialization to populate VSCode-specific state
   */
  private async loadVSCodeSettings() {
    try {
      // Load VSCode settings
      const settings = await vscode.getVSCodeSettings();
      this.storage.setVSCodeSettings({
        enablePlaygroundProxy: settings.enablePlaygroundProxy,
        featureFlags: settings.featureFlags,
      });
      console.log('SDK: Loaded VSCode settings:', settings);
    } catch (e) {
      console.log('SDK: Not in VSCode environment or failed to load settings:', e);
      // Not in VSCode environment - settings remain null
    }

    try {
      // Load playground port
      const port = await vscode.getPlaygroundPort();
      this.storage.setPlaygroundPort(port);
      console.log('SDK: Loaded playground port:', port);
    } catch (e) {
      console.log('SDK: Failed to load playground port:', e);
      // Port remains 0
    }
  }

  // ============================================================================
  // PRIVATE: Runtime Recreation Helper
  // ============================================================================

  /**
   * Recreate runtime with current files, env vars, and feature flags
   * This is the central place where runtime recreation happens
   *
   * WASM module is now cached at the module level, so this only recreates
   * WasmProject and WasmRuntime instances (not the entire WASM module)
   */
  private async recreateRuntime() {
    console.log('SDK: Recreating runtime instance');

    const envVars = this.storage.getEnvVars();
    const featureFlags = this.storage.getFeatureFlags();

    // Create new runtime instance (WASM module is cached, only WasmProject/WasmRuntime recreated)
    this.runtime = await this.runtimeFactory(this.currentFiles, envVars, featureFlags);

    // Store runtime instance - this automatically updates all derived atoms
    this.storage.setRuntime(this.runtime);

    // Populate parsed BAML files atom (for navigation, DebugPanel, etc.)
    const parsedFiles = this.runtime.getBAMLFiles();
    console.log('aaron: [SDK] Parsed files:', parsedFiles.map((file: any) => file.path));
    parsedFiles.forEach((file: any) => {
      file.functions.forEach((fn: any) => {
        console.log(
          `aaron: ParsedFiles: Function: ${fn.displayName || fn.name}, Nodes:`,
          Array.isArray(fn.nodes) ? fn.nodes.map((node: any) => node.id).join(', ') : []
        );
      });
    });
    this.storage.setParsedBAMLFiles(parsedFiles);

    // Store last valid WASM instance if no errors
    const diagnostics = this.runtime.getDiagnostics();
    const hasErrors = diagnostics.some((d) => d.type === 'error');
    if (!hasErrors) {
      const wasmInstance = this.runtime.getWasmRuntime();
      if (wasmInstance) {
        this.storage.setWasmRuntime(wasmInstance);
      }
    }

    // Log what was extracted from the runtime
    const workflows = this.runtime.getWorkflows();
    const functions = this.runtime.getFunctions();
    const allTestCases = this.runtime.getTestCases();
    console.log('SDK: Runtime recreated with', workflows.length, 'workflows,', diagnostics.length, 'diagnostics');
    console.log('SDK: Extracted', functions.length, 'functions:', functions.map(f => f.name));
    console.log('SDK: Extracted', allTestCases.length, 'test cases:', allTestCases.map(tc => `${tc.name} (${tc.functionId})`));

    // Retry pending navigation if in loading state
    await this.retryPendingNavigation();

    // Restore cursor position if it was updated recently (< 3 seconds ago)
    const lastCursorPosition = this.storage.getLastCursorPosition();
    if (lastCursorPosition) {
      const timeSinceLastUpdate = Date.now() - lastCursorPosition.timestamp;
      const THREE_SECONDS = 3000;

      if (timeSinceLastUpdate < THREE_SECONDS) {
        console.log('aaron: [SDK] Restoring cursor position after runtime recreation:', lastCursorPosition);
        // Re-apply the cursor position to restore navigation state
        setTimeout(() => {
          this.navigation.updateCursor({
            fileName: lastCursorPosition.fileName,
            line: lastCursorPosition.line,
            column: lastCursorPosition.column,
          });
        }, 100);
      } else {
        console.log('aaron: [SDK] Cursor position too old to restore (age:', timeSinceLastUpdate, 'ms)');
      }
    }
  }

  /**
   * Retry pending navigation if we're in a loading state
   * Called after runtime recreation to complete pending navigations
   */
  private async retryPendingNavigation() {
    const currentState = this.storage.store.get(this.atoms.unifiedSelectionStateAtom);

    if (currentState.mode !== 'loading') {
      return; // Nothing to retry
    }

    console.log('🔄 Retrying pending navigation:', currentState.intent);

    // Get coordinator (this automatically updates context with latest runtime data)
    const coordinator = this.getNavigationCoordinator();

    // Navigate with preserved intent
    await coordinator.navigate(
      currentState.intent,
      this.storage.store.get,
      this.storage.store.set
    );
  }

  // ============================================================================
  // File Management API
  // ============================================================================

  files = {
    /**
     * Update files and recreate runtime
     * Atoms handle reactivity - we just update state and recreate
     */
    update: async (files: Record<string, string>) => {
      if (Object.keys(files).length === 0) {
        throw new Error('Files cannot be empty');
      }

      // Efficiently detect if file contents have changed
      const oldFiles = this.currentFiles;
      const oldKeys = Object.keys(oldFiles);
      const newKeys = Object.keys(files);

      let changed = false;

      if (oldKeys.length !== newKeys.length) {
        changed = true;
      } else {
        for (const key of newKeys) {
          if (!(key in oldFiles) || oldFiles[key] !== files[key]) {
            changed = true;
            break;
          }
        }
        if (!changed) {
          for (const key of oldKeys) {
            if (!(key in files)) {
              changed = true;
              break;
            }
          }
        }
      }

      if (!changed) {
        console.log('files: SDK: No file content changes detected, skipping runtime update');
        return;
      }


      // Update files in storage (updates atom)
      this.currentFiles = files;
      this.storage.setBAMLFiles(files);

      // Recreate runtime with new files
      await this.recreateRuntime();
    },

    getCurrent: () => {
      return { ...this.currentFiles };
    },
  };

  // ============================================================================
  // Workflow API
  // ============================================================================

  workflows = {
    getAll: (): FunctionWithCallGraph[] => this.storage.getWorkflows(),

    getById: (id: string): FunctionWithCallGraph | null => {
      return this.storage.getWorkflows().find((w) => w.id === id) ?? null;
    },

    getActive: (): FunctionWithCallGraph | null => {
      const id = this.storage.getActiveWorkflowId();
      if (!id) return null;
      return this.workflows.getById(id);
    },

    setActive: (id: string) => {
      const workflow = this.workflows.getById(id);
      if (!workflow) {
        throw new Error(`Workflow "${id}" not found`);
      }
      this.storage.setActiveWorkflowId(id);
    },
  };

  // ============================================================================
  // Execution API
  // ============================================================================

  executions = {
    start: async (
      workflowId: string,
      inputs: Record<string, any>,
      options?: { clearCache?: boolean; startFromNodeId?: string }
    ): Promise<string> => {
      if (!this.runtime) {
        throw new Error('SDK not initialized');
      }

      const workflow = this.workflows.getById(workflowId);
      if (!workflow) {
        throw new Error(`Workflow "${workflowId}" not found`);
      }

      // Clear cache if requested
      if (options?.clearCache) {
        this.cache.clear({ workflowId });
      }

      // Clear node states
      this.storage.clearAllNodeStates();

      // Wait for visual feedback
      await new Promise((resolve) => setTimeout(resolve, 200));

      // Generate execution ID
      const executionId = `exec_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

      // Create execution snapshot
      const execution: ExecutionSnapshot = {
        id: executionId,
        workflowId,
        timestamp: Date.now(),
        graphSnapshot: {
          nodes: workflow.nodes,
          edges: workflow.edges,
          codeHash: workflow.codeHash,
        },
        status: 'running',
        nodeExecutions: new Map(),
        trigger: 'manual',
        branchPath: [],
        inputs,
      };

      // Add to storage
      this.storage.addExecution(workflowId, execution);

      // Run execution via runtime
      this.runExecution(executionId, workflowId, inputs, options);

      return executionId;
    },

    getAll: (workflowId: string): ExecutionSnapshot[] => {
      return this.storage.getExecutions(workflowId);
    },

    cancel: (executionId: string) => {
      const controller = this.activeExecutions.get(executionId);
      if (controller) {
        controller.abort();
        this.activeExecutions.delete(executionId);
      }
    },
  };

  // ============================================================================
  // Cache API
  // ============================================================================

  cache = {
    get: (nodeId: string, inputsHash: string): CacheEntry | null => {
      return this.storage.getCacheEntry(nodeId, inputsHash);
    },

    set: (entry: CacheEntry) => {
      this.storage.setCacheEntry(entry);
    },

    clear: (scope?: { workflowId?: string; nodeId?: string }) => {
      this.storage.clearCache(scope);
    },
  };


  // ============================================================================
  // Graph API
  // ============================================================================

  graph = {
    /**
     * Get graph structure for a workflow
     */
    getGraph: (workflowId: string): { nodes: any[]; edges: any[] } | null => {
      const workflow = this.workflows.getById(workflowId);
      if (!workflow) return null;

      return {
        nodes: workflow.nodes,
        edges: workflow.edges,
      };
    },

    /**
     * Update node positions (for layout persistence)
     */
    updateNodePositions: (
      workflowId: string,
      positions: Map<string, { x: number; y: number }>
    ): void => {
      const workflows = this.storage.getWorkflows();
      const updatedWorkflows = workflows.map((w) => {
        if (w.id !== workflowId) return w;

        return {
          ...w,
          nodes: w.nodes.map((node) => {
            const pos = positions.get(node.id);
            if (!pos) return node;

            return { ...node, position: pos };
          }),
        };
      });

      this.storage.setWorkflows(updatedWorkflows);
    },
  };

  // ============================================================================
  // Test Cases API
  // ============================================================================

  testCases = {
    get: (nodeId: string): TestCaseInput[] => {
      if (!this.runtime) {
        throw new Error('SDK not initialized');
      }
      // Filter and map TestCaseMetadata to TestCaseInput
      const testCases = this.runtime.getTestCases(nodeId);
      return testCases
        .filter((tc): tc is TestCaseMetadata & { source: 'test' } => tc.source === 'test')
        .map(tc => {
          // Convert ParameterInfo[] to Record<string, any>
          const inputs: Record<string, any> = {};
          for (const param of tc.inputs) {
            inputs[param.name] = param.value;
          }

          return {
            id: tc.id,
            name: tc.name,
            source: 'test' as const,
            functionId: tc.functionId,
            filePath: tc.filePath,
            inputs,
            expectedOutput: undefined,
            status: undefined,
            lastRun: undefined,
          };
        });
    },
  };

  // ============================================================================
  // Environment Variables API
  // ============================================================================

  envVars = {
    /**
     * Update environment variables and recreate runtime
     * Atoms handle reactivity - we just update state and recreate
     */
    update: async (envVars: Record<string, string>) => {
      console.log('SDK: Updating environment variables');

      // Update env vars in storage (updates atom)
      this.storage.setEnvVars(envVars);

      // Recreate runtime with new env vars
      await this.recreateRuntime();
    },

    getCurrent: () => {
      return this.storage.getEnvVars();
    },
  };

  // ============================================================================
  // Feature Flags API
  // ============================================================================

  featureFlags = {
    /**
     * Update feature flags and recreate runtime
     * Atoms handle reactivity - we just update state and recreate
     */
    update: async (featureFlags: string[]) => {
      console.log('SDK: Updating feature flags');

      // Update feature flags in storage (updates atom)
      this.storage.setFeatureFlags(featureFlags);

      // Recreate runtime with new feature flags
      await this.recreateRuntime();
    },

    getCurrent: () => {
      return this.storage.getFeatureFlags();
    },
  };

  // ============================================================================
  // Settings API (VSCode settings, proxy, etc.)
  // ============================================================================

  settings = {
    /**
     * Update VSCode settings (enablePlaygroundProxy, featureFlags, etc.)
     * Called when baml_settings_updated message is received
     */
    updateVSCodeSettings: (settings: Partial<coreAtoms.VSCodeSettings>) => {
      const current = this.storage.getVSCodeSettings() || {};
      const updated = { ...current, ...settings };
      console.log('[SDK] Updating VSCode settings:', { current, settings, updated });
      this.storage.setVSCodeSettings(updated);
      this.recreateRuntime();
      console.log('[SDK] Recreated runtime after updating VSCode settings');
    },

    /**
     * Get current VSCode settings
     */
    getVSCodeSettings: () => {
      return this.storage.getVSCodeSettings();
    },

    /**
     * Update playground proxy port
     */
    setPlaygroundPort: (port: number) => {
      console.log('[SDK] Setting playground port:', port);
      this.storage.setPlaygroundPort(port);
    },

    /**
     * Get current playground port
     */
    getPlaygroundPort: () => {
      return this.storage.getPlaygroundPort();
    },
  };

  // ============================================================================
  // Generated Files API
  // ============================================================================

  generatedFiles = {
    /**
     * Get all generated files
     */
    getAll: (): typeof coreAtoms.generatedFilesAtom extends infer T ? T : never => {
      return this.storage.getGeneratedFiles() as any;
    },

    /**
     * Get generated files filtered by language
     */
    getByLanguage: (lang: string) => {
      return this.storage.getGeneratedFiles().filter((f) => f.outputDir.includes(lang));
    },
  };

  // ============================================================================
  // Files & Diagnostics API
  // ============================================================================

  diagnostics = {
    /**
     * Get BAML file structure (for debug panel, navigation, etc.)
     * Returns files organized with their functions and tests
     */
    getBAMLFiles: (): BAMLFile[] => {
      if (!this.runtime) {
        return [];
      }
      return this.runtime.getBAMLFiles();
    },

    /**
     * Get all compilation diagnostics
     */
    getDiagnostics: () => {
      return this.storage.getDiagnostics();
    },

    /**
     * Get all functions
     */
    getFunctions: (): FunctionWithCallGraph[] => {
      if (!this.runtime) {
        return [];
      }
      return this.runtime.getFunctions();
    },
  };

  // ============================================================================
  // Selection API
  // ============================================================================

  selection = {
    /**
     * Set the currently selected function
     */
    setFunction: (functionName: string | null): void => {
      this.storage.setSelectedFunctionName(functionName);
      // Clear test case selection when changing function
      if (functionName === null) {
        this.storage.setSelectedTestCaseName(null);
      }
    },

    /**
     * Set the currently selected test case
     */
    setTestCase: (testCaseName: string | null): void => {
      this.storage.setSelectedTestCaseName(testCaseName);
    },

    /**
     * Set both function and test case at once
     */
    set: (functionName: string | null, testCaseName: string | null): void => {
      this.storage.setSelectedFunctionName(functionName);
      this.storage.setSelectedTestCaseName(testCaseName);
    },

    /**
     * Get current selection
     */
    get: () => {
      return {
        functionName: this.storage.getSelectedFunctionName(),
        testCaseName: this.storage.getSelectedTestCaseName(),
      };
    },

    /**
     * Clear selection
     */
    clear: (): void => {
      this.storage.setSelectedFunctionName(null);
      this.storage.setSelectedTestCaseName(null);
    },
  };

  // ============================================================================
  // Navigation API
  // ============================================================================

  /**
   * Get or create the navigation coordinator
   * Updates context when runtime or workflows change
   */
  private getNavigationCoordinator(): NavigationCoordinator {
    const workflows = this.storage.getWorkflows();
    const functions = this.runtime?.getFunctions() || [];
    const bamlFiles = this.runtime?.getBAMLFiles() || [];
    const tests = bamlFiles.flatMap((file) => file.tests || []);

    const context: NavigationContext = {
      workflows,
      functions,
      bamlFiles,
      tests,
    };

    // If we already have a coordinator, update its context
    if (this.coordinator) {
      this.coordinator.updateContext(context);
      return this.coordinator;
    }

    // Create new coordinator
    this.coordinator = createNavigationCoordinator(context);
    return this.coordinator;
  }

  /**
   * Navigate to a function, test, or node
   * This is the main entry point for navigation from the SDK
   */
  async navigate(input: NavigationInput): Promise<void> {
    const coordinator = this.getNavigationCoordinator();
    console.log('[SDK] navigating', input);

    // Pass raw Jotai store methods directly - navigation updates atoms directly
    await coordinator.navigate(input, this.storage.store.get, this.storage.store.set);
  }

  navigation = {
    /**
     * Update cursor position from IDE
     * Dispatches navigation event based on what's at the cursor
     */
    updateCursor: (cursor: { fileName: string; line: number; column: number }): void => {
      if (!this.runtime) {
        console.warn('[SDK] Cannot update cursor: runtime not initialized');
        return;
      }

      // Store cursor position and timestamp for runtime recreation
      this.storage.setLastCursorPosition({
        fileName: cursor.fileName,
        line: cursor.line,
        column: cursor.column,
        timestamp: Date.now(),
      });

      const fileContents = this.storage.getBAMLFiles();
      const currentSelection = this.storage.getSelectedFunctionName();

      // Resolve what's at the cursor position via runtime
      const result = this.runtime.updateCursor(cursor, fileContents, currentSelection);
      console.log('[SDK] updateCursor result', result);

      if (!result.functionName) {
        console.debug('aaron: [SDK] Cursor not on any function');
        return;
      }

      // Build navigation input
      // If we have a nodeId, it's a node within a workflow
      // Otherwise, it's either a test or a function
      const kind = result.nodeId
        ? ('node' as const)
        : result.testCaseName
          ? ('test' as const)
          : ('function' as const);

      const navigationInput: NavigationInput = {
        kind,
        source: 'cursor' as const,
        functionName: result.functionName,
        testName: result.testCaseName ?? undefined,
        nodeId: result.nodeId ?? undefined,
        workflowId: result.nodeId ? result.functionName : undefined,
        timestamp: Date.now(),
        cursorPosition: {
          filePath: cursor.fileName,
          line: cursor.line,
          column: cursor.column,
        },
      };

      // Navigate to the target
      console.log('[SDK] navigating to', navigationInput);
      this.navigate(navigationInput);
    },

    /**
     * Update cursor position from range
     * Uses the start position as the cursor
     */
    updateCursorFromRange: (params: {
      fileName: string;
      start: { line: number; character: number };
      end: { line: number; character: number };
    }): void => {
      this.navigation.updateCursor({
        fileName: params.fileName,
        line: params.start.line,
        column: params.start.character,
      });
    },

    /**
     * Select a function (navigate to it in the UI)
     */
    selectFunction: (functionName: string): void => {
      console.debug('[SDK] Function selected:', functionName);

      // Build navigation input
      const navigationInput: NavigationInput = {
        kind: 'function' as const,
        source: 'api' as const,
        functionName,
        timestamp: Date.now(),
      };

      // Navigate to the function
      this.navigate(navigationInput);
    },
  };

  // ============================================================================
  // Tests API
  // ============================================================================

  /**
   * Enrich watch notification with blockName from JSON parsing
   */
  private enrichNotification(notification: WatchNotification): WatchNotification {
    if (!notification.blockName) {
      try {
        const parsed = JSON.parse(notification.value) as { type?: string; label?: string } | undefined;
        if (parsed?.type === 'block' && typeof parsed.label === 'string') {
          notification.blockName = parsed.label;
        }
      } catch { }
    }
    return notification;
  }

  tests = {
    /**
     * Run multiple tests (sequential or parallel)
     *
     * The SDK automatically manages all test state:
     * - Creates test history run
     * - Updates areTestsRunningAtom
     * - Tracks execution progress
     * - Handles watch notifications and highlighting
     * - Updates test state with results
     * - Streams partial responses in real-time
     *
     * UI components just call this and read atoms - no manual state management needed!
     */
    runAll: async (
      tests: Array<{ functionName: string; testName: string }>,
      options?: {
        apiKeys?: Record<string, string>;
        parallel?: boolean;
      }
    ): Promise<void> => {
      console.log('[SDK] Running tests:', tests.length, 'parallel:', options?.parallel);

      if (!this.runtime) {
        throw new Error('SDK not initialized');
      }

      if (tests.length === 0) {
        console.warn('[SDK] No tests to run');
        return;
      }

      // Create abort controller for this test run
      const controller = new AbortController();
      this.storage.setCurrentAbortController(controller);

      // Set running state
      this.storage.setAreTestsRunning(true);
      this.storage.clearWatchNotifications();
      this.storage.clearHighlightedBlocks();
      this.storage.clearFlashRanges();

      // Create test history run with all tests
      const historyRun: testAtoms.TestHistoryRun = {
        timestamp: Date.now(),
        tests: tests.map((test) => {
          const testCases = this.runtime!.getTestCases(test.functionName);
          const testCase = testCases.find((tc) => tc.name === test.testName);
          return {
            timestamp: Date.now(),
            functionName: test.functionName,
            testName: test.testName,
            response: { status: 'queued' },
            input: testCase?.inputs,
          };
        }),
      };
      this.storage.addTestHistoryRun(historyRun);
      this.storage.setSelectedHistoryIndex(0);

      // Set first test as selected
      this.storage.setSelectedFunctionName(tests[0]!.functionName);
      this.storage.setSelectedTestCaseName(tests[0]!.testName);

      // Send telemetry
      vscode.sendTelemetry({
        action: 'run_tests',
        data: {
          num_tests: tests.length,
          parallel: options?.parallel || false,
        },
      });

      // Track watch notifications per test
      const watchNotificationsByTest: Record<string, WatchNotification[]> = {};

      // Initialize watch notifications tracking and mark all tests as running
      for (let i = 0; i < tests.length; i++) {
        const test = tests[i]!;
        const testKey = `${test.functionName}:${test.testName}`;
        watchNotificationsByTest[testKey] = [];
        // Mark as running - execution is about to begin
        this.storage.updateTestInHistory(0, i, { status: 'running' });
      }

      try {
        // Execute tests via runtime - runtime decides parallel vs sequential
        // SDK just handles callbacks uniformly
        await this.runtime.executeTests(tests, {
          apiKeys: options?.apiKeys,
          abortSignal: controller.signal,
          parallel: options?.parallel,

          // Called when a partial response is received during streaming
          onPartialResponse: (functionName, testName, partial) => {
            console.log('[SDK] onPartialResponse:', functionName, testName, partial);
            const testIndex = tests.findIndex(t =>
              t.functionName === functionName && t.testName === testName
            );
            if (testIndex !== -1) {
              const testKey = `${functionName}:${testName}`;
              this.storage.updateTestInHistory(0, testIndex, {
                status: 'running',
                response: partial,
                watchNotifications: watchNotificationsByTest[testKey] || [],
              });
            }
          },

          // Called when a test completes
          onTestComplete: (functionName, testName, response, status, latencyMs) => {
            console.log('[SDK] onTestComplete:', functionName, testName, status);
            const testIndex = tests.findIndex(t =>
              t.functionName === functionName && t.testName === testName
            );
            if (testIndex !== -1) {
              const testKey = `${functionName}:${testName}`;
              this.storage.updateTestInHistory(0, testIndex, {
                status: 'done',
                response,
                response_status: status,
                latency_ms: latencyMs,
                watchNotifications: watchNotificationsByTest[testKey] || [],
              });
            }
          },

          // Called when a watch notification is received
          onWatchNotification: (notification) => {
            const enriched = this.enrichNotification(notification);
            this.storage.addWatchNotification(enriched);
            if (enriched.blockName) {
              this.storage.addHighlightedBlock(enriched.blockName);
            }
          },

          // Called when code should be highlighted
          onHighlight: (spans) => {
            try {
              const vscodeSpans = spans.map((span) => ({
                file_path: span.filePath,
                start_line: span.startLine,
                start: span.start,
                end_line: span.endLine,
                end: span.end,
              }));
              vscode.setFlashingRegions(vscodeSpans);
              this.storage.setFlashRanges(spans.map((span) => ({
                filePath: span.filePath,
                startLine: span.startLine,
                startCol: span.start,
                endLine: span.endLine,
                endCol: span.end,
              })));
            } catch (e) {
              console.error('[SDK] Failed to set flashing regions:', e);
            }
          },
        });
      } catch (e) {
        console.error('[SDK] Test execution error:', e);
        const err = e instanceof Error ? e : new Error(String(e));

        // Update all running/queued tests to error
        tests.forEach((_, i) => {
          this.storage.updateTestInHistory(0, i, {
            status: 'error',
            message: err.message,
          });
        });
      } finally {
        this.storage.setAreTestsRunning(false);
        this.storage.setCurrentAbortController(null);
      }
    },

    /**
     * Cancel currently running tests
     */
    cancel: (): void => {
      console.log('[SDK] Cancelling tests');
      const controller = this.storage.getCurrentAbortController();
      if (controller) {
        controller.abort();
        this.storage.setCurrentAbortController(null);
        this.storage.setAreTestsRunning(false);
      } else {
        console.warn('[SDK] No active tests to cancel');
      }
    },
  };

  // ============================================================================
  // Private: Run execution and update storage based on runtime events
  // ============================================================================

  private async runExecution(
    executionId: string,
    workflowId: string,
    inputs: Record<string, any>,
    options?: { clearCache?: boolean; startFromNodeId?: string }
  ) {
    if (!this.runtime) {
      console.error('Cannot run execution: runtime not initialized');
      return;
    }

    const controller = new AbortController();
    this.activeExecutions.set(executionId, controller);

    try {
      // Execute via runtime (stateless)
      for await (const event of this.runtime.executeWorkflow(workflowId, inputs, options)) {
        if (controller.signal.aborted) break;

        // Update storage based on event
        this.handleExecutionEvent(executionId, event);
      }

      // Mark execution as completed
      this.storage.updateExecution(executionId, {
        status: 'completed',
        duration: Date.now() - this.storage.getExecutions(workflowId).find((e) => e.id === executionId)!.timestamp,
      });
    } catch (error) {
      // Mark execution as error
      this.storage.updateExecution(executionId, {
        status: 'error',
        error: error as Error,
      });
    } finally {
      this.activeExecutions.delete(executionId);
    }
  }

  /**
   * Handle execution events from runtime and update storage
   * This is the key method that translates runtime events to state updates
   */
  private handleExecutionEvent(executionId: string, event: any) {
    switch (event.type) {
      case 'node.enter':
        console.log(`▶️ Node started: ${event.nodeId}`);
        this.storage.setNodeState(event.nodeId, 'running');

        // Create preliminary node execution entry
        this.storage.addNodeExecution(executionId, event.nodeId, {
          nodeId: event.nodeId,
          executionId,
          state: 'running',
          inputs: event.inputs,
          outputs: undefined,
          logs: [],
          startTime: Date.now(),
          endTime: undefined,
          duration: undefined,
        });
        break;

      case 'node.exit':
        if (event.error) {
          console.error(`❌ Node error: ${event.nodeId}`);
          this.storage.setNodeState(event.nodeId, 'error');

          const errorNode = this.storage.getNodeExecution(executionId, event.nodeId);
          this.storage.addNodeExecution(executionId, event.nodeId, {
            ...errorNode!,
            state: 'error',
            error: event.error,
            endTime: Date.now(),
            duration: event.duration,
          });
        } else {
          console.log(`✅ Node completed: ${event.nodeId}`);
          this.storage.setNodeState(event.nodeId, 'success');

          // Update node execution with results
          const existingNode = this.storage.getNodeExecution(executionId, event.nodeId);
          this.storage.addNodeExecution(executionId, event.nodeId, {
            ...existingNode!,
            state: 'success',
            outputs: event.outputs,
            endTime: Date.now(),
            duration: event.duration,
          });
        }
        break;

      case 'cache.hit':
        console.log(`💾 Node cached: ${event.nodeId}`);
        this.storage.setNodeState(event.nodeId, 'cached');
        break;

      case 'log':
        // Add log to node execution
        const node = this.storage.getNodeExecution(executionId, event.nodeId);
        if (node) {
          this.storage.addNodeExecution(executionId, event.nodeId, {
            ...node,
            logs: [...node.logs, event.message],
          });
        }
        break;
    }
  }

  // ============================================================================
  // Cleanup
  // ============================================================================

  dispose(): void {
    // Cancel all running executions
    for (const controller of this.activeExecutions.values()) {
      controller.abort();
    }
    this.activeExecutions.clear();
  }
}
