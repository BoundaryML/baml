# V1 vs V2 Diagnostics Diff Report

## Summary

| Status | Count | Description |
|--------|-------|-------------|
| OK_CLEAN | 41 | Both compilers agree: no errors |
| HAS_BOTH | 36 | Both produce errors (need comparison) |
| V2_MISSING | **54** | V2 misses errors V1 expected (regressions) |
| V2_NEW | 11 | V2 finds errors V1 didn't |
| V2_PANIC | 0 | V2 crashed |
| **Total** | **142** | |

## V2_MISSING Regressions (by error category)

These are errors V1 caught but V2 does not. Grouped by error type to help
identify which compiler2 features need work.

### `jinja_warning` (67 errors across 5 files)

- **enum/enums_in_jinja.baml** (33 errors)
  - `warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.`
  - ... and 30 more
- **enum/nullable_enums_in_jinja.baml** (15 errors)
  - `warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Status.Inactive` instead of "inactive" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.`
  - ... and 12 more
- **functions_v2/prompt_errors/prompt1.baml** (3 errors)
  - `warning: Function 'Foo' expects 0 arguments, but got 1`
  - `warning: Function 'Foo' referenced without parentheses. Did you mean 'Foo()'?`
  - `warning: Function 'Foo2' referenced without parentheses. Did you mean 'Foo2()'?`
- **functions_v2/valid.baml** (13 errors)
  - `warning: Use `Color.RED` instead of "RED" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Color.BLUE` instead of "BLUE" - comparing enums with strings will soon be deprecated.`
  - `warning: Use `Color.GREEN` instead of "GREEN" - comparing enums with strings will soon be deprecated.`
  - ... and 10 more
- **template_string/bad_calls.baml** (3 errors)
  - `warning: Function 'WithParams' expects 1 arguments, but got 2`
  - `warning: Function 'WithParams' expects argument 'a' to be of type int, but got literal["a"]`
  - `warning: Function 'WithParams' expects 1 arguments, but got 0`

### `config_validation` (17 errors across 13 files)

- **client/bad_response_format.baml** (1 errors)
  - `error: client_response_type must be one of "openai", "openai-responses", "anthropic", "google", or "vertex". Got: invali`
- **client/http_config_composite_wrong_field.baml** (2 errors)
  - `error: Unrecognized field 'request_timeout_ms' in http configuration block. Did you mean 'total_timeout_ms'? Composite c`
  - `error: Unrecognized field 'connect_timeout_ms' in http configuration block. Did you mean 'total_timeout_ms'? Composite c`
- **client/http_config_invalid_fields.baml** (1 errors)
  - `error: Unrecognized field 'conect_timeout_ms' in http configuration block. Did you mean 'connect_timeout_ms'?`
- **client/http_config_negative_timeout.baml** (1 errors)
  - `error: connect_timeout_ms must be non-negative, got: -1000ms`
- **client/http_config_not_map.baml** (1 errors)
  - `error: http must be a configuration block with timeout settings`
- **client/http_config_regular_with_total.baml** (1 errors)
  - `error: Unrecognized field 'total_timeout_ms' in http configuration block. 'total_timeout_ms' is only available for compo`
- **client/remap_role_empty_allowed_roles.baml** (2 errors)
  - `error: allowed_roles must not be empty`
  - `error: remap_roles values must be one of: . Got: user. To support different remap roles, add allowed_roles ["user", "ass`
- **client/remap_role_invalid_role.baml** (1 errors)
  - `error: remap_roles values must be one of: Value("user"), Value("assistant"). Got: system. To support different remap rol`
- **client/remap_role_invalid_type.baml** (1 errors)
  - `error: remap_roles must be a map. Got: string`
- **client/remap_role_non_string_values.baml** (1 errors)
  - `error: remap_role must be a map of strings to strings. Got: number`
- **client/unknown_prop.baml** (1 errors)
  - `error: Error validating: Unknown field `myExtraProp` in client`
- **generators/error.baml** (2 errors)
  - `error: Property not known: "language". Did you mean one of these: "version", "on_generate", "project", "output_type", "o`
  - `error: Property not known: "o". Did you mean one of these: "version", "project", "output_dir", "output_type", "on_genera`
- **tests/values.baml** (2 errors)
  - `error: Property not known: "input". Did you mean one of these: "args", "functions"?`
  - `error: Error validating: Missing `args` property`

### `cycle_detection` (14 errors across 6 files)

