# LSP V1 vs V2 Diagnostics Diff Report

Compares diagnostics expectations between `baml_lsp_actions_tests` (compiler v1)
and `baml_lsp2_actions_tests` (compiler2), and checks actual compiler2 output.

## Summary

| Metric | Count | Description |
|--------|-------|-------------|
| Total files | 231 | LSP syntax test files |
| Expectations match | 207 | V1 and V2 expect the same diagnostics |
| Expectations differ | 24 | V2 expectations were updated for compiler2 |
| V2 passing | 207 | Actual output matches V2 expectations |
| **V2 failing** | **24** | **Actual output doesn't match V2 expectations** |
| V2 panic | 0 | Compiler2 crashed |

## Files with updated expectations

These files have different expected diagnostics between v1 and v2 LSP tests,
meaning compiler2 intentionally changed behavior here.

| File | V2 passes? | Status |
|------|-----------|--------|
| catch/invalid_arm_syntax.baml | **no** | UPDATED_BUT_FAILING |
| catch/unknown_binding_types.baml | **no** | UPDATED_BUT_FAILING |
| class/map_types2.baml | **no** | UPDATED_BUT_FAILING |
| class/misspeled_boolean_literals.baml | **no** | UPDATED_BUT_FAILING |
| class/secure_types.baml | **no** | UPDATED_BUT_FAILING |
| enum/enum_unquoted_description.baml | **no** | UPDATED_BUT_FAILING |
| expr/expr_full.baml | **no** | UPDATED_BUT_FAILING |
| functions_v2/invalid.baml | **no** | UPDATED_BUT_FAILING |
| functions_v2/invalid2.baml | **no** | UPDATED_BUT_FAILING |
| headers/complex_headers_test.baml | **no** | UPDATED_BUT_FAILING |
| headers/invalid.baml | **no** | UPDATED_BUT_FAILING |
| hover/function_throws.baml | **no** | UPDATED_BUT_FAILING |
| loops/header_requires_let_negative.baml | **no** | UPDATED_BUT_FAILING |
| maps/inconsistent_style.baml | **no** | UPDATED_BUT_FAILING |
| misc/dynamic_types_parser_errors.baml | **no** | UPDATED_BUT_FAILING |
| parens.baml | **no** | UPDATED_BUT_FAILING |
| strings/unquoted_strings.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_caller_sees_contract.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_caller_variant_contract.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_enum_exact_match.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_enum_extraneous.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_enum_variant_precise.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_enum_variant_violation.baml | **no** | UPDATED_BUT_FAILING |
| throw/throws_mixed.baml | **no** | UPDATED_BUT_FAILING |

## Failing files (actual != expected)

Compiler2 output doesn't match the V2 expectations for these files.

### catch/invalid_arm_syntax.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:18 ]
//    │
//  7 │     _: string => "ok",
//    │                  ──┬─  
//    │                    ╰─── unreachable arm
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:23 ]
//    │
//  7 │ ╭─▶     _: string => "ok",
//  8 │ ├─▶     let y = "hi";
//    │ │                       
//    │ ╰─────────────────────── unreachable arm
//    │     
//    │     Note: 
```

V2 expected:
```
// Error: Expected pattern, found let
//    ╭─[ catch_invalid_arm_syntax.baml:8:5 ]
//    │
//  8 │     let y = "hi";
//    │     ─┬─  
//    │      ╰─── Expected pattern, found let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected '=>' after catch pattern, found identifier
//    ╭─[ catch_invalid_arm_syntax.baml:8:9 ]
//    │
//  8 │     let y = "hi";
//    │         ┬  
//    │         ╰── Expected '=>' after catch pattern, found identifier
//    │ 
//    │ Note: Error code: 
```

V2 actual:
```
// Error: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:18 ]
//    │
//  7 │     _: string => "ok",
//    │                  ──┬─  
//    │                    ╰─── unreachable arm
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:23 ]
//    │
//  7 │ ╭─▶     _: string => "ok",
//  8 │ ├─▶     let y = "hi";
//    │ │                       
//    │ ╰─────────────────────── unreachable arm
//    │     
//    │     Note: 
```

### catch/unknown_binding_types.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: invalid catch binding type `any`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │   ─────┬────  
//    │        ╰────── invalid catch binding type `any`; use a concrete type instead
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: invalid catch binding type `unknown`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//   
```

