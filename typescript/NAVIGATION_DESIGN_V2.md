# Playground Navigation System - Redesign

> **Audience:** SDE2+ engineers
>
> **Goal:** Replace priority-based heuristic with a rule-based decision system

---

## Overview

### Problem

The current navigation system works but has architectural issues:
- **Implicit priorities** - Hard to understand why Priority 2 beats Priority 3
- **Coupled logic** - Test selection, state updates, and graph panning mixed together
- **Hard to extend** - Adding a new rule requires understanding the entire heuristic
- **Poor observability** - Can't easily debug why a decision was made
- **Missing data** - WASM doesn't expose function calls within workflow nodes

### Solution

Replace the monolithic heuristic with a composable rule engine:

```typescript
// Before: One large function with implicit priorities
function determineNavigationAction(event, context) {
  if (workflowId && nodeId) { /* Priority 0 */ }
  else if (activeWorkflowId && functionExistsInWorkflow) { /* Priority 1 */ }
  else if (findWorkflowContainingFunction) { /* Priority 2 */ }
  // ... 200 more lines
}

// After: Explicit rules with clear precedence
const rules = [
  { name: 'DirectNodeClick', priority: 0, when: ..., then: ... },
  { name: 'StayInWorkflow', priority: 1, when: ..., then: ... },
  { name: 'SwitchToWorkflow', priority: 2, when: ..., then: ... }
]
```

### Key Architectural Changes

1. **Separate concerns** - Resolution → Decision → Transaction → Application
2. **Explicit state machine** - All states and transitions defined upfront
3. **Rule-based decisions** - Easy to add/remove/reorder
4. **Transactional updates** - All atoms update atomically
5. **Event-based graph sync** - No more polling
6. **Complete audit trail** - Log every decision

---

## Architecture

### State Model

```typescript
/**
 * Core state: What is currently selected
 *
 * Key insight: In workflow mode, we need to track both:
 * - selectedNodeId: Which graph node is highlighted
 * - functionName: What function that node calls (for detail panel)
 */
type SelectionState =
  | {
      mode: 'workflow'
      workflowId: string
      selectedNodeId: string
      functionName: string | null  // Function called by this node
      testName: string | null
    }
  | {
      mode: 'function'
      functionName: string
      testName: string | null
    }
  | { mode: 'empty' }
```

**Why track `functionName` in workflow mode?**

Workflow nodes have complex IDs like `ConditionalWorkflow|root:0|hdr:validate-payload-structure:0`, but the detail panel needs to know which function to display. A node with label "validate payload structure" might call `ValidateInput()`, and we need that mapping.

**Note:** Nodes can call **multiple functions**:
```
### some node name
callFunc1()
callFunc2()
```

We track the **primary function** (first one) in `functionName` for the detail panel, but the full list is available in the node's metadata.

### Event Model

```typescript
/**
 * Input: What the user clicked
 */
type NavigationInput = {
  kind: 'function' | 'test' | 'node'

  // Function click
  functionName?: string

  // Test click
  testName?: string

  // Node click (has workflow context)
  workflowId?: string
  nodeId?: string

  // Metadata
  source: 'cursor' | 'debug-panel' | 'graph' | 'test-panel'
  timestamp: number
}

/**
 * Enriched: Input + all context we can gather
 */
type EnrichedTarget = {
  // What was clicked
  name: string
  kind: 'function' | 'test' | 'node'
  exists: boolean

  // Where is it used?
  workflowMemberships: Array<{
    workflowId: string
    nodeId: string
    nodeLabel: string
    calledFunction: string | null  // If node calls a function
  }>

  // Related data
  availableTests: string[]
  functionType?: FunctionType
}
```

### Decision Engine

A rule is a predicate + action pair:

```typescript
type NavigationRule = {
  id: string                    // Unique identifier
  priority: number              // Lower = higher priority

  // When does this rule apply?
  matches: (target: EnrichedTarget, current: SelectionState) => boolean

  // What should we do?
  resolve: (target: EnrichedTarget, current: SelectionState) => SelectionState

  // Optional: Why did this rule match? (for debugging)
  explain?: (target: EnrichedTarget, current: SelectionState) => string
}
```

**Rule Execution:**

