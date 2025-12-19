# Template Strings in Test Arguments

## Status: IMPLEMENTED

This feature has been implemented. See the "Implementation Summary" section at the end for details.

---

## Problem Statement

Currently, test arguments in BAML must be literal `BamlValue` expressions in map-literal syntax. Users cannot invoke `template_string` functions within test arguments. For example, the following is not currently supported:

```baml
template_string Greeting(name: string) #"
Hello, {{ name }}!
"#

function SayHello(message: string) -> string {
  client GPT4
  prompt #"{{ message }}"#
}

test MyTest {
  functions [SayHello]
  args {
    message: Greeting("World")  // NOT currently supported
  }
}
```

## Current Architecture Analysis

### 1. Test Arguments Parsing Flow

The flow from BAML source to test execution:

```
BAML Source
    ↓
Parser (pest grammar)
    ↓
Expression AST (ast/src/ast/expression.rs)
    ↓
to_unresolved_value() conversion
    ↓
UnresolvedValue<Span> (baml-types/src/value_expr.rs)
    ↓
TestCase in ParserDatabase (parser-database/src/types/mod.rs)
    ↓
TestCase in IR (baml-core/src/ir/repr.rs:2787-2793)
    ↓
test_case_params() resolution (baml-core/src/ir/walker.rs:312-319)
    ↓
BamlValue (baml-runtime)
```

### 2. Key Source Files

| File | Purpose |
|------|---------|
| `engine/baml-lib/ast/src/ast/expression.rs:647-743` | `Expression::to_unresolved_value()` - converts AST expressions to `UnresolvedValue` |
| `engine/baml-lib/baml-types/src/value_expr.rs:14-26` | `Resolvable<Id, Meta>` enum - the core value representation |
| `engine/baml-lib/baml-types/src/value_expr.rs:202-259` | `StringOr` enum - handles env vars, literal strings, and Jinja expressions |
| `engine/baml-lib/baml-types/src/value_expr.rs:229-235` | `StringOr::resolve()` - resolves to final string value (**has `todo!()` for JinjaExpression**) |
| `engine/baml-lib/parser-database/src/types/configurations.rs:203-300` | `visit_test_case()` - parses test definitions |
| `engine/baml-lib/baml-core/src/ir/repr.rs:2786-2886` | `TestCase` IR representation |
| `engine/baml-lib/baml-core/src/ir/walker.rs:312-319` | `test_case_params()` - resolves args to `BamlValue` |
| `engine/baml-lib/baml-core/src/ir/repr.rs:1924-1965` | `TemplateString` IR representation |
| `engine/baml-lib/jinja-runtime/src/lib.rs:197-201` | `TemplateStringMacro` - runtime template representation |
| `engine/baml-runtime/src/internal/prompt_renderer/mod.rs:128-165` | `render_prompt()` - how templates are used in prompts |

### 3. Current Blockers

**Blocker 1: `Expression::App` returns `None`**

In `engine/baml-lib/ast/src/ast/expression.rs:732`:
```rust
Expression::App(_) => None,  // Function applications can't be converted
```

This means `MyTemplate("arg")` cannot be converted to an `UnresolvedValue`.

**Blocker 2: `StringOr::JinjaExpression` has unimplemented `resolve()`**

In `engine/baml-lib/baml-types/src/value_expr.rs:233`:
```rust
Self::JinjaExpression(_) => todo!("Jinja expressions cannot yet be resolved"),
```

Even if we could represent template calls, we can't evaluate them.

**Blocker 3: No access to IR during resolution**

`StringOr::resolve()` only has access to an `EvaluationContext` (env vars), not the IR which contains template_string definitions.

## Proposed Implementation

### Approach: Add `TemplateStringCall` variant to `StringOr`

This approach adds native support for template_string invocations as a special case of `StringOr`.

### Step 1: Extend `StringOr` enum

**File: `engine/baml-lib/baml-types/src/value_expr.rs`**

Add a new variant to `StringOr`:

```rust
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum StringOr {
    EnvVar(String),
    Value(String),
    JinjaExpression(JinjaExpression),
    // NEW: Template string invocation
    TemplateStringCall {
        name: String,
        args: Vec<(String, Resolvable<StringOr, ()>)>,  // (param_name, value)
    },
}
```

### Step 2: Parse `Expression::App` as `TemplateStringCall`

**File: `engine/baml-lib/ast/src/ast/expression.rs`**

Modify `to_unresolved_value()` to handle `Expression::App`:

```rust
Expression::App(app) => {
    // Convert function application to TemplateStringCall
    // Note: At this stage we don't know if it's a template_string -
    // that validation happens later
    let args = app.args.iter()
        .zip(/* param names from somewhere */)
        .filter_map(|(arg, param_name)| {
            arg.to_unresolved_value(_diagnostics)
                .map(|v| (param_name.clone(), v))
        })
        .collect();

    Some(UnresolvedValue::String(
        StringOr::TemplateStringCall {
            name: app.name.name().to_string(),
            args,
        },
        app.span().clone(),
    ))
}
```

**Challenge**: At the AST level, we don't have parameter names from the template_string definition. Two options:
1. Use positional arguments `args: Vec<Resolvable<StringOr, ()>>` instead
2. Defer name binding to a later stage

**Recommended**: Use positional arguments at AST level, bind names during IR construction.

### Step 3: Create a new resolution context with IR access

**File: `engine/baml-lib/baml-types/src/value_expr.rs`**

Create a new trait/context for resolution that includes IR access:

```rust
pub trait GetEnvVarAndTemplates: GetEnvVar {
    fn render_template_string(
        &self,
        name: &str,
        args: &[BamlValue],
    ) -> Result<String>;
}
```

Or alternatively, add a new `resolve_with_templates()` method:

```rust
impl StringOr {
    pub fn resolve_with_templates(
        &self,
        ctx: &impl GetEnvVar,
        template_renderer: &impl TemplateRenderer,
    ) -> Result<String> {
        match self {
            Self::EnvVar(name) => ctx.get_env_var(name),
            Self::Value(value) => Ok(value.to_string()),
            Self::JinjaExpression(j) => {
                // Could also be implemented here
                todo!("Jinja expressions")
            }
            Self::TemplateStringCall { name, args } => {
                let resolved_args: Vec<BamlValue> = args.iter()
                    .map(|arg| /* resolve recursively */)
                    .collect::<Result<_>>()?;
                template_renderer.render(name, &resolved_args)
            }
        }
    }
}
```

### Step 4: Implement template rendering at resolution time

**File: `engine/baml-lib/baml-core/src/ir/ir_helpers/mod.rs` (new function)**

```rust
impl IntermediateRepr {
    pub fn render_template_string(
        &self,
        name: &str,
        args: &BamlMap<String, BamlValue>,
    ) -> Result<String> {
        let template = self.find_template_string(name)?;

        // Use minijinja to render the template
        let mut env = minijinja::Environment::new();
        env.add_template("template", template.content())?;

        let tmpl = env.get_template("template")?;
        let rendered = tmpl.render(args.to_minijinja_value())?;

        Ok(rendered)
    }
}
```

### Step 5: Update `test_case_params()` to use new resolution

**File: `engine/baml-lib/baml-core/src/ir/walker.rs`**

Modify `test_case_params()` to pass the IR for template resolution:

```rust
pub fn test_case_params(
    &self,
    ctx: &EvaluationContext<'_>,
) -> Result<IndexMap<String, Result<BamlValue>>> {
    self.args()
        .iter()
        .map(|(k, v)| {
            let resolved = v.resolve_with_templates(ctx, self.ir)?;
            Ok((k.clone(), serde_json::from_value(resolved)?))
        })
        .collect()
}
```

### Step 6: Add validation for template_string calls

**File: `engine/baml-lib/baml-core/src/validate/validation_pipeline/validations/` (new file or extend existing)**

Add validation to ensure:
1. The called function name exists as a `template_string`
2. Argument count matches parameter count
3. Argument types are compatible with parameter types

### Alternative Approaches Considered

#### Approach B: Evaluate as Jinja Expressions

Instead of a new `StringOr` variant, treat template_string calls as Jinja macro invocations:

```baml
args {
    message: {{ Greeting("World") }}  // Jinja syntax
}
```

**Pros:**
- Leverages existing Jinja infrastructure
- More powerful (can use other Jinja features)

**Cons:**
- Different syntax than normal BAML
- More complex to implement (need full Jinja environment at resolution time)
- Users might expect Jinja features that aren't available

#### Approach C: Compile-time Expansion

Expand template_string calls at compile time in the parser/IR construction phase.

**Pros:**
- Simple runtime (no new resolution logic)
- Values are "inlined"

