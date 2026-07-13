//! Colored, hyperlinked terminal output: a stream-bound [`Painter`] that owns
//! the color decision, plus the BAML syntax highlighting it renders (used by
//! `baml describe`).
//!
//! Reuses the compiler's own semantic-token classifier
//! (`baml_lsp2_actions::semantic_tokens`) instead of a separate TextMate/Sublime
//! grammar. `describe` has already compiled the project by the time it renders,
//! so classification is effectively free (it reads Salsa-cached queries) and can
//! never drift from the language the way a hand-maintained grammar would.
//!
//! [`Highlighter`] caches the per-file token set so a single description (body +
//! dependency/reference rows, which can touch several files) computes each file's
//! tokens at most once.

use std::{cell::RefCell, collections::HashMap, fmt::Write, path::Path, rc::Rc};

use baml_db::{
    FileId, SourceFile,
    baml_compiler_lexer::{TokenKind, lex_lossless},
};
use baml_lsp2_actions::{DefinitionKind, SemanticToken, SemanticTokenType, semantic_tokens};
use baml_project::ProjectDatabase;
use console::Style;
use text_size::{TextRange, TextSize};

// ── Output mode (color / hyperlinks) ───────────────────────────────────────────

/// When to emit color and hyperlinks. Mirrors the conventional `--color` flag.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Color on an interactive terminal; off when piped or inside an AI agent.
    #[default]
    Auto,
    /// Always emit color/hyperlinks.
    Always,
    /// Never emit color/hyperlinks.
    Never,
}

/// Environment variables that signal a non-interactive AI agent is capturing
/// output, where ANSI color and hyperlinks are noise rather than UI. Sourced
/// from each agent's docs and from maintained detection matrices
/// (`@vercel/detect-agent`, Bun's agent checks).
const AGENT_ENV_VARS: &[&str] = &[
    "CLAUDECODE",      // Claude Code (code.claude.com/docs/en/env-vars)
    "CODEX_SANDBOX",   // OpenAI Codex CLI (set in its sandbox)
    "PI_CODING_AGENT", // Pi (earendil-works/pi)
    "OPENCODE_CLIENT", // opencode
    "AI_AGENT",        // @vercel/detect-agent universal var (+ custom agents)
    "CURSOR_TRACE_ID", // Cursor agent terminal
    "REPL_ID",         // Replit
    "AGENT",           // generic (Codex `AGENT=codex`, Bun)
];