- **class/dependency_cycle.baml** (5 errors)
  - `error: Error validating: These classes form a dependency cycle: InterfaceTwo -> InterfaceOne`
  - `error: Error validating: These classes form a dependency cycle: InterfaceThree`
  - `error: Error validating: These classes form a dependency cycle: One -> Two -> Three -> Four -> Five`
  - ... and 2 more
- **class/recursive_type_aliases.baml** (4 errors)
  - `error: Error validating: These aliases form a dependency cycle: One -> Two`
  - `error: Error validating: These aliases form a dependency cycle: A -> B -> C`
  - `error: Error validating: These aliases form a dependency cycle: EnterCycle -> NoStop`
  - ... and 1 more
- **class/type_aliases_jinja.baml** (1 errors)
  - `error: Error validating: These aliases form a dependency cycle: I -> J`
- **client/infinite_fallback_cycle.baml** (2 errors)
  - `error: Error validating: These fallback clients form a dependency cycle: SelfReferentialClient`
  - `error: Error validating: These fallback clients form a dependency cycle: ClientA -> ClientB -> ClientC`
- **tests/dynamic_types_external_cycle_errors.baml** (1 errors)
  - `error: Error validating: These classes form a dependency cycle: A -> B -> C`
- **tests/dynamic_types_internal_cycle_errors.baml** (1 errors)
  - `error: Error validating: These classes form a dependency cycle: DynamicClass`

### `constraint_validation` (10 errors across 6 files)

- **class/invalid_attrs_on_type_alias.baml** (3 errors)
  - `error: Error validating: type aliases may only have @check and @assert attributes`
  - `error: Error validating: type aliases may only have @check and @assert attributes`
  - `error: Error validating: type aliases may only have @check and @assert attributes`
- **constraints/malformed_expression.baml** (3 errors)
  - `error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in <expression>:1)`
  - `error: Error validating: Error parsing jinja template: syntax error: unexpected `)` (in FunctionName:3)`
  - `error: Error validating: Error parsing jinja template: syntax error: unexpected identifier, expected end of variable blo`
- **functions_v2/check_in_parameter.baml** (1 errors)
  - `error: Error validating: Types with checks are not allowed as function parameters.`
- **functions_v2/check_in_parameter_type_alias.baml** (1 errors)
  - `error: Error validating: Types with checks are not allowed as function parameters.`
- **functions_v2/tests/field_level_assertions_v2.baml** (1 errors)
  - `error: Error validating: @assert is not allowed on test fields. Use @@assert at the test block level instead.`
- **functions_v2/tests/field_level_check.baml** (1 errors)
  - `error: Error validating: @check is not allowed on test fields. Use @@check at the test block level instead.`

### `map_key_validation` (9 errors across 1 files)

- **class/map_types.baml** (9 errors)
  - `error: Error validating: Maps may only have strings, enums or literal strings as keys`
  - `error: Error validating: Maps may only have strings, enums or literal strings as keys`
  - `error: Error validating: Maps may only have strings, enums or literal strings as keys`
  - ... and 6 more

### `duplicate_name` (7 errors across 2 files)

- **class/duplicate_definitions.baml** (3 errors)
  - `error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".`
  - `error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".`
  - `error: The type_alias "A" cannot be re-defined because it is already defined as a class.`
- **functions_v2/tests/failing_tests.baml** (4 errors)
  - `error: Test "Foo" is already defined for function "InputImage"`
  - `error: Test "Foo" is already defined for function "InputImage"`
  - `error: Test "Bar" is already defined for function "InputEnum"`
  - ... and 1 more

### `syntax_error` (7 errors across 4 files)

- **class/invalid_attrs_on_type_alias.baml** (1 errors)
  - `error: Attribute not known: "@unknown".`
- **client/bad_template_args.baml** (1 errors)
  - `error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.`
- **enum/invalid_commas.baml** (3 errors)
  - `error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be`
  - `error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be`
  - `error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be`
- **expr/missing_semicolons.baml** (2 errors)
  - `error: Statement must end with a semicolon.`
  - `error: Statement must end with a semicolon.`

### `unknown_variable` (6 errors across 4 files)

- **constraints/valid_but_invalid_expressions.baml** (1 errors)
  - `warning: Variable `bar` does not exist. Did you mean `this`?`
- **functions_v2/prompt_errors/prompt1.baml** (2 errors)
  - `warning: 'b' is undefined, expected function`
  - `warning: Variable `b` does not exist. Did you mean one of these: `_`, `ctx`?`