**Cons:**
- Less flexible (can't use env vars in template args)
- Loses information about the original call structure
- Can't support runtime-only features (like file references)

## Implementation Order

1. **Phase 1: Data Model Changes**
   - Add `TemplateStringCall` variant to `StringOr`
   - Update `Hash`, `Clone`, `Debug`, `Display` implementations
   - Update `required_env_vars()` to recurse into template args

2. **Phase 2: Parsing Changes**
   - Modify `Expression::to_unresolved_value()` to handle `App`
   - Decide on positional vs named argument handling

3. **Phase 3: Resolution Infrastructure**
   - Create `resolve_with_templates()` method or equivalent
   - Implement template rendering logic
   - Integrate with existing jinja-runtime where possible

4. **Phase 4: Integration**
   - Update `test_case_params()` to use new resolution
   - Add validation for template_string calls
   - Update error messages

5. **Phase 5: Testing**
   - Unit tests for parsing template_string calls
   - Unit tests for resolution
   - Integration tests with actual test execution
   - Error case testing

## Open Questions

1. **Nested template calls**: Should `Greeting(OtherTemplate("arg"))` be supported?
   - Adds complexity but is more powerful
   - Recommendation: Support in Phase 1, it should "just work" with recursive resolution

2. **Type checking**: Should we validate that template args match expected types?
   - Template strings are stringly-typed (output is always string)
   - But input parameters have types
   - Recommendation: Add type validation in Phase 4

3. **Return type**: Template strings always return strings. What if used where non-string is expected?
   - Could auto-parse JSON output for structured types
   - Recommendation: Keep simple initially (string only), add JSON parsing later if needed

4. **Syntax**: Is `TemplateName("arg")` the right syntax, or should it be something else?
   - Current function call syntax is intuitive
   - Recommendation: Keep the function call syntax

## Files Changed Summary

| File | Change |
|------|--------|
| `engine/baml-lib/baml-types/src/value_expr.rs` | Add `TemplateStringCall` variant, update resolution |
| `engine/baml-lib/ast/src/ast/expression.rs` | Handle `Expression::App` in `to_unresolved_value()` |
| `engine/baml-lib/baml-core/src/ir/walker.rs` | Update `test_case_params()` |
| `engine/baml-lib/baml-core/src/ir/ir_helpers/mod.rs` | Add template rendering helper |
| `engine/baml-lib/baml-core/src/validate/...` | Add validation for template_string calls |
| `engine/baml-lib/jinja-runtime/src/lib.rs` | Possibly reuse rendering logic |

---

## Implementation Summary

The feature has been fully implemented. Here's what was done:

### Files Modified

| File | Changes |
|------|---------|
| `engine/baml-lib/baml-types/src/value_expr.rs` | Added `TemplateStringCall` variant to `StringOr`, added `TemplateStringRenderer` trait, added `resolve_with_templates()` methods, added `PartialEq`/`Eq` implementations for `Resolvable` |
| `engine/baml-lib/baml-types/src/lib.rs` | Exported new types `TemplateStringRenderer` and `NoTemplateRenderer` |
| `engine/baml-lib/ast/src/ast/expression.rs` | Modified `Expression::to_unresolved_value()` to handle `Expression::App` and create `TemplateStringCall` |
| `engine/baml-lib/baml-core/src/ir/walker.rs` | Updated both `test_case_params()` implementations to use `resolve_serde_with_templates()` |
| `engine/baml-lib/baml-core/src/ir/ir_helpers/mod.rs` | Implemented `TemplateStringRenderer` for `IntermediateRepr` |
| `engine/baml-lib/llm-client/src/clients/fallback.rs` | Added match case for `TemplateStringCall` |
| `engine/baml-lib/llm-client/src/clients/round_robin.rs` | Added match case for `TemplateStringCall` |

### Key Design Decisions

1. **Positional Arguments**: Template string calls use positional arguments at the AST level (parameter names are bound during rendering using the IR's template definition).

2. **Trait-based Resolution**: A `TemplateStringRenderer` trait is defined in `baml-types` and implemented by `IntermediateRepr` in `baml-core`. This allows the resolution code in `baml-types` to render templates without a circular dependency.

3. **Recursive Resolution**: Template arguments are resolved recursively, so nested template calls like `Greeting(OtherTemplate("arg"))` work automatically.

4. **Validation**: Template name validation and argument count validation happen at render time via `find_template_string()` which returns a helpful error if the template doesn't exist.

### Example Usage

```baml
template_string Greeting(name: string) #"
Hello, {{ name }}!
"#

function SayHello(message: string) -> string {
  client GPT4
  prompt #"{{ message }}"#
}

test MyTest {
  functions [SayHello]
  args {
    message: Greeting("World")  // NOW SUPPORTED!
  }
}
```

The test argument `message` will be resolved to `"Hello, World!\n"` at test execution time.
