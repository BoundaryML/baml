# LSP Tests

This directory contains inline assertion tests for the BAML compiler's LSP features. Each `.baml` file is a self-contained test that includes both the source code and expected diagnostics/hover information.

## Running Tests

```bash
# Run all LSP tests
cargo test -p baml_lsp_tests test_files

# Run a specific test folder
cargo test -p baml_lsp_tests test_files::type_errors

# Run a specific test
cargo test -p baml_lsp_tests test_files::hover::test_class
```

## Updating Expectations

When you add new tests or change compiler behavior, update the expected output:

```bash
UPDATE_EXPECT=1 cargo test -p baml_lsp_tests test_files
```

This will automatically update the expectations in the test files to match the actual compiler output.

## Test File Format

Each test file has two sections separated by `//----`:

1. **Source section**: BAML code to compile
2. **Expectations section**: Expected diagnostics and hover information

### Basic Example

```baml
function Foo(x: int) -> string {
  y
}
//----
//- diagnostics
// Error: test.baml:2:3
//   |
// 2 |   y
//   |   ^ unknown variable `y`
//   |
```

### Multiple Files

Use `// file: <filename>` markers to define multiple virtual files:

```baml
// file: types.baml
class Person {
  name string
}

// file: main.baml
function GetPerson() -> Person {
  Person { name: "Alice" }
}
//----
//- diagnostics
// (no errors expected)
```

### Cursor Hover Assertions

Use cursor markers in the source section to verify hover information. The hover expectations are listed under `//- on_hover expressions` and must match the cursor positions:

```baml
class Person<[CURSOR] {
  name string
  age int
}
//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at test.baml:1:13
// class Person {
//   name string
//   age int
// }
```

### Preserved Comments

You can add comments in the diagnostics or hovers sections that will be preserved when running `UPDATE_EXPECT=1`. Comments starting with `// (` are preserved:

```baml
function Foo() -> int { x }
//----
//- diagnostics
// (expect one unknown variable error)
// Error: test.baml:1:23
//   |
// 1 | function Foo() -> int { x }
//   |                       ^ unknown variable `x`
```

When you run `UPDATE_EXPECT=1`, the `// (expect one unknown variable error)` comment will be kept, while the actual error output will be regenerated.

## Directory Structure

Organize tests by category:

```
test_files/
├── README.md
├── type_errors/           # Type checking errors
│   ├── unknown_variable.baml
│   └── assign_wrong_type.baml
├── hover/                 # Hover information tests
│   ├── class.baml
│   └── function.baml
├── parse_errors/          # Syntax/parse errors (future)
└── name_errors/           # Duplicate name errors (future)
```

## Adding New Tests

1. Create a new `.baml` file in the appropriate subdirectory
2. Write the BAML source code
3. Add the `//----` separator
4. Run with `UPDATE_EXPECT=1` to generate expectations:
   ```bash
   UPDATE_EXPECT=1 cargo test -p baml_lsp_tests test_files::your_folder::test_your_file
   ```
5. Review the generated expectations and commit