```typescript
function applyRules(
  target: EnrichedTarget,
  current: SelectionState,
  rules: NavigationRule[]
): { state: SelectionState, appliedRule: string } {
  // Sort by priority (ascending)
  const sorted = [...rules].sort((a, b) => a.priority - b.priority)

  // Find first matching rule
  for (const rule of sorted) {
    if (rule.matches(target, current)) {
      const state = rule.resolve(target, current)
      return { state, appliedRule: rule.id }
    }
  }

  throw new Error('No rule matched - should have catch-all rule')
}
```

---

## Rule Definitions

### Core Rules

```typescript
export const NAVIGATION_RULES: NavigationRule[] = [
  // Priority 0: Direct node selection from graph
  {
    id: 'direct-node-click',
    priority: 0,
    matches: (target) => target.kind === 'node',
    resolve: (target) => {
      const workflow = getWorkflow(target.workflowId!)
      const node = workflow.nodes.find(n => n.id === target.nodeId)
      const calledFunction = extractCalledFunction(node)

      return {
        mode: 'workflow',
        workflowId: target.workflowId!,
        selectedNodeId: target.nodeId!,
        functionName: calledFunction,
        testName: selectPreferredTest(target.availableTests, null)
      }
    }
  },

  // Priority 1: Test selection
  {
    id: 'test-click',
    priority: 1,
    matches: (target) => target.kind === 'test',
    resolve: (target, current) => {
      // If function is in a workflow, show workflow mode
      if (target.workflowMemberships.length > 0) {
        const membership = selectBestWorkflow(target.workflowMemberships, current)
        return {
          mode: 'workflow',
          workflowId: membership.workflowId,
          selectedNodeId: membership.nodeId,
          functionName: membership.calledFunction,
          testName: target.testName!
        }
      }

      // Otherwise, show function mode
      return {
        mode: 'function',
        functionName: target.functionName!,
        testName: target.testName!
      }
    }
  },

  // Priority 2: Context preservation (stay in current workflow)
  {
    id: 'stay-in-workflow',
    priority: 2,
    matches: (target, current) =>
      target.kind === 'function' &&
      current.mode === 'workflow' &&
      target.workflowMemberships.some(m => m.workflowId === current.workflowId),
    resolve: (target, current) => {
      const membership = target.workflowMemberships.find(
        m => m.workflowId === current.workflowId
      )!

      return {
        mode: 'workflow',
        workflowId: current.workflowId,
        selectedNodeId: membership.nodeId,
        functionName: membership.calledFunction,
        testName: selectPreferredTest(target.availableTests, current.testName)
      }
    },
    explain: (target, current) =>
      `Staying in ${current.workflowId} because ${target.name} is a node there`
  },

  // Priority 3: Workflow discovery
  {
    id: 'switch-to-workflow',
    priority: 3,
    matches: (target) =>
      target.kind === 'function' &&
      target.workflowMemberships.length > 0,
    resolve: (target) => {
      const membership = target.workflowMemberships[0]  // Pick first workflow

      return {
        mode: 'workflow',
        workflowId: membership.workflowId,
        selectedNodeId: membership.nodeId,
        functionName: membership.calledFunction,
        testName: selectPreferredTest(target.availableTests, null)
      }
    }
  },

  // Priority 4: Function isolation
  {
    id: 'show-function',
    priority: 4,
    matches: (target) => target.kind === 'function' && target.exists,
    resolve: (target) => ({
      mode: 'function',
      functionName: target.name,
      testName: selectPreferredTest(target.availableTests, null)
    })
  },

  // Priority 999: Catch-all
  {
    id: 'empty-state',
    priority: 999,
    matches: () => true,
    resolve: () => ({ mode: 'empty' })
  }
]
```

### Helper: Extract Called Function

```typescript
/**
 * Extract which function a node calls (if any)
 *
 * Current limitation: WASM doesn't expose this, so we parse from labels.
 * Future: WASM should provide node.functionCalls[]
 */
function extractCalledFunction(node: WorkflowNode | undefined): string | null {
  if (!node) return null

  // Method 1: Future WASM enhancement
  if (node.metadata?.functionCalls?.[0]) {
    return node.metadata.functionCalls[0].functionName
  }

  // Method 2: Parse from label
  // e.g., "if (CheckCondition(...))" -> "CheckCondition"
  const match = node.label?.match(/(\w+)\s*\(/)
  if (match) return match[1]

  // Method 3: Check if label exactly matches a function name
  const func = getFunctions().find(f => f.name === node.label)
  if (func) return func.name

  return null
}
```

### Helper: Test Selection Strategy