- **template_string/bad_calls.baml** (2 errors)
  - `warning: Variable `Random` does not exist. Did you mean one of these: `_`, `ctx`?`
  - `warning: 'Random' is undefined, expected function`
- **template_string/invalid.baml** (1 errors)
  - `warning: 'param' is undefined, expected class`

### `jinja_type_alias_warning` (4 errors across 1 files)

- **class/type_aliases_jinja.baml** (4 errors)
  - `warning: 'pid' is a type alias ProjectId (resolves to int), expected class`
  - `warning: 'c' is a type alias C (resolves to float), expected class`
  - `warning: 'j' is a recursive type alias JsonValue, expected class`
  - ... and 1 more

### `unknown_field` (4 errors across 3 files)

- **client/class_alias.baml** (1 errors)
  - `warning: property 'inner' does not exist on Bar in type alias A`
- **expr/access.baml** (1 errors)
  - `error: Error validating: Class Foo has no field names`
- **template_string/union_type_narrowing.baml** (2 errors)
  - `warning: property 'bark_volume' does not exist on Cat`
  - `warning: property 'radius' does not exist on Dog, Cat in type alias Thing`

### `reserved_keyword` (4 errors across 4 files)

- **enum/enum_is_valid.baml** (1 errors)
  - `error: Error validating field `None` in enum `Test`: Enum value 'None' is a reserved word in Python, try changing the na`
- **expr/keywords.baml** (1 errors)
  - `error: Error validating: 'emit' is a reserved keyword.`
- **expr/var_keyword_let_async.baml** (1 errors)
  - `error: Error validating: 'async' is a reserved keyword.`
- **expr/var_keyword_let_await.baml** (1 errors)
  - `error: Error validating: 'await' is a reserved keyword.`

### `type_mismatch` (3 errors across 3 files)

- **assert.baml** (1 errors)
  - `error: Expected a bool value, but received string value `"string"`.`
- **assign_wrong_type.baml** (1 errors)
  - `error: Error validating: Cannot assign string to int`
- **expr/constructors_nested.baml** (1 errors)
  - `error: Error validating: Bar.name expected type string, but found int`

### `duplicate_attribute` (3 errors across 1 files)

- **class/attributes.baml** (3 errors)
  - `error: Attribute "@description" can only be defined once.`
  - `error: Attribute "@description" can only be defined once.`
  - `error: Attribute "@description" can only be defined once.`

### `uncategorized` (3 errors across 2 files)

- **constraints/misspelled.baml** (1 errors)
  - `error: Error validating: `
- **enum/enums_in_jinja.baml** (2 errors)
  - `warning: Comparing enum Status to string variable - enum-string comparisons will soon be deprecated. Please see https://`
  - `warning: Comparing enum Priority to string variable - enum-string comparisons will soon be deprecated. Please see https:`

### `field_access_validation` (3 errors across 1 files)

- **expr/access.baml** (3 errors)
  - `error: Error validating: Can only access fields on class instances`
  - `error: Error validating: Array index must be integer`
  - `error: Error validating: Array index must be integer`

### `client_validation` (2 errors across 2 files)

- **client/remap_role_non_string_allowed_roles.baml** (1 errors)
  - `error: values in allowed_roles must be strings. Got: number`
- **client/required_provider.baml** (1 errors)
  - `error: Error validating: Missing `provider` field in client. e.g. `provider openai``

### `unknown_type` (2 errors across 1 files)

- **template_string/invalid.baml** (2 errors)
  - `error: Type `Unknown` does not exist. Did you mean one of these: `int`, `float`, `bool`, `string`, `true`, `false`?`
  - `error: Type `Unknown2` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `true`, `false`?`

### `attribute_validation` (1 errors across 1 files)

- **class/invalid_attrs_on_field.baml** (1 errors)
  - `error: Error validating: Class field with @skip attribute must be optional. Try making the type nullable: skip_this_one `

### `test_validation` (1 errors across 1 files)

- **tests/missing_arg.baml** (1 errors)
  - `warning: Test 'FooTest' is missing required arguments for function 'Foo'. Add an args block like:`

### Category summary

