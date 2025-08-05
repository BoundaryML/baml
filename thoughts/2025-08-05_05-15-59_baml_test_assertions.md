---
date: 2025-08-05T05:15:59Z
researcher: dex
git_commit: 63f45d4b34b4682b297e024e5ac96b15030a2fcf
branch: canary
repository: baml
topic: "BAML Test Assertions - @assert vs @@assert Issue #1252"
tags: [research, codebase, baml, test-assertions, linter, validation]
status: complete
last_updated: 2025-08-05
last_updated_by: dex
---

# Research: BAML Test Assertions - @assert vs @@assert Issue #1252

**Date**: 2025-08-05T05:15:59Z
**Researcher**: dex
**Git Commit**: 63f45d4b34b4682b297e024e5ac96b15030a2fcf
**Branch**: canary
**Repository**: baml

## Research Question
Issue #1252 reports that BAML tests incorrectly accept @assert (single @) syntax without any linter warnings, but these assertions are silently ignored at runtime. Only @@assert (double @) assertions are actually evaluated during test execution. Need to understand why this happens and where to fix it.

## Summary
The issue occurs because:
1. **Parser accepts both syntaxes**: The parser correctly parses field attributes (@) on test fields
2. **No validation exists**: There's no linter validation that flags @ attributes on test fields as invalid
3. **Runtime ignores field attributes**: The `visit_test_case` function only collects block-level (@@) attributes, so field-level (@) assertions are never added to the test's constraints list

The fix is straightforward: Add validation in the test validator to reject @assert and @check attributes on test fields with a clear error message.

## Detailed Findings

### Test Block Parsing
- Test blocks are parsed in `engine/baml-lib/ast/src/parser/parse_value_expression_block.rs`
- Test blocks are identified by `ValueExprBlockType::Test` (lines 34, 65, 106)
- Block attributes (@@) are parsed at lines 103-126
- Field attributes (@) are parsed when parsing value expressions in fields

### Attribute Grammar
From `engine/baml-lib/ast-lsp/src/lib/internal_ast/src/parser/datamodel.pest`:
- `field_attribute = { "@" ~ identifier ~ arguments_list? }` (line 176)
- `block_attribute = { "@@" ~ identifier ~ arguments_list? }` (line 175)
- Both syntaxes are valid in the grammar, but semantically @ should not be used on test fields

### Test Constraint Collection (THE BUG)
In `engine/baml-lib/parser-database/src/types/configurations.rs:203-300`:
```rust
fn visit_test_case(config: &ConfigBlockProperty, db: &mut ParserDatabase) {
    // ... setup code ...

    // Only collects constraints from config.attributes (block-level @@)
    let constraints = constraint::visit_constraint_attributes(config.attributes.clone(), db);

    // Field attributes (@) on individual fields are completely ignored
    // No code processes f.attributes for constraints
}
```

### Runtime Assertion Evaluation
1. **Field constraints ARE evaluated during parsing**:
   - `engine/baml-lib/jsonish/src/deserializer/coercer/ir_ref/coerce_class.rs:481-488` - `apply_constraints` is called
   - Constraints are evaluated and results stored as flags

2. **But test execution only checks block-level constraints**:
   - `engine/baml-runtime/src/lib.rs:356-373` - `get_test_constraints` only retrieves test block constraints
   - `engine/baml-runtime/src/lib.rs:546-561` - `evaluate_test_constraints` only evaluates block-level constraints
   - Field-level constraint results are ignored for test pass/fail determination

### The Fix Location
The validation should be added in `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/tests.rs`:
```rust
// At the beginning of the validate function
let test_ast = walker.ast_node();
for (_field_id, field) in test_ast.iter_fields() {
    for attr in field.attributes() {
        if attr.name() == "assert" || attr.name() == "check" {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!(
                    "@{} is not allowed on test fields. These attributes can only be used on type fields (in classes) or as block-level attributes in tests.",
                    attr.name()
                ),
                attr.span().clone(),
            ));
        }
    }
}
```

## Code References
- `engine/baml-lib/ast/src/parser/parse_value_expression_block.rs:103-126` - Block attribute parsing in tests
- `engine/baml-lib/parser-database/src/types/configurations.rs:203-300` - Test case visitor (ignores field attributes)
- `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/tests.rs` - Where validation should be added
- `engine/baml-runtime/src/lib.rs:356-373` - Runtime only uses block-level constraints

## Architecture Insights
1. **Attribute System Design**:
   - Single `@` = field-level attributes (for class fields, function parameters)
   - Double `@@` = block-level attributes (for entire blocks: classes, functions, tests)
   - Tests are blocks, therefore require `@@` for assertions

2. **Validation Gap**:
   - Parser accepts both syntaxes (correctly, as they're valid grammar)
   - No semantic validation rejects @ on test fields
   - Runtime assumes only @@ is used in tests

3. **Clean Architecture**:
   - Clear separation between parsing (syntax) and validation (semantics)
   - Fix belongs in validation layer, not parser

## Historical Context (from thoughts/)
- `thoughts/shared/issues/issue-1252.md` - Contains the original issue report (ENG-1252)
- The issue explicitly states: "This should show a linter error since tests only allow @@assert"
- Changelog mentions fixes for `@@assert` syntax highlighting, indicating ongoing work on proper handling

## Related Research
None found in thoughts/shared/research/ yet.

## Open Questions
1. Should the error message suggest the correct syntax (@@assert)?
2. Are there other contexts where field attributes are incorrectly accepted?
3. Should we add a test case to ensure this validation works correctly?
