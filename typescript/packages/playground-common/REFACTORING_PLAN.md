# Playground Common Refactoring Plan

## Overview
Refactor `typescript/packages/playground-common` from a complex, deeply nested structure with 50+ scattered Jotai atoms to a **flat, maintainable React architecture**.

## 🚨 Current Problems

### 1. **Deeply Nested Structure**
```
❌ Current nightmare:
shared/baml-project-panel/playground-panel/prompt-preview/test-panel/components/SimpleTestResultView.tsx
shared/baml-project-panel/playground-panel/atoms.ts
shared/baml-project-panel/playground-panel/atoms-orch-graph.ts
baml_wasm_web/EventListener.tsx (288 lines of mixed concerns)
```

### 2. **Atom Hell**
- 50+ Jotai atoms scattered across files
- Complex interdependencies (`wasmAtom` → `runtimeAtom` → `diagnosticsAtom`)
- No clear ownership or boundaries
- Examples: `flashRangesAtom`, `selectedFunctionAtom`, `runningTestsAtom`

### 3. **God Components**
- `EventListener.tsx`: 288 lines handling VSCode, state, effects
- `test-runner.ts`: 464 lines with complex hooks and state management
- Mixed UI, business logic, and side effects

## 🎯 Solution: Flat & Simple

### New Structure
```
src/
├── components/           # All React components (flat)
├── hooks/               # All custom hooks (flat)
├── contexts/            # React contexts (4-5 max)
├── services/            # Business logic classes (flat)
├── utils/               # Pure utilities (flat)
├── types.ts             # All TypeScript interfaces
└── index.ts             # Clean barrel exports
```

## 📋 Concrete Refactoring Steps

### Phase 1: Flatten Directory Structure (Week 1)

#### Step 1.1: Move Components
```bash
# From deeply nested paths to flat structure
mv shared/baml-project-panel/playground-panel/prompt-preview/test-panel/components/SimpleTestResultView.tsx → components/test-result-view.tsx
mv shared/baml-project-panel/playground-panel/prompt-preview/test-panel/components/TabularView.tsx → components/test-tabular-view.tsx
mv shared/baml-project-panel/playground-panel/prompt-preview/test-panel/index.tsx → components/test-panel.tsx
```

#### Step 1.2: Update All Imports
```typescript
// ❌ Before
import { SimpleTestResultView } from '../shared/baml-project-panel/playground-panel/prompt-preview/test-panel/components/SimpleTestResultView'

// ✅ After
import { TestResultView } from '../components/test-result-view'
```

### Phase 2: Replace Atom Hell with Simple Contexts (Week 2)

#### Step 2.1: Create Core Contexts
```typescript
// contexts/runtime-context.tsx
interface RuntimeState {
  wasm?: WasmRuntime;
  project?: WasmProject;
  diagnostics: WasmDiagnosticError[];
  isReady: boolean;
}

export const RuntimeContext = createContext<RuntimeState>();
export const useRuntime = () => useContext(RuntimeContext);
```

#### Step 2.2: Replace Atom Groups
```typescript
// ❌ Before: Multiple scattered atoms
export const wasmAtom = unwrap(wasmAtomAsync);
export const projectAtom = atom((get) => { /* complex logic */ });
export const runtimeAtom = atom<{rt: WasmRuntime}>((get) => { /* more complexity */ });
export const diagnosticsAtom = atom((get) => { /* even more */ });

// ✅ After: Single context with clear state
export function RuntimeProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(runtimeReducer, initialState);
  return (
    <RuntimeContext.Provider value={{ state, dispatch }}>
      {children}
    </RuntimeContext.Provider>
  );
}
```

#### Step 2.3: Test Context
```typescript
// contexts/test-context.tsx
interface TestState {
  runningTests: TestExecution[];
  history: TestHistoryRun[];
  selectedHistoryIndex: number;
  isRunning: boolean;
}

export const useTest = () => {
  const context = useContext(TestContext);
  if (!context) throw new Error('useTest must be used within TestProvider');
  return context;
};
```

### Phase 3: Extract Business Logic (Week 3)

#### Step 3.1: Create Service Classes
```typescript
// services/test-service.ts
export class TestService {
  static async runTest(runtime: WasmRuntime, testCase: TestCase): Promise<TestResult> {
    // Move all the complex logic from test-runner.ts here
    const startTime = performance.now();
    const result = await testCase.fn.run_test_with_expr_events(/* ... */);
    return { result, latency: performance.now() - startTime };
  }

  static async runParallelTests(runtime: WasmRuntime, tests: TestCase[]): Promise<TestResult[]> {
    // Move parallel test logic here
  }
}
```

#### Step 3.2: Create Custom Hooks
```typescript
// hooks/use-test-runner.ts
export function useTestRunner() {
  const { runtime } = useRuntime();
  const { dispatch } = useTest();

  const runTests = useCallback(async (tests: TestCase[]) => {
    dispatch({ type: 'START_TESTS', tests });

    try {
      const results = await TestService.runTests(runtime, tests);
      dispatch({ type: 'TESTS_COMPLETE', results });
    } catch (error) {
      dispatch({ type: 'TESTS_ERROR', error });
    }
  }, [runtime, dispatch]);

  return { runTests };
}
```

#### Step 3.3: VSCode Integration Hook
```typescript
// hooks/use-vscode.ts
export function useVSCode() {
  const postMessage = useCallback((message: VSCodeMessage) => {
    if (vscode.isVscode()) {
      vscode.postMessage(message);
    }
  }, []);

  const useMessageHandler = useCallback((handler: MessageHandler) => {
    useEffect(() => {
      const listener = (event: MessageEvent) => {
        handler(event.data);
      };
      window.addEventListener('message', listener);
      return () => window.removeEventListener('message', listener);
    }, [handler]);
  }, []);

  return { postMessage, useMessageHandler };
}
```

