# LSP Message Recording & Replay Implementation Plan

## Overview

Implementation plan for debugging the WASM playground issue by adding LSP message recording and replay capabilities to `baml-cli lsp`.

**Problem**: "recursive use of an object detected which would lead to unsafe aliasing in rust" error in WASM playground needs a fast, reliable reproduction method.

**Solution**: Record LSP message sequences, then replay them to consistently reproduce the bug.

## Architecture Overview

The LSP server in `engine/language_server` uses:
- **Connection handling**: `server/connection.rs` manages message I/O via `crossbeam::channel`
- **Message processing**: `server.rs:367` has the main event loop that processes incoming messages
- **CLI entry point**: `engine/cli/src/lsp.rs` starts the server

## Phase 1: Message Recording System

### 1.1 CLI Flags
Add new flags to `engine/cli/src/lsp.rs`:
```rust
--record <file>           # Record LSP messages to file
--replay <file>           # Replay LSP messages from file  
--replay-check-playground # After replay, validate playground state
```

### 1.2 Message Format
Create a JSON Lines format for easy streaming:
```json
{"timestamp": "2024-01-01T12:00:00.123Z", "direction": "in", "message": {...}}
{"timestamp": "2024-01-01T12:00:00.456Z", "direction": "out", "message": {...}}
```

Messages should use the same `lsp_server::Message` type used by the existing LSP server.

### 1.3 Recording Implementation
- Wrap message channels in `server/connection.rs` to intercept all messages
- Log both incoming (from client) and outgoing (to client) messages
- Add timestamping for timing analysis
- Handle file I/O asynchronously to avoid blocking LSP performance

## Phase 2: Message Replay System

### 2.1 Replay Architecture
- Create mock connection that feeds recorded messages instead of stdin/stdout
- Replay messages as fast as possible (ignore original timing)
- Allow faster reproduction of complex bug scenarios

### 2.2 Validation
- After replay completes, expose playground state on a known port
- Provide manual verification steps for checking WASM playground state
- Log differences in LSP server responses between original and replay

## Phase 3: Integration & Testing

### 3.1 Reproduce Bug Workflow
1. Start LSP with recording: `baml-cli lsp --record /tmp/bug-session.jsonl`
2. Trigger bug via VS Code/editor interactions
3. Stop LSP when bug occurs
4. Replay: `baml-cli lsp --replay /tmp/bug-session.jsonl --replay-check-playground`
5. Check playground at `localhost:3030` for "recursive use of object" error

### 3.2 File Structure
```
engine/language_server/src/
├── recording/
│   ├── mod.rs           # Main recording module
│   ├── recorder.rs      # Message recording logic
│   ├── replayer.rs      # Message replay logic
│   └── format.rs        # Serialization format
├── server/
│   └── connection.rs    # Modified to support recording
└── lib.rs              # Updated exports
```

## Implementation Details

### Message Types
Use existing `lsp_server::Message` type which is serde-compatible:
- `Message::Request(Request)` 
- `Message::Response(Response)`
- `Message::Notification(Notification)`

### Recording Strategy
1. Wrap `ConnectionSender` and `ConnectionReceiver` with recording decorators
2. Intercept messages at the transport layer (before/after JSON serialization)
3. Write to file in JSON Lines format with timestamp and direction

### Replay Strategy  
1. Create `MockConnection` that reads from recorded file instead of stdin
2. Feed messages to existing server event loop at full speed
3. Capture and compare server responses for debugging

## Key Benefits

1. **Fast reproduction**: Convert a complex multi-step bug into a single command
2. **Deterministic**: Same sequence every time, no manual steps
3. **Debuggable**: Can add logging/breakpoints in replay mode
4. **Shareable**: Send recorded session file to other developers
5. **CI/CD ready**: Can run replays in automated testing

This approach will make it much easier to isolate and debug the "recursive use of object" error in the WASM playground by creating a reliable, fast reproduction mechanism.

## Usage

### Recording LSP Sessions

To record an LSP session that reproduces the bug:

```bash
# Start LSP server with recording enabled
baml-cli lsp --record /tmp/bug-reproduction.jsonl

# The server will now record all incoming and outgoing LSP messages
# Use your editor (VS Code, Cursor, etc.) to trigger the bug
# When the bug occurs, stop the LSP server (Ctrl+C)
```

### Replaying LSP Sessions

To replay a recorded session and continue debugging:

```bash
# Replay the recorded session, then continue with live input
baml-cli lsp --replay /tmp/bug-reproduction.jsonl

# The server will:
# 1. First replay all recorded messages as fast as possible
# 2. Then switch to live stdin input for continued interaction
# 3. The playground will be available at localhost:3030
```

### Workflow for Bug Reproduction

1. **Record the bug**:
   ```bash
   baml-cli lsp --record /tmp/wasm-bug.jsonl
   ```
   - Open your editor and connect to the LSP
   - Perform the actions that trigger the "recursive use of object" error
   - Stop the server when the bug occurs

2. **Reproduce the bug**:
   ```bash
   baml-cli lsp --replay /tmp/wasm-bug.jsonl
   ```
   - The bug state will be reproduced automatically
   - Check the playground at localhost:3030 for the error
   - Continue debugging with live input

3. **Share the reproduction**:
   - Send `/tmp/wasm-bug.jsonl` to other developers
   - They can replay the exact same sequence to see the bug

### File Format

The recorded files use JSON Lines format with serde-compatible LSP messages:

```json
{"timestamp":"2024-01-01T12:00:00.123Z","direction":"in","message":{"jsonrpc":"2.0","method":"initialize",...}}
{"timestamp":"2024-01-01T12:00:00.456Z","direction":"out","message":{"jsonrpc":"2.0","id":1,"result":{...}}}
```

- `direction: "in"` - Messages from client to server  
- `direction: "out"` - Messages from server to client
- Only "in" messages are replayed (server responses are regenerated)

This approach will make it much easier to isolate and debug the "recursive use of object" error in the WASM playground by creating a reliable, fast reproduction mechanism.