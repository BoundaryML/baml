# BAML Onionskin - Implementation Summary

## Overview

BAML Onionskin is a Terminal User Interface (TUI) for exploring BAML compiler phases in real-time. Inspired by animation onion skinning techniques, it provides live file watching, phase navigation, and snapshot diffing capabilities.

## Name Origin

**Onion skinning** is an animation technique where translucent sheets (originally actual onion skin paper) allow animators to see previous and next frames overlaid on their current work. This tool applies that same concept to compiler development - you can snapshot your compiler output and see exactly how changes propagate through each compilation phase.

## Architecture

### Module Structure

```
baml_onionskin/
├── src/
│   ├── main.rs          # Entry point, CLI parsing
│   ├── app.rs           # Application state and event loop
│   ├── compiler.rs      # Compiler phase execution
│   ├── ui.rs            # Terminal UI rendering
│   └── watcher.rs       # File system watching
├── examples/
│   └── demo.baml        # Example BAML file
├── Cargo.toml          # Dependencies
├── README.md           # User-facing documentation
├── USAGE.md            # Detailed usage guide
├── QUICKSTART.md       # Quick reference
└── IMPLEMENTATION.md   # This file
```

### Key Components

#### 1. CLI (main.rs)
- Uses `clap` for argument parsing
- Single required argument: `--from <FILE>`
- Initializes terminal and runs the app
- Handles cleanup on exit

#### 2. Application State (app.rs)
- Manages the current state:
  - File path and content
  - Current compiler phase
  - Current output
  - Optional snapshot (the "onion skin")
- Event loop:
  - Checks for file changes
  - Handles keyboard input
  - Triggers UI redraws
- Key bindings:
  - `q` / `Ctrl+C`: Quit
  - `s`: Toggle snapshot (create/update onion skin)
  - `←` / `→`: Navigate phases (both panes update)

#### 3. Compiler Runner (compiler.rs)
- Six compiler phases:
  1. **Lexer**: Tokenizes input
  2. **Parser**: Builds syntax tree
  3. **HIR**: High-level IR
  4. **THIR**: Type inference
  5. **Diagnostics**: Collects errors
  6. **Codegen**: Generates bytecode
- Each phase produces formatted string output
- Output format matches `baml_tests` snapshots
- Uses `RootDatabase` for compiler integration

#### 4. File Watcher (watcher.rs)
- Uses `notify` crate
- Non-blocking change detection
- Monitors single file
- Triggers recompilation on changes

#### 5. UI Renderer (ui.rs)
- Uses `ratatui` for terminal rendering
- Four-section layout:
  1. Header: File path with "BAML Onionskin" title
  2. Phase tabs: Navigation
  3. Content: Output or diff (onion skin mode)
  4. Status bar: Help text
- Onion skin diff view:
  - Side-by-side comparison
  - Color coding: red for deletions, green for additions
  - Uses `similar` crate for diffing
  - Both panes update when changing phases

## Dependencies

### Core Functionality
- `baml_db`: Compiler database
- `baml_lexer`, `baml_parser`, `baml_hir`, `baml_thir`, `baml_codegen`: Compiler phases
- `baml_diagnostics`: Error handling

### UI/UX
- `ratatui` (v0.29): Terminal UI framework
- `crossterm` (v0.28): Cross-platform terminal control
- `notify` (v7.0): File system watching
- `similar` (v2.6): Text diffing (like comparing animation frames)
- `clap` (v4.5): CLI argument parsing
- `anyhow` (v1.0): Error handling

## Features Implemented

✅ CLI with `--from` flag
✅ Real-time file watching
✅ Six compiler phases (Lexer, Parser, HIR, THIR, Diagnostics, Codegen)
✅ Phase navigation with arrow keys
✅ Snapshot creation with 's' key (onion skin mode)
✅ Side-by-side diff view
✅ Snapshot update functionality
✅ Color-coded diffs (red/green)
✅ **Both panes update when navigating phases** (key onion skinning feature)
✅ Status bar with help text
✅ Graceful exit handling
✅ Output format matching insta snapshots

## Usage Example

```bash
# Terminal 1: Start the TUI
cargo run --bin baml_onionskin -- --from test.baml

# Terminal 2: Edit the file
echo "class NewClass { field string }" >> test.baml

# Terminal 1: See the update in real-time!
# Navigate phases to see how the change propagates through compilation
```