| Category | Errors | Files | Priority |
|----------|--------|-------|----------|
| jinja_warning | 67 | 5 | P1 - jinja/templates |
| config_validation | 17 | 13 | P1 - config |
| cycle_detection | 14 | 6 | P0 - core |
| constraint_validation | 10 | 6 | P1 - jinja/templates |
| map_key_validation | 9 | 1 | P2 - other |
| duplicate_name | 7 | 2 | P0 - core |
| syntax_error | 7 | 4 | P2 - syntax |
| unknown_variable | 6 | 4 | P2 - other |
| jinja_type_alias_warning | 4 | 1 | P2 - other |
| unknown_field | 4 | 3 | P2 - other |
| reserved_keyword | 4 | 4 | P2 - syntax |
| type_mismatch | 3 | 3 | P0 - core |
| duplicate_attribute | 3 | 1 | P2 - other |
| uncategorized | 3 | 2 | P2 - other |
| field_access_validation | 3 | 1 | P2 - other |
| client_validation | 2 | 2 | P1 - config |
| unknown_type | 2 | 1 | P0 - core |
| attribute_validation | 1 | 1 | P2 - other |
| test_validation | 1 | 1 | P2 - other |

## HAS_BOTH Files (error count comparison)

Both compilers produce errors. Comparing counts to spot divergences.

| File | V1 errors | V2 errors | Delta | Assessment |
|------|-----------|-----------|-------|------------|
| class/generator_keywords1.baml | 2 | 4 | +2 | V2 finds 2 more |
| class/incomplete_class.baml | 1 | 1 | 0 | count matches |
| class/invalid_keyword_in_type_def.baml | 2 | 2 | 0 | count matches |
| class/invalid_type_aliases.baml | 5 | 1 | -4 | **V2 misses 4** |
| class/map_types2.baml | 3 | 4 | +1 | V2 finds 1 more |
| class/misspeled_boolean_literals.baml | 2 | 2 | 0 | count matches |
| class/secure_types.baml | 1 | 29 | +28 | V2 finds 28 more |
| class/spelling_error.baml | 2 | 2 | 0 | count matches |
| class/unknown_type.baml | 1 | 1 | 0 | count matches |
| class/unsupported_literal_types.baml | 1 | 1 | 0 | count matches |
| enum/duplicate_value.baml | 1 | 1 | 0 | count matches |
| enum/invalid_value_expr.baml | 1 | 1 | 0 | count matches |
| expr/builtin.baml | 2 | 3 | +1 | V2 finds 1 more |
| expr/constructors_invalid.baml | 7 | 3 | -4 | **V2 misses 4** |
| expr/extra_dot.baml | 2 | 3 | +1 | V2 finds 1 more |
| expr/let_annotations.baml | 2 | 2 | 0 | count matches |
| expr/missing_return_value.baml | 1 | 1 | 0 | count matches |
| expr/unknown_name.baml | 8 | 4 | -4 | **V2 misses 4** |
| expr/watch_when.baml | 3 | 3 | 0 | count matches |
| functions_v2/duplicate_names.baml | 6 | 6 | 0 | count matches |
| functions_v2/invalid.baml | 2 | 4 | +2 | V2 finds 2 more |
| functions_v2/invalid2.baml | 11 | 10 | -1 | **V2 misses 1** |
| functions_v2/invalid_no_return.baml | 1 | 1 | 0 | count matches |
| loops/break.baml | 1 | 1 | 0 | count matches |
| loops/c_for.baml | 7 | 3 | -4 | **V2 misses 4** |
| loops/continue.baml | 1 | 2 | +1 | V2 finds 1 more |
| loops/for.baml | 3 | 1 | -2 | **V2 misses 2** |
| loops/header_requires_let_negative.baml | 9 | 6 | -3 | **V2 misses 3** |
| maps/inconsistent_style.baml | 1 | 2 | +1 | V2 finds 1 more |
| maps/key_and_value_typecheck.baml | 4 | 3 | -1 | **V2 misses 1** |
| parens.baml | 18 | 72 | +54 | V2 finds 54 more |
| strings/unquoted_strings.baml | 1 | 7 | +6 | V2 finds 6 more |
| tests/bad_syntax.baml | 3 | 1 | -2 | **V2 misses 2** |
| tests/dynamic_types_parser_errors.baml | 4 | 17 | +13 | V2 finds 13 more |
| tests/dynamic_types_validation_errors.baml | 11 | 1 | -10 | **V2 misses 10** |
| tests/return.baml | 1 | 2 | +1 | V2 finds 1 more |

### HAS_BOTH files where V2 finds fewer errors

These may contain partial regressions (V2 catches some but not all V1 errors).

#### class/invalid_type_aliases.baml

