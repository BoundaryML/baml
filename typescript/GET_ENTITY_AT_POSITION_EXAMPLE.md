# Using `get_entity_at_position` API

## Overview

The `get_entity_at_position` WASM API provides precise entity resolution at a cursor position. It returns the most specific entity at that position:
- If the cursor is inside a workflow node → returns node information
- Otherwise → returns the containing function

## API

```typescript
interface WasmRuntime {
  get_entity_at_position(
    file_name: string,
    cursor_idx: number
  ): WasmEntityAtPosition | null
}

interface WasmEntityAtPosition {
  entity_type: string  // "function" | "node"
  entity_name: string  // Name of the function
  span: WasmSpan       // Span of the entity
  function_type?: "Llm" | "Expr"
  node_id?: string     // Present if entity_type === "node"
  node_label?: string  // Present if entity_type === "node"
}

interface WasmSpan {
  file_path: string
  start: number        // Character index
  end: number          // Character index
  start_line: number
  start_column: number
  end_line: number
  end_column: number
}
```

## Example Usage

### Scenario 1: Cursor in a standalone function

```baml
function ValidateInput(input: string) -> bool {
  client GPT4
  //  ^ Cursor at position 50 (example)
  prompt #"Validate: {{ input }}"#
}
```

```typescript
const entity = runtime.get_entity_at_position("file.baml", 50)

// Returns:
{
  entity_type: "function",
  entity_name: "ValidateInput",
  function_type: "Llm",
  span: {
    file_path: "file.baml",
    start: 0,
    end: 120,
    start_line: 0,
    start_column: 0,
    end_line: 3,
    end_column: 1
  },
  node_id: null,
  node_label: null
}
```

### Scenario 2: Cursor inside a workflow node

```baml
function MyWorkflow(input: string) -> string {
  client GPT4

  // ### validate input
  result = ValidateInput(input)
  //       ^ Cursor at position 150 (example)

  if (result.isValid) {
    return "ok"
  }
}
```

```typescript
const entity = runtime.get_entity_at_position("file.baml", 150)

// Returns:
{
  entity_type: "node",
  entity_name: "MyWorkflow",
  function_type: "Llm",
  span: {
    file_path: "file.baml",
    start: 80,
    end: 160,
    start_line: 3,
    start_column: 2,
    end_line: 4,
    end_column: 30
  },
  node_id: "MyWorkflow|root:0|hdr:validate-input:0",
  node_label: "validate input"
}
```

## Integration with Navigation System

```typescript
// In updateCursor handler
async function updateCursor(
  line: number,
  column: number,
  filePath: string,
  runtime: WasmRuntime,
  dispatchNavigation: (input: NavigationInput) => void
) {
  // Convert line/column to character index
  const cursorIdx = lineColToIndex(filePath, line, column)

  // Get entity at position
  const entity = runtime.get_entity_at_position(filePath, cursorIdx)

  if (!entity) {
    return // No entity at cursor
  }

  // Dispatch navigation based on entity type
  if (entity.entity_type === "node" && entity.node_id) {
    // Navigate to workflow node
    dispatchNavigation({
      kind: 'node',
      workflowId: entity.entity_name,
      nodeId: entity.node_id,
      source: 'cursor',
      timestamp: Date.now()
    })
  } else {
    // Navigate to function
    dispatchNavigation({
      kind: 'function',
      functionName: entity.entity_name,
      source: 'cursor',
      timestamp: Date.now()
    })
  }
}
```

## Helper: Line/Column to Character Index

Since `get_entity_at_position` takes a character index, you'll need to convert from line/column:

```typescript
function lineColToIndex(
  filePath: string,
  line: number,
  column: number,
  fileContents: string
): number {
  let currentLine = 0
  let currentColumn = 0
  let charIndex = 0

  for (const char of fileContents) {
    if (currentLine === line && currentColumn === column) {
      return charIndex
    }

    if (char === '\n') {
      currentLine++
      currentColumn = 0
    } else {
      currentColumn++
    }

    charIndex++
  }

  return charIndex
}
```

## Benefits

1. **Simple API** - One function call, one result
2. **Precise** - Returns the most specific entity (node over function)
3. **Reuses existing code** - Built on top of `get_function_at_position` and `function_graph_v2`
4. **No indexing needed** - Uses existing span checking logic
5. **Type-safe** - Strongly typed through WASM bindings

## Next Steps

For the full navigation system, you'll also want:
- `find_workflows_containing_function(functionName: string)` - Find which workflows use a function
- `get_node_function_calls(workflowName: string, nodeId: string)` - Get functions called by a node

See `CURSOR_NAVIGATION_ENHANCEMENT.md` for the complete design.
