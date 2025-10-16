# GitHub Actions - Setup Actions

This directory contains modular setup actions for the BAML project. Each action is focused on setting up a specific technology stack, allowing for better composability and caching strategies.

## Available Actions

### setup-all
Sets up the complete development environment using all modular actions.

```yaml
- name: Setup All
  uses: ./.github/actions/setup-all
  with:
    # Node.js configuration
    setup-node: 'true'                    # Optional, default: 'true'
    node-version: '20'                    # Optional, default: '20'
    pnpm-version: '9.12.0'               # Optional, default: '9.12.0'
    install-node-dependencies: 'true'    # Optional, default: 'true'
    enable-turbo-cache: 'true'           # Optional, default: 'true'

    # Rust configuration
    setup-rust: 'true'                    # Optional, default: 'true'
    rust-toolchain: 'stable'             # Optional, default: 'stable'
    rust-enable-wasm: 'true'             # Optional, default: 'true'
    rust-targets: ''                      # Optional, space-separated targets
    rust-workspace: 'engine'             # Optional, default: 'engine'

    # Python configuration
    setup-python: 'false'                # Optional, default: 'false'
    python-version: '3.13'               # Optional, default: '3.13'
    python-use-uv: 'false'               # Optional, default: 'false'

    # Go configuration
    setup-go: 'true'                      # Optional, default: 'true'
    go-version: '1.24'                    # Optional, default: '1.24'
    go-install-protoc-gen-go: 'true'     # Optional, default: 'true'

    # Tools configuration
    setup-tools: 'true'                   # Optional, default: 'true'
    tools-install-mise: 'true'           # Optional, default: 'true'
```

### setup-node
Sets up Node.js with pnpm package manager.

```yaml
- name: Setup Node.js
  uses: ./.github/actions/setup-node
  with:
    node-version: '20'              # Optional, default: '20'
    pnpm-version: '9.12.0'          # Optional, default: '9.12.0'
    install-dependencies: 'true'     # Optional, default: 'true'
    frozen-lockfile: 'true'          # Optional, default: 'true'
    enable-turbo-cache: 'true'       # Optional, default: 'true'
    turbo-cache-path: '.turbo'       # Optional, default: '.turbo'
```

### setup-rust
Sets up Rust toolchain with caching and optional WASM support.

```yaml
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  with:
    toolchain: 'stable'                           # Optional, default: 'stable'
    enable-wasm: 'false'                         # Optional, default: 'false'
    targets: 'x86_64-pc-windows-msvc'           # Optional, space-separated targets
    workspace: 'engine'                          # Optional, default: 'engine'
```

### setup-python
Sets up Python with optional uv package manager.

```yaml
- name: Setup Python
  uses: ./.github/actions/setup-python
  with:
    python-version: '3.13'          # Optional, default: '3.13'
    use-uv: 'false'                 # Optional, default: 'false'
    cache: 'true'                   # Optional, default: 'true'
```

### setup-go
Sets up Go with optional protoc-gen-go.

```yaml
- name: Setup Go
  uses: ./.github/actions/setup-go
  with:
    go-version: '1.24'              # Optional, default: '1.24'
    install-protoc-gen-go: 'false'  # Optional, default: 'false'
    cache: 'true'                   # Optional, default: 'true'
```

### setup-tools
Sets up common development tools.

```yaml
- name: Setup Tools
  uses: ./.github/actions/setup-tools
  with:
    install-mise: 'false'           # Optional, default: 'false'
```

## Usage Patterns

### Complete Environment Setup
For jobs that need everything (like full integration tests):
```yaml
- name: Setup All
  uses: ./.github/actions/setup-all
  with:
    setup-python: 'true'
    python-use-uv: 'true'
```

### Full Environment (Minimal Python)
For jobs that need most tools but minimal Python setup:
```yaml
- name: Setup All
  uses: ./.github/actions/setup-all
  with:
    setup-python: 'true'
    python-use-uv: 'false'
```

### No Python Environment
For jobs that don't need Python at all:
```yaml
- name: Setup All
  uses: ./.github/actions/setup-all
  # Python is disabled by default
```

### TypeScript Lint Job
Only needs Node.js and pnpm:
```yaml
- name: Setup Node.js
  uses: ./.github/actions/setup-node
  with:
    node-version: ${{ env.NODE_VERSION }}
    pnpm-version: ${{ env.PNPM_VERSION }}
```

### Rust Build Job
Only needs Rust toolchain:
```yaml
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  with:
    toolchain: ${{ env.RUST_TOOLCHAIN }}
    targets: ${{ matrix.target }}
```

### WASM Build Job
Needs Rust with WASM support:
```yaml
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  with:
    toolchain: ${{ env.RUST_TOOLCHAIN }}
    enable-wasm: 'true'
```

### Integration Test Job
Needs multiple technologies:
```yaml
- name: Setup Rust
  uses: ./.github/actions/setup-rust
  with:
    toolchain: ${{ env.RUST_TOOLCHAIN }}

- name: Setup Node.js
  uses: ./.github/actions/setup-node
  with:
    node-version: ${{ env.NODE_VERSION }}
    pnpm-version: ${{ env.PNPM_VERSION }}

- name: Setup Python
  uses: ./.github/actions/setup-python
  with:
    python-version: ${{ env.PYTHON_VERSION }}
    use-uv: 'true'
```

## Benefits

1. **Modularity**: Each job only sets up what it needs
2. **Reusability**: Actions can be reused across different workflows
3. **Caching**: Each action handles its own caching strategy
4. **Maintainability**: Changes to setup logic are isolated to specific actions
5. **Performance**: Faster builds by avoiding unnecessary setup steps

## Migration from setup-environment

The old monolithic `setup-environment` action has been replaced with these modular actions. When migrating, you have two options:

### Option 1: Use setup-all (Easy Migration)
Replace `setup-environment` with `setup-all`:
```yaml
# Before
- name: Setup environment
  uses: ./.github/actions/setup-environment

# After
- name: Setup All
  uses: ./.github/actions/setup-all
  # Uses sensible defaults for most BAML jobs
```

### Option 2: Use Granular Actions (Optimal Performance)
1. Identify what technologies your job actually needs
2. Use only the relevant setup actions
3. Remove the `setup-environment` action call
4. Update any hardcoded tool versions to use the action inputs

This approach gives you the fastest builds by only setting up what you need.

## Turborepo Caching

The setup actions include optimized [Turborepo caching](https://turborepo.com/docs/guides/ci-vendors/github-actions) configuration:

### Dual Caching Strategy
- **GitHub Actions Cache**: Fast local caching using `actions/cache@v4`
- **Vercel Remote Cache**: Team-wide cache sharing using `TURBO_TOKEN` and `TURBO_TEAM`

### Configuration
The `setup-node` action automatically configures Turbo caching:
```yaml
- name: Cache Turbo build setup
  uses: actions/cache@v4
  with:
    path: .turbo
    key: ${{ runner.os }}-turbo-${{ github.sha }}
    restore-keys: |
      ${{ runner.os }}-turbo-
```

### Environment Variables
Set these in your workflow for remote caching:
```yaml
env:
  TURBO_TOKEN: ${{ secrets.TURBO_TOKEN }}
  TURBO_TEAM: gloo
```

## pnpm Integration

As requested, the Node.js setup action prioritizes pnpm usage:
- Automatically sets up pnpm caching
- Installs dependencies via pnpm by default
- Supports frozen lockfile mode for CI
- All package.json scripts should run through pnpm when possible