```typescript
/**
 * Select the best test to show
 *
 * Priority:
 * 1. Currently selected test (if still valid)
 * 2. First available test
 * 3. null
 */
function selectPreferredTest(
  availableTests: string[],
  currentTest: string | null
): string | null {
  if (!availableTests.length) return null

  // Preserve current test if valid
  if (currentTest && availableTests.includes(currentTest)) {
    return currentTest
  }

  // Otherwise pick first
  return availableTests[0]
}
```

---

## Component Architecture

### 1. Target Enricher

**Responsibility:** Take raw click input and enrich with all context

```typescript
class TargetEnricher {
  constructor(
    private runtime: BamlRuntime,
    private workflows: Workflow[],
    private functions: Function[],
    private tests: Test[]
  ) {}

  /**
   * Enrich a navigation input with full context
   */
  async enrich(input: NavigationInput): Promise<EnrichedTarget> {
    // If clicking from cursor, resolve to function/test
    if (input.source === 'cursor' && input.cursorPosition) {
      const resolved = await this.runtime.getEntityAtPosition(
        input.cursorPosition.line,
        input.cursorPosition.column
      )
      input = { ...input, ...resolved }
    }

    // Build enriched target
    const target: EnrichedTarget = {
      name: input.functionName || input.testName || input.nodeId || '',
      kind: input.kind,
      exists: this.checkExists(input),
      workflowMemberships: this.findWorkflowUsages(input),
      availableTests: this.findTests(input),
      functionType: this.getFunctionType(input)
    }

    return target
  }

  private findWorkflowUsages(input: NavigationInput): WorkflowMembership[] {
    const memberships: WorkflowMembership[] = []

    for (const workflow of this.workflows) {
      for (const node of workflow.nodes) {
        // Check if this node relates to our input
        const calledFunction = extractCalledFunction(node)

        if (
          node.id === input.nodeId ||
          node.functionName === input.functionName ||
          calledFunction === input.functionName
        ) {
          memberships.push({
            workflowId: workflow.id,
            nodeId: node.id,
            nodeLabel: node.label,
            calledFunction
          })
        }
      }
    }

    return memberships
  }
}
```

### 2. Rule Engine

**Responsibility:** Apply rules to determine target state

```typescript
class RuleEngine {
  constructor(private rules: NavigationRule[]) {}

  /**
   * Determine target state by applying rules
   */
  decide(
    target: EnrichedTarget,
    current: SelectionState
  ): { state: SelectionState, rule: string } {
    const sorted = [...this.rules].sort((a, b) => a.priority - b.priority)

    for (const rule of sorted) {
      if (rule.matches(target, current)) {
        const state = rule.resolve(target, current)

        return {
          state,
          rule: rule.id
        }
      }
    }

    throw new NavigationError('No rule matched', { target, current })
  }
}
```

### 3. State Manager

**Responsibility:** Apply state changes to atoms transactionally

```typescript
type SideEffect =
  | { type: 'switch-tab'; tab: 'preview' | 'curl' | 'graph' }
  | { type: 'pan-to-node'; workflowId: string; nodeId: string }
  | { type: 'open-panel' }
  | { type: 'close-panel' }
  | { type: 'select-test'; testName: string }
  | { type: 'clear-test' }

class StateManager {
  /**
   * Build a transaction (state + side effects)
   */
  buildTransaction(
    targetState: SelectionState,
    currentState: SelectionState
  ): SideEffect[] {
    const effects: SideEffect[] = []

    if (targetState.mode === 'workflow') {
      effects.push({ type: 'switch-tab', tab: 'graph' })
      effects.push({ type: 'open-panel' })
      effects.push({
        type: 'pan-to-node',
        workflowId: targetState.workflowId,
        nodeId: targetState.selectedNodeId
      })

      if (targetState.testName) {
        effects.push({ type: 'select-test', testName: targetState.testName })
      }

    } else if (targetState.mode === 'function') {
      effects.push({ type: 'switch-tab', tab: 'preview' })
      effects.push({ type: 'open-panel' })

      if (targetState.testName) {
        effects.push({ type: 'select-test', testName: targetState.testName })
      }

    } else {
      effects.push({ type: 'switch-tab', tab: 'preview' })
      effects.push({ type: 'close-panel' })
      effects.push({ type: 'clear-test' })
    }

    return effects
  }

  /**
   * Apply transaction atomically
   */
  async apply(
    state: SelectionState,
    effects: SideEffect[],
    atomSet: JotaiSet
  ): Promise<void> {
    // 1. Update selection atom
    atomSet(unifiedSelectionStateAtom, state)

    // 2. Apply side effects
    for (const effect of effects) {
      switch (effect.type) {
        case 'switch-tab':
          atomSet(activeTabAtom, effect.tab)
          break
        case 'open-panel':
          atomSet(detailPanelAtom, { isOpen: true })
          break
        case 'close-panel':
          atomSet(detailPanelAtom, { isOpen: false })
          break
        case 'select-test':
          atomSet(selectedInputSourceAtom, { testName: effect.testName })
          break
        case 'clear-test':
          atomSet(selectedInputSourceAtom, null)
          break
        case 'pan-to-node':
          await this.panToNode(effect.workflowId, effect.nodeId)
          break
      }
    }
  }

  private async panToNode(workflowId: string, nodeId: string): Promise<void> {
    // Wait for graph to render
    await waitForEvent('graph-ready', { workflowId }, { timeout: 2000 })

    // Pan to node
    const node = flowStore.value.getNodes?.().find(n => n.id === nodeId)
    if (node) {
      flowStore.value.setCenter?.(node.position.x, node.position.y, {
        zoom: 1,
        duration: 300
      })
    }
  }
}
```