### Phase 4: Simplify Components (Week 4)

#### Step 4.1: Break Down EventListener.tsx
```typescript
// ❌ Before: 288 lines of mixed concerns
export const EventListener: React.FC = ({ children }) => {
  // VSCode integration
  // State management
  // Effect handling
  // UI rendering
  // Error handling
  // All mixed together!
};

// ✅ After: Separate concerns
// components/vscode-handler.tsx
export function VSCodeHandler() {
  const { useMessageHandler } = useVSCode();
  const { dispatch } = useRuntime();

  useMessageHandler(useCallback((message) => {
    switch (message.command) {
      case 'add_project':
        dispatch({ type: 'SET_FILES', files: message.content.files });
        break;
      // Handle other messages
    }
  }, [dispatch]));

  return null; // Pure side effect component
}

// components/runtime-initializer.tsx
export function RuntimeInitializer() {
  const { dispatch } = useRuntime();

  useEffect(() => {
    const initWasm = async () => {
      const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');
      dispatch({ type: 'WASM_READY', wasm });
    };
    initWasm();
  }, [dispatch]);

  return null;
}

// components/app-root.tsx
export function AppRoot({ children }: { children: ReactNode }) {
  return (
    <RuntimeProvider>
      <TestProvider>
        <VSCodeHandler />
        <RuntimeInitializer />
        {children}
      </TestProvider>
    </RuntimeProvider>
  );
}
```

#### Step 4.2: Simplify Test Components
```typescript
// ❌ Before: Complex component with mixed concerns
const TestPanel = () => {
  const [selectedHistoryIndex, setSelectedHistoryIndex] = useAtom(selectedHistoryIndexAtom);
  const testHistory = useAtomValue(testHistoryAtom);
  const viewType = useAtomValue(testPanelViewTypeAtom);
  // More atom usage...
};

// ✅ After: Clean component with clear dependencies
export function TestPanel() {
  const { history, selectedIndex, viewType } = useTest();
  const currentRun = history[selectedIndex];

  if (!currentRun) {
    return <EmptyTestState />;
  }

  return (
    <div>
      <TestMenu />
      <TestResults results={currentRun.tests} />
    </div>
  );
}
```

### Phase 5: Clean Up & Optimize (Week 5)

#### Step 5.1: Remove Dead Code
- Delete all unused atom files
- Remove complex atom composition
- Clean up old nested directories

#### Step 5.2: Optimize Performance
```typescript
// Add React.memo where needed
export const TestResults = React.memo(({ results }: { results: TestResult[] }) => {
  // Expensive rendering logic
});

// Use proper dependency arrays
const memoizedResults = useMemo(
  () => results.filter(r => r.status === 'completed'),
  [results] // Clear, single dependency
);
```

## 📊 Success Metrics

### Before Refactor
- **50+ Jotai atoms** scattered across files
- **6 levels of nesting** in directory structure
- **288 lines** in EventListener.tsx
- **464 lines** in test-runner.ts
- **Complex import paths** (40+ characters)

### After Refactor
- **4-5 React contexts** maximum
- **2 levels max** directory nesting
- **<100 lines** per component
- **Clear separation** of concerns
- **Simple imports** (`../components/test-panel`)

## 📅 Detailed Timeline

### Week 1: Flatten Structure
- **Day 1-2**: Move all components to flat structure
- **Day 3-4**: Update all import statements
- **Day 5**: Test that everything still works

### Week 2: Replace Atoms
- **Day 1-2**: Create runtime-context.tsx and test-context.tsx
- **Day 3-4**: Replace atom usage in components
- **Day 5**: Remove unused atoms

### Week 3: Extract Logic
- **Day 1-2**: Create service classes
- **Day 3-4**: Extract custom hooks
- **Day 5**: Move business logic out of components

### Week 4: Component Refactor
- **Day 1-2**: Break down EventListener.tsx
- **Day 3-4**: Simplify other large components
- **Day 5**: Add proper error boundaries

### Week 5: Polish
- **Day 1-2**: Remove dead code
- **Day 3-4**: Performance optimization
- **Day 5**: Documentation and final testing

## 🚀 Quick Start: First Steps Today

1. **Create the new flat directories**:
```bash
mkdir -p src/{components,hooks,contexts,services,utils}
```

2. **Move one component** as a proof of concept:
```bash
mv shared/baml-project-panel/playground-panel/prompt-preview/test-panel/components/SimpleTestResultView.tsx src/components/test-result-view.tsx
```

3. **Create first context**:
```bash
touch src/contexts/runtime-context.tsx
```

4. **Start extracting VSCode logic**:
```bash
touch src/hooks/use-vscode.ts
```

## 📝 File Naming Convention

All files should use **dash-case** (kebab-case) naming:

```typescript
// ✅ Components
test-panel.tsx
prompt-preview.tsx
code-editor.tsx

// ✅ Hooks
use-test-runner.ts
use-baml-runtime.ts
use-vscode.ts

// ✅ Services
test-service.ts
baml-service.ts
vscode-service.ts

// ✅ Contexts
runtime-context.tsx
test-context.tsx
ui-context.tsx

// ✅ Utils
file-utils.ts
format-utils.ts
```

This approach will transform the codebase from a maintenance nightmare into a clean, navigable React application that any developer can quickly understand and contribute to!