V2 expected:
```
// Error: invalid catch binding type `any`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │   ─────┬────  
//    │        ╰────── invalid catch binding type `any`; use a concrete type instead
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//    ╭─[ catch_unknown_binding_types.baml:7:10 ]
//    │
//  7 │     _ => "caught any"
//    │          ──────┬─────  
//    │                ╰──────
```

V2 actual:
```
// Error: invalid catch binding type `any`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//    │   ─────┬────  
//    │        ╰────── invalid catch binding type `any`; use a concrete type instead
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: invalid catch binding type `unknown`; use a concrete type instead
//    ╭─[ catch_unknown_binding_types.baml:6:3 ]
//    │
//  6 │   MayFail(x) catch (e: any) {
//   
```

### class/map_types2.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved type: map
//     ╭─[ class_map_types2.baml:26:5 ]
//     │
//  26 │   d1 map<>
//     │     ───┬──  
//     │        ╰──── unresolved type: map
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Expected type, found '>'
//     ╭─[ class_map_types2.baml:26:10 ]
//     │
//  26 │   d1 map<>
//     │          ┬  
//     │          ╰── Expected type, found '>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: unresolved type: map
//     ╭─[ class_map_types
```

V2 expected:
```
// Error: Expected type, found '>'
//     ╭─[ class_map_types2.baml:26:10 ]
//     │
//  26 │   d1 map<>
//     │          ┬  
//     │          ╰── Expected type, found '>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: unresolved type: map
//     ╭─[ class_map_types2.baml:26:5 ]
//     │
//  26 │   d1 map<>
//     │     ───┬──  
//     │        ╰──── unresolved type: map
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: unresolved type: map
//     ╭─[ class_map_types
```

V2 actual:
```
// Error: unresolved type: map
//     ╭─[ class_map_types2.baml:26:5 ]
//     │
//  26 │   d1 map<>
//     │     ───┬──  
//     │        ╰──── unresolved type: map
//     │ 
//     │ Note: Error code: E0002
// ────╯
// Error: Expected type, found '>'
//     ╭─[ class_map_types2.baml:26:10 ]
//     │
//  26 │   d1 map<>
//     │          ┬  
//     │          ╰── Expected type, found '>'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: unresolved type: map
//     ╭─[ class_map_types
```

### class/misspeled_boolean_literals.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved type: False
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: False
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: True
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolve
```

V2 expected:
```
// Error: unresolved type: True
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: True
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: False
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved
```

V2 actual:
```
// Error: unresolved type: False
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: False
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: True
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolve
```

### class/secure_types.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved type: apple_pie
//    ╭─[ class_secure_types.baml:3:4 ]
//    │
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    │    ───────────────────────┬──────────────────────  
//    │                           ╰──────────────────────── unresolved type: apple_pie
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: char
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[]
```

V2 expected:
```
// Error: unresolved type: apple_pie
//    ╭─[ class_secure_types.baml:3:4 ]
//    │
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    │    ───────────────────────┬──────────────────────  
//    │                           ╰──────────────────────── unresolved type: apple_pie
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: char
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[]
```

V2 actual:
```
// Error: unresolved type: apple_pie
//    ╭─[ class_secure_types.baml:3:4 ]
//    │
//  3 │   a map<string[], (int | bool[]) | apple_pie[][]>
//    │    ───────────────────────┬──────────────────────  
//    │                           ╰──────────────────────── unresolved type: apple_pie
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: char
//    ╭─[ class_secure_types.baml:4:4 ]
//    │
//  4 │   b (int, map<bool, string?>, (char | float)[][] | long_word_123.foobar[]
```

### enum/enum_unquoted_description.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: Expected ')', found identifier
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected ')', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ N
```

