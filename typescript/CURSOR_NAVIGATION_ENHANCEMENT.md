# Cursor-Based Navigation Enhancement

> **Audience:** SDE2+ engineers
>
> **Goal:** Use cursor position + existing span data to enable precise navigation
>
> **Related:** NAVIGATION_DESIGN_V2.md

---

## Overview

### The Simple Approach

We already have:
- ✅ Span information for all entities (functions, tests, etc.)
- ✅ Node spans from `function_graph_v2`
- ✅ Function call information in the AST

We just need to:
1. Check if cursor position is inside entity spans
2. Check if cursor is inside a workflow node span
3. Find which function(s) a node calls

---

## Required WASM APIs

### 1. `list_entities_at_position` (Enhance existing)

**What it does:** Return all entities whose span contains the cursor position

```rust
#[wasm_bindgen]
impl BamlRuntime {
    /// Get all entities at a specific position
    /// Returns entities ordered by span size (smallest/most specific first)
    pub fn list_entities_at_position(&self, line: u32, column: u32) -> JsValue {
        let mut entities = Vec::new();

        // Check all functions
        for func in self.functions.values() {
            if func.span.contains(line, column) {
                entities.push(EntityAtPosition {
                    entity_type: "function",
                    entity_name: func.name.clone(),
                    span: func.span.clone(),
                    function_type: Some(func.function_type.clone()),
                    file_path: func.file_path.clone(),
                });
            }
        }

        // Check all tests
        for test in &self.tests {
            if test.span.contains(line, column) {
                entities.push(EntityAtPosition {
                    entity_type: "test",
                    entity_name: test.name.clone(),
                    span: test.span.clone(),
                    file_path: test.file_path.clone(),
                    ..Default::default()
                });
            }
        }

        // Check all classes, enums, etc.
        // ... similar checks for other entity types

        // Sort by span size (smallest first = most specific)
        entities.sort_by_key(|e| e.span.size());

        serde_wasm_bindgen::to_value(&entities).unwrap()
    }
}
```

**TypeScript Interface:**

```typescript
interface EntityAtPosition {
  entityType: 'function' | 'test' | 'class' | 'enum' | 'client' | 'retry_policy'
  entityName: string
  span: { start: { line: number; column: number }; end: { line: number; column: number } }
  functionType?: 'llm' | 'workflow' | 'variant_of'
  filePath: string
}

interface BamlRuntime {
  list_entities_at_position(line: number, column: number): EntityAtPosition[]
}
```

---

### 2. `find_workflow_node_at_position` (NEW)

**What it does:** Check if cursor is inside a workflow node, return node info

```rust
#[wasm_bindgen]
impl BamlRuntime {
    /// Find which workflow node (if any) contains this position
    pub fn find_workflow_node_at_position(
        &self,
        workflow_name: &str,
        line: u32,
        column: u32
    ) -> JsValue {
        let graph = match self.function_graph_v2(workflow_name) {
            Some(g) => g,
            None => return JsValue::NULL,
        };

        // Check each node's span
        for node in &graph.nodes {
            if node.span.contains(line, column) {
                return serde_wasm_bindgen::to_value(&WorkflowNodeInfo {
                    node_id: node.id.clone(),
                    node_label: node.label.clone(),
                    node_type: node.node_type.clone(),
                    span: node.span.clone(),
                }).unwrap();
            }
        }

        JsValue::NULL
    }
}
```

**TypeScript Interface:**

```typescript
interface WorkflowNodeInfo {
  nodeId: string
  nodeLabel: string
  nodeType: 'header' | 'if' | 'else' | 'implicit'
  span: { start: { line: number; column: number }; end: { line: number; column: number } }
}

interface BamlRuntime {
  find_workflow_node_at_position(
    workflowName: string,
    line: number,
    column: number
  ): WorkflowNodeInfo | null
}
```

---

### 3. `get_node_function_calls` (NEW)

**What it does:** Return which functions are called by a specific node

