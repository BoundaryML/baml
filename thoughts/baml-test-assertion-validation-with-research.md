# BAML Test Assertion Validation Implementation Plan

## Overview

Fix validation issue where BAML tests incorrectly accept `@assert` (single @) syntax without warnings. These field-level assertions are silently ignored at runtime. Only block-level `@@assert` (double @) assertions work correctly for tests.

## Current State Analysis

The parser correctly accepts both @ (field-level) and @@ (block-level) attributes as valid grammar, but the test runtime only evaluates block-level attributes. There's no semantic validation to reject field-level attributes on test fields.

### Key Discoveries:
- Parser accepts both syntaxes in `engine/baml-lib/ast/src/parser/parse_value_expression_block.rs:103-126`
- Test visitor only collects block-level attributes in `engine/baml-lib/parser-database/src/types/configurations.rs:265-275`
- No validation exists in `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/tests.rs` to reject field attributes
- Similar validation pattern exists for type aliases in `engine/baml-lib/parser-database/src/attributes/mod.rs:217`

## What We're NOT Doing

- Changing the parser grammar (it correctly accepts both syntaxes)
- Modifying the runtime behavior (it correctly only uses block-level attributes)
- Adding support for field-level assertions in tests
- Changing how block-level assertions work

## Implementation Approach

Add semantic validation to reject `@assert` and `@check` attributes on test fields with a clear error message. Follow the established pattern used for type alias attribute restrictions.

## Phase 1: Add Validation for Field Attributes in Tests

### Overview
Add validation logic to detect and reject field-level assertion attributes on test blocks.

### Changes Required:

#### 1. Test Validator Enhancement
**File**: `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/tests.rs`
**Changes**: Add validation at the beginning of the validate function

```rust
pub(super) fn validate(ctx: &mut Context<'_>) {
    let tests = ctx.db.walk_test_cases().collect::<Vec<_>>();
    tests.iter().for_each(|walker| {
        // NEW: Validate that test fields don't have @assert or @check attributes
        let test_ast = walker.ast_node();
        for (_field_id, field) in test_ast.iter_fields() {
            for attr in field.attributes() {
                if attr.name() == "assert" || attr.name() == "check" {
                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "@{} is not allowed on test fields. Use @@{} at the test block level instead.",
                            attr.name(),
                            attr.name()
                        ),
                        attr.span().clone(),
                    ));
                }
            }
        }

        // EXISTING: Continue with constraint validation
        let constraints = &walker.test_case().constraints;
        // ... rest of existing code
```

### Success Criteria:

#### Automated Verification:
- [x] Validation test passes: `cargo test test_validation test_field_assertions`
- [x] All existing tests continue to pass: `cargo test`
- [x] Linting passes: `cargo clippy`

#### Manual Verification:
- [x] Error message appears when using @assert on test fields
- [x] Error points to the exact location of the invalid attribute
- [x] No false positives for valid @@assert usage

---

## Phase 2: Add Validation Test Case

### Overview
Create a test case to ensure the validation works correctly and prevents regression.

### Changes Required:

#### 1. Create Test Files
**Files**: 
- `engine/baml-lib/baml/tests/validation_files/functions_v2/tests/field_level_assertions_v2.baml`
- `engine/baml-lib/baml/tests/validation_files/functions_v2/tests/field_level_check.baml`

**Changes**: Created test files to validate that field-level attributes are rejected on test block fields

**Note**: The original plan assumed test args could have field-level attributes, but test args are key-value pairs, not fields with attributes. The validation correctly catches attributes on test block fields like `functions` and `args`.

### Success Criteria:

#### Automated Verification:
- [x] Test file is automatically included in validation test suite
- [x] Running tests with `UPDATE_EXPECT=1` generates expected error messages
- [x] Test passes when run normally: `cargo test validation_test_field_level_assertions`

#### Manual Verification:
- [x] Error messages match the expected format
- [x] Spans point to the correct attribute locations

---

## Testing Strategy

### Unit Tests:
- Validation test ensures field attributes are rejected
- Test covers both @assert and @check attributes
- Test verifies error message clarity

### Integration Tests:
- Existing test suite ensures no regression
- Valid @@assert tests continue to work

### Manual Testing Steps:
1. Create a BAML file with @assert on test fields
2. Run BAML validation and verify error appears
3. Change to @@assert and verify it works correctly
4. Test with @check attribute as well

## Performance Considerations

The validation adds a nested loop over test fields and their attributes, but:
- Test blocks typically have few fields
- Fields typically have few attributes
- Performance impact is negligible compared to existing validation

## Migration Notes

No migration needed - this is a validation-only change that makes invalid syntax properly error.

## References

- Original ticket: `thoughts/shared/research/2025-08-05_05-15-59_baml_test_assertions.md`
- Issue #1252 in BAML repository
- Similar implementation: `engine/baml-lib/parser-database/src/attributes/mod.rs:217`
- Test validation: `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/tests.rs`