V1 (5 errors):
- `error: Error validating: Unexpected keyword used in assignment: typpe`
- `error: The class "One" cannot be re-defined because a type_alias with that name already exists.`
- `error: The type_alias "One" cannot be re-defined because a class with that name already exists.`
- `error: Error validating: Type alias points to unknown identifier `i``
- `error: Error validating: Type alias points to unknown identifier `b``

V2 (1 errors):
- `Unknown keyword 'typpe'. Did you mean 'type'? Usage: type Name = expression`

#### expr/constructors_invalid.baml

V1 (7 errors):
- `error: Error validating: Bar.a expected type int, but found string`
- `error: Error validating: Class Bar has no field c`
- `error: Error validating: Class Bar is missing fields: b`
- `error: Error validating: baml.HttpRequest.url expected type string, but found int`
- `error: Error validating: baml.HttpRequest.query_params expected type (map<string, string> | null), but found map<string,`
- `error: Error validating: Class baml.HttpRequest is missing fields: method`
- `error: Error validating: Class baml.HttpRequest is missing fields: method`

V2 (3 errors):
- `unresolved name: a`
- `unresolved name: Authorization`
- `unresolved name: foo`

#### expr/unknown_name.baml

V1 (8 errors):
- `error: Unknown variable a`
- `error: Unknown variable b`
- `error: Unknown function Unknown`
- `error: Error validating: Unknown variable a`
- `error: Error validating: Type mismatch in argument`
- `error: Error validating: Unknown variable b`
- `error: Error validating: Function Go expects 1 arguments, got 3`
- `error: Error validating: Unknown function Unknown`

V2 (4 errors):
- `unresolved name: a`
- `expected 1 argument(s), got 3`
- `unresolved name: b`
- `unresolved name: Unknown`

#### functions_v2/invalid2.baml

V1 (11 errors):
- `error: Error validating Function "Foo4": This field declaration is invalid. It is either missing a name or a type.`
- `error: Error validating Function "Foo5": This field declaration is invalid. It is either missing a name or a type.`
- `error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.`
- `error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.`
- `error: Error validating: Missing `prompt` field in function. Add to the block:`
- `error: Error validating: Missing `client` field in function. Add to the block:`
- `error: Expected a template_string value, but received string value `"..."`.`
- `error: Error validating: Missing `prompt` field in function. Add to the block:`
- `error: Error validating: Missing `prompt` field in function. Add to the block:`
- `error: Error validating: Missing `client` field in function. Add to the block:`
- `error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:`

V2 (10 errors):
- `unresolved name: Foo`
- `Expected LLM function missing 'prompt' field, found '}'`
- `type mismatch: expected baml.llm.Client, got null`
- `Expected LLM function missing 'client' field, found '}'`
- `unresolved name: bar`
- `unresolved name: bar`
- `Expected prompt string, found '}'`
- `type mismatch: expected baml.llm.Client, got null`
- `type mismatch: expected baml.llm.Client, got null`
- `Expected prompt string, found '}'`

#### loops/c_for.baml