V2 expected:
```
// Error: Expected ')', found identifier
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected ')', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ N
```

V2 actual:
```
// Error: Expected ')', found identifier
//     ╭─[ enum_enum_unquoted_description.baml:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected ')', found identifier
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found ')'
//     ╭─[ enum_enum_unquoted_description.baml:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found ')'
//     │ 
//     │ N
```

### expr/expr_full.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved name: poem
//     ╭─[ expr_expr_full.baml:41:16 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                ──┬─  
//     │                  ╰─── unresolved name: poem
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved name: another
//     ╭─[ expr_expr_full.baml:41:22 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                      ───┬───  
//     │                         ╰───── unresolved name: another
//     │ 
//     │
```

V2 expected:
```
// Error: remove parentheses from test name: `test TestPipeline`
//     ╭─[ expr_expr_full.baml:44:18 ]
//     │
//  44 │ test TestPipeline() {
//     │                  ─┬  
//     │                   ╰── remove parentheses from test name: `test TestPipeline`
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: remove parentheses from test name: `test TestPyramid`
//     ╭─[ expr_expr_full.baml:49:17 ]
//     │
//  49 │ test TestPyramid() {
//     │                 ─┬  
//     │       
```

V2 actual:
```
// Error: unresolved name: poem
//     ╭─[ expr_expr_full.baml:41:16 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                ──┬─  
//     │                  ╰─── unresolved name: poem
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved name: another
//     ╭─[ expr_expr_full.baml:41:22 ]
//     │
//  41 │   CombinePoems(poem, another)
//     │                      ───┬───  
//     │                         ╰───── unresolved name: another
//     │ 
//     │
```

### functions_v2/invalid.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved name: Bar
//    ╭─[ functions_v2_invalid.baml:1:18 ]
//    │
//  1 │ ╭─▶ function Foo() -> {
//    ┆ ┆   
//  4 │ ├─▶ }
//    │ │       
//    │ ╰─────── unresolved name: Bar
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected type, found '{'
//    ╭─[ functions_v2_invalid.baml:1:19 ]
//    │
//  1 │ function Foo() -> {
//    │                   ┬  
//    │                   ╰── Expected type, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
```

V2 expected:
```
// Error: Expected type, found '{'
//    ╭─[ functions_v2_invalid.baml:1:19 ]
//    │
//  1 │ function Foo() -> {
//    │                   ┬  
//    │                   ╰── Expected type, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found ')'
//    ╭─[ functions_v2_invalid.baml:6:20 ]
//    │
//  6 │ function FooBar(arg) -> bar {
//    │                    ┬  
//    │                    ╰── Expected type annotation, found ')'
//    │ 
//    │ No
```

V2 actual:
```
// Error: unresolved name: Bar
//    ╭─[ functions_v2_invalid.baml:1:18 ]
//    │
//  1 │ ╭─▶ function Foo() -> {
//    ┆ ┆   
//  4 │ ├─▶ }
//    │ │       
//    │ ╰─────── unresolved name: Bar
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected type, found '{'
//    ╭─[ functions_v2_invalid.baml:1:19 ]
//    │
//  1 │ function Foo() -> {
//    │                   ┬  
//    │                   ╰── Expected type, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
```

