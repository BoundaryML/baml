# BAML Codegen Test Rig

E2E test crates for BAML code generation. Each language+fixture combination gets its own test crate.

## Quick Start

```bash
# Generate all test crates from templates
cargo run -p tools_rig

# Run tests for a specific fixture
cargo test -p python_empty

# Check if crates are in sync with templates
cargo run -p tools_rig -- --check
```

## How to Add a New Fixture

### Step 1: Define the fixture

Edit `crates/baml_codegen_tests/src/builders.rs`:

```rust
define_fixtures! {
    empty => {
        ObjectPool::empty()
    },
    your_new_fixture => {
        ObjectPool::new()
            .with_class(Class { ... })
            .with_enum(Enum { ... })
    },
}
```

### Step 2: Generate test crates

```bash
cargo run -p tools_rig
```

This creates test crates for all languages:

- `rig_tests/crates/python_your_new_fixture/`
- `rig_tests/crates/typescript_your_new_fixture/` (when TypeScript is added)

### Step 3: Customize tests (optional)

```bash
vim rig_tests/crates/python_your_new_fixture/customizable/test_main.py
```

Files in `customizable/` are preserved across regenerations.

### Step 4: Run tests

```bash
cargo test -p python_your_new_fixture
```

## How to Add a New Language

### Step 1: Create template directory

```bash
mkdir -p rig_tests/crate_templates/your_language/{src,customizable}
```

### Step 2: Create templates

You need 4 templates with `{{fixture_name}}` placeholders:

#### `Cargo.toml.template`

```toml
[package]
name = "rig_your_language_{{fixture_name}}"
version.workspace = true
edition.workspace = true

[dependencies]
baml_codegen_tests = { path = "../../../crates/baml_codegen_tests" }
baml_codegen_your_language = { path = "../../../crates/baml_codegen_your_language" }

[[test]]
name = "generated_code"
path = "src/lib.rs"
harness = true
```

#### `build.rs.template`

```rust
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let generated_dir = manifest_dir.join("generated");

    // Clean and recreate
    if generated_dir.exists() {
        fs::remove_dir_all(&generated_dir).unwrap();
    }
    fs::create_dir_all(&generated_dir).unwrap();

    // Generate code
    let fixture = baml_codegen_tests::fixtures::{{fixture_name}}();
    let output = baml_codegen_your_language::to_source_code(&fixture, &PathBuf::from("."));

    // Write generated files
    for (path, content) in output {
        let file_path = generated_dir.join(&path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, &content).unwrap();
    }

    // Symlink customizable files
    let customizable_dir = manifest_dir.join("customizable");
    if customizable_dir.exists() {
        for entry in fs::read_dir(&customizable_dir).unwrap() {
            let entry = entry.unwrap();
            let src = entry.path();
            if !src.is_file() { continue; }

            let dst = generated_dir.join(entry.file_name());
            if dst.exists() || dst.symlink_metadata().is_ok() {
                let _ = fs::remove_file(&dst);
            }

            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, &dst).unwrap();

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&src, &dst).unwrap();
        }
    }

    // Write test script (language-specific)
    let test_sh = r#"#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
# Add your language-specific test commands here
"#;
    fs::write(generated_dir.join("test.sh"), test_sh).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(generated_dir.join("test.sh")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(generated_dir.join("test.sh"), perms).unwrap();
    }

    println!("cargo:rerun-if-changed=build.rs");

    // Watch customizable files
    if customizable_dir.exists() {
        for entry in fs::read_dir(&customizable_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                println!("cargo:rerun-if-changed={}", entry.path().display());
            }
        }
    }
}
```

#### `src/lib.rs.template`

```rust
use std::path::PathBuf;
use std::process::Command;

fn generated_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("generated")
}

#[test]
fn test_generated_code() {
    let dir = generated_dir();
    let test_script = dir.join("test.sh");

    assert!(test_script.exists(), "test.sh not found");

    let output = Command::new("bash")
        .arg(&test_script)
        .current_dir(&dir)
        .output()
        .expect("Failed to run test.sh");

    assert!(
        output.status.success(),
        "test.sh failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
```

#### `customizable/test_main.*` (language-specific)

For Python (`test_main.py.template`):

```python
#!/usr/bin/env python3
"""Pytest tests for {{fixture_name}} fixture."""


def test_imports():
    """Test that baml_client can be imported."""
    import baml_client  # noqa: F401


def test_fixture_specific():
    """Fixture-specific tests for {{fixture_name}}."""
    # TODO: Add fixture-specific tests here
    pass
```

### Step 3: Generate test crates

```bash
cargo run -p tools_rig
```

Creates `<language>_<fixture>` crates for all fixtures.

### Step 4: Verify

```bash
cargo test -p your_language_*
```

## Directory Structure

```text
rig_tests/
├── crate_templates/              # Templates (NOT in Cargo workspace)
│   └── <language>/
│       ├── Cargo.toml.template
│       ├── build.rs.template
│       ├── src/lib.rs.template
│       └── customizable/*.template
│
└── crates/                       # Generated crates (IN workspace)
    └── <language>_<fixture>/
        ├── Cargo.toml            # Always regenerated
        ├── build.rs              # Always regenerated
        ├── src/lib.rs            # Always regenerated
        ├── customizable/         # Preserved across regenerations
        └── generated/            # Build output (gitignored)
```

## How It Works

1. **Templates** in `crate_templates/<language>/` define structure
2. **`tools_rig`** generates crates by replacing `{{fixture_name}}`
3. **`build.rs`** (at compile time):
   - Calls `baml_codegen_<language>::to_source_code(fixture)`
   - Writes code to `generated/`
   - Symlinks `customizable/*` into `generated/`
   - Creates `test.sh`
4. **`lib.rs`** runs `test.sh` via bash
5. **`test.sh`** runs language-specific checks

## Template Preservation

**Always regenerated:**

- `Cargo.toml`, `build.rs`, `src/lib.rs`

**Preserved (only created if missing):**

- `customizable/*`

## CI

```bash
cargo run -p tools_rig -- --check
```

Fails if crates are out of sync with templates.