```rust
#[wasm_bindgen]
impl BamlRuntime {
    /// Get all functions called within a workflow node
    pub fn get_node_function_calls(&self, workflow_name: &str, node_id: &str) -> JsValue {
        // Get the node from the graph
        let graph = match self.function_graph_v2(workflow_name) {
            Some(g) => g,
            None => return serde_wasm_bindgen::to_value(&Vec::<String>::new()).unwrap(),
        };

        let node = match graph.nodes.iter().find(|n| n.id == node_id) {
            Some(n) => n,
            None => return serde_wasm_bindgen::to_value(&Vec::<String>::new()).unwrap(),
        };

        // Extract function names from the node's AST
        let function_names = self.extract_function_calls_from_node(node);

        serde_wasm_bindgen::to_value(&function_names).unwrap()
    }

    /// Internal: Extract function call names from a node's AST
    fn extract_function_calls_from_node(&self, node: &WorkflowNode) -> Vec<String> {
        let mut function_names = Vec::new();

        // Traverse the node's expressions to find function calls
        // This requires walking the AST
        for expr in &node.expressions {
            if let Expression::FunctionCall(call) = expr {
                function_names.push(call.function_name.clone());
            }
            // Handle nested expressions (e.g., in if conditions)
            function_names.extend(self.extract_nested_calls(expr));
        }

        function_names
    }
}
```

**TypeScript Interface:**

```typescript
interface BamlRuntime {
  // Returns array of function names called by this node
  get_node_function_calls(workflowName: string, nodeId: string): string[]
}
```

---

### 4. `find_workflows_containing_function` (NEW)

**What it does:** Find all workflows that call a given function

```rust
#[wasm_bindgen]
impl BamlRuntime {
    /// Find all workflows that call the specified function
    pub fn find_workflows_containing_function(&self, function_name: &str) -> JsValue {
        let mut results = Vec::new();

        // Iterate through all workflow functions
        for func in self.functions.values() {
            if func.function_type != FunctionType::Workflow {
                continue;
            }

            let graph = match self.function_graph_v2(&func.name) {
                Some(g) => g,
                None => continue,
            };

            // Check each node in the workflow
            for node in &graph.nodes {
                let called_functions = self.extract_function_calls_from_node(&node);

                if called_functions.contains(&function_name.to_string()) {
                    results.push(WorkflowMembership {
                        workflow_id: func.name.clone(),
                        node_id: node.id.clone(),
                        node_label: node.label.clone(),
                        node_type: node.node_type.clone(),
                    });
                }
            }
        }

        serde_wasm_bindgen::to_value(&results).unwrap()
    }
}
```

**TypeScript Interface:**

```typescript
interface WorkflowMembership {
  workflowId: string
  nodeId: string
  nodeLabel: string
  nodeType: 'header' | 'if' | 'else' | 'implicit'
}

interface BamlRuntime {
  find_workflows_containing_function(functionName: string): WorkflowMembership[]
}
```

---

## TypeScript Integration

### Enhanced `updateCursor`

```typescript
/**
 * Called when cursor position changes in the editor
 */
async function updateCursor(
  line: number,
  column: number,
  filePath: string,
  runtime: BamlRuntime,
  dispatchNavigation: (input: NavigationInput) => void
) {
  // 1. Get all entities at this position
  const entities = runtime.list_entities_at_position(line, column)

  if (entities.length === 0) {
    // Cursor not on any entity - could clear selection or do nothing
    return
  }

  // 2. Get the primary entity (most specific = first in array)
  const primary = entities[0]

  // 3. If primary is a workflow, check which node we're in
  let workflowContext: { workflowId: string; nodeId: string } | undefined

  if (primary.entityType === 'function' && primary.functionType === 'workflow') {
    const nodeInfo = runtime.find_workflow_node_at_position(
      primary.entityName,
      line,
      column
    )

    if (nodeInfo) {
      workflowContext = {
        workflowId: primary.entityName,
        nodeId: nodeInfo.nodeId
      }
    }
  }

  // 4. Check if we're inside a parent workflow
  const parentWorkflow = entities.find(e =>
    e.entityType === 'function' && e.functionType === 'workflow'
  )

  if (parentWorkflow && !workflowContext) {
    const nodeInfo = runtime.find_workflow_node_at_position(
      parentWorkflow.entityName,
      line,
      column
    )

    if (nodeInfo) {
      workflowContext = {
        workflowId: parentWorkflow.entityName,
        nodeId: nodeInfo.nodeId
      }
    }
  }

  // 5. Dispatch navigation
  const input: NavigationInput = {
    kind: primary.entityType === 'test' ? 'test' :
          workflowContext ? 'node' : 'function',
    source: 'cursor',
    timestamp: Date.now(),
    cursorPosition: { line, column, filePath }
  }

  if (primary.entityType === 'function') {
    input.functionName = primary.entityName
  } else if (primary.entityType === 'test') {
    input.testName = primary.entityName
  }

  if (workflowContext) {
    input.workflowId = workflowContext.workflowId
    input.nodeId = workflowContext.nodeId
  }

  dispatchNavigation(input)
}
```

### Simplified `TargetEnricher`