### 4. Navigation Coordinator

**Responsibility:** Orchestrate the entire flow

```typescript
class NavigationCoordinator {
  constructor(
    private enricher: TargetEnricher,
    private engine: RuleEngine,
    private stateManager: StateManager,
    private logger: NavigationLogger
  ) {}

  /**
   * Main entry point for navigation
   */
  async navigate(
    input: NavigationInput,
    atomGet: JotaiGet,
    atomSet: JotaiSet
  ): Promise<void> {
    const startTime = performance.now()

    try {
      // 1. Enrich target with context
      const target = await this.enricher.enrich(input)

      // 2. Get current state
      const current = atomGet(unifiedSelectionStateAtom)

      // 3. Decide where to go
      const { state: targetState, rule } = this.engine.decide(target, current)

      // 4. Build transaction
      const effects = this.stateManager.buildTransaction(targetState, current)

      // 5. Apply transaction
      await this.stateManager.apply(targetState, effects, atomSet)

      // 6. Log
      this.logger.log({
        input,
        target,
        from: current,
        to: targetState,
        rule,
        effects,
        duration: performance.now() - startTime
      })

    } catch (error) {
      this.logger.error(input, error)
      throw error
    }
  }
}
```

### 5. Navigation Logger

**Responsibility:** Record all navigation events for debugging

```typescript
type NavigationLogEntry = {
  input: NavigationInput
  target: EnrichedTarget
  from: SelectionState
  to: SelectionState
  rule: string
  effects: SideEffect[]
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

    // Console output
    console.groupCollapsed(
      `%cNav: ${entry.from.mode} → ${entry.to.mode} (${entry.rule})`,
      'color: #00aa00; font-weight: bold'
    )
    console.log('Input:', entry.input)
    console.log('Target:', entry.target)
    console.log('Rule:', entry.rule)
    console.log('From:', entry.from)
    console.log('To:', entry.to)
    console.log('Effects:', entry.effects)
    console.log('Duration:', `${entry.duration.toFixed(2)}ms`)
    console.groupEnd()
  }

  error(input: NavigationInput, error: Error): void {
    console.error('Navigation failed', { input, error })
  }

  getHistory(): NavigationLogEntry[] {
    return [...this.logs]
  }

  export(): string {
    return JSON.stringify(this.logs, null, 2)
  }
}

// Expose for debugging
if (typeof window !== 'undefined') {
  (window as any).__navLogs = () => navLogger.getHistory()
  (window as any).__navExport = () => navLogger.export()
}
```

---

## Integration

### Main Dispatcher Atom

```typescript
import { atom } from 'jotai'

// Feature flag
const USE_NEW_NAVIGATION = import.meta.env.VITE_NEW_NAVIGATION === 'true'

// Singleton coordinator
const coordinator = new NavigationCoordinator(
  new TargetEnricher(runtime, workflows, functions, tests),
  new RuleEngine(NAVIGATION_RULES),
  new StateManager(),
  new NavigationLogger()
)

export const navigationDispatcherAtom = atom(
  null,
  async (get, set, input: NavigationInput) => {
    if (USE_NEW_NAVIGATION) {
      await coordinator.navigate(input, get, set)
    } else {
      // Old system
      await oldNavigationHandler(input, get, set)
    }
  }
)
```