## Design Decisions

### Why "Onion Skinning"?
- Perfect metaphor: traditional animation technique of overlaying frames
- Captures the dual-view, comparative nature of the tool
- More evocative than "polaroid" or "snapshot"
- Industry term that conveys the layered transparency concept

### Why Update Both Panes When Navigating?
- Core onion skinning principle: see how changes look across different "frames"
- Allows cross-phase analysis of a single change
- More useful than freezing snapshot at one phase
- Treats compiler phases like animation frames

### Why Non-Blocking File Watching?
- Allows UI to remain responsive
- Prevents freezing during compilation
- Better user experience

### Why Side-by-Side Diff?
- Easier to understand changes
- Matches familiar diff UIs (git diff, GitHub, etc.)
- Uses full terminal width efficiently
- Resembles traditional onion skinning layouts

### Why Match Insta Format?
- Consistency with existing test infrastructure
- Familiar to BAML developers
- Easier to debug snapshot tests

### Why Six Phases?
- Covers the full compilation pipeline
- Each phase provides unique insights
- Matches the test infrastructure phases
- Like viewing an animation at different points in the timeline

## Performance Considerations

- **Lazy Compilation**: Only compiles on file changes
- **Non-Blocking UI**: Event loop uses 100ms timeout
- **Efficient Diffing**: `similar` crate is optimized for text diffs
- **Salsa Integration**: Leverages incremental compilation
- **Dual Compilation**: In snapshot mode, compiles both versions per phase (acceptable overhead for the insight gained)

## Key Implementation Detail: Phase Navigation with Snapshots

When a snapshot exists and the user navigates to a different phase:

```rust
fn recompile(&mut self) -> Result<()> {
    // Compile current content for current phase
    self.current_output = self.compiler
        .run_phase(self.current_phase, &self.current_content)?;
    
    // If snapshot exists, also recompile it for the current phase
    if let Some(ref snapshot) = self.snapshot {
        let snapshot_output = self.compiler
            .run_phase(self.current_phase, &snapshot.content)?;
        self.snapshot = Some(Snapshot {
            content: snapshot.content.clone(),
            output: snapshot_output,
        });
    }
    
    Ok(())
}
```

This ensures both panes always show the same phase, enabling true onion skin comparison across the compilation pipeline.

## Future Enhancements (Not Implemented)

Possible improvements:
- [ ] Scrolling support for large outputs
- [ ] Multiple file watching
- [ ] Export snapshot to file
- [ ] Compare two files side-by-side
- [ ] Syntax highlighting in output
- [ ] Filter/search within output
- [ ] Performance metrics display
- [ ] History of snapshots (animation timeline)
- [ ] Mouse support
- [ ] Configuration file
- [ ] Ghosted/transparent view option (more literal onion skinning)

## Testing

The tool can be tested with existing BAML test files:

```bash
# Test with basic types
cargo run --bin baml_onionskin -- --from crates/baml_tests/projects/basic_types/types.baml

# Test with error cases
cargo run --bin baml_onionskin -- --from crates/baml_tests/projects/error_cases/syntax_errors.baml

# Test with demo file
cargo run --bin baml_onionskin -- --from crates/baml_onionskin/examples/demo.baml
```

## Integration with Workspace

- Added to `Cargo.toml` workspace members
- Uses workspace dependencies
- Follows workspace lint rules
- Binary available via `cargo run --bin baml_onionskin`

## Error Handling

- File not found: Early validation with clear error message
- Compilation errors: Displayed in respective phase outputs
- Terminal errors: Cleanup via `restore_terminal()`
- Watch errors: Silent failure, continues watching

## Cross-Platform Support

- Terminal handling: `crossterm` (Windows, macOS, Linux)
- File watching: `notify` (all platforms)
- Path handling: Uses `PathBuf` and `display()`

## Onion Skinning Metaphor Throughout

The tool carries the onion skinning metaphor throughout:
- Name: BAML Onionskin
- Snapshot terminology: "onion skin snapshot"
- Documentation: Consistent animation references
- Functionality: True frame-by-frame comparison across phases
- Philosophy: Treating compilation as a multi-frame animation process