```typescript
class TargetEnricher {
  constructor(private runtime: BamlRuntime) {}

  async enrich(input: NavigationInput): Promise<EnrichedTarget> {
    if (input.source === 'cursor' && input.cursorPosition) {
      return this.enrichCursorPosition(input)
    }

    if (input.kind === 'node' && input.workflowId && input.nodeId) {
      return this.enrichNodeClick(input)
    }

    if (input.kind === 'function' && input.functionName) {
      return this.enrichFunctionClick(input.functionName)
    }

    if (input.kind === 'test' && input.testName) {
      return this.enrichTestClick(input.testName)
    }

    return this.emptyTarget()
  }

  private enrichCursorPosition(input: NavigationInput): EnrichedTarget {
    const { line, column } = input.cursorPosition!

    // Get entities at position
    const entities = this.runtime.list_entities_at_position(line, column)
    if (entities.length === 0) return this.emptyTarget()

    const primary = entities[0]

    // If clicking in a workflow node
    if (input.workflowId && input.nodeId) {
      const calledFunctions = this.runtime.get_node_function_calls(
        input.workflowId,
        input.nodeId
      )
      const primaryFunction = calledFunctions[0] || null

      return {
        name: primaryFunction || input.nodeId,
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: input.workflowId,
          nodeId: input.nodeId,
          nodeLabel: '',
          nodeType: 'header'
        }],
        availableTests: primaryFunction
          ? this.runtime.getTestsForFunction(primaryFunction).map(t => t.name)
          : [],
        directWorkflowContext: {
          workflowId: input.workflowId,
          nodeId: input.nodeId
        }
      }
    }

    // Regular function/test
    if (primary.entityType === 'function') {
      return this.enrichFunctionClick(primary.entityName)
    }

    if (primary.entityType === 'test') {
      return this.enrichTestClick(primary.entityName)
    }

    return this.emptyTarget()
  }

  private enrichFunctionClick(functionName: string): EnrichedTarget {
    const workflowMemberships = this.runtime.find_workflows_containing_function(functionName)
    const availableTests = this.runtime.getTestsForFunction(functionName).map(t => t.name)

    return {
      name: functionName,
      kind: 'function',
      exists: this.runtime.getFunctions().some(f => f.name === functionName),
      workflowMemberships,
      availableTests
    }
  }

  private enrichNodeClick(input: NavigationInput): EnrichedTarget {
    const calledFunctions = this.runtime.get_node_function_calls(
      input.workflowId!,
      input.nodeId!
    )
    const primaryFunction = calledFunctions[0] || null

    return {
      name: input.nodeId!,
      kind: 'node',
      exists: true,
      workflowMemberships: [{
        workflowId: input.workflowId!,
        nodeId: input.nodeId!,
        nodeLabel: '',
        nodeType: 'header'
      }],
      availableTests: primaryFunction
        ? this.runtime.getTestsForFunction(primaryFunction).map(t => t.name)
        : [],
      directWorkflowContext: {
        workflowId: input.workflowId!,
        nodeId: input.nodeId!
      }
    }
  }

  private enrichTestClick(testName: string): EnrichedTarget {
    const test = this.runtime.getTests().find(t => t.name === testName)
    const functionName = test?.functionId

    return {
      name: testName,
      kind: 'test',
      exists: !!test,
      workflowMemberships: functionName
        ? this.runtime.find_workflows_containing_function(functionName)
        : [],
      availableTests: functionName
        ? this.runtime.getTestsForFunction(functionName).map(t => t.name)
        : [],
      functionName
    }
  }

  private emptyTarget(): EnrichedTarget {
    return {
      name: '',
      kind: 'function',
      exists: false,
      workflowMemberships: [],
      availableTests: []
    }
  }
}
```

---

## Example Flow

### Scenario: User clicks inside a workflow node

**BAML Code:**
```baml
function MyWorkflow(input: string) -> string {
  client GPT4

  // ### validate input
  result = ValidateInput(input)
  //        ^^^^^^^^^^^^^ cursor here (line 5, column 12)

  if (result.isValid) {
    return "ok"
  }
}
```

**Step 1: TypeScript calls WASM**
```typescript
const entities = runtime.list_entities_at_position(5, 12)
// Returns:
// [
//   { entityType: 'function', entityName: 'ValidateInput', functionType: 'llm', ... },
//   { entityType: 'function', entityName: 'MyWorkflow', functionType: 'workflow', ... }
// ]
```

