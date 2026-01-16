# BAML IDE Features Status

This document tracks the current implementation status of IDE features for BAML based on test results.

## Test Infrastructure ✅

The cursor-based test infrastructure is fully functional and supports:
- `<[CURSOR]` markers to indicate cursor position
- Multi-file test scenarios
- Consistent API for hover, goto-definition, and find-references

## Goto-Definition Feature

### ✅ Working (7/13 tests passing)
- **Local variables**: Can navigate to `let` bindings within functions
- **Parameters**: Can navigate to function parameters
- **Variables in blocks**: Can navigate to variables defined in nested blocks
- **Match pattern bindings**: Can navigate to pattern-bound variables (though limited)
- **Undefined references**: Correctly reports "No definition found"
- **Word detection**: Correctly identifies words/identifiers at cursor position

### ❌ Not Working (6/13 tests failing)
- **Function calls**: Cannot navigate to function definitions when calling them
- **Class references**: Cannot navigate to class definitions when using class names
- **Enum variants**: Cannot navigate to enum or variant definitions
- **Field access**: Cannot navigate to field definitions in classes
- **Multi-file navigation**: Cannot navigate across files
- **Match scrutinee**: Has issues with finding the correct expression in some cases

## Find-All-References Feature

### ✅ Working (3/10 tests passing)
- **Pattern bindings**: Can find references in limited scenarios
- **Fields**: Can find some field references (though limited)
- **No references**: Correctly identifies when there are no references

### ❌ Not Working (7/10 tests failing)
- **Local variables**: Cannot find all references to local variables
- **Parameters**: Cannot find all references to function parameters
- **Functions**: Cannot find all references to function calls
- **Classes**: Cannot find all references to class usage
- **Enums**: Cannot find all references to enum usage
- **Cross-block references**: Cannot find references across nested blocks
- **Multi-file references**: Cannot find references across files

## Root Causes

Based on the test results, the main issues appear to be:

1. **Symbol Resolution**: The system can resolve local variables and parameters but struggles with:
   - Global symbols (functions, classes, enums)
   - Cross-file references
   - Type references vs value references

2. **Expression Tracking**: While we fixed the span tracking for expressions, there are still issues with:
   - Complex expressions (function calls, field access)
   - Expression type inference

3. **Scope Handling**: The system handles simple local scopes but has issues with:
   - Global scope
   - Cross-file scope
   - Type namespace vs value namespace

## Next Steps for Full Implementation

To get all tests passing, the following work is needed:

1. **Enhance Symbol Table**:
   - Track definition spans for all global symbols (functions, classes, enums)
   - Implement proper FQN (Fully Qualified Name) resolution
   - Support cross-file symbol lookup

2. **Improve Type Inference**:
   - Track resolutions for all expression types
   - Handle method calls and field access
   - Support enum variant resolution

3. **Implement Reference Finding**:
   - Build a proper index of all symbol usages
   - Support incremental updates
   - Handle different types of references (type vs value)

4. **Multi-file Support**:
   - Properly handle project-wide symbol resolution
   - Track dependencies between files
   - Support workspace-wide searches

## Summary

The test suite successfully validates that:
- ✅ The test infrastructure works well
- ✅ Basic local variable and parameter navigation works
- ✅ The HIR span tracking improvements are effective
- ❌ Global symbol navigation needs implementation
- ❌ Cross-file features need work
- ❌ Find-all-references needs significant enhancement

The failing tests provide a clear roadmap for what needs to be implemented to achieve full IDE functionality.