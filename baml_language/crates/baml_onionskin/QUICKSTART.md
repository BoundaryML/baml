# BAML Onionskin - Quick Start

## Install & Run

```bash
# Watch a single file
cargo run --bin baml_onionskin -- --from path/to/file.baml

# Watch an entire directory
cargo run --bin baml_onionskin -- --from path/to/directory
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `←` | Previous compiler phase |
| `→` | Next compiler phase |
| `↑` / `↓` | Scroll output |
| `PgUp` / `PgDn` | Page scroll |
| `Home` | Scroll to top |
| `Mouse Wheel` | Scroll output |
| `s` | Create/Update onion skin snapshot |
| `S` (Shift+S) | Delete snapshot |
| `q` | Quit |
| `Ctrl+C` | Exit |

## Compiler Phases

Navigate with `←` / `→`:

1. **Lexer** - View tokens
2. **Parser** - View syntax tree + errors
3. **HIR** - View high-level IR items
4. **THIR** - View type inference
5. **Diagnostics** - View all errors
6. **Codegen** - View bytecode

## Workflow

```
1. Start:  cargo run --bin baml_onionskin -- --from file.baml
2. View:   See lexer tokens (default)
3. Nav:    Press → to see parser output
4. Snap:   Press 's' to create onion skin snapshot
5. Edit:   Modify file.baml in your editor
6. Diff:   Watch live diff appear (red=removed, green=added)
7. Phase:  Press → or ← to view other phases (both panes update!)
8. Update: Press 's' again to update snapshot
9. Repeat: Continue editing and watching
```

## Example

```bash
# Create a test file
echo 'class User { name string }' > test.baml

# Start watching
cargo run --bin baml_onionskin -- --from test.baml

# In the TUI:
# - See tokens by default
# - Press → to see parser tree
# - Press 's' to snapshot
# - Edit test.baml (add a field)
# - Watch diff appear automatically
# - Press → to see HIR diff
```

## Tips

- Start with lexer to verify tokens
- Use parser to debug syntax errors
- Use HIR/THIR to understand type system
- Create snapshot before major changes
- Navigate phases to track transformations through entire pipeline

## What You See

### Without Snapshot
```
┌─ BAML Onionskin: test.baml ───────┐
├─ Lexer | Parser | ... ────────────┤
│                                   │
│  Compiler output here             │
│  (updates on file save)           │
│                                   │
├─ [s] Create Snapshot | [q] Quit ──┤
└───────────────────────────────────┘
```

### With Snapshot (Onion Skin Mode)
```
┌─ BAML Onionskin: test.baml ───────┐
├─ Lexer | Parser | ... ────────────┤
├────────────┬──────────────────────┤
│ Snapshot   │ Current              │
│            │                      │
│   line     │   line               │
│ - removed  │                      │
│            │ + added              │
│   line     │   line               │
│            │                      │
├─ [s] Update Snapshot | [q] Quit ──┤
└────────────┴──────────────────────┘
```

## That's It!

Start watching your BAML files and explore the compiler pipeline like an animator reviewing frames! 🧅🎬