### functions_v2/invalid2.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved name: Foo
//    ╭─[ functions_v2_invalid2.baml:1:1 ]
//    │
//  1 │ ╭─▶ function Foo1(arg: int) -> float {
//    ┆ ┆   
//  3 │ ├─▶ }
//    │ │       
//    │ ╰─────── unresolved name: Foo
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected LLM function missing 'prompt' field, found '}'
//    ╭─[ functions_v2_invalid2.baml:3:1 ]
//    │
//  3 │ }
//    │ ┬  
//    │ ╰── Expected LLM function missing 'prompt' field, found '}'
//    │ 
//    │ Note: Er
```

V2 expected:
```
// Error: Expected LLM function missing 'prompt' field, found '}'
//    ╭─[ functions_v2_invalid2.baml:3:1 ]
//    │
//  3 │ }
//    │ ┬  
//    │ ╰── Expected LLM function missing 'prompt' field, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected LLM function missing 'client' field, found '}'
//    ╭─[ functions_v2_invalid2.baml:7:1 ]
//    │
//  7 │ }
//    │ ┬  
//    │ ╰── Expected LLM function missing 'client' field, found '}'
//    │ 
//    │ Note: Error code: E0
```

V2 actual:
```
// Error: unresolved name: Foo
//    ╭─[ functions_v2_invalid2.baml:1:1 ]
//    │
//  1 │ ╭─▶ function Foo1(arg: int) -> float {
//    ┆ ┆   
//  3 │ ├─▶ }
//    │ │       
//    │ ╰─────── unresolved name: Foo
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected LLM function missing 'prompt' field, found '}'
//    ╭─[ functions_v2_invalid2.baml:3:1 ]
//    │
//  3 │ }
//    │ ┬  
//    │ ╰── Expected LLM function missing 'prompt' field, found '}'
//    │ 
//    │ Note: Er
```

### headers/complex_headers_test.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: Duplicate binding `hello` in `ComplexHeaderTest`
//     ╭─[ headers_complex_headers_test.baml:20:17 ]
//     │
//  11 │         let hello = "Hello";
//     │             ──┬──  
//     │               ╰──── first defined as binding here
//     │ 
//  20 │             let hello = "Hello";
//     │                 ──┬──  
//     │                   ╰──── duplicate binding definition
//     │ 
//  23 │             let hello = "Hello";
//     │                 ──┬──  
//     │             
```

V2 expected:
```
// Error: Duplicate binding `hello` in `ComplexHeaderTest`
//     ╭─[ headers_complex_headers_test.baml:20:17 ]
//     │
//  11 │         let hello = "Hello";
//     │             ──┬──  
//     │               ╰──── first defined as binding here
//     │ 
//  20 │             let hello = "Hello";
//     │                 ──┬──  
//     │                   ╰──── duplicate binding definition
//     │ 
//  23 │             let hello = "Hello";
//     │                 ──┬──  
//     │             
```

V2 actual:
```
// Error: Duplicate binding `hello` in `ComplexHeaderTest`
//     ╭─[ headers_complex_headers_test.baml:20:17 ]
//     │
//  11 │         let hello = "Hello";
//     │             ──┬──  
//     │               ╰──── first defined as binding here
//     │ 
//  20 │             let hello = "Hello";
//     │                 ──┬──  
//     │                   ╰──── duplicate binding definition
//     │ 
//  23 │             let hello = "Hello";
//     │                 ──┬──  
//     │             
```

### headers/invalid.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:21 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                     ┬  
//     │                     ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:22 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                      ┬  
//     │       
```

V2 expected:
```
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:21 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                     ┬  
//     │                     ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:22 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                      ┬  
//     │       
```

V2 actual:
```
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:21 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                     ┬  
//     │                     ╰── Expected expression, found '#'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found '#'
//     ╭─[ headers_invalid.baml:12:22 ]
//     │
//  12 │     let x = "hello" ## Inline Header // Should fail
//     │                      ┬  
//     │       
```

### hover/function_throws.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ hover_function_throws.baml:6:42 
```

V2 expected:
```
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: user.Errors
//    ╭─[ h
```

V2 actual:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ hover_function_throws.baml:6:42 
```

### loops/header_requires_let_negative.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved name: i
//    ╭─[ loops_header_requires_let_negative.baml:4:7 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │          ┬  
//    │          ╰── unresolved name: i
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ';'
//    │ 
//    │ Note: Err
```

V2 expected:
```
// Error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ';'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after for expression, found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                  
```

V2 actual:
```
// Error: unresolved name: i
//    ╭─[ loops_header_requires_let_negative.baml:4:7 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │          ┬  
//    │          ╰── unresolved name: i
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ';'
//    │ 
//    │ Note: Err
```

