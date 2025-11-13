# Playground Navigation System Design

## Executive Summary

This document proposes a redesigned navigation system for the BAML Playground that addresses current complexity, improves maintainability, and provides a clearer mental model for reasoning about user interactions.

**Current State:** Navigation works but has grown organically complex with multiple entry points, implicit state dependencies, and heuristics that handle edge cases through priority-based fallbacks.

**Proposed State:** A state machine-based architecture with explicit transitions, clearer separation of concerns, and a resolver pattern that makes navigation decisions testable and predictable.

**Key Innovation:** In workflow mode, track both the selected node AND the function it calls:
```typescript
{
  mode: 'workflow',
  workflowId: 'ConditionalWorkflow',
  selectedNodeId: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
  functionName: 'ValidateInput',  // ← Function called by this node
  testName: 'test1'
}
```

This enables the UI to show function-specific details (prompt, tests, signature) even when viewing a function call within a workflow graph.

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Current Architecture Analysis](#current-architecture-analysis)
3. [Design Principles](#design-principles)
4. [Proposed Architecture](#proposed-architecture)
5. [Implementation Plan](#implementation-plan)
6. [Migration Strategy](#migration-strategy)
7. [Testing Strategy](#testing-strategy)

---

## Problem Statement

### What We're Solving

Users interact with the BAML Playground through multiple surfaces:
- **IDE cursor movement** → "I'm editing this function"
- **Debug panel clicks** → "I want to inspect this function/test"
- **Graph node clicks** → "I want to see this node's details"
- **Test panel clicks** → "I want to run/view this test"
- **Sidebar navigation** → "I want to view this file's functions"

Each interaction should result in a coherent UI state showing the right content in the right panels.

### Current Pain Points

1. **Hidden Complexity in Heuristics**
   - Priority-based decision tree with 4 levels of fallback
   - Unclear when Priority 1 vs Priority 2 will be chosen
   - Hard to predict behavior when clicking a function that exists in multiple workflows

2. **Expression Functions Not Tracked**
   - WASM runtime doesn't expose which functions are called within expression nodes
   - Can't navigate from LLM function → workflow that uses it
   - Tests are skipped because this is known to be broken

3. **Two-Stage Selection Process**
   - Heuristic determines mode (workflow/function/empty)
   - Dispatcher adds test auto-selection afterward
   - Test selection logic is duplicated in two places

4. **Node ID Resolution Fragility**
   - Three different ways to identify a node (ID, label, function name prefix)
   - No clear rule for which to use
   - `resolveNodeId` tries all three and hopes one works

5. **Timing Dependencies**
   - Switching workflows requires polling to wait for graph to render
   - 100ms delays with retry logic
   - No guarantee that pan-to-node will succeed

6. **Multiple Atoms to Synchronize**
   - `unifiedSelectionStateAtom`, `activeTabAtom`, `selectedInputSourceAtom`, `detailPanelAtom`, ReactFlow store
   - Easy to miss updating one
   - No transaction-like guarantee that all update together

7. **Unclear Intent Tracking**
   - `NavigationIntent` stores the original event, but isn't used for decision-making
   - Useful for debugging but adds cognitive overhead

---

## Current Architecture Analysis

### Data Flow Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      NAVIGATION SOURCES                          │
├─────────────┬──────────────┬─────────────┬──────────────────────┤
│ IDE Cursor  │ Debug Panel  │ Graph Click │ Test Panel           │
│ (line/col)  │ (function)   │ (node)      │ (test)               │
└──────┬──────┴──────┬───────┴──────┬──────┴──────┬───────────────┘
       │             │               │             │
       v             v               v             v
┌─────────────────────────────────────────────────────────────────┐
│                        BamlRuntime                               │
│  updateCursor() → WASM get_function_at_position()               │
│  Returns: { functionName, testCaseName }                        │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               v
┌─────────────────────────────────────────────────────────────────┐
│                   Navigation Dispatcher                          │
│  1. Clear pending timeouts                                       │
│  2. Store navigationIntentAtom (debugging)                       │
│  3. Build current context from atoms                             │
│  4. Call determineNavigationAction()                             │
│  5. Call applyNavigationAction()                                 │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               v
┌─────────────────────────────────────────────────────────────────┐
│                   Navigation Heuristic                           │
│  Priority 0: Direct workflow node selection                      │
│  Priority 1: Stay in current workflow                            │
│  Priority 2: Find workflow containing function                   │
│  Priority 3: Show function in isolation                          │
│  Priority 4: Empty state                                         │
│  Returns: SelectionState                                         │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               v
┌─────────────────────────────────────────────────────────────────┐
│                    Apply Action (Dispatcher)                     │
│  - Auto-select first test if not specified                       │
│  - Update unifiedSelectionStateAtom                              │
│  - Update activeTabAtom                                          │
│  - Update selectedInputSourceAtom                                │
│  - Update detailPanelAtom                                        │
│  - Schedule pan-to-node (workflow mode)                          │
└──────────────────────────────┬──────────────────────────────────┘
                               │
                               v
┌─────────────────────────────────────────────────────────────────┐
│                      UI Updates                                  │
│  - Graph re-renders with selected node                           │
│  - Detail panel opens/closes                                     │
│  - Test panel highlights selected test                           │
│  - Tab switches to preview/graph                                 │
└─────────────────────────────────────────────────────────────────┘
```

### State Representation

```typescript
// Current selection state (single source of truth)
type SelectionState =
  | {
      mode: 'workflow'
      workflowId: string
      selectedNodeId: string
      functionName: string | null  // Function called by this node (if any)
      testName: string | null
    }
  | {
      mode: 'function'
      functionName: string
      testName: string | null
    }
  | { mode: 'empty' }

// Navigation event (user action)
type CodeClickEvent =
  | { type: 'function'; functionName; functionType; filePath; workflowId?; nodeId? }
  | { type: 'test'; testName; functionName; filePath; nodeType }

// Navigation intent (event + metadata)
type NavigationIntent = CodeClickEvent & { source?: NavigationSource }
```

### Complexity Metrics

**Lines of Code:**
- `navigationHeuristic.ts`: ~560 lines
- `dispatcher.ts`: ~230 lines
- `provider.tsx`: ~180 lines (navigation hooks)
- Total: ~970 lines of navigation logic

**Decision Points:**
- Heuristic has 4 priority levels × 2 event types = 8 code paths
- Dispatcher has 3 mode applications × 2 branches (with/without test) = 6 code paths
- Total: 14 major decision points

**Dependencies:**
- 6 atoms read during navigation
- 5 atoms written during navigation
- 1 external store (ReactFlow)
- 1 WASM runtime call

---

## Design Principles

### 1. Explicit Over Implicit

**Before:** Priority-based fallbacks with implicit context checking

**After:** Explicit transition functions with clear preconditions

```typescript
// ❌ Before: Priority 1 vs Priority 2 is unclear
if (functionExistsInWorkflow(activeWorkflowId, functionName)) {
  // Stay in workflow (Priority 1)
} else {
  const workflow = findWorkflowContainingFunction(functionName);
  if (workflow) {
    // Switch to workflow (Priority 2)
  }
}

// ✅ After: Clear transition with explicit preconditions
transition({
  from: currentState,
  event: 'selectFunction',
  to: determineTargetState(functionName, currentContext),
  guards: ['functionExists', 'workflowContainsFunction']
})
```

### 2. Single Responsibility

**Before:** Dispatcher handles timing, test selection, atom updates, and graph panning

**After:** Separate concerns into focused modules:
- **Navigator**: Determines target state
- **StateManager**: Updates atoms transactionally
- **GraphSynchronizer**: Handles timing and graph updates
- **TestSelector**: Resolves test selection

### 3. Testability

**Before:** Tests require full Jotai setup with WASM runtime

**After:** Pure functions that can be tested in isolation

```typescript
// ✅ Pure function, easy to test
function resolveTargetState(
  event: NavigationEvent,
  context: NavigationContext
): NavigationState {
  // No side effects, returns new state
}
```

### 4. Predictability

**Before:** Can't easily predict what will happen when clicking a function

**After:** Decision tree visualizable as state machine diagram

### 5. Debuggability

**Before:** `navigationIntentAtom` stores history but isn't actionable

**After:** Navigation log with timestamps, decisions, and state diffs

---

## Proposed Architecture

### Overview

Replace the priority-based heuristic with a **Navigation Resolver** that uses a **Decision Matrix** to determine state transitions. Separate state determination from state application.

### Core Components

```
┌─────────────────────────────────────────────────────────────────┐
│                    NAVIGATION PIPELINE                           │
└─────────────────────────────────────────────────────────────────┘

1. Event Capture
   ↓
2. Entity Resolution (BamlRuntime)
   ↓
3. Context Building (gather current state)
   ↓
4. Decision Matrix (determine target state)
   ↓
5. State Validation (ensure valid transition)
   ↓
6. Transaction Application (update all atoms)
   ↓
7. Side Effects (graph pan, panel open)
   ↓
8. Logging (record transition)
```

### 1. Navigation Event Model

```typescript
// Core navigation primitives
type EntityReference =
  | { kind: 'function'; name: string }
  | { kind: 'test'; functionName: string; testName: string }
  | { kind: 'node'; workflowId: string; nodeId: string }

type NavigationEvent = {
  entity: EntityReference
  context: {
    source: 'cursor' | 'debug-panel' | 'graph' | 'test-panel' | 'sidebar'
    filePath?: string
    cursorPosition?: { line: number; column: number }
  }
  timestamp: number
}

// Resolved entity (after WASM lookup)
type ResolvedEntity = EntityReference & {
  functionType: FunctionType
  availableTests: string[]
  memberOfWorkflows: WorkflowMembership[]
  exists: boolean
}

type WorkflowMembership = {
  workflowId: string
  nodeId: string
  nodeType: NodeType
  nodeLabel: string           // Human-readable label
  calledFunction: string | null  // Function called by this node (if any)
  callPath: string[]          // For nested functions
}
```

**Key Improvement:** Separate the raw event from the resolved entity. WASM tells us what exists, Decision Matrix determines what to show.

**Why track `functionName` in workflow mode?**

When a workflow node represents a function call (e.g., `ValidateInput(task_summary)`), the UI needs to know:
1. **Which workflow** to display → `workflowId`
2. **Which node** is selected → `selectedNodeId`
3. **Which function** the node calls → `functionName` (if any)

This enables:
- **Detail panel:** Show function-specific info (signature, return type, LLM client)
- **Prompt preview:** Display the actual prompt for the called function
- **Test selection:** Run tests for the specific function being called
- **Click-through:** Allow user to jump from workflow node → function in isolation
- **Breadcrumbs:** Show path like `ConditionalWorkflow > ValidateInput > test1`

Without `functionName`, we only know which node is selected (e.g., `ConditionalWorkflow|root:0|hdr:validate-payload-structure:0`), but not that it calls `ValidateInput`.

### 2. Navigation Context

```typescript
type NavigationContext = {
  // Current state
  current: SelectionState

  // Available options
  workflows: WorkflowInfo[]
  functions: FunctionInfo[]

  // Preferences (for context preservation)
  preferences: {
    stickToWorkflow: boolean    // Try to stay in current workflow
    autoSelectTest: boolean      // Auto-select first test
    preferWorkflowView: boolean  // Show workflow over function when ambiguous
  }

  // Capabilities (what can we display?)
  capabilities: {
    hasGraphView: boolean
    hasTestPanel: boolean
  }
}
```

**Key Improvement:** Make context explicit rather than querying atoms throughout the code.

### 3. Decision Matrix

Replace the priority-based heuristic with a **Decision Matrix** that maps (event, context) → target state.

```typescript
type DecisionRule = {
  name: string
  priority: number // Lower = higher priority
  condition: (event: ResolvedEntity, context: NavigationContext) => boolean
  resolver: (event: ResolvedEntity, context: NavigationContext) => NavigationState
}

const DECISION_RULES: DecisionRule[] = [
  {
    name: 'DirectNodeSelection',
    priority: 0,
    condition: (event, ctx) =>
      event.kind === 'node' &&
      ctx.workflows.some(w => w.id === event.workflowId),
    resolver: (event, ctx) => {
      const workflow = ctx.workflows.find(w => w.id === event.workflowId)!
      const node = workflow.nodes.find(n => n.id === event.nodeId)
      const calledFunction = extractCalledFunction(node)  // Extract from node metadata

      return {
        mode: 'workflow',
        workflowId: event.workflowId,
        selectedNodeId: event.nodeId,
        functionName: calledFunction,  // null for group/conditional nodes
        testName: resolvePreferredTest(event, ctx)
      }
    }
  },

  {
    name: 'TestSelection',
    priority: 1,
    condition: (event, ctx) => event.kind === 'test',
    resolver: (event, ctx) => {
      const targetFunction = ctx.functions.find(f => f.name === event.functionName)
      if (!targetFunction) return { mode: 'empty' }

      // Check if function is in a workflow
      const workflow = targetFunction.memberOfWorkflows[0]
      if (workflow && ctx.preferences.preferWorkflowView) {
        return {
          mode: 'workflow',
          workflowId: workflow.workflowId,
          selectedNodeId: workflow.nodeId,
          functionName: workflow.calledFunction,  // Function called by this node
          testName: event.testName
        }
      }

      return {
        mode: 'function',
        functionName: event.functionName,
        testName: event.testName
      }
    }
  },

  {
    name: 'StayInCurrentWorkflow',
    priority: 2,
    condition: (event, ctx) =>
      event.kind === 'function' &&
      ctx.current.mode === 'workflow' &&
      ctx.preferences.stickToWorkflow &&
      event.memberOfWorkflows.some(m => m.workflowId === ctx.current.workflowId),
    resolver: (event, ctx) => {
      const membership = event.memberOfWorkflows.find(
        m => m.workflowId === ctx.current.workflowId
      )!
      return {
        mode: 'workflow',
        workflowId: ctx.current.workflowId,
        selectedNodeId: membership.nodeId,
        functionName: membership.calledFunction,  // Function called by this node
        testName: resolvePreferredTest(event, ctx)
      }
    }
  },

  {
    name: 'SwitchToWorkflowContainingFunction',
    priority: 3,
    condition: (event, ctx) =>
      event.kind === 'function' &&
      event.memberOfWorkflows.length > 0 &&
      ctx.preferences.preferWorkflowView,
    resolver: (event, ctx) => {
      const membership = event.memberOfWorkflows[0]
      return {
        mode: 'workflow',
        workflowId: membership.workflowId,
        selectedNodeId: membership.nodeId,
        functionName: membership.calledFunction,  // Function called by this node
        testName: resolvePreferredTest(event, ctx)
      }
    }
  },

  {
    name: 'ShowFunctionInIsolation',
    priority: 4,
    condition: (event, ctx) =>
      event.kind === 'function' &&
      event.exists,
    resolver: (event, ctx) => ({
      mode: 'function',
      functionName: event.name,
      testName: resolvePreferredTest(event, ctx)
    })
  },

  {
    name: 'EmptyState',
    priority: 999,
    condition: () => true, // Catch-all
    resolver: () => ({ mode: 'empty' })
  }
]

function resolveNavigation(
  event: ResolvedEntity,
  context: NavigationContext
): NavigationState {
  // Find first matching rule
  const rule = DECISION_RULES
    .sort((a, b) => a.priority - b.priority)
    .find(rule => rule.condition(event, context))

  if (!rule) {
    console.error('No rule matched navigation event')
    return { mode: 'empty' }
  }

  console.debug(`Applied rule: ${rule.name}`)
  return rule.resolver(event, context)
}

// Helper: Extract which function a node calls (if any)
function extractCalledFunction(node: WorkflowNode | undefined): string | null {
  if (!node) return null

  // For now, we parse it from the node label or metadata
  // FUTURE: WASM should expose this directly in node.functionCalls[]

  // Method 1: Check if node has explicit functionCalls metadata (FUTURE)
  if (node.metadata?.functionCalls?.length > 0) {
    return node.metadata.functionCalls[0].functionName
  }

  // Method 2: Parse from label for function call nodes
  // e.g., "if (CheckCondition(validation.summary))" -> "CheckCondition"
  const functionCallMatch = node.label?.match(/(\w+)\s*\(/)?.[1]
  if (functionCallMatch) {
    return functionCallMatch
  }

  // Method 3: For simple assignment nodes
  // e.g., "validate payload structure" might correspond to "ValidateInput" call
  // This requires WASM to expose the actual function call

  return null
}
```

**Key Improvements:**
- Rules are named and self-documenting
- Easy to add/remove/reorder rules
- Each rule's condition is explicit
- Can log which rule was applied for debugging
- Rules can be tested individually

### 4. Test Resolution

Extract test selection into its own module:

```typescript
type TestResolutionStrategy =
  | 'explicit'       // Test name provided in event
  | 'preserve'       // Keep currently selected test if valid
  | 'first'          // Select first available test
  | 'most-recent'    // Select most recently run test
  | 'none'           // No test selected

type TestResolutionContext = {
  requestedTest: string | null
  currentTest: string | null
  availableTests: string[]
  recentlyRunTests: string[]
}

function resolvePreferredTest(
  entity: ResolvedEntity,
  navContext: NavigationContext
): string | null {
  const testCtx: TestResolutionContext = {
    requestedTest: entity.kind === 'test' ? entity.testName : null,
    currentTest: navContext.current.mode !== 'empty' ? navContext.current.testName : null,
    availableTests: entity.availableTests,
    recentlyRunTests: getRecentlyRunTests(entity)
  }

  // Strategy 1: Explicit request
  if (testCtx.requestedTest) {
    if (testCtx.availableTests.includes(testCtx.requestedTest)) {
      return testCtx.requestedTest
    }
    console.warn(`Requested test "${testCtx.requestedTest}" not found`)
  }

  // Strategy 2: Preserve current test if still valid
  if (navContext.preferences.autoSelectTest && testCtx.currentTest) {
    if (testCtx.availableTests.includes(testCtx.currentTest)) {
      return testCtx.currentTest
    }
  }

  // Strategy 3: Most recently run test
  if (navContext.preferences.autoSelectTest && testCtx.recentlyRunTests.length > 0) {
    const recent = testCtx.recentlyRunTests.find(t =>
      testCtx.availableTests.includes(t)
    )
    if (recent) return recent
  }

  // Strategy 4: First available test
  if (navContext.preferences.autoSelectTest && testCtx.availableTests.length > 0) {
    return testCtx.availableTests[0]
  }

  // Strategy 5: None
  return null
}
```

**Key Improvements:**
- Test selection logic in one place
- Clear strategy priority
- Can preserve current test when switching functions
- Can prefer recently-run tests (better UX)

### 5. State Manager (Transactional Updates)

```typescript
type StateTransaction = {
  selection: SelectionState
  sideEffects: SideEffect[]
}

type SideEffect =
  | { type: 'openDetailPanel' }
  | { type: 'closeDetailPanel' }
  | { type: 'switchTab'; tab: 'preview' | 'curl' | 'graph' }
  | { type: 'panToNode'; workflowId: string; nodeId: string }
  | { type: 'selectInputSource'; testName: string }
  | { type: 'clearInputSource' }

function buildTransaction(
  targetState: NavigationState,
  context: NavigationContext
): StateTransaction {
  const sideEffects: SideEffect[] = []

  // Determine which tab to show
  if (targetState.mode === 'workflow') {
    sideEffects.push({ type: 'switchTab', tab: 'graph' })
    sideEffects.push({ type: 'openDetailPanel' })
    sideEffects.push({
      type: 'panToNode',
      workflowId: targetState.workflowId,
      nodeId: targetState.selectedNodeId
    })

    if (targetState.testName) {
      sideEffects.push({
        type: 'selectInputSource',
        testName: targetState.testName
      })
    }
  } else if (targetState.mode === 'function') {
    sideEffects.push({ type: 'switchTab', tab: 'preview' })
    sideEffects.push({ type: 'openDetailPanel' })

    if (targetState.testName) {
      sideEffects.push({
        type: 'selectInputSource',
        testName: targetState.testName
      })
    }
  } else {
    sideEffects.push({ type: 'switchTab', tab: 'preview' })
    sideEffects.push({ type: 'closeDetailPanel' })
    sideEffects.push({ type: 'clearInputSource' })
  }

  return { selection: targetState, sideEffects }
}

function applyTransaction(
  transaction: StateTransaction,
  atomSet: JotaiSet
): void {
  // 1. Update core selection state
  atomSet(unifiedSelectionStateAtom, transaction.selection)

  // 2. Apply side effects in order
  for (const effect of transaction.sideEffects) {
    switch (effect.type) {
      case 'openDetailPanel':
        atomSet(detailPanelAtom, { isOpen: true })
        break
      case 'closeDetailPanel':
        atomSet(detailPanelAtom, { isOpen: false })
        break
      case 'switchTab':
        atomSet(activeTabAtom, effect.tab)
        break
      case 'panToNode':
        scheduleGraphPan(effect.workflowId, effect.nodeId, atomSet)
        break
      case 'selectInputSource':
        atomSet(selectedInputSourceAtom, { testName: effect.testName })
        break
      case 'clearInputSource':
        atomSet(selectedInputSourceAtom, null)
        break
    }
  }
}
```

**Key Improvements:**
- Side effects are data, not imperative code
- Can validate transaction before applying
- Can log/replay transactions
- Can test transaction building separately from application

### 6. Graph Synchronizer

Handle timing issues with graph rendering:

```typescript
type GraphSyncStrategy =
  | { type: 'immediate' }  // No delay, pan immediately
  | { type: 'poll'; maxAttempts: number; intervalMs: number }  // Current approach
  | { type: 'event-based'; timeoutMs: number }  // Wait for graph ready event

class GraphSynchronizer {
  private strategy: GraphSyncStrategy

  constructor(strategy: GraphSyncStrategy = { type: 'poll', maxAttempts: 10, intervalMs: 100 }) {
    this.strategy = strategy
  }

  async panToNode(workflowId: string, nodeId: string): Promise<boolean> {
    switch (this.strategy.type) {
      case 'immediate':
        return this.attemptPan(nodeId)

      case 'poll':
        return this.pollForNode(nodeId, this.strategy.maxAttempts, this.strategy.intervalMs)

      case 'event-based':
        return this.waitForGraphReady(workflowId, nodeId, this.strategy.timeoutMs)
    }
  }

  private async pollForNode(
    nodeId: string,
    attemptsLeft: number,
    intervalMs: number
  ): Promise<boolean> {
    if (this.attemptPan(nodeId)) {
      return true
    }

    if (attemptsLeft <= 0) {
      console.warn(`Failed to pan to node ${nodeId} after polling`)
      return false
    }

    await new Promise(resolve => setTimeout(resolve, intervalMs))
    return this.pollForNode(nodeId, attemptsLeft - 1, intervalMs)
  }

  private async waitForGraphReady(
    workflowId: string,
    nodeId: string,
    timeoutMs: number
  ): Promise<boolean> {
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        graphReadyEmitter.off('ready', handler)
        console.warn(`Graph did not become ready within ${timeoutMs}ms`)
        resolve(false)
      }, timeoutMs)

      const handler = (readyWorkflowId: string) => {
        if (readyWorkflowId === workflowId) {
          clearTimeout(timeout)
          graphReadyEmitter.off('ready', handler)
          resolve(this.attemptPan(nodeId))
        }
      }

      graphReadyEmitter.on('ready', handler)
    })
  }

  private attemptPan(nodeId: string): boolean {
    // Call ReactFlow panTo
    const node = flowStore.value.getNodes?.().find(n => n.id === nodeId)
    if (!node) return false

    const { x, y } = node.position
    flowStore.value.setCenter?.(x, y, { zoom: 1, duration: 300 })
    return true
  }
}

// In graph-view.tsx, emit ready event
useEffect(() => {
  if (nodes.length > 0 && activeWorkflowId) {
    graphReadyEmitter.emit('ready', activeWorkflowId)
  }
}, [nodes, activeWorkflowId])
```

**Key Improvements:**
- Encapsulate timing logic
- Support multiple strategies (can switch based on performance)
- Event-based approach is more reliable than polling
- Returns success/failure for logging

### 7. Navigation Coordinator

The main entry point that orchestrates everything:

```typescript
class NavigationCoordinator {
  private resolver: NavigationResolver
  private stateManager: StateManager
  private graphSync: GraphSynchronizer
  private logger: NavigationLogger

  async navigate(event: NavigationEvent, atomGet: JotaiGet, atomSet: JotaiSet): Promise<void> {
    const startTime = performance.now()

    try {
      // 1. Resolve entity via WASM
      const resolved = await this.resolveEntity(event, atomGet)

      // 2. Build context
      const context = this.buildContext(atomGet)

      // 3. Determine target state
      const targetState = this.resolver.resolve(resolved, context)

      // 4. Validate transition
      if (!this.isValidTransition(context.current, targetState)) {
        console.warn('Invalid state transition', { from: context.current, to: targetState })
        return
      }

      // 5. Build transaction
      const transaction = this.stateManager.buildTransaction(targetState, context)

      // 6. Apply transaction
      this.stateManager.applyTransaction(transaction, atomSet)

      // 7. Handle async side effects
      const graphPan = transaction.sideEffects.find(e => e.type === 'panToNode')
      if (graphPan && graphPan.type === 'panToNode') {
        await this.graphSync.panToNode(graphPan.workflowId, graphPan.nodeId)
      }

      // 8. Log transition
      const duration = performance.now() - startTime
      this.logger.log({
        event,
        resolved,
        from: context.current,
        to: targetState,
        transaction,
        duration,
        timestamp: Date.now()
      })

    } catch (error) {
      console.error('Navigation failed', error)
      this.logger.logError(event, error)

      // Safe fallback: don't change state
      // OR: set to empty state
      // atomSet(unifiedSelectionStateAtom, { mode: 'empty' })
    }
  }

  private async resolveEntity(
    event: NavigationEvent,
    atomGet: JotaiGet
  ): Promise<ResolvedEntity> {
    const runtime = atomGet(bamlRuntimeAtom)

    // If entity is already specific (from debug panel), just enrich it
    if (event.entity.kind === 'test' || event.entity.kind === 'node') {
      return this.enrichEntity(event.entity, atomGet)
    }

    // For cursor events, ask WASM what was clicked
    if (event.context.source === 'cursor' && event.context.cursorPosition) {
      const { line, column } = event.context.cursorPosition
      const result = await runtime.updateCursor(line, column)
      return this.enrichEntity(
        { kind: 'function', name: result.functionName },
        atomGet
      )
    }

    // For function clicks from other sources
    return this.enrichEntity(event.entity, atomGet)
  }

  private enrichEntity(
    entity: EntityReference,
    atomGet: JotaiGet
  ): ResolvedEntity {
    const bamlFiles = atomGet(bamlFilesAtom)
    const workflows = atomGet(workflowsAtom)

    // Find the function
    const allFunctions = bamlFiles.flatMap(f => f.functions)
    const functionName = entity.kind === 'function' ? entity.name :
                         entity.kind === 'test' ? entity.functionName :
                         entity.nodeId

    const func = allFunctions.find(f => f.name === functionName)
    if (!func) {
      return { ...entity, exists: false, functionType: 'unknown', availableTests: [], memberOfWorkflows: [] }
    }

    // Find tests for this function
    const availableTests = bamlFiles
      .flatMap(f => f.tests)
      .filter(t => t.functionName === functionName)
      .map(t => t.name)

    // Find workflows containing this function
    const memberOfWorkflows: WorkflowMembership[] = []
    for (const workflow of workflows) {
      const nodes = workflow.nodes || []
      for (const node of nodes) {
        if (node.functionName === functionName || node.id === functionName || node.label === functionName) {
          memberOfWorkflows.push({
            workflowId: workflow.name,
            nodeId: node.id,
            nodeType: node.type,
            callPath: [] // TODO: compute call path
          })
        }
      }
    }

    return {
      ...entity,
      exists: true,
      functionType: func.type,
      availableTests,
      memberOfWorkflows
    }
  }

  private buildContext(atomGet: JotaiGet): NavigationContext {
    return {
      current: atomGet(unifiedSelectionStateAtom),
      workflows: atomGet(workflowsAtom).map(w => ({
        id: w.name,
        nodes: w.nodes || [],
        edges: w.edges || []
      })),
      functions: atomGet(bamlFilesAtom).flatMap(f => f.functions),
      preferences: {
        stickToWorkflow: true,
        autoSelectTest: true,
        preferWorkflowView: true
      },
      capabilities: {
        hasGraphView: true,
        hasTestPanel: true
      }
    }
  }

  private isValidTransition(from: SelectionState, to: SelectionState): boolean {
    // Define valid state transitions
    // For now, all transitions are valid
    return true
  }
}

// Main dispatcher atom
export const navigationDispatcherAtom = atom(
  null,
  async (get, set, event: NavigationEvent) => {
    const coordinator = new NavigationCoordinator(/* ... */)
    await coordinator.navigate(event, get, set)
  }
)
```

**Key Improvements:**
- Single entry point for all navigation
- Async/await for better control flow
- Error handling with fallback
- Performance logging
- Clear step-by-step process
- Separation of concerns (each step is a method)

### 8. Navigation Logger

```typescript
type NavigationLogEntry = {
  event: NavigationEvent
  resolved: ResolvedEntity
  from: SelectionState
  to: SelectionState
  transaction: StateTransaction
  appliedRule?: string
  duration: number
  timestamp: number
}

class NavigationLogger {
  private logs: NavigationLogEntry[] = []
  private maxLogs = 100

  log(entry: NavigationLogEntry): void {
    this.logs.push(entry)
    if (this.logs.length > this.maxLogs) {
      this.logs.shift()
    }

    // Pretty print for debugging
    console.groupCollapsed(
      `%cNav: ${entry.from.mode} → ${entry.to.mode}`,
      'color: #00aa00; font-weight: bold'
    )
    console.log('Event:', entry.event)
    console.log('Resolved:', entry.resolved)
    console.log('From:', entry.from)
    console.log('To:', entry.to)
    console.log('Rule:', entry.appliedRule)
    console.log('Transaction:', entry.transaction)
    console.log('Duration:', `${entry.duration.toFixed(2)}ms`)
    console.groupEnd()
  }

  logError(event: NavigationEvent, error: Error): void {
    console.error('Navigation error:', { event, error })
  }

  getHistory(): NavigationLogEntry[] {
    return [...this.logs]
  }

  exportLogs(): string {
    return JSON.stringify(this.logs, null, 2)
  }
}

// Expose logger for debugging
if (typeof window !== 'undefined') {
  (window as any).__navigationLogs = () => coordinator.logger.getHistory()
}
```

**Key Improvements:**
- Complete audit trail
- Easy to debug navigation issues
- Can replay navigation sequences
- Export for bug reports

---

## Implementation Plan

### Phase 1: Foundation (Week 1)

**Goal:** Set up new architecture without breaking existing code

1. **Create new types** (`navigation/types.ts`)
   - `EntityReference`, `ResolvedEntity`, `NavigationContext`, `StateTransaction`, `SideEffect`

2. **Implement NavigationLogger** (`navigation/logger.ts`)
   - Basic logging infrastructure
   - Console output formatting

3. **Implement DecisionMatrix** (`navigation/decision-matrix.ts`)
   - Port existing heuristic rules to new format
   - Add rule logging

4. **Write unit tests** (`navigation/__tests__/decision-matrix.test.ts`)
   - Test each rule in isolation
   - Test rule priority ordering

### Phase 2: Core Logic (Week 2)

**Goal:** Implement navigation resolution and state management

1. **Implement NavigationResolver** (`navigation/resolver.ts`)
   - Entity enrichment
   - Context building
   - Decision matrix application

2. **Implement TestResolver** (`navigation/test-resolver.ts`)
   - Test selection strategies
   - Test preservation logic

3. **Implement StateManager** (`navigation/state-manager.ts`)
   - Transaction building
   - Transaction application
   - Side effect execution

4. **Write integration tests** (`navigation/__tests__/resolver-integration.test.ts`)
   - Test full resolution pipeline
   - Test transaction building

### Phase 3: Graph Synchronization (Week 3)

**Goal:** Solve timing issues with graph rendering

1. **Implement GraphSynchronizer** (`navigation/graph-sync.ts`)
   - Event-based strategy
   - Fallback to polling
   - Success/failure tracking

2. **Add graph ready events** to `graph-view.tsx`
   - Emit when nodes are rendered
   - Emit when layout completes

3. **Update tests** to handle async graph panning
   - Mock graph synchronizer
   - Test timeout scenarios

### Phase 4: Migration (Week 4)

**Goal:** Replace old navigation system with new one

1. **Create NavigationCoordinator** (`navigation/coordinator.ts`)
   - Orchestrate all components
   - Error handling
   - Performance logging

2. **Update dispatcher atom** to use new coordinator
   - Keep same external API
   - Gradual cutover (feature flag?)

3. **Update DebugPanel** to emit new event format
   - Migrate to `EntityReference`
   - Test with new system

4. **Update graph-view** to emit new event format
   - Migrate node click handlers
   - Test with new system

5. **Run full test suite**
   - Fix any regressions
   - Update skipped tests (expression function tracking)

### Phase 5: Enhancements (Week 5)

**Goal:** Add features that were hard with old architecture

1. **Navigation history**
   - Back/forward buttons
   - Keyboard shortcuts (Cmd+[ and Cmd+])

2. **Better test selection**
   - Remember most-recently-run test
   - Persist test selection across function switches

3. **Expression function tracking**
   - Work with WASM team to expose call graph
   - Update entity enrichment to populate `memberOfWorkflows`

4. **Performance optimizations**
   - Cache entity resolution
   - Debounce rapid cursor movements
   - Memoize context building

### Phase 6: Polish (Week 6)

**Goal:** Production-ready system

1. **Documentation**
   - Architecture guide (this doc)
   - API reference for adding new rules
   - Debugging guide

2. **Developer tools**
   - Navigation inspector panel
   - Rule coverage analysis
   - Performance profiling

3. **User-facing improvements**
   - Loading states during navigation
   - Error messages when navigation fails
   - Breadcrumbs showing current location

---

## Migration Strategy

### Backward Compatibility

The new system should be a drop-in replacement:

```typescript
// Old API (keep working)
dispatchNavigation({
  type: 'function',
  functionName: 'MyFunction',
  functionType: 'llm',
  filePath: 'main.baml'
})

// New API (internal conversion)
navigationDispatcherAtom({
  entity: { kind: 'function', name: 'MyFunction' },
  context: { source: 'debug-panel', filePath: 'main.baml' },
  timestamp: Date.now()
})
```

### Feature Flag

Use a feature flag to enable the new system gradually:

```typescript
const USE_NEW_NAVIGATION = import.meta.env.VITE_NEW_NAVIGATION === 'true'

export const navigationDispatcherAtom = atom(
  null,
  async (get, set, event) => {
    if (USE_NEW_NAVIGATION) {
      return newNavigationCoordinator.navigate(event, get, set)
    } else {
      return oldNavigationDispatcher(event, get, set)
    }
  }
)
```

### Gradual Rollout

1. **Week 1-3:** New system available via feature flag, default off
2. **Week 4:** Enable for internal users, monitor logs
3. **Week 5:** Enable for 50% of users (A/B test)
4. **Week 6:** Enable for 100% of users
5. **Week 7:** Remove old system code

### Rollback Plan

If the new system has issues:
1. Set feature flag to `false`
2. Old system continues working
3. Fix issues in new system
4. Try rollout again

---

## Testing Strategy

### Unit Tests

Test each component in isolation:

```typescript
describe('DecisionMatrix', () => {
  describe('DirectNodeSelection rule', () => {
    it('should select node when workflowId and nodeId are provided', () => {
      const event: ResolvedEntity = {
        kind: 'node',
        workflowId: 'MyWorkflow',
        nodeId: 'node-123',
        exists: true,
        functionType: 'expression',
        availableTests: ['test1'],
        memberOfWorkflows: []
      }

      const context: NavigationContext = {
        current: { mode: 'empty' },
        workflows: [{ id: 'MyWorkflow', nodes: [], edges: [] }],
        functions: [],
        preferences: { stickToWorkflow: true, autoSelectTest: true, preferWorkflowView: true },
        capabilities: { hasGraphView: true, hasTestPanel: true }
      }

      const result = resolveNavigation(event, context)

      expect(result).toEqual({
        mode: 'workflow',
        workflowId: 'MyWorkflow',
        selectedNodeId: 'node-123',
        testName: 'test1'
      })
    })
  })
})

describe('TestResolver', () => {
  it('should preserve current test when switching functions', () => {
    const entity: ResolvedEntity = {
      kind: 'function',
      name: 'NewFunction',
      availableTests: ['test1', 'test2', 'current-test'],
      // ...
    }

    const context: NavigationContext = {
      current: { mode: 'function', functionName: 'OldFunction', testName: 'current-test' },
      // ...
    }

    const test = resolvePreferredTest(entity, context)
    expect(test).toBe('current-test')
  })
})
```

### Integration Tests

Test the full pipeline with real data:

```typescript
describe('NavigationCoordinator', () => {
  it('should navigate from function to workflow when function exists in workflow', async () => {
    const { get, set } = setupTestStore({
      bamlFiles: [mockBamlFile],
      workflows: [mockWorkflow],
      selection: { mode: 'empty' }
    })

    const coordinator = new NavigationCoordinator()

    await coordinator.navigate({
      entity: { kind: 'function', name: 'MyFunction' },
      context: { source: 'debug-panel' },
      timestamp: Date.now()
    }, get, set)

    const newSelection = get(unifiedSelectionStateAtom)
    expect(newSelection).toEqual({
      mode: 'workflow',
      workflowId: 'MyWorkflow',
      selectedNodeId: 'node-myfunction',
      testName: 'test1'
    })

    expect(get(activeTabAtom)).toBe('graph')
    expect(get(detailPanelAtom).isOpen).toBe(true)
  })
})
```

### E2E Tests

Test user interactions:

```typescript
describe('Navigation flows', () => {
  it('should switch from function view to workflow view when clicking a workflow node', async () => {
    // 1. Start in function mode
    await debugPanel.clickFunction('StandaloneFunction')
    expect(await screen.findByText('Function: StandaloneFunction')).toBeVisible()

    // 2. Click a workflow in the sidebar
    await sidebar.clickWorkflow('MyWorkflow')
    expect(await screen.findByText('Workflow: MyWorkflow')).toBeVisible()

    // 3. Click a node in the graph
    await graph.clickNode('node-123')
    expect(await graph.getSelectedNode()).toBe('node-123')
    expect(await detailPanel.isOpen()).toBe(true)
  })
})
```

### Regression Tests

Ensure existing tests still pass:

```bash
# Run existing selection integration tests with new system
VITE_NEW_NAVIGATION=true pnpm test selection-integration

# Run navigation heuristic tests (should all pass with new decision matrix)
pnpm test navigationHeuristic
```

### Property-Based Tests

Test that invariants hold:

```typescript
import { fc } from 'fast-check'

describe('Navigation invariants', () => {
  it('should never enter an invalid state', () => {
    fc.assert(
      fc.property(
        fc.record({
          entity: arbitraryResolvedEntity,
          context: arbitraryNavigationContext
        }),
        ({ entity, context }) => {
          const result = resolveNavigation(entity, context)

          // Invariant 1: Result is always a valid SelectionState
          expect(result.mode).toMatch(/^(workflow|function|empty)$/)

          // Invariant 2: Workflow mode must have workflowId and selectedNodeId
          if (result.mode === 'workflow') {
            expect(result.workflowId).toBeTruthy()
            expect(result.selectedNodeId).toBeTruthy()
          }

          // Invariant 3: Function mode must have functionName
          if (result.mode === 'function') {
            expect(result.functionName).toBeTruthy()
          }

          // Invariant 4: testName is either string or null
          if (result.mode !== 'empty') {
            expect(result.testName === null || typeof result.testName === 'string').toBe(true)
          }
        }
      )
    )
  })
})
```

---

## Benefits of New Architecture

### 1. Clarity

**Before:** "Why did it switch to workflow mode instead of staying in function mode?"

**After:** Check navigation log → See which rule was applied → Understand why

### 2. Testability

**Before:** Need full Jotai + WASM setup to test navigation

**After:** Test decision matrix with plain objects, no dependencies

### 3. Extensibility

**Before:** Add new priority level to heuristic, hope it doesn't break existing logic

**After:** Add new rule to decision matrix, specify priority and conditions explicitly

### 4. Debuggability

**Before:** Add `console.log` statements throughout the code

**After:** Check `__navigationLogs()` in console, see complete history

### 5. Performance

**Before:** Query atoms multiple times during navigation

**After:** Build context once, pass it through pipeline

### 6. Reliability

**Before:** Timing issues cause graph panning to fail silently

**After:** Graph synchronizer tracks success/failure, can retry or show error

---

## Future Enhancements

### 1. Smart Navigation

Use machine learning to predict user intent:
- If user often clicks function then immediately clicks test, auto-select that test
- If user frequently switches between two functions, add quick-switch button
- Learn preferred workflow view vs function view per user

### 2. Navigation Shortcuts

Keyboard shortcuts for common actions:
- `Cmd+P` → Quick open function/workflow
- `Cmd+T` → Quick open test
- `Cmd+[` and `Cmd+]` → Back and forward
- `Cmd+Shift+G` → Toggle graph view

### 3. Breadcrumbs

Show navigation path:
```
main.baml > MyWorkflow > GetUserInfo > test_happy_path
```

Click any breadcrumb to jump to that level.

### 4. Multi-Selection

Allow selecting multiple nodes to compare:
- Side-by-side view of two functions
- Diff view of test outputs
- Bulk operations (run all selected tests)

### 5. Navigation Analytics

Track which navigation paths are most common:
- Cursor → Function (60%)
- Graph → Node (25%)
- Test → Function (10%)
- Debug Panel → Function (5%)

Use this data to optimize the decision matrix.

### 6. Persistent Navigation State

Save navigation state to URL or local storage:
- Shareable links to specific functions/tests
- Restore last viewed state on reload
- Per-project navigation preferences

---

## Appendix A: Decision Matrix Rules Reference

Complete list of rules and their priorities:

| Priority | Rule Name | Condition | Result | Notes |
|----------|-----------|-----------|--------|-------|
| 0 | DirectNodeSelection | event is node + workflow exists | Select node in workflow | Extracts `functionName` from node |
| 1 | ExplicitTestSelection | event is test | Select function/workflow with test | If in workflow, includes `functionName` |
| 2 | StayInCurrentWorkflow | event is function + function in current workflow | Select node in current workflow | Sets `functionName` from membership |
| 3 | SwitchToWorkflowContainingFunction | event is function + function in any workflow + prefer workflow view | Select node in that workflow | Sets `functionName` from membership |
| 4 | ShowFunctionInIsolation | event is function + function exists | Show function in function mode | N/A (function mode) |
| 999 | EmptyState | always | Empty state | N/A (empty mode) |

---

## Appendix B: State Transition Diagram

```
                    ┌─────────┐
                    │  Empty  │
                    └────┬────┘
                         │
          ┌──────────────┼──────────────┐
          │              │              │
     [function]      [workflow]      [test]
          │              │              │
          v              v              v
    ┌──────────┐   ┌──────────┐   ┌──────────┐
    │ Function │   │ Workflow │   │   Test   │
    │   Mode   │   │   Mode   │   │  Select  │
    └────┬─────┘   └────┬─────┘   └────┬─────┘
         │              │              │
         └──────────────┼──────────────┘
                        │
                   [navigate]
                        │
                        v
                 ┌─────────────┐
                 │  Decision   │
                 │   Matrix    │
                 └──────┬──────┘
                        │
              ┌─────────┼─────────┐
              │         │         │
         [workflow] [function] [empty]
              │         │         │
              v         v         v
         ┌─────────┬─────────┬─────────┐
         │Workflow │Function │  Empty  │
         └─────────┴─────────┴─────────┘
```

---

## Appendix C: Example Navigation Flows

### Flow 1: Cursor movement in IDE

```
User moves cursor to line 42 in main.baml
    ↓
NavigationEvent {
  entity: { kind: 'cursor', line: 42, column: 5 },
  context: { source: 'cursor', filePath: 'main.baml' }
}
    ↓
BamlRuntime.updateCursor(42, 5)
    ↓
Returns { functionName: 'GetUserInfo', testCaseName: null }
    ↓
ResolvedEntity {
  kind: 'function',
  name: 'GetUserInfo',
  exists: true,
  functionType: 'llm',
  availableTests: ['test_happy_path', 'test_error'],
  memberOfWorkflows: [
    { workflowId: 'UserFlow', nodeId: 'node-getuserinfo', ... }
  ]
}
    ↓
Decision Matrix applies "StayInCurrentWorkflow" rule (if already in UserFlow)
OR "SwitchToWorkflowContainingFunction" rule (if not in UserFlow)
    ↓
NavigationState {
  mode: 'workflow',
  workflowId: 'UserFlow',
  selectedNodeId: 'node-getuserinfo',
  testName: 'test_happy_path'  // auto-selected
}
    ↓
StateTransaction {
  selection: { ... },
  sideEffects: [
    { type: 'switchTab', tab: 'graph' },
    { type: 'openDetailPanel' },
    { type: 'panToNode', workflowId: 'UserFlow', nodeId: 'node-getuserinfo' },
    { type: 'selectInputSource', testName: 'test_happy_path' }
  ]
}
    ↓
Apply transaction → UI updates
```

### Flow 2: Clicking a test in test panel

```
User clicks test "test_error" for function "GetUserInfo"
    ↓
NavigationEvent {
  entity: {
    kind: 'test',
    functionName: 'GetUserInfo',
    testName: 'test_error'
  },
  context: { source: 'test-panel' }
}
    ↓
ResolvedEntity (enriched with function info and workflow memberships)
    ↓
Decision Matrix applies "ExplicitTestSelection" rule
    ↓
If GetUserInfo is in a workflow AND preferWorkflowView:
  NavigationState { mode: 'workflow', workflowId: 'UserFlow', selectedNodeId: 'node-getuserinfo', testName: 'test_error' }
Else:
  NavigationState { mode: 'function', functionName: 'GetUserInfo', testName: 'test_error' }
    ↓
Apply transaction → UI updates
```

### Flow 3: Clicking a node in graph

```
User clicks node "ConditionalWorkflow|root:0|hdr:validate-payload-structure:0"
in workflow "ConditionalWorkflow"
    ↓
NavigationEvent {
  entity: {
    kind: 'node',
    workflowId: 'ConditionalWorkflow',
    nodeId: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0'
  },
  context: { source: 'graph' }
}
    ↓
ResolvedEntity (enriched with function called by this node)
    ↓
Decision Matrix applies "DirectNodeSelection" rule (Priority 0)
    ↓
Extracts: node calls ValidateInput() function
    ↓
NavigationState {
  mode: 'workflow',
  workflowId: 'ConditionalWorkflow',
  selectedNodeId: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
  functionName: 'ValidateInput',  // ← Node calls this function!
  testName: 'test1'  // auto-selected
}
    ↓
Apply transaction → UI updates:
  - Graph highlights the node
  - Detail panel shows ValidateInput function details
  - Prompt preview displays ValidateInput's prompt
  - Test panel shows tests for ValidateInput
  - Breadcrumbs: "ConditionalWorkflow > ValidateInput > test1"
```

**Note:** If the node doesn't call a function (e.g., it's a conditional or group), `functionName` is `null`:
```typescript
{
  mode: 'workflow',
  workflowId: 'ConditionalWorkflow',
  selectedNodeId: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
  functionName: null,  // ← This is a group/header node, not a function call
  testName: null
}
```

---

## Conclusion

This redesign transforms the navigation system from a complex, implicit priority-based heuristic into a clear, explicit, testable decision matrix. By separating concerns and making state transitions explicit, we gain:

1. **Predictability** - Easy to understand what will happen for any given event
2. **Testability** - Pure functions, no dependencies, easy to test
3. **Debuggability** - Complete audit trail of all navigation decisions
4. **Extensibility** - Add new rules without breaking existing ones
5. **Reliability** - Better handling of timing issues and edge cases
6. **Performance** - Fewer redundant queries, better caching

The migration path is gradual and safe, with feature flags and rollback options. The new architecture sets us up for future enhancements like navigation history, smart navigation, and keyboard shortcuts.

---

## Appendix D: Real Runtime Data Structures

Based on actual test output from the WASM runtime, here's what the data structures actually look like:

### Node ID Structure

Node IDs follow a hierarchical lexical pattern:

```
{WorkflowName}|root:0|hdr:{header-name}:{index}|bg:{branch-name}:{index}|arm:{arm-name}:{index}
```

**Examples from ConditionalWorkflow:**

1. **Root node:**
   ```
   id: "ConditionalWorkflow"
   lexicalId: "ConditionalWorkflow|root:0"
   type: "group"
   functionName: "ConditionalWorkflow"
   ```

2. **Header node (comment-derived):**
   ```
   id: "ConditionalWorkflow|root:0|hdr:validate-payload-structure:0"
   label: "validate payload structure"
   type: "group"
   parent: "ConditionalWorkflow"
   ```

3. **Conditional branch group:**
   ```
   id: "ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0"
   label: "if (CheckCondition(validation.summary))"
   type: "conditional"
   parent: "ConditionalWorkflow|root:0|hdr:check-summary-confidence:1"
   ```

4. **Nested header in branch:**
   ```
   id: "ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0|arm:if-checkcondition-validation-summary:0|hdr:run-enrichment-subgraph:0"
   label: "run enrichment subgraph"
   type: "group"
   parent: "ConditionalWorkflow|root:0|hdr:check-summary-confidence:1"
   ```

### Key Observations

1. **Node IDs are lexical paths, not function names**
   - A node's ID describes its position in the syntax tree
   - Multiple nodes can represent the same logical step
   - No direct mapping from function name to node ID

2. **Labels come from BAML comments**
   - `//# validate payload structure` becomes label "validate payload structure"
   - Labels are human-readable, IDs are machine-readable

3. **Node types:**
   - `group` - Header context (from comments) or workflow root
   - `conditional` - If/else branches
   - `llm_function` - Direct LLM function calls (when standalone)
   - `function` - Expression function calls

4. **Parent hierarchy:**
   - Nodes have a `parent` field pointing to parent node ID
   - Root node has `parent: "N/A"`
   - Nested nodes point to their containing context

5. **Metadata structure:**
   ```json
   {
     "wasmNodeId": 0,  // Internal WASM graph node ID
     "lexicalId": "ConditionalWorkflow|root:0|hdr:...",
     "controlFlowType": "HeaderContextEnter" | "BranchGroup" | "FunctionRoot"
   }
   ```

### Real Workflow Example: ConditionalWorkflow

**BAML Source:**
```baml
function ConditionalWorkflow(task_summary: string) -> string {
  //# validate payload structure
  let validation = ValidateInput(task_summary);

  //# check summary confidence
  if (CheckCondition(validation.summary)) {
    //# run enrichment subgraph
    let enriched = SubgraphProcess(task_summary);

    //# finalize success report
    return SubgraphValidate(enriched);
  } else {
    //# return remediation guidance
    return HandleFailure(task_summary);
  }
}
```

**Resulting Graph:**

7 Nodes:
1. `ConditionalWorkflow` (root)
2. `ConditionalWorkflow|root:0|hdr:validate-payload-structure:0`
3. `ConditionalWorkflow|root:0|hdr:check-summary-confidence:1`
4. `ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0` (conditional)
5. `ConditionalWorkflow|root:0|...:|arm:if-checkcondition-validation-summary:0|hdr:run-enrichment-subgraph:0`
6. `ConditionalWorkflow|root:0|...:|arm:if-checkcondition-validation-summary:0|hdr:finalize-success-report:1`
7. `ConditionalWorkflow|root:0|...:|arm:else:1|hdr:return-remediation-guidance:0`

4 Edges:
1. validate → check summary confidence
2. run enrichment → finalize success
3. conditional → run enrichment (if branch)
4. conditional → return remediation (else branch)

### Problem: No Function Call Information

**Critical Discovery:** The call graph does NOT expose which functions are called within nodes.

Looking at the WASM debug output:
```
CheckCondition graph: ControlFlowVisualization {
    nodes: {
        NodeId(0,): Node {
            lexical_id: "CheckCondition|root:0",
            label: "CheckCondition",
            node_type: FunctionRoot,
        },
    },
    edges_by_src: {},
}
```

`CheckCondition` appears as its own graph, but there's no link showing that it's CALLED from within `ConditionalWorkflow`'s conditional node.

**Impact on Navigation:**

When user clicks on `CheckCondition` in the IDE:
- We know it's a function at line X
- We DON'T know it's used in `ConditionalWorkflow`
- Navigation heuristic can't switch to the workflow automatically
- This is why the test is skipped (line 123 in selection-integration.test.ts)

**Current Workaround:** String matching on node labels/IDs, which is fragile:
```typescript
function resolveNodeId(workflow, targetId) {
  // Try direct match
  const directMatch = workflow.nodes?.find((node) => node.id === targetId);

  // Try label match
  const labelMatch = workflow.nodes?.find((node) => node.label === targetId);

  // Try function name prefix (HACKY!)
  const rootMatch = workflow.nodes?.find((node) =>
    node.id?.startsWith(`${targetId}|`)
  );
}
```

**What We Need from WASM:**

Add function call information to nodes:
```typescript
type Node = {
  id: string
  label: string
  type: NodeType
  parent?: string

  // NEW: Function calls made by this node
  functionCalls?: {
    functionName: string  // e.g., "CheckCondition", "ValidateInput"
    span: Span            // Where in the code this call happens
    argPath?: string      // e.g., "validation.summary" for arguments
    callType: 'direct' | 'conditional' | 'assignment'  // How it's called
  }[]
}
```

**Example with enriched data:**

Node: `ConditionalWorkflow|root:0|hdr:validate-payload-structure:0`
```json
{
  "id": "ConditionalWorkflow|root:0|hdr:validate-payload-structure:0",
  "label": "validate payload structure",
  "type": "group",
  "functionCalls": [
    {
      "functionName": "ValidateInput",
      "span": { "start": 123, "end": 153 },
      "callType": "assignment"
    }
  ]
}
```

Node: `ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0`
```json
{
  "id": "...|bg:if-checkcondition-validation-summary:0",
  "label": "if (CheckCondition(validation.summary))",
  "type": "conditional",
  "functionCalls": [
    {
      "functionName": "CheckCondition",
      "span": { "start": 236, "end": 276 },
      "argPath": "validation.summary",
      "callType": "conditional"
    }
  ]
}
```

**Then we could:**

1. **Index function calls:** Build reverse mapping
   ```typescript
   CheckCondition → used in ConditionalWorkflow (node: ...bg:if-checkcondition...)
   ValidateInput → used in ConditionalWorkflow (node: ...hdr:validate-payload...)
   ```

2. **Navigate from function to workflow containing it:**
   - User clicks `CheckCondition` in IDE
   - Resolver finds it's called in `ConditionalWorkflow` node
   - Switches to workflow mode with that node selected
   - SelectionState includes `functionName: "CheckCondition"` so detail panel knows what to show

3. **Track current function in workflow mode:**
   ```typescript
   {
     mode: 'workflow',
     workflowId: 'ConditionalWorkflow',
     selectedNodeId: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
     functionName: 'ValidateInput',  // ← This node calls ValidateInput!
     testName: 'test1'
   }
   ```
   Now the UI can:
   - Show function-specific details in the panel
   - Allow clicking through to see `ValidateInput` in isolation
   - Run tests for `ValidateInput` from within the workflow view
   - Display the function's prompt in the preview

4. **Fix the skipped tests** - Full navigation flow works!

---

**Document Version:** 1.0
**Last Updated:** 2025-01-12
**Author:** Claude + Aaron Villalpando
**Status:** Proposal
