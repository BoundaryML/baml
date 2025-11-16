# Cursor Navigation Implementation Summary

## Overview

Implemented cursor-based navigation using the new WASM `get_entity_at_position` API and integrated it with the navigation coordinator system described in NAVIGATION_DESIGN_V2.md.

## Changes Made

### 1. Rust/WASM Layer (engine/baml-schema-wasm)

**File:** `engine/baml-schema-wasm/src/runtime_wasm/mod.rs`

#### Added Types

```rust
#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmEntityAtPosition {
    pub entity_type: String,        // "function" | "node"
    pub entity_name: String,         // Function name
    pub span: WasmSpan,
    pub function_type: Option<WasmFunctionKind>,  // "Llm" | "Expr"
    pub node_id: Option<String>,     // Present if entity_type === "node"
    pub node_label: Option<String>,  // Present if entity_type === "node"
}
```

#### Added WASM Method

```rust
impl WasmRuntime {
    pub fn get_entity_at_position(
        &self,
        file_name: &str,
        cursor_idx: usize,
    ) -> Option<WasmEntityAtPosition>
}
```

**Logic:**
1. Finds the function at the cursor position using existing `get_function_at_position`
2. If it's a workflow (Llm function), gets the function graph
3. Finds the smallest node containing the cursor position
4. Returns node entity if found, otherwise function entity

### 2. TypeScript Runtime Interface

**File:** `typescript/packages/playground-common/src/sdk/runtime/BamlRuntimeInterface.ts`

#### Added Types

```typescript
export interface EntityAtPosition {
  entity_type: 'function' | 'node';
  entity_name: string;
  function_type?: 'Llm' | 'Expr';
  node_id?: string;
  node_label?: string;
  span: {
    file_path: string;
    start: number;
    end: number;
    start_line: number;
    start_column: number;
    end_line: number;
    end_column: number;
  };
}
```

#### Added Interface Method

```typescript
interface BamlRuntimeInterface {
  getEntityAtPosition(
    cursor: CursorPosition,
    fileContents: Record<string, string>
  ): EntityAtPosition | null;
}
```

### 3. Runtime Implementation

**File:** `typescript/packages/playground-common/src/sdk/runtime/BamlRuntime.ts`

Implemented `getEntityAtPosition`:
- Converts line/column to character index
- Calls WASM `get_entity_at_position`
- Returns typed TypeScript entity

**File:** `typescript/packages/playground-common/src/sdk/runtime/MockBamlRuntime.ts`

Added mock implementation (returns null).

### 4. Navigation Integration

**File:** `typescript/packages/playground-common/src/shared/baml-project-panel/playground-panel/atoms.ts`

#### Updated `updateCursorAtom`

**Before:** Used `get_function_at_position` and manually dispatched navigation

**After:**
1. Uses `get_entity_at_position` to get precise entity information
2. Handles test case detection (preserved existing behavior)
3. Dispatches navigation based on entity type:
   - If `entity_type === 'node'` → Dispatch node navigation (workflow graph view)
   - If `entity_type === 'function'` → Dispatch function navigation
4. Passes cursor position to navigation dispatcher for enrichment

```typescript
if (entity.entity_type === 'node' && entity.node_id) {
  // Navigate to workflow node
  set(navigationDispatcherAtom, {
    kind: 'node',
    workflowId: entity.entity_name,
    nodeId: entity.node_id,
    source: 'cursor',
    timestamp: Date.now(),
    cursorPosition: { filePath, line, column }
  });
} else {
  // Navigate to function
  set(navigationDispatcherAtom, {
    kind: 'function',
    functionName: entity.entity_name,
    functionType: entity.function_type?.toLowerCase(),
    source: 'cursor',
    timestamp: Date.now(),
    cursorPosition: { filePath, line, column }
  });
}
```

## How It Works

### Scenario 1: Cursor in Standalone Function

```baml
function MyFunction(input: string) -> bool {
  client GPT4  // <- Cursor here
  prompt #"..."#
}
```

1. `get_entity_at_position` returns `{ entity_type: 'function', entity_name: 'MyFunction' }`
2. Navigation dispatcher receives `{ kind: 'function', functionName: 'MyFunction' }`
3. Navigation coordinator enriches and navigates to function view

### Scenario 2: Cursor in Workflow Node

```baml
function MyWorkflow(input: string) -> string {
  client GPT4

  // ### validate input
  result = ValidateInput(input)
  //       ^ Cursor here

  if (result) {
    return "ok"
  }
}
```

1. `get_entity_at_position` returns:
   ```typescript
   {
     entity_type: 'node',
     entity_name: 'MyWorkflow',
     node_id: 'MyWorkflow|root:0|hdr:validate-input:0',
     node_label: 'validate input'
   }
   ```
2. Navigation dispatcher receives:
   ```typescript
   {
     kind: 'node',
     workflowId: 'MyWorkflow',
     nodeId: 'MyWorkflow|root:0|hdr:validate-input:0',
     source: 'cursor'
   }
   ```
3. Navigation coordinator enriches the target:
   - Finds workflow memberships
   - Determines which function the node calls (`ValidateInput`)
   - Selects appropriate test
4. Navigation system:
   - Switches to graph tab
   - Pans to the specific node
   - Shows function details for `ValidateInput` in detail panel

### Scenario 3: Nested Workflows

If `ValidateInput` is itself a workflow called by `MyWorkflow`, the navigation system will:

1. Detect that cursor is in a node of `MyWorkflow`
2. Find that the node calls `ValidateInput`
3. Check if `ValidateInput` is used in a larger workflow
4. Navigate to the **largest workflow** that contains this chain

This "longest workflow" logic is handled by the navigation enricher and rules in the navigation coordinator.

## Integration with Navigation System

The implementation follows the NAVIGATION_DESIGN_V2.md pattern:

```
Cursor Update
    ↓
get_entity_at_position (WASM)
    ↓
NavigationInput (with cursor position)
    ↓
Navigation Dispatcher Atom
    ↓
Navigation Coordinator
    ↓
Target Enricher → Find workflow memberships
    ↓
Rule Engine → Apply navigation rules
    ↓
State Manager → Update atoms atomically
```

### Key Benefits

1. **Precise**: Returns the most specific entity (node > function)
2. **Simple API**: One WASM call, one result
3. **Reuses existing code**: Built on `get_function_at_position` and `function_graph_v2`
4. **Consistent**: Uses the same navigation coordinator as other navigation events
5. **Workflow-aware**: Automatically detects and navigates to workflow nodes

## Testing

The changes compile successfully with no TypeScript errors.

To test:
1. Move cursor inside a workflow node → Should navigate to graph view with node highlighted
2. Move cursor in standalone function → Should navigate to function detail view
3. Move cursor in test case → Should preserve existing test navigation behavior

## Future Enhancements

As outlined in CURSOR_NAVIGATION_ENHANCEMENT.md, additional WASM APIs could be added:

1. `find_workflows_containing_function(functionName)` - Find all workflows using a function
2. `get_node_function_calls(workflowId, nodeId)` - Get functions called by a node

These would enable even richer navigation features like "jump to all usages" and "find largest containing workflow".