V1 (7 errors):
- `error: Unknown variable i`
- `error: Unknown variable i`
- `error: Unknown variable x`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable x`

V2 (3 errors):
- `unresolved name: i`
- `unresolved name: i`
- `unreachable code: 1 statement(s) after diverging statement`

#### loops/for.baml

V1 (3 errors):
- `error: Error validating: Cannot assign string to int`
- `error: Error validating: iterable in `for` loop must be an array`
- `error: Error validating: Cannot assign int to string`

V2 (1 errors):
- `cannot iterate over type `int``

#### loops/header_requires_let_negative.baml

V1 (9 errors):
- `error: Unknown variable i`
- `error: Unknown variable i`
- `error: Unknown variable i`
- `error: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`
- `error: Error validating: Unknown variable i`

V2 (6 errors):
- `unresolved name: i`
- `Expected ')', found ';'`
- `Expected block after for expression, found ';'`
- `unresolved name: i`
- `Expected expression, found ')'`
- `unresolved name: i`

#### maps/key_and_value_typecheck.baml

V1 (4 errors):
- `error: Error validating: Map keys must be string literals`
- `error: Error validating: Type mismatch in argument, expected: string, got: int`
- `error: Error validating: Map access must be a string`
- `error: Error validating: Return type mismatch: function return type is int but got string`

V2 (3 errors):
- `Expected expression, found ':'`
- `type mismatch: expected map<int, string>, got "2"`
- `type mismatch: expected int, got string`

#### tests/bad_syntax.baml

V1 (3 errors):
- `error: Error validating: Invalid array syntax detected.`
- `error: Property not known: "input". Did you mean one of these: "args", "functions"?`
- `error: Error validating: Missing `args` property`

V2 (1 errors):
- `Expected identifier, found ','`

#### tests/dynamic_types_validation_errors.baml

V1 (11 errors):
- `error: Error validating: Type 'NonDynamic' does not contain the `@@dynamic` attribute so it cannot be modified in a type`
- `error: Error validating: The `dynamic` keyword only works on classes and enums, but type 'SomeAlias' is a type alias`
- `error: Error validating: The `@@dynamic` attribute is not allowed in type_builder blocks`
- `error: Error validating: The `@@dynamic` attribute is not allowed in type_builder blocks`
- `error: Error validating: Dynamic type definitions cannot contain the `@@dynamic` attribute`
- `error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.`
- `error: The class "DynamicClass" cannot be re-defined because a class with that name already exists.`
- `error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.`
- `error: The class "NonDynamic" cannot be re-defined because a class with that name already exists.`
- `error: Error validating: Type 'DynamicEnum' is an enum, but the dynamic block is defined as 'dynamic class'`
- `error: Error validating: Type 'DynamicClass' is a class, but the dynamic block is defined as 'dynamic enum'`

V2 (1 errors):
- `field 'C' is missing a type annotation`

## V2_NEW (V2 finds errors V1 didn't)

These are likely intentional syntax changes in V2 or stricter parsing.

- **class/generator_keywords2.baml** (2 errors)
  - `Expected config key, found '/'`
  - `Expected identifier, found '}'`
- **client/period_in_model_type.baml** (4 errors)
  - `Expected config key, found '.'`
  - `Expected config key, found integer`
  - `Expected config key, found '-'`
  - ... and 1 more
- **constraints/block_level.baml** (4 errors)
  - `Expected config key, found '.'`
  - `Expected config key, found integer`
  - `Expected config key, found '-'`
  - ... and 1 more
- **dictionary/valid_dictionary.baml** (8 errors)
  - `Expected config key, found '!'`
  - `Expected config key, found '!'`
  - `Expected config key, found integer`
  - ... and 5 more
- **enum/enum_unquoted_description.baml** (5 errors)
  - `Expected ')', found identifier`
  - `Expected Unexpected token in enum body, found ')'`
  - `Duplicate variant `TestEnum.is``
  - ... and 2 more
- **expr/expr_full.baml** (6 errors)
  - `unresolved name: poem`
  - `unresolved name: another`
  - `remove parentheses from test name: `test TestPipeline``
  - ... and 3 more
- **expr/instanceof_narrowing.baml** (4 errors)
  - `unresolved member: user.Bar.field`
  - `unresolved member: null.field`
  - `unresolved member: user.Bar.field`
  - ... and 1 more
- **generators/v1.baml** (6 errors)
  - `Expected config key, found '/'`
  - `Expected identifier, found '}'`
  - `Expected config key, found '/'`
  - ... and 3 more
- **loops/header_requires_let_positive.baml** (3 errors)
  - `Expected ')', found ';'`
  - `Expected block after for expression, found ';'`
  - `Expected expression, found ')'`
- **maps/maps.baml** (2 errors)
  - `unresolved name: hello`
  - `type mismatch: expected map<string, string>, got "world"`
- **prompt_fiddle_example.baml** (1 errors)
  - `Expected config key, found ':'`

## Recommended next steps

1. **Triage by category** - The biggest gaps are:
   1. `jinja_warning` (67 errors, 5 files)
   2. `config_validation` (17 errors, 13 files)
   3. `cycle_detection` (14 errors, 6 files)
   4. `constraint_validation` (10 errors, 6 files)
   5. `map_key_validation` (9 errors, 1 files)

2. **Split the work** - Each category likely maps to a specific compiler2 subsystem:
   - `config_validation` / `client_validation` / `generator_validation` → config/client lowering
   - `cycle_detection` → TIR cycle detection pass
   - `jinja_warning` / `constraint_validation` → Jinja template type checking
   - `type_mismatch` / `unknown_type` → TIR type inference
   - `duplicate_name` / `duplicate_attribute` → HIR validation

3. **Check HAS_BOTH files** with `V2 misses` for partial regressions

4. **V2_NEW files** are likely fine (stricter V2 parsing) — review to confirm