fn env_truthy(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether output is being captured by a known AI coding agent.
fn running_in_agent() -> bool {
    AGENT_ENV_VARS.iter().any(|var| env_truthy(var))
}

/// Resolve color/hyperlink output once at startup, applying the decision to both
/// stdout and stderr so the two streams agree (avoids leaking codes into a
/// redirected stream or dropping color on a redirected sibling).
pub fn init_color(choice: ColorChoice) {
    match choice {
        ColorChoice::Always => {
            console::set_colors_enabled(true);
            console::set_colors_enabled_stderr(true);
        }
        ColorChoice::Never => {
            console::set_colors_enabled(false);
            console::set_colors_enabled_stderr(false);
        }
        ColorChoice::Auto => {
            // An explicit `CLICOLOR_FORCE` (honored by console's defaults) always
            // wins; only suppress for agents when color was not force-requested.
            if !env_truthy("CLICOLOR_FORCE") && running_in_agent() {
                console::set_colors_enabled(false);
                console::set_colors_enabled_stderr(false);
            }
            // Otherwise leave console's per-stream TTY / NO_COLOR / CLICOLOR defaults.
        }
    }
}

/// Color for each semantic token type. Loosely mirrors a typical editor theme.
///
/// Styling is **forced** so emission is decided by the caller per output stream
/// (gating on `colors_enabled()` for stdout vs `colors_enabled_stderr()` for
/// stderr), not by console's ambient stdout flag.
fn style_for(token_type: SemanticTokenType) -> Style {
    use SemanticTokenType as T;
    let style = match token_type {
        T::Keyword | T::Modifier => Style::new().magenta(),
        T::Class | T::Struct | T::Interface | T::Enum | T::Type | T::TypeParameter => {
            Style::new().yellow()
        }
        T::Function | T::Method | T::Macro => Style::new().blue().bright(),
        T::EnumMember | T::Property => Style::new().cyan(),
        T::Parameter => Style::new().color256(173),
        T::Namespace => Style::new().cyan().bright(),
        T::String | T::Regexp => Style::new().green(),
        T::Number => Style::new().yellow().bright(),
        T::Comment => Style::new().color256(244),
        T::Decorator => Style::new().color256(179),
        T::Operator => Style::new().color256(245),
        T::Variable | T::Event => Style::new().white(),
    };
    style.force_styling(true)
}

/// Color for a definition kind, used to highlight listing rows (which have no
/// source slice to tokenize). Maps each kind onto the same palette as the body.
fn kind_style(kind: DefinitionKind) -> Style {
    use DefinitionKind as K;
    use SemanticTokenType as T;
    style_for(match kind {
        K::Class => T::Class,
        K::Enum => T::Enum,
        K::Interface => T::Interface,
        K::TypeAlias | K::AssociatedType => T::Type,
        K::Function | K::TemplateString => T::Function,
        K::Method => T::Method,
        K::Client | K::Test | K::RetryPolicy => T::Struct,
        K::Field => T::Property,
        K::Variant => T::EnumMember,
        K::Parameter => T::Parameter,
        K::Let | K::Binding => T::Variable,
    })
}

/// Highlight a (possibly dotted) name as it would appear in source: each
/// qualifier segment in the namespace color, the `.` separators as punctuation,
/// and the final segment in `leaf_kind`'s color.
fn highlight_fqn(name: &str, leaf_kind: DefinitionKind) -> String {
    highlight_fqn_opt(name, Some(leaf_kind))
}

/// Like [`highlight_fqn`] but the leaf kind is optional; `None` (a namespace or
/// package path) colors the whole path in the namespace color.
fn highlight_fqn_opt(name: &str, leaf_kind: Option<DefinitionKind>) -> String {
    let namespace = style_for(SemanticTokenType::Namespace);
    let dot = style_for(SemanticTokenType::Operator);
    let leaf = leaf_kind.map_or_else(|| namespace.clone(), kind_style);
    let mut out = String::new();
    let mut parts = name.split('.').peekable();
    while let Some(part) = parts.next() {
        let style = if parts.peek().is_none() {
            &leaf
        } else {
            &namespace
        };
        let _ = write!(out, "{}", style.apply_to(part));
        if parts.peek().is_some() {
            let _ = write!(out, "{}", dot.apply_to('.'));
        }
    }
    out
}

/// Like [`highlight_fqn`], padded with plain trailing spaces so the *visible*
/// text occupies `width` columns (ANSI escapes don't count toward width).
fn highlight_name_padded(name: &str, leaf_kind: DefinitionKind, width: usize) -> String {
    let pad = width.saturating_sub(name.chars().count());
    format!("{}{}", highlight_fqn(name, leaf_kind), " ".repeat(pad))
}

// ── Lexer-based highlighting for synthesized fragments ──────────────────────────

/// Primitive/builtin type names the lexer emits as plain words; colored as types.
const PRIMITIVE_TYPES: &[&str] = &[
    "string",
    "int",
    "bigint",
    "float",
    "bool",
    "null",
    "bytes",
    "uint8array",
    "image",
    "audio",
    "video",
    "pdf",
    "json",
    "true",
    "false",
];

/// The definition kind a declaration keyword introduces, so the name token that
/// follows can be colored accordingly (`function Foo` -> `Foo` as a function).
fn decl_keyword_kind(kind: TokenKind) -> Option<DefinitionKind> {
    use DefinitionKind as K;
    use TokenKind as T;
    Some(match kind {
        T::Class => K::Class,
        T::Enum => K::Enum,
        T::Interface => K::Interface,
        T::Function => K::Function,
        T::TemplateString => K::TemplateString,
        T::Client => K::Client,
        T::Test => K::Test,
        T::RetryPolicy => K::RetryPolicy,
        _ => return None,
    })
}

fn is_keyword(kind: TokenKind) -> bool {
    use TokenKind as T;
    matches!(
        kind,
        T::Class
            | T::Enum
            | T::Interface
            | T::Implements
            | T::Implement
            | T::Extends
            | T::Requires
            | T::Function
            | T::Client
            | T::Generator
            | T::Test
            | T::TestSet
            | T::RetryPolicy
            | T::TypeBuilder
            | T::Dynamic
            | T::Let
            | T::If
            | T::Else
            | T::For
            | T::While
            | T::Match
            | T::Return
            | T::Break
            | T::Continue
            | T::Throw
            | T::Throws
            | T::Catch
            | T::CatchAll
            | T::Defer
            | T::Spawn
            | T::Await
            | T::In
            | T::Is
            | T::Instanceof
    )
}

/// Highlight an arbitrary BAML fragment that has no source backing (synthesized
/// signatures, keyword-doc examples).
///
/// Lexer-based, so it is *syntactic only*: it colors keywords, primitive types,
/// names introduced by a declaration keyword, strings, line comments, numbers,
/// and operators. It cannot tell a user type from a variable the way the
/// type-aware [`Highlighter`] (used for real source ranges) can.
///
/// Always emits color; callers go through [`Painter::fragment`], which gates.
fn highlight_str(text: &str) -> String {
    let toks = lex_lossless(text, FileId::new(0));
    let mut out = String::new();
    let mut i = 0;
    // Set after a declaration keyword so the next name token is colored by kind.
    let mut pending_decl: Option<DefinitionKind> = None;
    while i < toks.len() {
        let t = &toks[i];

        // Trivia passes through and does not clear a pending declaration.
        if matches!(t.kind, TokenKind::Whitespace | TokenKind::Newline) {
            out.push_str(&t.text);
            i += 1;
            continue;
        }

        // Line comment: `//` through end of line.
        if t.kind == TokenKind::Slash && toks.get(i + 1).map(|n| n.kind) == Some(TokenKind::Slash) {
            let mut buf = String::new();
            while i < toks.len()
                && toks[i].kind != TokenKind::Newline
                && !toks[i].text.contains('\n')
            {
                buf.push_str(&toks[i].text);
                i += 1;
            }
            push_styled(&mut out, &buf, &style_for(SemanticTokenType::Comment));
            pending_decl = None;
            continue;
        }

        // Double-quoted string: consume through the closing unescaped quote.
        if t.kind == TokenKind::Quote {
            let mut buf = String::from(t.text.as_str());
            let mut j = i + 1;
            while j < toks.len() {
                buf.push_str(&toks[j].text);
                let closes =
                    toks[j].kind == TokenKind::Quote && toks[j - 1].kind != TokenKind::Backslash;
                j += 1;
                if closes {
                    break;
                }
            }
            push_styled(&mut out, &buf, &style_for(SemanticTokenType::String));
            i = j;
            pending_decl = None;
            continue;
        }

        // A single significant token.
        let style = if is_keyword(t.kind) {
            Some(style_for(SemanticTokenType::Keyword))
        } else if matches!(
            t.kind,
            TokenKind::IntegerLiteral | TokenKind::FloatLiteral | TokenKind::BigintLiteral
        ) {
            Some(style_for(SemanticTokenType::Number))
        } else if t.kind == TokenKind::Word {
            if let Some(k) = pending_decl {
                Some(kind_style(k))
            } else if PRIMITIVE_TYPES.contains(&t.text.as_str()) {
                Some(style_for(SemanticTokenType::Type))
            } else {
                None // bare identifier: leave at the default foreground
            }
        } else {
            Some(style_for(SemanticTokenType::Operator)) // punctuation / operators
        };
        match style {
            Some(s) => {
                let _ = write!(out, "{}", s.apply_to(&t.text));
            }
            None => out.push_str(&t.text),
        }
        pending_decl = decl_keyword_kind(t.kind);
        i += 1;
    }
    out
}

// ── Terminal hyperlinks (OSC 8) ────────────────────────────────────────────────

/// Wrap `text` in an OSC 8 terminal hyperlink to `uri`.
fn osc8(uri: &str, text: &str) -> String {
    format!("\x1b]8;;{uri}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Minimal `file://` URI builder. Percent-encodes the few characters most likely
/// to break a URI in a source path; not a full RFC 3986 encoder.
fn file_uri(path: &Path) -> String {
    let mut s = String::from("file://");
    for ch in path.to_string_lossy().chars() {
        match ch {
            ' ' => s.push_str("%20"),
            '%' => s.push_str("%25"),
            '#' => s.push_str("%23"),
            '?' => s.push_str("%3F"),
            _ => s.push(ch),
        }
    }
    s
}

// ── Painter ─────────────────────────────────────────────────────────────────────

/// Owns the color decision for one output stream and produces colored-or-plain
/// text accordingly.
///
/// Construct [`Painter::stdout`] / [`Painter::stderr`] so the decision matches
/// where the text is written; call sites then never branch on color and the
/// helpers never have to guess which stream they're feeding (which previously
/// risked leaking codes into a redirected sibling stream).
pub struct Painter {
    enabled: bool,
}

impl Painter {
    /// Painter for stdout-bound output.
    pub fn stdout() -> Self {
        Self {
            enabled: console::colors_enabled(),
        }
    }

    /// Painter for stderr-bound output.
    pub fn stderr() -> Self {
        Self {
            enabled: console::colors_enabled_stderr(),
        }
    }

    /// Whether this stream renders color. Also gates the verbatim-vs-cleaned body
    /// rendering fork in `describe` (a behavior choice, not just a color toggle).
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// A dotted name: namespace-colored qualifiers, leaf colored by `leaf_kind`.
    pub fn fqn(&self, name: &str, leaf_kind: DefinitionKind) -> String {
        self.fqn_opt(name, Some(leaf_kind))
    }

    /// Like [`Painter::fqn`] but the leaf kind is optional (`None` => namespace).
    pub fn fqn_opt(&self, name: &str, leaf_kind: Option<DefinitionKind>) -> String {
        if self.enabled {
            highlight_fqn_opt(name, leaf_kind)
        } else {
            name.to_string()
        }
    }

    /// A name whose *visible* width is padded to `width` columns.
    pub fn name_padded(&self, name: &str, leaf_kind: DefinitionKind, width: usize) -> String {
        if self.enabled {
            highlight_name_padded(name, leaf_kind, width)
        } else {
            format!("{name:<width$}")
        }
    }

    /// A definition-kind label padded to `width` columns, colored by kind.
    pub fn kind_label(&self, kind: DefinitionKind, width: usize) -> String {
        let label = kind.as_str();
        if self.enabled {
            kind_style(kind)
                .apply_to(format!("{label:<width$}"))
                .to_string()
        } else {
            format!("{label:<width$}")
        }
    }

    /// An arbitrary BAML fragment with no source backing (signatures, examples).
    pub fn fragment(&self, text: &str) -> String {
        if self.enabled {
            highlight_str(text)
        } else {
            text.to_string()
        }
    }

    /// `text` styled as a keyword (for keyword-doc headers).
    pub fn keyword(&self, text: &str) -> String {
        if self.enabled {
            style_for(SemanticTokenType::Keyword)
                .apply_to(text)
                .to_string()
        } else {
            text.to_string()
        }
    }

    /// A `display:line_label` location, rendered as a clickable `file://`
    /// hyperlink when this stream is colored and `abs_path` is a real (absolute)
    /// file. Plain `display:line_label` otherwise (pipes / JSON / tests / builtin
    /// `<...>` paths).
    pub fn location(&self, abs_path: &Path, display: &str, line_label: &str) -> String {
        let text = format!("{display}:{line_label}");
        if self.enabled && abs_path.is_absolute() {
            osc8(&file_uri(abs_path), &text)
        } else {
            text
        }
    }
}

// ── Source highlighting ─────────────────────────────────────────────────────────

/// Highlights source ranges using the compiler's semantic tokens, caching the
/// token set per file so repeated lookups within one render are cheap.
pub struct Highlighter<'db> {
    db: &'db ProjectDatabase,
    cache: RefCell<HashMap<SourceFile, Rc<[SemanticToken]>>>,
}

impl<'db> Highlighter<'db> {
    pub fn new(db: &'db ProjectDatabase) -> Self {
        Self {
            db,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Semantic tokens for `file`, computed once and cached (sorted by start).
    fn tokens(&self, file: SourceFile) -> Rc<[SemanticToken]> {
        if let Some(cached) = self.cache.borrow().get(&file) {
            return Rc::clone(cached);
        }
        let mut toks = semantic_tokens(self.db, file).clone();
        toks.sort_by_key(|t| t.range.start());
        let rc: Rc<[SemanticToken]> = toks.into();
        self.cache.borrow_mut().insert(file, Rc::clone(&rc));
        rc
    }

    /// Highlight the verbatim source slice `file.text()[range]`.
    ///
    /// Styling is re-opened on every line so no SGR run crosses a `'\n'`, which
    /// keeps the result safe to `split('\n')` for line-budgeting by the caller.
    pub fn range(&self, file: SourceFile, range: TextRange) -> String {
        let text = file.text(self.db);
        let start: usize = range.start().into();
        let end: usize = usize::min(range.end().into(), text.len());
        if start >= end {
            return String::new();
        }
        let slice = &text[start..end];

        let mut out = String::new();
        let mut cursor = 0usize;
        for tok in self.tokens(file).iter() {
            let ts: usize = tok.range.start().into();
            let te: usize = tok.range.end().into();
            if te <= start {
                continue;
            }
            if ts >= end {
                break; // tokens are sorted by start; nothing further overlaps
            }
            let s = ts.max(start) - start;
            let e = te.min(end) - start;
            if s < cursor {
                continue; // overlapping token; the first writer wins
            }
            out.push_str(&slice[cursor..s]); // gap (whitespace/punctuation) stays plain
            push_styled(&mut out, &slice[s..e], &style_for(tok.token_type));
            cursor = e;
        }
        out.push_str(&slice[cursor..]);
        out
    }

    /// Highlight the full source line(s) enclosing `range`, trimmed of
    /// surrounding whitespace. Used for reference previews, where only the
    /// reference span is known but the whole line is shown.
    pub fn enclosing_line(&self, file: SourceFile, range: TextRange) -> String {
        let text = file.text(self.db);
        // Clamp both bounds: a stale/out-of-range `range` must not panic the
        // `text[..start]` / `text[end..]` slices below.
        let start: usize = usize::min(range.start().into(), text.len());
        let end: usize = usize::min(range.end().into(), text.len());
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
        let line_range = TextRange::new(
            TextSize::from(line_start as u32),
            TextSize::from(line_end as u32),
        );
        self.range(file, line_range).trim().to_string()
    }
}

/// Apply `style` to `seg`, re-opening it on each line so no escape run spans a
/// newline.
fn push_styled(out: &mut String, seg: &str, style: &Style) {
    for (i, line) in seg.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !line.is_empty() {
            let _ = write!(out, "{}", style.apply_to(line));
        }
    }
}