### maps/inconsistent_style.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved name: name
//    ╭─[ maps_inconsistent_style.baml:8:9 ]
//    │
//  8 │         name "Hello"
//    │         ──┬─  
//    │           ╰─── unresolved name: name
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected expression, found ':'
//    ╭─[ maps_inconsistent_style.baml:9:19 ]
//    │
//  9 │         some_ident: "yes"
//    │                   ┬  
//    │                   ╰── Expected expression, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
```

V2 expected:
```
// Error: Expected expression, found ':'
//    ╭─[ maps_inconsistent_style.baml:9:19 ]
//    │
//  9 │         some_ident: "yes"
//    │                   ┬  
//    │                   ╰── Expected expression, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: unresolved name: name
//    ╭─[ maps_inconsistent_style.baml:8:9 ]
//    │
//  8 │         name "Hello"
//    │         ──┬─  
//    │           ╰─── unresolved name: name
//    │ 
//    │ Note: Error code: E0001
// ───╯
```

V2 actual:
```
// Error: unresolved name: name
//    ╭─[ maps_inconsistent_style.baml:8:9 ]
//    │
//  8 │         name "Hello"
//    │         ──┬─  
//    │           ╰─── unresolved name: name
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected expression, found ':'
//    ╭─[ maps_inconsistent_style.baml:9:19 ]
//    │
//  9 │         some_ident: "yes"
//    │                   ┬  
//    │                   ╰── Expected expression, found ':'
//    │ 
//    │ Note: Error code: E0010
// ───╯
```

### misc/dynamic_types_parser_errors.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: unresolved type: Resume
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:45 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │                                             ───┬───  
//    │                                                ╰───── unresolved type: Resume
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dy
```

V2 expected:
```
// Error: Expected Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dynamic` attribute., found identifier
//     ╭─[ misc_dynamic_types_parser_errors.baml:40:13 ]
//     │
//  40 │     dynamic Bar {
//     │             ─┬─  
//     │              ╰─── Expected Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dynamic` attribute., found identifier
// 
```

V2 actual:
```
// Error: unresolved type: Resume
//    ╭─[ misc_dynamic_types_parser_errors.baml:1:45 ]
//    │
//  1 │ function TypeBuilderFn(from_text: string) -> Resume {
//    │                                             ───┬───  
//    │                                                ╰───── unresolved type: Resume
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected Incomplete 'dynamic' type definition. Use 'dynamic class' or 'dynamic enum' to add properties to types that contain the `@@dy
```

### parens.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: missing return: expected `int`
//     ╭─[ parens.baml:1:22 ]
//     │
//   1 │ ╭─▶ function foo() -> int {
//     ┆ ┆   
//  63 │ ├─▶ }
//     │ │       
//     │ ╰─────── missing return: expected `int`
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: Expected block after if condition, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found ')'
//    │ 
//    │ Note:
```

V2 expected:
```
// Error: Expected block after if condition, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected '
```

V2 actual:
```
// Error: missing return: expected `int`
//     ╭─[ parens.baml:1:22 ]
//     │
//   1 │ ╭─▶ function foo() -> int {
//     ┆ ┆   
//  63 │ ├─▶ }
//     │ │       
//     │ ╰─────── missing return: expected `int`
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: Expected block after if condition, found ')'
//    ╭─[ parens.baml:9:11 ]
//    │
//  9 │    if true) { }
//    │           ┬  
//    │           ╰── Expected block after if condition, found ')'
//    │ 
//    │ Note:
```

### strings/unquoted_strings.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: Expected config key, found error
//    ╭─[ strings_unquoted_strings.baml:4:16 ]
//    │
//  4 │     thing hello'world
//    │                ┬  
//    │                ╰── Expected config key, found error
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found '#'
//    ╭─[ strings_unquoted_strings.baml:6:13 ]
//    │
//  6 │     banned2 #helloworld
//    │             ┬  
//    │             ╰── Expected config key, found '#'
//    │ 
//    │ Note: Error 
```

