# Ty Language Server Architecture Comparison with BAML

This document analyzes the decoupled architecture of `ty` (Ruff's type checker) and compares it with BAML's current language server implementation to identify improvements for BAML.

## Overview of Recent BAML Changes

Based on the git diff, we recently reorganized BAML's language server into more modular crates:

- **`baml_lsp_ide`**: LSP-agnostic IDE features (hover, completions, etc.)
- **`baml_lsp_project`**: Project management, file tracking, symbol utilities
- **`baml_lsp_tests`**: Test infrastructure with cursor markers

This is moving in the right direction, following the separation of concerns exemplified by ty.

---

## Ty Architecture Overview

Ty separates its language server into three core crates:

### 1. `ty_ide` - Pure IDE Features (No LSP Dependencies)
- **Purpose**: Provides language intelligence features completely independent of LSP
- **Key characteristics**:
  - No `lsp_types` dependency
  - Uses the semantic database directly via `ty_project::Db`
  - Returns typed, internal structs (not LSP types)
  - All functions take `db: &dyn Db` as first parameter

**Example: Hover API**
```rust
pub fn hover(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<Hover<'_>>> {
    let parsed = parsed_module(db, file).load(db);
    let model = SemanticModel::new(db, file);
    let goto_target = find_goto_target(&model, &parsed, offset)?;
    // ... returns internal Hover type
}
```

**Exported Functions**:
- `hover(db, file, offset)` → `Option<RangedValue<Hover>>`
- `goto_definition(db, file, offset)` → `NavigationTargets`
- `completion(db, file, offset, settings)` → `Vec<Completion>`
- `code_actions(db, file, range)` → `Vec<QuickFix>`
- `semantic_tokens(db, file)` → `SemanticTokens`
- `inlay_hints(db, file, range, settings)` → `Vec<InlayHint>`

### 2. `ty_project` - Project Management (No LSP Dependencies)
- **Purpose**: Manages project files, settings, and the Salsa database
- **Key characteristics**:
  - Contains `ProjectDatabase` (the main Salsa database)
  - Defines `Project` as a Salsa input containing files, settings, metadata
  - Exposes `check_file(file)` → `Vec<Diagnostic>` for diagnostics
  - Uses `ruff_db::diagnostic::Diagnostic` (internal, rich diagnostic type)

**Core Components**:
```rust
#[salsa::db]
pub trait Db: SemanticDb {
    fn project(&self) -> Project;
    fn dyn_clone(&self) -> Box<dyn Db>;
}

#[salsa::tracked]
impl Project {
    pub fn check_file(self, db: &dyn Db, file: File) -> Vec<Diagnostic>
    pub fn files(self, db: &dyn Db) -> Indexed<'_>
    pub fn open_file(self, db: &mut dyn Db, file: File)
    pub fn close_file(self, db: &mut dyn Db, file: File) -> bool
}
```

### 3. `ty_server` - LSP Protocol Layer
- **Purpose**: Handles LSP protocol, converts internal types to LSP types
- **Key characteristics**:
  - Depends on both `ty_ide` and `ty_project`
  - Uses `lsp_types` for protocol types
  - Contains conversion logic (e.g., `to_lsp_diagnostic()`)
  - Manages document synchronization, sessions, client capabilities

**Diagnostics Flow**:
```rust
// ty_server/src/server/api/diagnostics.rs
pub(super) fn compute_diagnostics(
    db: &ProjectDatabase,
    document: &DocumentHandle,
    encoding: PositionEncoding,
) -> Option<Diagnostics> {
    let file = document.notebook_or_file(db)?;
    let diagnostics = db.check_file(file);  // Calls ty_project
    Some(Diagnostics { items: diagnostics, encoding, file_or_notebook: file })
}

// Conversion to LSP types
pub(super) fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: PositionEncoding,
    client_capabilities: ResolvedClientCapabilities,
    global_settings: &GlobalSettings,
) -> Option<(Option<lsp_types::Url>, lsp_types::Diagnostic)> {
    // ... converts internal Diagnostic to LSP Diagnostic
}
```

### 4. `ruff_db::diagnostic` - Unified Diagnostic Type
- **Purpose**: A single, rich diagnostic type used across all crates
- **Key features**:
  - `Diagnostic` struct with `id`, `severity`, `message`, `annotations`, `sub_diagnostics`
  - `Annotation` for primary/secondary source spans
  - `Span` can reference either `File` (ty) or `SourceFile` (ruff)
  - Supports fixes via `Fix` and `Applicability`
  - Multiple rendering formats (Full, Concise, JSON, GitHub, etc.)

```rust
pub struct Diagnostic {
    id: DiagnosticId,
    severity: Severity,
    message: DiagnosticMessage,
    annotations: Vec<Annotation>,
    subs: Vec<SubDiagnostic>,
    fix: Option<Fix>,
    // ... more fields
}

pub enum DiagnosticId {
    Panic,
    Io,
    InvalidSyntax,
    Lint(LintName),
    RevealedType,
    // ...
}
```

---

## BAML Current Architecture

### Current Crate Structure

```
baml_language_server/     # LSP server + diagnostics + IDE features (mixed)
baml_lsp_ide/             # New: IDE features (hover)
baml_lsp_project/         # New: LspDatabase wrapper, symbols, position utilities
baml_lsp_tests/           # New: Test infrastructure
baml_diagnostics/         # Error rendering (Ariadne-based)
```

### Current Diagnostic Flow (BAML)

```rust
// baml_language_server/src/server/api/diagnostics.rs
pub(super) fn project_diagnostics(...) -> HashMap<Url, Vec<lsp_types::Diagnostic>> {
    let lsp_db = guard.lsp_db();
    let db = lsp_db.db();
    
    // 1. Gather parse errors (returns ParseError)
    for error in baml_parser::parse_errors(db, source_file) {
        let diag = parse_error_to_diagnostic(&error, ...);  // → lsp_types::Diagnostic
        add_diagnostic(file_id, diag);
    }
    
    // 2. Gather HIR diagnostics (returns HirDiagnostic)
    for error in lowering_result.diagnostics(db) {
        let diag = hir_diagnostic_to_lsp_diagnostic(error, ...);  // → lsp_types::Diagnostic
        add_diagnostic(file_id, diag);
    }
    
    // 3. Gather type errors (returns TypeError)
    for type_error in inference_result.errors {
        let diag = type_error_to_diagnostic(type_error, ...);  // → lsp_types::Diagnostic
        add_diagnostic(file_id, diag);
    }
}
```

### Key Differences from Ty

| Aspect | Ty | BAML |
|--------|-----|------|
| **Diagnostic Type** | Single unified `ruff_db::Diagnostic` | Multiple types: `ParseError`, `HirDiagnostic`, `TypeError`, `NameError` |
| **Conversion Location** | Server layer (`ty_server`) | Compiler layer (each error type knows how to convert) |
| **IDE Features** | Pure functions, no LSP types | Mixed: `baml_lsp_ide` is LSP-agnostic but incomplete |
| **Database Access** | `db: &dyn Db` trait object | `&RootDatabase` + `LspDatabase` wrapper |
| **Check Entry Point** | `db.check_file(file) → Vec<Diagnostic>` | Manual traversal in `project_diagnostics()` |
| **Span Type** | `ruff_db::Span` with `File` + optional range | `baml_base::Span` with `FileId` + `TextRange` |

---

## Recommendations for BAML

### 1. Create a Unified Diagnostic Type

**Current Problem**: Multiple error types (`ParseError`, `TypeError`, `NameError`, `HirDiagnostic`) each need separate conversion to LSP diagnostics.

**Recommendation**: Create a `baml_diagnostics::Diagnostic` type similar to `ruff_db::Diagnostic`:

```rust
// baml_diagnostics/src/diagnostic.rs
pub struct Diagnostic {
    pub id: DiagnosticId,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Span,
    pub annotations: Vec<Annotation>,
    pub related_info: Vec<RelatedInformation>,
}

pub enum DiagnosticId {
    Parse(ParseErrorKind),
    Type(TypeErrorKind),
    Name(NameErrorKind),
    Hir(HirErrorKind),
}

pub struct Annotation {
    pub span: Span,
    pub message: Option<String>,
    pub is_primary: bool,
}
```

**Benefits**:
- Single conversion function to LSP types
- Richer diagnostics with related information
- Easier to add new diagnostic sources
- Better for CLI/testing output

### 2. Centralize Diagnostics Collection

**Current Problem**: `project_diagnostics()` manually iterates through files and collects errors from different sources.

**Recommendation**: Add a `check_file()` method on the database/project:

```rust
// baml_lsp_project/src/lib.rs
impl Project {
    pub fn check_file(&self, db: &RootDatabase, file: SourceFile) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        
        // Parse errors
        diagnostics.extend(
            baml_parser::parse_errors(db, file)
                .iter()
                .map(|e| e.to_diagnostic())
        );
        
        // HIR lowering diagnostics
        let lowering = file_lowering(db, file);
        diagnostics.extend(
            lowering.diagnostics(db)
                .iter()
                .map(|e| e.to_diagnostic())
        );
        
        // Type errors (for expression functions)
        // ...
        
        diagnostics
    }
}
```

**Benefits**:
- Single entry point for all diagnostics
- Salsa caching at the file level
- Cleaner separation between collection and presentation

### 3. Rename `baml_lsp_ide` → `baml_ide` and Complete Separation

**Current State**: `baml_lsp_ide` has hover but still references some LSP concepts. The name still contains "lsp" even though the crate should be LSP-agnostic.

**Recommendation**: Rename to `baml_ide` and move ALL IDE features there:

```rust
// baml_ide/src/lib.rs
pub mod hover;
pub mod goto_definition;
pub mod completion;
pub mod code_action;
pub mod code_lens;
pub mod semantic_tokens;
pub mod document_symbols;

// All functions should have this signature pattern:
pub fn hover(db: &dyn Db, file: SourceFile, project: Project, offset: TextSize) -> Option<RangedValue<Hover>>
pub fn goto_definition(db: &dyn Db, file: SourceFile, project: Project, offset: TextSize) -> NavigationTargets
pub fn completions(db: &dyn Db, file: SourceFile, project: Project, offset: TextSize) -> Vec<Completion>
```

**Key Changes Needed**:
- Rename crate from `baml_lsp_ide` to `baml_ide` (removes misleading "lsp" from name)
- Move code lens generation from `baml_language_server` to `baml_ide`
- Move code action generation
- Create internal types (`Completion`, `CodeLens`, etc.) that are then converted in the server layer

### 4. Improve the Database Trait Hierarchy

**Current State**: `LspDatabase` wraps `RootDatabase` with file management.

**Recommendation**: Follow ty's pattern with trait-based database access:

```rust
// baml_db/src/lib.rs
#[salsa::db]
pub trait Db: baml_hir::Db {
    fn project(&self) -> Project;
}

// baml_lsp_project/src/lib.rs
pub struct ProjectDatabase {
    storage: salsa::Storage<ProjectDatabase>,
    project: Option<Project>,
    // ...
}

#[salsa::db]
impl Db for ProjectDatabase {
    fn project(&self) -> Project {
        self.project.unwrap()
    }
}
```

**Benefits**:
- Consistent database access across all crates
- Easier testing (can use trait objects)
- More flexible for different database implementations

### 5. Add a Check Mode for Open Files vs All Files

**Ty Feature**: `CheckMode::OpenFiles` vs `CheckMode::AllFiles`

```rust
pub enum CheckMode {
    OpenFiles,   // Only check files open in editor
    AllFiles,    // Check entire project
}

impl Project {
    pub fn set_check_mode(&mut self, mode: CheckMode)
    pub fn should_check_file(&self, db: &dyn Db, file: File) -> bool
}
```

**Benefits**:
- Better LSP performance (don't check closed files)
- CLI can still check all files

### 6. Improve Test Infrastructure

**Current State**: `baml_lsp_tests` has cursor-based testing for hover.

**Recommendation**: Expand to all IDE features:

```rust
// baml_ide/src/testing.rs (or separate crate)
impl CursorTest {
    pub fn hover(&self) -> String { ... }
    pub fn goto_definition(&self) -> Vec<NavigationTarget> { ... }
    pub fn completions(&self) -> Vec<Completion> { ... }
    pub fn diagnostics(&self) -> Vec<Diagnostic> { ... }
    
    // Helper to render diagnostics for snapshots
    pub fn render_diagnostics(&self, diagnostics: &[Diagnostic]) -> String { ... }
}
```

---

## Proposed New Architecture

```
baml_db/                  # Core Salsa database, Db trait
baml_diagnostics/         # Unified Diagnostic type, rendering
baml_lexer/               # Lexer
baml_parser/              # Parser, produces ParseError → Diagnostic
baml_hir/                 # HIR, produces HirDiagnostic → Diagnostic
baml_tir/                 # TIR, produces TypeError → Diagnostic
baml_project/             # NEW: Project management (rename from baml_lsp_project)
  - ProjectDatabase       # Main database
  - Project               # Salsa input for project files/settings
  - check_file()          # Single entry point for diagnostics
  - CheckMode             # Open files vs all files
baml_ide/                 # NEW: Pure IDE features (rename from baml_lsp_ide)
  - hover
  - goto_definition
  - completion
  - code_action
  - code_lens
  - semantic_tokens
baml_language_server/     # LSP protocol layer only
  - Type conversions (internal → LSP)
  - Document synchronization
  - Session management
baml_cli/                 # CLI commands
```

### Naming Rationale

The renames follow ty's convention:
- `ty_ide` (not `ty_lsp_ide`) - IDE features are LSP-agnostic
- `ty_project` (not `ty_lsp_project`) - Project management is LSP-agnostic

Similarly, BAML should use:
- `baml_ide` - Pure IDE intelligence, no LSP types
- `baml_project` - Project/workspace management, no LSP types

---

---

## Deep Dive: Multi-Format Rendering

A critical aspect of ty's architecture is how it handles **multiple rendering formats** for both diagnostics and IDE features like hover. The same underlying data can be rendered for different contexts (CLI, LSP, tests, JSON export).

### Ty's Diagnostic Rendering Architecture

Ty has a single `Diagnostic` type with a **pluggable renderer system**:

```rust
// ruff_db/src/diagnostic/mod.rs

pub enum DiagnosticFormat {
    Full,       // Pretty CLI output with snippets (like Rust errors)
    Concise,    // One-line per diagnostic (file:line:col: message)
    Azure,      // Azure Pipelines format
    Json,       // JSON export
    JsonLines,  // JSON Lines (one JSON object per line)
    Rdjson,     // Reviewdog JSON format
    Pylint,     // Pylint-style output
    Junit,      // JUnit XML format
    Gitlab,     // GitLab Code Quality format
    Github,     // GitHub Actions annotations
}
```

The rendering is configured via `DisplayDiagnosticConfig`:

```rust
pub struct DisplayDiagnosticConfig {
    format: DiagnosticFormat,
    color: bool,                    // Enable ANSI colors
    context: usize,                 // Lines of context around errors
    show_fix_status: bool,          // Show if fix is available
    show_fix_diff: bool,            // Show the actual fix diff
    fix_applicability: Applicability,
    // ...
}
```

### Rendering Flow

```
                    ┌─────────────────────────────────────┐
                    │         Diagnostic                  │
                    │  (single unified type)              │
                    └─────────────────┬───────────────────┘
                                      │
                    ┌─────────────────┼───────────────────┐
                    │                 │                   │
                    ▼                 ▼                   ▼
           ┌────────────────┐ ┌────────────────┐ ┌────────────────┐
           │ FullRenderer   │ │ ConciseRenderer│ │ JsonRenderer   │
           │ (pretty CLI)   │ │ (one-line)     │ │ (export)       │
           └────────────────┘ └────────────────┘ └────────────────┘
                    │                 │                   │
                    ▼                 ▼                   ▼
           ┌────────────────┐ ┌────────────────┐ ┌────────────────┐
           │   Tests use    │ │   IDE status   │ │   CI systems   │
           │   Full format  │ │   bar uses     │ │   use JSON/    │
           │   for snapshots│ │   Concise      │ │   GitHub fmt   │
           └────────────────┘ └────────────────┘ └────────────────┘
                                      
                                      │
                                      ▼
                            ┌────────────────────┐
                            │  LSP Conversion    │
                            │  to_lsp_diagnostic │
                            │  (in ty_server)    │
                            └────────────────────┘
```

### Ty's Hover Rendering with MarkupKind

For hover content, ty uses `MarkupKind` to render the same content in different formats:

```rust
// ty_ide/src/markup.rs

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

impl MarkupKind {
    /// Render a code block appropriately for the format
    pub fn fenced_code_block<T>(&self, code: T, language: &str) -> FencedCodeBlock<T>
    where T: fmt::Display {
        FencedCodeBlock { language, code, kind: *self }
    }
    
    /// Render a horizontal separator
    pub fn horizontal_line(&self) -> HorizontalLine {
        HorizontalLine { kind: *self }
    }
}

// The FencedCodeBlock renders differently based on format:
impl<T: Display> Display for FencedCodeBlock<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.kind {
            MarkupKind::PlainText => self.code.fmt(f),  // Just the code
            MarkupKind::Markdown => write!(
                f,
                "```{language}\n{code}\n```",  // Fenced code block
                language = self.language,
                code = self.code
            ),
        }
    }
}
```

### Hover Content with Display Trait

```rust
// ty_ide/src/hover.rs

pub struct Hover<'db> {
    contents: Vec<HoverContent<'db>>,
}

impl<'db> Hover<'db> {
    /// Renders the hover to a string using the specified markup kind.
    pub fn display(&self, db: &'db dyn Db, kind: MarkupKind) -> DisplayHover<'db> {
        DisplayHover { db, hover: self, kind }
    }
}

pub struct DisplayHover<'db, 'a> {
    db: &'db dyn Db,
    hover: &'a Hover<'db>,
    kind: MarkupKind,
}

impl Display for DisplayHover<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for content in &self.hover.contents {
            content.display(self.db, self.kind).fmt(f)?;
        }
        Ok(())
    }
}
```

### Docstrings with Format-Aware Rendering

```rust
// ty_ide/src/docstring.rs

pub struct Docstring(String);

impl Docstring {
    /// Render the docstring to the given markup format
    pub fn render(&self, kind: MarkupKind) -> String {
        match kind {
            MarkupKind::PlainText => self.render_plaintext(),
            MarkupKind::Markdown => self.render_markdown(),
        }
    }
}
```

---

## BAML's Current Rendering Problem

### Multiple Render Functions per Error Type

BAML currently has separate render functions for each error type:

```rust
// baml_diagnostics/src/compiler_error.rs

pub fn render_parse_error(error: &ParseError, sources: &HashMap<FileId, String>, color: bool) -> String
pub fn render_type_error<Ty>(error: &TypeError<Ty>, sources: &HashMap<FileId, String>, color: bool) -> String
pub fn render_name_error(error: &NameError, sources: &HashMap<FileId, String>, color: bool) -> String
pub fn render_hir_diagnostic(error: &HirDiagnostic, sources: &HashMap<FileId, String>, color: bool) -> String
```

These all produce Ariadne-formatted output (pretty CLI errors), but:
1. **No LSP format** - Separate conversion code in `diagnostics.rs`
2. **No JSON format** - Would need another set of functions
3. **No concise format** - Would need yet another set

### The Duplication Problem

```
                 ParseError               TypeError              HirDiagnostic
                     │                        │                       │
        ┌────────────┼────────────┐          │           ┌───────────┼──────────┐
        │            │            │          │           │           │          │
        ▼            ▼            ▼          ▼           ▼           ▼          ▼
render_parse_error   │   parse_error_     type_error_   │   hir_diagnostic_    │
 (Ariadne)           │   to_diagnostic   to_diagnostic  │   to_lsp_diagnostic  │
                     │   (LSP)            (LSP)         │   (LSP)              │
                     │                                  │                      │
                     └──────────────────────────────────┴──────────────────────┘
                                        Duplicated logic!
```

---

## Recommended Rendering Architecture for BAML

### 1. Single Diagnostic Type with Renderers

```rust
// baml_diagnostics/src/lib.rs

pub struct Diagnostic {
    pub id: DiagnosticId,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Span,
    pub annotations: Vec<Annotation>,
    pub related_info: Vec<RelatedInfo>,
    pub error_code: ErrorCode,
}

pub enum DiagnosticFormat {
    Ariadne,    // Pretty CLI (what tests use)
    Concise,    // One-line (for quick checks)
    Lsp,        // lsp_types::Diagnostic
    Json,       // JSON export
}
```

### 2. Renderer Trait

```rust
pub trait DiagnosticRenderer {
    type Output;
    fn render(&self, diagnostic: &Diagnostic, config: &RenderConfig) -> Self::Output;
}

// Ariadne renderer for CLI/tests
pub struct AriadneRenderer;
impl DiagnosticRenderer for AriadneRenderer {
    type Output = String;
    fn render(&self, diagnostic: &Diagnostic, config: &RenderConfig) -> String {
        // Build Ariadne Report from unified Diagnostic
    }
}

// LSP renderer for language server
pub struct LspRenderer;
impl DiagnosticRenderer for LspRenderer {
    type Output = lsp_types::Diagnostic;
    fn render(&self, diagnostic: &Diagnostic, config: &RenderConfig) -> lsp_types::Diagnostic {
        // Convert unified Diagnostic to LSP format
    }
}
```

### 3. Display Trait Pattern (simpler alternative)

```rust
impl Diagnostic {
    /// Render for CLI/tests using Ariadne
    pub fn display_ariadne(&self, sources: &SourceCache, color: bool) -> String {
        AriadneRenderer::new(color).render(self, sources)
    }
    
    /// Render for LSP
    pub fn to_lsp(&self, encoding: PositionEncoding) -> lsp_types::Diagnostic {
        LspRenderer::new(encoding).render(self)
    }
    
    /// Render concise (one-line)
    pub fn display_concise(&self) -> String {
        format!("{}:{}:{}: {} [{}]", 
            self.file_path(),
            self.line(),
            self.column(),
            self.message,
            self.error_code
        )
    }
}
```

### 4. Hover with MarkupKind (following ty)

BAML already has this partially:

```rust
// baml_lsp_ide/src/lib.rs (current)
pub enum MarkupKind {
    PlainText,
    Markdown,
}
```

Extend it to all IDE features:

```rust
// baml_ide/src/lib.rs (proposed)

pub enum MarkupKind {
    PlainText,
    Markdown,
}

impl MarkupKind {
    pub fn code_block(&self, code: &str, language: &str) -> String {
        match self {
            MarkupKind::PlainText => code.to_string(),
            MarkupKind::Markdown => format!("```{}\n{}\n```", language, code),
        }
    }
    
    pub fn bold(&self, text: &str) -> String {
        match self {
            MarkupKind::PlainText => text.to_string(),
            MarkupKind::Markdown => format!("**{}**", text),
        }
    }
    
    pub fn italic(&self, text: &str) -> String {
        match self {
            MarkupKind::PlainText => text.to_string(),
            MarkupKind::Markdown => format!("*{}*", text),
        }
    }
}
```

### 5. Unified Rendering Config

```rust
pub struct RenderConfig {
    /// Enable colors (for CLI)
    pub color: bool,
    /// Markup format (for hover/docs)
    pub markup: MarkupKind,
    /// Diagnostic format
    pub diagnostic_format: DiagnosticFormat,
    /// Show error codes
    pub show_error_codes: bool,
    /// Context lines around error
    pub context_lines: usize,
}

impl RenderConfig {
    pub fn for_tests() -> Self {
        Self {
            color: false,
            markup: MarkupKind::PlainText,
            diagnostic_format: DiagnosticFormat::Ariadne,
            show_error_codes: true,
            context_lines: 2,
        }
    }
    
    pub fn for_lsp() -> Self {
        Self {
            color: false,
            markup: MarkupKind::Markdown,  // LSP clients usually want Markdown
            diagnostic_format: DiagnosticFormat::Lsp,
            show_error_codes: true,
            context_lines: 0,  // Not needed for LSP
        }
    }
}
```

---

## How Tests Should Use Rendering

### Current (problematic)

```rust
// runner.rs - tests directly use Ariadne render functions
all_errors.push(render_parse_error(error, &sources, false));
all_errors.push(render_type_error(error, &sources, false));
```

### Proposed

```rust
// runner.rs - tests use unified Diagnostic with Ariadne renderer
let diagnostics = project.check(&db);
let config = RenderConfig::for_tests();

let rendered = diagnostics
    .iter()
    .map(|d| d.display_ariadne(&sources, config.color))
    .collect::<Vec<_>>()
    .join("\n");
```

Or even simpler with a helper:

```rust
let rendered = render_diagnostics(&diagnostics, &sources, RenderConfig::for_tests());
```

---

## Deep Dive: Test Infrastructure (`baml_lsp_tests`)

### Current Problem: Duplicated Diagnostic Collection

The `baml_lsp_tests/src/runner.rs` file contains the **exact same diagnostic collection logic** as `baml_language_server/src/server/api/diagnostics.rs`:

**From `runner.rs` (lines 70-124):**
```rust
// Collect parse errors
for source_file in &source_files {
    let errors = baml_parser::parse_errors(&db, *source_file);
    for error in errors.iter() {
        all_errors.push(render_parse_error(error, &sources, false));
    }
}

// Collect HIR lowering diagnostics
for source_file in &source_files {
    let lowering_result = baml_hir::file_lowering(&db, *source_file);
    for diag in lowering_result.diagnostics(&db) {
        all_errors.push(render_hir_diagnostic(diag, &sources, false));
    }
}

// Collect validation errors
let validation_result = baml_hir::validate_hir(&db, root);
// ... same pattern ...

// Collect type errors
for source_file in &source_files {
    // ... manually iterate functions, infer types ...
}
```

**From `diagnostics.rs` (lines 183-254):**
```rust
// 1. Gather parse errors
for source_file in &source_files {
    let parse_errors = baml_parser::parse_errors(db, *source_file);
    // ... same pattern ...
}

// 2. Gather HIR lowering diagnostics
for source_file in &source_files {
    let lowering_result = file_lowering(db, *source_file);
    // ... same pattern ...
}

// 3. Gather validation errors
let validation_result = baml_hir::validate_hir(db, project_root);
// ... same pattern ...

// ... type errors ...
```

This is **problematic** because:
1. **Code duplication** - Any new diagnostic source must be added in two places
2. **Inconsistency risk** - Test and production might diverge
3. **No Salsa caching** - Tests don't benefit from incremental computation
4. **Maintenance burden** - Changes require updates in multiple files

### Recommended Solution: Centralized `check_project()`

Create a single entry point on `Project` that both the LSP and tests use:

```rust
// baml_project/src/lib.rs (formerly baml_lsp_project)

impl Project {
    /// Check all files in the project and return diagnostics.
    ///
    /// This is the single entry point for all diagnostic collection.
    pub fn check(&self, db: &RootDatabase) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        
        for source_file in self.files(db) {
            diagnostics.extend(self.check_file(db, source_file));
        }
        
        // Add project-level validation errors (duplicates across files)
        diagnostics.extend(self.check_project_level(db));
        
        diagnostics
    }
    
    /// Check a single file and return diagnostics.
    #[salsa::tracked]
    pub fn check_file(&self, db: &RootDatabase, file: SourceFile) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        
        // 1. Parse errors
        for error in baml_parser::parse_errors(db, file) {
            diagnostics.push(error.to_diagnostic());
        }
        
        // 2. HIR lowering diagnostics
        let lowering = baml_hir::file_lowering(db, file);
        for diag in lowering.diagnostics(db) {
            diagnostics.push(diag.to_diagnostic());
        }
        
        // 3. Type errors (for expression functions)
        // ... infer types and collect errors ...
        
        diagnostics
    }
    
    /// Project-level validation (duplicates, reserved names).
    fn check_project_level(&self, db: &RootDatabase) -> Vec<Diagnostic> {
        let validation = baml_hir::validate_hir(db, *self);
        
        let mut diagnostics = Vec::new();
        for diag in validation.hir_diagnostics {
            diagnostics.push(diag.to_diagnostic());
        }
        for error in validation.name_errors {
            diagnostics.push(error.to_diagnostic());
        }
        diagnostics
    }
}
```

### Updated Test Runner

With centralized diagnostics, `runner.rs` becomes dramatically simpler:

```rust
// baml_lsp_tests/src/runner.rs (NEW)

pub fn run_test(parsed: &ParsedTestFile) -> TestResult {
    // 1. Create database and add files
    let mut db = RootDatabase::new();
    let project = setup_project(&mut db, &parsed.files);
    
    // 2. Collect ALL diagnostics with ONE call
    let diagnostics = project.check(&db);
    
    // 3. Render diagnostics for comparison
    let actual_diagnostics = render_diagnostics(&diagnostics, &db);
    
    // 4. Handle cursor hovers (unchanged)
    let actual_hovers = if !parsed.cursor_markers.is_empty() {
        Some(collect_hovers(&db, project, &parsed.cursor_markers))
    } else {
        None
    };
    
    // 5. Compare
    compare_results(parsed, actual_diagnostics, actual_hovers)
}
```

### Diagnostic Rendering for Tests

The test framework needs Ariadne-formatted output (pretty console errors).
With a unified `Diagnostic` type, we can add rendering methods:

```rust
impl Diagnostic {
    /// Render this diagnostic using Ariadne for pretty console output.
    pub fn render_ariadne(&self, sources: &HashMap<FileId, String>) -> String {
        // Use existing render_* functions from baml_diagnostics
        baml_diagnostics::render_diagnostic(self, sources, ColorMode::Off)
    }
    
    /// Convert to LSP diagnostic for language server.
    pub fn to_lsp_diagnostic(&self, ...) -> lsp_types::Diagnostic {
        // Existing conversion logic
    }
}
```

### Test Output Format

The tests use Ariadne-formatted output embedded in BAML comments:

```baml
//----
//- diagnostics
// Error: Expected int, found string
//     ╭─[ 0:21:5 ]
//     │
//  21 │     e = "oops";
//     │        ───┬───  
//     │           ╰───── Expected int, found string
// ────╯
```

This format should be preserved. The key change is **where** the diagnostics come from:
- **Before**: Manual collection in `runner.rs`
- **After**: Single call to `project.check(&db)`

### Benefits of Centralized Diagnostics for Tests

1. **Single source of truth** - LSP and tests use the same code path
2. **Salsa caching** - `check_file` is a tracked query, results are cached
3. **Easy to extend** - Add new diagnostic source in one place
4. **Consistent behavior** - Tests verify exactly what users see in the IDE
5. **Simpler test code** - 50+ lines of collection code becomes 1 line

### How Ty Does Test Diagnostics

From `ty_ide/src/lib.rs` (tests module):

```rust
impl CursorTest {
    pub fn render_diagnostics<I, D>(&self, diagnostics: I) -> String
    where
        I: IntoIterator<Item = D>,
        D: IntoDiagnostic,
    {
        let config = DisplayDiagnosticConfig::default()
            .color(false)
            .format(DiagnosticFormat::Full);
        
        let mut buf = String::new();
        for diagnostic in diagnostics {
            let diag = diagnostic.into_diagnostic();
            write!(buf, "{}", diag.display(&self.db, &config)).unwrap();
        }
        buf
    }
}
```

And diagnostics come from a single source:

```rust
// ty_project uses check_file_impl as the single entry point
let diagnostics = db.check_file(file);
```

### Migration for `baml_lsp_tests`

#### Step 1: Add `ToDiagnostic` Trait

```rust
// baml_diagnostics/src/lib.rs
pub trait ToDiagnostic {
    fn to_diagnostic(&self) -> Diagnostic;
}

impl ToDiagnostic for ParseError { ... }
impl ToDiagnostic for TypeError<T> { ... }
impl ToDiagnostic for HirDiagnostic { ... }
impl ToDiagnostic for NameError { ... }
```

#### Step 2: Add `check()` to Project

```rust
// baml_project/src/lib.rs
impl Project {
    pub fn check(&self, db: &RootDatabase) -> Vec<Diagnostic> { ... }
    pub fn check_file(&self, db: &RootDatabase, file: SourceFile) -> Vec<Diagnostic> { ... }
}
```

#### Step 3: Simplify `runner.rs`

Replace 50+ lines of diagnostic collection with:
```rust
let diagnostics = project.check(&db);
```

#### Step 4: Update LSP `diagnostics.rs`

Replace manual collection with:
```rust
let diagnostics = project.check(&db);
for diag in diagnostics {
    let lsp_diag = diag.to_lsp_diagnostic(encoding, ...);
    add_diagnostic(diag.file_id(), lsp_diag);
}
```

---

## Migration Path

### Phase 1: Unified Diagnostics with Multi-Format Rendering
1. Create `baml_diagnostics::Diagnostic` type (unified across all error kinds)
2. Add `ToDiagnostic` trait to error types (`ParseError`, `TypeError`, `HirDiagnostic`, `NameError`)
3. Create `DiagnosticFormat` enum: `Ariadne`, `Concise`, `Lsp`, `Json`
4. Add rendering methods to `Diagnostic`:
   - `display_ariadne()` for CLI and tests (pretty output)
   - `display_concise()` for quick one-line checks
   - `to_lsp()` for language server
5. Create `RenderConfig` for unified rendering configuration

### Phase 2: Centralized Check on Project
1. Add `Project::check(&self, db) -> Vec<Diagnostic>` method
2. Add `Project::check_file(&self, db, file) -> Vec<Diagnostic>` (Salsa-tracked)
3. **Refactor `baml_lsp_tests/src/runner.rs`** to use `project.check()` (simplifies 50+ lines to 1)
4. **Refactor `baml_language_server/diagnostics.rs`** to use `project.check()`
5. Remove duplicated diagnostic collection code from both

### Phase 3: Rename and Complete IDE Separation
1. **Rename `baml_lsp_ide` → `baml_ide`** (remove "lsp" from name)
2. **Rename `baml_lsp_project` → `baml_project`** (remove "lsp" from name)
3. Move remaining IDE features (code lens, code actions) to `baml_ide`
4. Create internal types for all IDE features
5. Move conversion logic to server layer

### Phase 4: Database and Testing Cleanup
1. Implement proper `Db` trait hierarchy matching ty's pattern
2. Add `CheckMode` support (OpenFiles vs AllFiles)
3. Update `baml_lsp_tests` to use `CursorTest` from `baml_ide::testing`
4. Consolidate testing infrastructure between `baml_ide` and `baml_lsp_tests`

### Rename Commands

```bash
# Rename baml_lsp_ide to baml_ide
mv crates/baml_lsp_ide crates/baml_ide
# Update Cargo.toml package name
sed -i '' 's/name = "baml_lsp_ide"/name = "baml_ide"/' crates/baml_ide/Cargo.toml
# Update all dependencies in workspace
grep -r "baml_lsp_ide" --include="*.toml" -l | xargs sed -i '' 's/baml_lsp_ide/baml_ide/g'
# Update all imports in Rust files
grep -r "baml_lsp_ide" --include="*.rs" -l | xargs sed -i '' 's/baml_lsp_ide/baml_ide/g'

# Rename baml_lsp_project to baml_project
mv crates/baml_lsp_project crates/baml_project
sed -i '' 's/name = "baml_lsp_project"/name = "baml_project"/' crates/baml_project/Cargo.toml
grep -r "baml_lsp_project" --include="*.toml" -l | xargs sed -i '' 's/baml_lsp_project/baml_project/g'
grep -r "baml_lsp_project" --include="*.rs" -l | xargs sed -i '' 's/baml_lsp_project/baml_project/g'
```

---

## Summary

The key improvements from ty's architecture are:

1. **Single diagnostic type** - Makes code cleaner and more maintainable
2. **Multi-format rendering** - Same diagnostic renders to Ariadne (CLI), LSP, JSON, etc.
3. **Centralized check entry point** - `project.check()` is simpler than manual collection
4. **Clean separation**: IDE features → Project management → LSP protocol
5. **Trait-based database access** - More flexible and testable
6. **LSP-agnostic naming** - `ty_ide` and `ty_project` (not `ty_lsp_ide`)
7. **MarkupKind for hover** - Same hover content renders to PlainText or Markdown
8. **Shared code between tests and production** - Same diagnostic pipeline used everywhere

The recent reorganization of BAML is already moving in this direction. The main remaining work is:

| Priority | Task | Impact |
|----------|------|--------|
| 🔴 High | Create unified `Diagnostic` type | Enables all other improvements |
| 🔴 High | Add multi-format rendering (`DiagnosticFormat`) | Tests use Ariadne, LSP uses `to_lsp()` |
| 🔴 High | Add `Project::check()` entry point | Eliminates duplicated code in LSP and tests |
| 🟡 Medium | Rename `baml_lsp_ide` → `baml_ide` | Cleaner naming |
| 🟡 Medium | Rename `baml_lsp_project` → `baml_project` | Cleaner naming |
| 🟢 Lower | Move remaining IDE features | Better separation |
| 🟢 Lower | Add `CheckMode` | LSP performance |

### Key Insights

**From test infrastructure analysis:**
The `baml_lsp_tests/src/runner.rs` and `baml_language_server/diagnostics.rs` contain nearly identical 50+ line blocks of diagnostic collection code. This duplication proves the need for a centralized `project.check()` method that both can use.

**From rendering analysis:**
BAML currently has 4 separate `render_*` functions (one per error type) plus 4 separate `*_to_diagnostic` functions for LSP conversion. This is 8 functions that would become 2 methods on a unified `Diagnostic` type:
- `display_ariadne()` - for CLI/tests  
- `to_lsp()` - for language server

The same pattern applies to hover, where `MarkupKind::PlainText` is used for tests and `MarkupKind::Markdown` is used for the LSP.

---

## Implementation Plan

### Phase 1: Unified Diagnostic Type (Week 1)

#### 1.1 Create `baml_diagnostics::Diagnostic` struct

- [ ] **Create the unified Diagnostic type**
  ```rust
  // baml_diagnostics/src/diagnostic.rs
  pub struct Diagnostic {
      pub id: DiagnosticId,
      pub severity: Severity,
      pub message: String,
      pub primary_span: Span,
      pub annotations: Vec<Annotation>,
      pub related_info: Vec<RelatedInfo>,
      pub error_code: ErrorCode,
  }
  ```
  
- [ ] **Create supporting types**
  - `DiagnosticId` enum (Parse, Type, Name, Hir variants)
  - `Severity` enum (Error, Warning, Info)
  - `Annotation` struct (span + optional message + is_primary)
  - `RelatedInfo` struct (for "first defined here" links)

- [ ] **Verification**: Unit tests for `Diagnostic` construction
  ```bash
  cargo test -p baml_diagnostics
  ```

#### 1.2 Implement `ToDiagnostic` trait

- [ ] **Create the trait**
  ```rust
  pub trait ToDiagnostic {
      fn to_diagnostic(&self) -> Diagnostic;
  }
  ```

- [ ] **Implement for `ParseError`**
  - [ ] Write implementation
  - [ ] Add tests comparing old `render_parse_error` output

- [ ] **Implement for `TypeError<T>`**
  - [ ] Write implementation  
  - [ ] Add tests comparing old `render_type_error` output

- [ ] **Implement for `HirDiagnostic`**
  - [ ] Write implementation
  - [ ] Add tests comparing old `render_hir_diagnostic` output

- [ ] **Implement for `NameError`**
  - [ ] Write implementation
  - [ ] Add tests comparing old `render_name_error` output

- [ ] **Verification**: All existing diagnostic tests still pass
  ```bash
  cargo test -p baml_diagnostics
  cargo test -p baml_lsp_tests
  ```

#### 1.3 Multi-Format Rendering

- [ ] **Create `DiagnosticFormat` enum**
  ```rust
  pub enum DiagnosticFormat {
      Ariadne,  // Pretty CLI (current behavior)
      Concise,  // One-line
      Lsp,      // lsp_types::Diagnostic
  }
  ```

- [ ] **Create `RenderConfig`**
  ```rust
  pub struct RenderConfig {
      pub format: DiagnosticFormat,
      pub color: bool,
      pub show_error_codes: bool,
  }
  ```

- [ ] **Implement `display_ariadne()` method**
  - [ ] Move existing Ariadne logic into this method
  - [ ] Test output matches current `render_*` functions

- [ ] **Implement `display_concise()` method**
  - [ ] Format: `file:line:col: [E0001] message`

- [ ] **Implement `to_lsp()` method**
  - [ ] Move logic from `*_to_diagnostic` functions in `diagnostics.rs`

- [ ] **Verification**: Snapshot tests for each format
  ```bash
  cargo test -p baml_diagnostics -- --test-threads=1
  cargo insta review  # If using insta
  ```

---

### Phase 2: Centralized `Project::check()` (Week 2)

#### 2.1 Add `check()` methods to Project

- [ ] **Add `Project::check_file()` method**
  ```rust
  impl Project {
      pub fn check_file(&self, db: &RootDatabase, file: SourceFile) -> Vec<Diagnostic>
  }
  ```
  - [ ] Collect parse errors
  - [ ] Collect HIR lowering diagnostics
  - [ ] Collect type errors for expression functions

- [ ] **Add `Project::check()` method**
  ```rust
  impl Project {
      pub fn check(&self, db: &RootDatabase) -> Vec<Diagnostic>
  }
  ```
  - [ ] Iterate all files, call `check_file`
  - [ ] Add project-level validation (duplicates across files)

- [ ] **Verification**: Compare output with manual collection
  ```rust
  #[test]
  fn check_matches_manual_collection() {
      let manual = /* old way */;
      let unified = project.check(&db);
      assert_eq!(manual.len(), unified.len());
  }
  ```

#### 2.2 Refactor `baml_lsp_tests/src/runner.rs`

- [ ] **Replace manual diagnostic collection**
  - [ ] Before: 50+ lines of manual iteration
  - [ ] After: `let diagnostics = project.check(&db);`

- [ ] **Update rendering to use unified type**
  ```rust
  let rendered = diagnostics
      .iter()
      .map(|d| d.display_ariadne(&sources, false))
      .collect::<Vec<_>>()
      .join("\n");
  ```

- [ ] **Verification**: All LSP tests pass
  ```bash
  cargo test -p baml_lsp_tests
  ```

#### 2.3 Refactor `baml_language_server/diagnostics.rs`

- [ ] **Replace `project_diagnostics()` implementation**
  - [ ] Use `project.check(&db)`
  - [ ] Use `diagnostic.to_lsp()` for conversion

- [ ] **Remove duplicated conversion functions**
  - [ ] `parse_error_to_diagnostic` → DELETE
  - [ ] `type_error_to_diagnostic` → DELETE
  - [ ] `hir_diagnostic_to_lsp_diagnostic` → DELETE
  - [ ] `name_error_to_diagnostic` → DELETE

- [ ] **Verification**: LSP still works correctly
  ```bash
  # Manual test: open VS Code with BAML extension, verify diagnostics appear
  cargo build -p baml_language_server
  ```

---

### Phase 3: Crate Renames (Week 3)

#### 3.1 Rename `baml_lsp_ide` → `baml_ide`

- [ ] **Rename directory**
  ```bash
  cd baml_language/crates
  mv baml_lsp_ide baml_ide
  ```

- [ ] **Update `Cargo.toml`**
  ```toml
  [package]
  name = "baml_ide"
  ```

- [ ] **Update workspace `Cargo.toml`**
  ```bash
  sed -i '' 's/baml_lsp_ide/baml_ide/g' Cargo.toml
  ```

- [ ] **Update all imports**
  ```bash
  grep -r "baml_lsp_ide" --include="*.rs" -l | xargs sed -i '' 's/baml_lsp_ide/baml_ide/g'
  grep -r "baml_lsp_ide" --include="*.toml" -l | xargs sed -i '' 's/baml_lsp_ide/baml_ide/g'
  ```

- [ ] **Verification**
  ```bash
  cargo build -p baml_ide
  cargo test -p baml_ide
  ```

#### 3.2 Rename `baml_lsp_project` → `baml_project`

- [ ] **Rename directory**
  ```bash
  mv baml_lsp_project baml_project
  ```

- [ ] **Update `Cargo.toml`**

- [ ] **Update all imports**
  ```bash
  grep -r "baml_lsp_project" --include="*.rs" -l | xargs sed -i '' 's/baml_lsp_project/baml_project/g'
  grep -r "baml_lsp_project" --include="*.toml" -l | xargs sed -i '' 's/baml_lsp_project/baml_project/g'
  ```

- [ ] **Verification**
  ```bash
  cargo build -p baml_project
  cargo test -p baml_project
  ```

#### 3.3 Full Build Verification

- [ ] **Clean build**
  ```bash
  cargo clean
  cargo build --all
  ```

- [ ] **All tests pass**
  ```bash
  cargo test --all
  ```

- [ ] **LSP works**
  ```bash
  # Test in VS Code
  ```

---

### Phase 4: Complete IDE Separation (Week 4)

#### 4.1 Move Code Lens to `baml_ide`

- [ ] **Create `baml_ide/src/code_lens.rs`**
  - [ ] Define `CodeLens` struct (LSP-agnostic)
  - [ ] Implement `code_lenses(db, file, project) -> Vec<CodeLens>`

- [ ] **Update `baml_language_server` to use it**
  - [ ] Import from `baml_ide`
  - [ ] Convert `CodeLens` → `lsp_types::CodeLens` in server layer

- [ ] **Verification**: Code lenses work in VS Code

#### 4.2 Move Code Actions to `baml_ide`

- [ ] **Create `baml_ide/src/code_action.rs`**
  - [ ] Define `CodeAction` struct (LSP-agnostic)
  - [ ] Implement `code_actions(db, file, project, range) -> Vec<CodeAction>`

- [ ] **Update `baml_language_server` to use it**

- [ ] **Verification**: Quick fixes work in VS Code

#### 4.3 Add Remaining IDE Features

- [ ] **Goto Definition** - `baml_ide/src/goto_definition.rs`
- [ ] **Find References** - `baml_ide/src/find_references.rs`
- [ ] **Completions** - `baml_ide/src/completion.rs`
- [ ] **Document Symbols** - `baml_ide/src/document_symbols.rs`

- [ ] **Verification**: Each feature works in VS Code

---

### Phase 5: Database Improvements (Week 5)

#### 5.1 Implement `Db` Trait Hierarchy

- [ ] **Define `baml_project::Db` trait**
  ```rust
  #[salsa::db]
  pub trait Db: baml_hir::Db {
      fn project(&self) -> Project;
  }
  ```

- [ ] **Implement for `ProjectDatabase`**

- [ ] **Update all IDE functions to use `&dyn Db`**

- [ ] **Verification**: All code compiles and tests pass

#### 5.2 Add `CheckMode`

- [ ] **Define enum**
  ```rust
  pub enum CheckMode {
      OpenFiles,
      AllFiles,
  }
  ```

- [ ] **Add to Project**
  - [ ] `Project::set_check_mode()`
  - [ ] `Project::should_check_file()`

- [ ] **Update LSP to use `OpenFiles` mode**

- [ ] **Verification**: Only open files are checked in LSP

#### 5.3 Add Salsa Tracking to `check_file`

- [ ] **Make `check_file` a tracked query**
  ```rust
  #[salsa::tracked]
  pub fn check_file(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic>
  ```

- [ ] **Verification**: Editing one file doesn't recompute diagnostics for other files
  ```rust
  #[test]
  fn check_file_is_incremental() {
      // Edit file A, verify file B's check wasn't recomputed
  }
  ```

---

### Verification Checklist (After Each Phase)

#### Automated Tests
- [ ] `cargo test --all` passes
- [ ] `cargo test -p baml_lsp_tests` passes (all inline assertion tests)
- [ ] `cargo clippy --all` has no warnings
- [ ] `cargo fmt -- --check` passes

#### Manual Tests
- [ ] LSP diagnostics appear in VS Code
- [ ] Hover works on symbols
- [ ] Code lenses appear on functions
- [ ] Quick fixes work

#### Performance
- [ ] LSP startup time is acceptable
- [ ] Diagnostics appear quickly after edits
- [ ] No visible lag when typing

---

### Rollback Plan

If any phase breaks things:

1. **Git revert** to last working state
   ```bash
   git revert HEAD~N  # Revert N commits
   ```

2. **Keep old code paths** during transition
   ```rust
   // During migration, keep both paths:
   if cfg!(feature = "unified_diagnostics") {
       project.check(&db)
   } else {
       // Old manual collection
   }
   ```

3. **Feature flag** new behavior
   ```toml
   [features]
   unified_diagnostics = []
   ```

---

### Timeline

| Week | Phase | Deliverable |
|------|-------|-------------|
| 1 | Unified Diagnostics | `Diagnostic` type + `ToDiagnostic` + rendering |
| 2 | Centralized Check | `Project::check()` + refactor tests + LSP |
| 3 | Crate Renames | `baml_ide` + `baml_project` |
| 4 | IDE Separation | Code lens, actions, other features |
| 5 | Database | `Db` trait, `CheckMode`, Salsa tracking |

---

### Success Criteria

✅ **Phase 1 Complete When:**
- Single `Diagnostic` type exists
- All error types implement `ToDiagnostic`
- `display_ariadne()` and `to_lsp()` work
- All existing tests pass

✅ **Phase 2 Complete When:**
- `project.check()` exists
- `runner.rs` uses it (50+ lines removed)
- `diagnostics.rs` uses it (duplicate conversion functions removed)
- All tests pass

✅ **Phase 3 Complete When:**
- Crates renamed to `baml_ide` and `baml_project`
- No references to old names
- All tests pass

✅ **Phase 4 Complete When:**
- All IDE features in `baml_ide`
- `baml_language_server` only does LSP conversion
- All features work in VS Code

✅ **Phase 5 Complete When:**
- `Db` trait hierarchy implemented
- `CheckMode` works
- Salsa caching active for `check_file`
- Incremental compilation verified