### Update Click Handlers

```typescript
// In DebugPanel.tsx
const handleFunctionClick = (functionName: string) => {
  dispatchNavigation({
    kind: 'function',
    functionName,
    source: 'debug-panel',
    timestamp: Date.now()
  })
}

// In graph-view.tsx
const handleNodeClick = (node: Node) => {
  dispatchNavigation({
    kind: 'node',
    workflowId: activeWorkflowId,
    nodeId: node.id,
    source: 'graph',
    timestamp: Date.now()
  })
}

// In test panel
const handleTestClick = (test: Test) => {
  dispatchNavigation({
    kind: 'test',
    testName: test.name,
    functionName: test.functionId,
    source: 'test-panel',
    timestamp: Date.now()
  })
}
```

---

## Testing Strategy

### Unit Tests

Test each component in isolation:

```typescript
describe('RuleEngine', () => {
  it('should apply highest priority rule', () => {
    const engine = new RuleEngine(NAVIGATION_RULES)

    const target: EnrichedTarget = {
      name: 'MyFunction',
      kind: 'function',
      exists: true,
      workflowMemberships: [
        { workflowId: 'MyWorkflow', nodeId: 'node-1', ... }
      ],
      availableTests: ['test1']
    }

    const current: SelectionState = {
      mode: 'workflow',
      workflowId: 'MyWorkflow',
      selectedNodeId: 'node-2',
      functionName: null,
      testName: null
    }

    const { state, rule } = engine.decide(target, current)

    expect(rule).toBe('stay-in-workflow')
    expect(state.mode).toBe('workflow')
    expect(state.selectedNodeId).toBe('node-1')
  })
})

describe('TargetEnricher', () => {
  it('should find all workflow memberships', async () => {
    const enricher = new TargetEnricher(runtime, workflows, functions, tests)

    const target = await enricher.enrich({
      kind: 'function',
      functionName: 'ValidateInput',
      source: 'debug-panel',
      timestamp: Date.now()
    })

    expect(target.workflowMemberships).toContainEqual(
      expect.objectContaining({
        workflowId: 'ConditionalWorkflow',
        calledFunction: 'ValidateInput'
      })
    )
  })
})
```

### Integration Tests

Test the full coordinator:

```typescript
describe('NavigationCoordinator', () => {
  it('should navigate from function to workflow', async () => {
    const { get, set } = setupTestStore()

    await coordinator.navigate({
      kind: 'function',
      functionName: 'ValidateInput',
      source: 'debug-panel',
      timestamp: Date.now()
    }, get, set)

    const state = get(unifiedSelectionStateAtom)

    expect(state).toMatchObject({
      mode: 'workflow',
      workflowId: 'ConditionalWorkflow',
      functionName: 'ValidateInput'
    })
  })

  it('should preserve context when clicking another function in same workflow', async () => {
    const { get, set } = setupTestStore({
      selection: {
        mode: 'workflow',
        workflowId: 'ConditionalWorkflow',
        selectedNodeId: 'node-1',
        functionName: 'ValidateInput',
        testName: 'test1'
      }
    })

    // Click different function in same workflow
    await coordinator.navigate({
      kind: 'function',
      functionName: 'CheckCondition',
      source: 'debug-panel',
      timestamp: Date.now()
    }, get, set)

    const state = get(unifiedSelectionStateAtom)

    // Should stay in workflow
    expect(state.workflowId).toBe('ConditionalWorkflow')
    expect(state.functionName).toBe('CheckCondition')
  })
})
```

---

## Migration Plan

### Phase 1: Foundation (Week 1)
- [ ] Create new types (`types.ts`)
- [ ] Implement rule engine (`rule-engine.ts`)
- [ ] Write unit tests for rules
- [ ] Document architecture decisions

### Phase 2: Core Logic (Week 2)
- [ ] Implement `TargetEnricher`
- [ ] Implement `StateManager`
- [ ] Implement `NavigationLogger`
- [ ] Write integration tests

### Phase 3: Integration (Week 3)
- [ ] Create `NavigationCoordinator`
- [ ] Add feature flag to dispatcher atom
- [ ] Update click handlers
- [ ] Test with feature flag enabled

