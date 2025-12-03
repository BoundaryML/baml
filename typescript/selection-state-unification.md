# Selection State Unification Plan

## Current State

We currently have two main data sources for UI selection state:

1. **`selectedNodeIdAtom` / `useSelectedNode()`** – legacy atom used by graph-specific features (detail panel, execution sync) to know the active node.
2. **`unifiedSelectionAtom`** – newer atom introduced for PromptPreview + graph integration. It already mirrors `functionName`, `testName`, `activeWorkflowId`, and `selectedNodeId`. DebugPanel clicks and code-navigation update both the graph atoms and this unified atom.

This duality leads to complexity and bugs (state drift when one atom updates but the other doesn’t). The long-term goal is to have a single source of truth, ideally `unifiedSelectionAtom`.

## Proposal

1. **Inventory Consumers**
   - Identify every call site that depends on `useSelectedNode()`, `selectedNodeIdAtom`, or their write helpers (e.g., `setSelectedNodeId`). Key areas include detail panel components, input-library selectors, execution sync, navigation heuristics.

2. **Introduce Read Adapters**
   - Create hooks such as `useUnifiedSelectedNode()` that read `unifiedSelectionAtom.selectedNodeId`. Start by migrating read-only consumers (places that only need the current node id) to this adapter. The legacy hook can internally read the unified atom so we change behavior without touching every consumer at once.

3. **Centralize Writes**
   - Force all write paths (GraphView clicks, DebugPanel, navigation heuristic, test runner) to update only `unifiedSelectionAtom`. Provide helper functions if needed (e.g., `setGraphSelection({ nodeId, workflowId })`). Deprecate direct use of `setSelectedNodeId` once all writers are converted.

4. **Update Derived Hooks**
   - Hooks like `useActiveNode`, detail panel selectors, and input-source logic should be refactored to depend on the unified state. Where they currently compute derived data off `selectedNodeIdAtom`, swap to `unifiedSelectionAtom.selectedNodeId`.

5. **Remove Legacy Atom**
   - Once no code reads or writes `selectedNodeIdAtom` directly, delete the atom and update SDK storage hooks to drop any dependencies. This also simplifies `useGraphSync` and navigation since they no longer have to coordinate two atoms.

6. **Regression Tests**
   - Expand `unified-selection-sync.test.ts` (or add a new test) to mount a minimal GraphView + DebugPanel setup. Verify that clicking nodes/functions updates the unified selection once and that detail-panel state follows. Also add tests around `useCodeNavigation` to ensure workflow switches still clear node selection properly.

## Risks

- **Missed Consumers:** Many hooks still use `selectedNodeIdAtom`. Missing a write path could leave parts of the UI (detail panel, execution I/O) stuck because they never see selection changes.
- **Implicit Behavior:** Some logic (e.g., clearing `selectedNodeId` to exit LLM-only mode) might hinge on `selectedNodeIdAtom` side effects. Centralizing selection means re-implementing those side effects around the unified atom.
- **Testing Surface:** The change touches navigation, execution sync, and prompt preview simultaneously. We should budget time for end-to-end testing (manual + automated) to catch regressions.

## Execution Strategy

- Treat this as a focused refactor rather than incremental hacks. Work in stages (adapter → consumer migration → cleanup) with tests at each milestone.
- Keep the old atom around until the very last step to avoid breaking intermediate commits.
- Communicate with anyone working on navigation/detail panels since their code will likely need updates.

Following this plan gives us a single, well-defined selection state that all features can rely on, reducing bugs like “graph view doesn’t show LLM tabs when clicking nodes” and making future features (e.g., cross-panel sync) simpler.