V2 expected:
```
// Error: Expected config key, found error
//    ╭─[ strings_unquoted_strings.baml:4:16 ]
//    │
//  4 │     thing hello'world
//    │                ┬  
//    │                ╰── Expected config key, found error
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected identifier, found '#'
//    ╭─[ strings_unquoted_strings.baml:6:13 ]
//    │
//  6 │     banned2 #helloworld
//    │             ┬  
//    │             ╰── Expected identifier, found '#'
//    │ 
//    │ Note: Error 
```

V2 actual:
```
// Error: Expected config key, found error
//    ╭─[ strings_unquoted_strings.baml:4:16 ]
//    │
//  4 │     thing hello'world
//    │                ┬  
//    │                ╰── Expected config key, found error
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found '#'
//    ╭─[ strings_unquoted_strings.baml:6:13 ]
//    │
//  6 │     banned2 #helloworld
//    │             ┬  
//    │             ╰── Expected config key, found '#'
//    │ 
//    │ Note: Error 
```

### throw/throws_caller_sees_contract.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_caller_sees_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_caller_
```

V2 expected:
```
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_caller_sees_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: user.Errors
```

V2 actual:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_caller_sees_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_caller_
```

### throw/throws_caller_variant_contract.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.AuthError, user.NotFoundError
//    ╭─[ throw_throws_caller_variant_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                          ────────────────────┬───────────────────  
//    │                                                              ╰───────────────────── extraneous throws declaration: user.AuthError, user.NotFoundError
//    │ 
//    
```

V2 expected:
```
// Error: throws contract violation: `user.AuthError | user.NotFoundError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    ╭─[ throw_throws_caller_variant_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                          ────────────────────┬───────────────────  
//    │                                                              ╰───────────────────── throws contract violat
```

V2 actual:
```
// Error: extraneous throws declaration: user.AuthError, user.NotFoundError
//    ╭─[ throw_throws_caller_variant_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                          ────────────────────┬───────────────────  
//    │                                                              ╰───────────────────── extraneous throws declaration: user.AuthError, user.NotFoundError
//    │ 
//    
```

### throw/throws_enum_exact_match.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_exact_match.baml:9:52 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                    ───┬───  
//    │                                                       ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.
```

V2 expected:
```
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.InternalError, user.Errors.NotFoundError
//    ╭─[ throw_throws_enum_exact_match.baml:9:52 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                    ───┬───  
//    │                                                       ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.InternalEr
```

V2 actual:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_exact_match.baml:9:52 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                    ───┬───  
//    │                                                       ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.
```

### throw/throws_enum_extraneous.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.Au
```

V2 expected:
```
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extr
```

V2 actual:
```
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.Au
```

### throw/throws_enum_variant_precise.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.AuthError, user.NotFoundError
//    ╭─[ throw_throws_enum_variant_precise.baml:9:55 ]
//    │
//  9 │ function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                                       ────────────────────┬───────────────────  
//    │                                                                           ╰───────────────────── extraneous throws declaration: user.AuthError, 
```

V2 expected:
```
// Error: throws contract violation: `user.AuthError | user.NotFoundError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_precise.baml:9:55 ]
//    │
//  9 │ function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                                       ────────────────────┬───────────────────  
//    │                                                                           ╰──────────
```

V2 actual:
```
// Error: extraneous throws declaration: user.AuthError, user.NotFoundError
//    ╭─[ throw_throws_enum_variant_precise.baml:9:55 ]
//    │
//  9 │ function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                                       ────────────────────┬───────────────────  
//    │                                                                           ╰───────────────────── extraneous throws declaration: user.AuthError, 
```

### throw/throws_enum_variant_violation.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract v
```

V2 expected:
```
// Error: throws contract violation: `user.AuthError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── throws contract violation: `user.AuthError` is missing user.Erro
```

V2 actual:
```
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract v
```

### throw/throws_mixed.baml

**Status:** `UPDATED_BUT_FAILING`

V1 expected:
```
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `string | use
```

V2 expected:
```
// Error: throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    │ 
//    │ No
```

V2 actual:
```
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `string | use
```

## Next steps

1. **24 files need attention** — actual compiler2 output differs from expected
2. **24 files had expectations updated** — review these for correctness
3. **207 files pass** — no action needed
