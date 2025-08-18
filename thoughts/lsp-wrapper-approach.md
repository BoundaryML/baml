# LSP Wrapper Binary Approach

## Overview

Instead of modifying the existing LSP server code, create a wrapper binary that:
1. Records and replays stdin messages to/from `baml-cli lsp`
2. Acts as a transparent proxy between the editor and the actual LSP server
3. Much simpler, non-invasive implementation

## Architecture

```
Editor <-> Wrapper Binary <-> baml-cli lsp
           (records/replays)
```

### Recording Mode
```
Editor -> Wrapper (record to file) -> baml-cli lsp -> Wrapper -> Editor
```

### Replay Mode  
```
Recorded file -> Wrapper -> baml-cli lsp -> Wrapper -> Editor (or continue live)
```

## Implementation Plan

### 1. Create New Binary: `baml-lsp-recorder`

Location: `engine/cli/src/bin/baml-lsp-recorder.rs`

**Features:**
- `--record <file>` - Record stdin/stdout to file while proxying to `baml-cli lsp`
- `--replay <file>` - Replay recorded stdin, then switch to live stdin
- `--server-path <path>` - Path to `baml-cli` binary (default: find in PATH)

### 2. Simple Message Format

Use raw stdin/stdout capture instead of parsed LSP messages:
```json
{"timestamp": "2024-01-01T12:00:00.123Z", "direction": "in", "data": "Content-Length: 123\r\n\r\n{...}"}
{"timestamp": "2024-01-01T12:00:00.456Z", "direction": "out", "data": "Content-Length: 456\r\n\r\n{...}"}
```

This captures the raw LSP protocol exactly as transmitted.

### 3. Wrapper Implementation

```rust
// Pseudo-code structure
fn main() {
    match args {
        Record { file, .. } => {
            let recorder = StdinRecorder::new(file);
            let child = spawn_baml_lsp();
            proxy_bidirectional(stdin, child.stdin, Some(recorder));
            proxy_bidirectional(child.stdout, stdout, None);
        },
        Replay { file, .. } => {
            let replayer = StdinReplayer::new(file);
            let child = spawn_baml_lsp();
            
            // First replay recorded messages
            replayer.replay_to(child.stdin);
            
            // Then switch to live stdin
            proxy_bidirectional(stdin, child.stdin, None);
            proxy_bidirectional(child.stdout, stdout, None);
        }
    }
}
```

## Key Benefits

### 1. **Non-invasive**
- Zero changes to existing LSP server code
- Wrapper binary is completely separate
- No risk of breaking existing functionality

### 2. **Simple & Robust**
- Records raw stdin/stdout (exact bytes)
- No need to parse/serialize LSP messages
- Works with any LSP protocol extensions or future changes

### 3. **Transparent**
- Editor sees normal LSP server behavior
- Server sees normal editor behavior
- Wrapper is invisible except for recording/replay

### 4. **Easy Testing**
- Can test wrapper independently
- Can compare recorded vs live behavior easily
- Simple to add debugging/logging

## Usage Examples

### Recording a Bug Session
```bash
# Start wrapper in record mode
baml-lsp-recorder --record /tmp/wasm-bug.jsonl

# Editor connects to wrapper instead of baml-cli lsp directly
# Trigger the bug, then stop
```

### Replaying for Debugging
```bash
# Start wrapper in replay mode  
baml-lsp-recorder --replay /tmp/wasm-bug.jsonl

# Recorded session plays back instantly
# Then continues with live input for debugging
# Playground available at localhost:3030
```

### Configuration in VS Code
Update VS Code settings to use wrapper:
```json
{
  "baml.lsp.command": "baml-lsp-recorder",
  "baml.lsp.args": ["--record", "/tmp/session.jsonl"]
}
```

## File Structure

```
engine/cli/src/bin/
├── baml-lsp-recorder.rs     # New wrapper binary
└── (existing files...)

engine/cli/src/lsp_recorder/
├── mod.rs                   # Module exports
├── recorder.rs              # StdinRecorder implementation  
├── replayer.rs              # StdinReplayer implementation
├── proxy.rs                 # Bidirectional I/O proxy
└── format.rs                # Message format definitions
```

## Implementation Steps

1. **Create wrapper binary structure**
   - New binary in `engine/cli/src/bin/baml-lsp-recorder.rs`
   - CLI argument parsing for record/replay modes

2. **Implement I/O recording**
   - Capture raw stdin/stdout bytes
   - JSON Lines format with timestamps and direction

3. **Implement I/O replay**
   - Read recorded file and replay stdin
   - Switch to live stdin after replay completes

4. **Add bidirectional proxy**
   - Forward stdin to child process
   - Forward child stdout to stdout
   - Handle process lifecycle properly

5. **Test and document**
   - Test recording and replay functionality
   - Document editor configuration changes

This approach is much cleaner, simpler, and less risky than modifying the core LSP server!