### Phase 4: Validation (Week 4)
- [ ] Enable for internal users
- [ ] Monitor logs for issues
- [ ] Fix any bugs discovered
- [ ] Update documentation

### Phase 5: Rollout (Week 5-7)
- [ ] A/B test (50% users)
- [ ] Monitor metrics (navigation time, errors)
- [ ] Enable for 100%
- [ ] Remove old code

---

## Open Questions

### 1. WASM Enhancement

**Problem:** WASM doesn't expose which functions are called within workflow nodes.

**Impact:** Can't navigate from function → workflow in many cases (tests are skipped for this).

**Proposed Solution:** Add `functionCalls` to node metadata:

```typescript
type Node = {
  id: string
  label: string
  type: NodeType

  // NEW: Functions called by this node
  functionCalls?: Array<{
    functionName: string
    span: Span
    callType: 'direct' | 'conditional' | 'assignment'
  }>
}
```

**Owner:** WASM team
**Timeline:** TBD

### 2. Test Selection Heuristics

**Current:** Always select first test.

**Better:** Consider:
- Recently run tests (prefer hot tests)
- Test that was selected before switching functions
- Test coverage (prefer tests that exercise more code)

**Owner:** Navigation team
**Timeline:** Post-MVP

### 3. Multi-Selection

**Request:** Allow selecting multiple nodes in a workflow for comparison.

**Complexity:** Requires changes to `SelectionState` to support arrays.

**Owner:** UX team (design first)
**Timeline:** Future

---

## Performance Considerations

### Enrichment Cost

`TargetEnricher` needs to search all workflows for memberships. With many workflows, this could be slow.

**Solution:** Build an index on init:

```typescript
class WorkflowIndex {
  private functionToWorkflows = new Map<string, WorkflowMembership[]>()

  constructor(workflows: Workflow[]) {
    for (const workflow of workflows) {
      for (const node of workflow.nodes) {
        const func = extractCalledFunction(node)
        if (func) {
          const memberships = this.functionToWorkflows.get(func) || []
          memberships.push({
            workflowId: workflow.id,
            nodeId: node.id,
            nodeLabel: node.label,
            calledFunction: func
          })
          this.functionToWorkflows.set(func, memberships)
        }
      }
    }
  }

  lookup(functionName: string): WorkflowMembership[] {
    return this.functionToWorkflows.get(functionName) || []
  }
}
```

**Complexity:** O(workflows × nodes) at startup, O(1) at lookup time.

### Atom Updates

Updating multiple atoms isn't truly atomic in Jotai. If a component reads atoms mid-update, it might see inconsistent state.

**Solution:** Use `batch()` from `jotai/utils`:

```typescript
import { batch } from 'jotai/utils'

async apply(state: SelectionState, effects: SideEffect[], atomSet: JotaiSet) {
  batch(() => {
    atomSet(unifiedSelectionStateAtom, state)
    atomSet(activeTabAtom, /* ... */)
    atomSet(detailPanelAtom, /* ... */)
    // ...
  })
}
```

---

## Debug Tools

### Browser Console Commands

```javascript
// View navigation history
window.__navLogs()

// Export logs as JSON
window.__navExport()

// Test a specific rule
const engine = new RuleEngine(NAVIGATION_RULES)
engine.decide(myTarget, myCurrentState)

// Enable verbose logging
localStorage.setItem('DEBUG_NAVIGATION', 'true')
```

### React DevTools

The `unifiedSelectionStateAtom` is visible in Jotai DevTools. You can:
- See current selection state
- Manually set state to test edge cases
- Time-travel through navigation history

---

## Comparison: Before vs After

| Aspect | Before | After |
|--------|--------|-------|
| **Lines of Code** | ~970 lines | ~800 lines (more modular) |
| **Decision Points** | 14 implicit branches | 6 explicit rules |
| **Testability** | Requires full WASM + Jotai setup | Pure functions, easy mocking |
| **Observability** | `console.log` in heuristic | Structured logs with context |
| **Extensibility** | Modify 200-line function | Add new rule to array |
| **Performance** | Good | Better (with indexing) |

---

## References

- Current implementation: `src/sdk/navigationHeuristic.ts`
- Tests: `src/sdk/__tests__/selection-integration.test.ts`
- Related: `src/sdk/provider.tsx` (navigation hooks)

---

**Document Version:** 2.0
**Last Updated:** 2025-01-12
**Target Audience:** SDE2
**Status:** Design Review