**Step 2: Check if inside workflow node**
```typescript
const nodeInfo = runtime.find_workflow_node_at_position('MyWorkflow', 5, 12)
// Returns:
// {
//   nodeId: 'MyWorkflow|root:0|hdr:validate-input:0',
//   nodeLabel: 'validate input',
//   nodeType: 'header',
//   span: { start: { line: 4, column: 2 }, end: { line: 5, column: 30 } }
// }
```

**Step 3: Get functions called by node**
```typescript
const calledFunctions = runtime.get_node_function_calls(
  'MyWorkflow',
  'MyWorkflow|root:0|hdr:validate-input:0'
)
// Returns: ['ValidateInput']
```

**Step 4: Find workflow memberships**
```typescript
const memberships = runtime.find_workflows_containing_function('ValidateInput')
// Returns:
// [
//   {
//     workflowId: 'MyWorkflow',
//     nodeId: 'MyWorkflow|root:0|hdr:validate-input:0',
//     nodeLabel: 'validate input',
//     nodeType: 'header'
//   }
// ]
```

**Step 5: Navigate**
```typescript
// Navigation system receives:
{
  mode: 'workflow',
  workflowId: 'MyWorkflow',
  selectedNodeId: 'MyWorkflow|root:0|hdr:validate-input:0',
  functionName: 'ValidateInput',
  testName: null
}
```

---

## Rust Implementation Notes

### The Key Helper: `extract_function_calls_from_node`

This is the only complex part - walking the AST to find function calls:

```rust
impl BamlRuntime {
    fn extract_function_calls_from_node(&self, node: &WorkflowNode) -> Vec<String> {
        let mut function_names = Vec::new();

        // Node has expressions from the BAML AST
        for expr in &node.expressions {
            self.collect_function_calls(expr, &mut function_names);
        }

        function_names
    }

    fn collect_function_calls(&self, expr: &Expression, acc: &mut Vec<String>) {
        match expr {
            Expression::FunctionCall(call) => {
                acc.push(call.function_name.clone());
                // Also check arguments for nested calls
                for arg in &call.arguments {
                    self.collect_function_calls(arg, acc);
                }
            }
            Expression::Assignment(assign) => {
                self.collect_function_calls(&assign.value, acc);
            }
            Expression::If(if_expr) => {
                self.collect_function_calls(&if_expr.condition, acc);
                for expr in &if_expr.then_block {
                    self.collect_function_calls(expr, acc);
                }
                if let Some(else_block) = &if_expr.else_block {
                    for expr in else_block {
                        self.collect_function_calls(expr, acc);
                    }
                }
            }
            Expression::Binary(binary) => {
                self.collect_function_calls(&binary.left, acc);
                self.collect_function_calls(&binary.right, acc);
            }
            // Handle other expression types as needed
            _ => {}
        }
    }
}
```

### Span Checking

Should already exist, but if not:

```rust
impl Span {
    pub fn contains(&self, line: u32, column: u32) -> bool {
        let pos_after_start = line > self.start.line ||
            (line == self.start.line && column >= self.start.column);

        let pos_before_end = line < self.end.line ||
            (line == self.end.line && column <= self.end.column);

        pos_after_start && pos_before_end
    }

    pub fn size(&self) -> usize {
        let line_diff = (self.end.line - self.start.line) as usize;
        let col_diff = if line_diff == 0 {
            (self.end.column - self.start.column) as usize
        } else {
            1000 * line_diff  // Approximate: prioritize smaller line spans
        };
        line_diff * 1000 + col_diff
    }
}
```

---

## Summary

### Simple 4-API Solution

1. **`list_entities_at_position(line, col)`** - What entities contain this position?
2. **`find_workflow_node_at_position(workflow, line, col)`** - Which node (if any)?
3. **`get_node_function_calls(workflow, nodeId)`** - What functions does this node call?
4. **`find_workflows_containing_function(functionName)`** - Which workflows use this function?

All use existing span data - no indexing, no caching, just simple lookups.

---

## Migration Checklist

### Rust (WASM)
- [ ] Ensure `list_entities_at_position` returns all entity types with spans
- [ ] Implement `find_workflow_node_at_position`
- [ ] Implement `get_node_function_calls`
- [ ] Implement `find_workflows_containing_function`
- [ ] Implement `extract_function_calls_from_node` (AST walker)
- [ ] Add TypeScript type definitions
- [ ] Write Rust tests

### TypeScript
- [ ] Update `updateCursor` to call new WASM APIs
- [ ] Simplify `TargetEnricher` to use WASM data
- [ ] Add debouncing to cursor updates
- [ ] Write integration tests
- [ ] Test with real BAML files

---

**Document Version:** 1.0
**Created:** 2025-01-13
**Status:** Design Proposal
