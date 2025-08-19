# Implementation Plan: Improve Alias Comparison Warning Messages

## Problem Statement

When users compare enums to their alias values (e.g., `status == "active"` where "active" is an alias for `Status.Active`), the current warning message is:

```
Use `Status.Active` instead of "active" (alias) - comparing enums with strings will soon be deprecated.
```

This message doesn't clearly communicate that **comparing to an alias will always return false** because enums compare by value name, not alias.

## Solution: Use "Did you mean..." Pattern with Clear Explanation

### Chosen Copy (Option A)

```
Did you mean `{}.{}`? Comparing enums to their alias values (like "{}") will always return false.
```

This format:
- Starts with a helpful suggestion
- Clearly explains the comparison will **always be false**
- Shows the problematic alias value in context

## Implementation Details

### File to Modify
`/Users/sam/baml2/engine/baml-lib/jinja/src/evaluate_type/mod.rs`

### Changes Required

#### 1. Update Exact Alias Match Message (Line ~268-274)

**Current:**
```rust
return Self {
    message: format!(
        "Use `{}.{}` instead of \"{}\" (alias) - comparing enums with strings will soon be deprecated.",
        enum_name, value_for_alias.name, literal_value
    ),
    span,
};
```

**New:**
```rust
return Self {
    message: format!(
        "Did you mean `{}.{}`? Comparing enums to their alias values (like \"{}\") will always return false.",
        enum_name, value_for_alias.name, literal_value
    ),
    span,
};
```

#### 2. Update Case-Insensitive Alias Match Message (Line ~281-287)

**Current:**
```rust
return Self {
    message: format!(
        "Use `{}.{}` instead of \"{}\" (alias) - comparing enums with strings will soon be deprecated.",
        enum_name, value_for_alias.name, literal_value
    ),
    span,
};
```

**New:**
```rust
return Self {
    message: format!(
        "Did you mean `{}.{}`? Comparing enums to their alias values (like \"{}\") will always return false.",
        enum_name, value_for_alias.name, literal_value
    ),
    span,
};
```

## Test Updates Required

### File to Update
`/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_jinja_syntax_validation.baml`

### Expected Warning Updates

Change alias-related warnings from:
```
// warning: Use `Status.Active` instead of "active" (alias) - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_jinja_syntax_validation.baml:41
```

To:
```
// warning: Did you mean `Status.Active`? Comparing enums to their alias values (like "active") will always return false.
//   -->  enum/enum_jinja_syntax_validation.baml:41
```

And similarly for:
```
// warning: Did you mean `Priority.High`? Comparing enums to their alias values (like "urgent") will always return false.
//   -->  enum/enum_jinja_syntax_validation.baml:45
```

## Verification Steps

1. Run the validation test to ensure new messages appear:
   ```bash
   cargo test -p baml-lib --test validation_tests enum_jinja_syntax_validation
   ```

2. Verify the messages are clear and helpful

3. Check that both exact and case-insensitive alias matches produce the new message

## Benefits

1. **Clearer Intent**: "Did you mean..." pattern is widely recognized as a helpful suggestion
2. **Explicit Warning**: States clearly that the comparison will **always return false**
3. **Educational**: Helps users understand that aliases are for display only, not comparison
4. **Actionable**: Provides the correct enum value to use immediately

## Risk Assessment

- **Low Risk**: This is a message-only change
- **No Behavioral Changes**: The type checking logic remains identical
- **Backward Compatible**: Only affects warning message text