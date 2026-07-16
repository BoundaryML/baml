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

use baml_db::SourceFile;
use baml_lsp2_actions::{
    DefinitionKind, ModifierSet, SemanticToken, SemanticTokenType, semantic_tokens,
};
use baml_project::ProjectDatabase;
// ── Output mode (color / hyperlinks) ───────────────────────────────────────────
/// Process-wide color policy (`--color` flag + agent detection). Defined in
/// `baml_term` so the `baml` wrapper resolves color the same way; re-exported
/// here so call sites keep the `paint::` path.
pub use baml_term::{ColorChoice, init_color};
use console::Style;
use text_size::{TextRange, TextSize};

/// Style for a semantic token, honoring its modifiers.
///
/// Colors are restricted to the terminal's *named* ANSI palette plus the
/// dim/bold attributes — never fixed 256-color values and never a concrete
/// white/black — so the theme decides what every color looks like and output
/// stays legible on light and dark backgrounds alike. Modifiers overlay
/// attributes the way editor themes do: declarations are bold, stdlib
/// entities italic, deprecated ones struck through.
///
/// Styling is **forced** so emission is decided by the caller per output stream
/// (gating on `colors_enabled()` for stdout vs `colors_enabled_stderr()` for
/// stderr), not by console's ambient stdout flag.
fn style_for(token_type: SemanticTokenType, modifiers: ModifierSet) -> Style {
    use SemanticTokenType as T;
    // (base color, dimmed) — dim is a *base* trait of quiet token types, kept
    // separate so a declaration can trade it for bold instead of stacking the
    // two contradictory weights.
    let (style, base_dim) = match token_type {
        T::Keyword | T::Modifier => (Style::new().magenta(), false),
        T::Class | T::Struct | T::Interface | T::Enum | T::Type | T::TypeParameter => {
            (Style::new().yellow(), false)
        }
        T::Function | T::Method | T::Macro => (Style::new().blue().bright(), false),
        T::EnumMember | T::Property => (Style::new().cyan(), false),
        T::Parameter => (Style::new().yellow(), true),
        T::Namespace => (Style::new().cyan().bright(), false),
        T::String | T::Regexp => (Style::new().green(), false),
        T::EscapeSequence => (Style::new().magenta().bright(), false),
        T::Number | T::Boolean => (Style::new().yellow().bright(), false),
        T::Comment => (Style::new(), true),
        T::Decorator => (Style::new().magenta(), true),
        T::Operator => (Style::new(), true),
        // Ordinary names keep the terminal's default foreground: forcing any
        // concrete color here would assume a background.
        T::Variable | T::Event => (Style::new(), false),
    };
    let declaration = modifiers.contains(ModifierSet::DECLARATION);
    let mut style = if base_dim && !declaration {
        style.dim()
    } else {
        style
    };
    if declaration {
        style = style.bold();
    }
    if modifiers.contains(ModifierSet::DEFAULT_LIBRARY) {
        style = style.italic();
    }
    if modifiers.contains(ModifierSet::DEPRECATED) {
        style = style.strikethrough();
    }
    style.force_styling(true)
}

/// [`style_for`] with no modifiers, for synthesized text (labels, paths) that
/// has no real token behind it.
fn style_for_plain(token_type: SemanticTokenType) -> Style {
    style_for(token_type, ModifierSet::empty())
}

/// Color for a definition kind, used to highlight listing rows (which have no
/// source slice to tokenize). Maps each kind onto the same palette as the body.
fn kind_style(kind: DefinitionKind) -> Style {
    use DefinitionKind as K;
    use SemanticTokenType as T;
    style_for_plain(match kind {
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
    let namespace = style_for_plain(SemanticTokenType::Namespace);
    let dot = style_for_plain(SemanticTokenType::Operator);
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

// ── Classifier-based highlighting for synthesized fragments ─────────────────────

/// Highlight an arbitrary BAML fragment that has no source backing (synthesized
/// signatures, keyword-doc examples).
///
/// Runs the exact compiler classifier the LSP uses: the fragment is parsed as a
/// scratch file in a private in-memory project and rendered from the resulting
/// tokens. Everything syntactic (keywords, strings, numbers, comments,
/// declarations, primitive types) classifies as it would in an editor; a name
/// only a real project could resolve (a user type mentioned in prose-level
/// example code) stays at the default foreground — the same neutrality the
/// editor shows for an unresolved name.
///
/// Always emits color; callers go through [`Painter::fragment`], which gates.
fn highlight_str(text: &str) -> String {
    thread_local! {
        static SCRATCH_DB: RefCell<ProjectDatabase> = RefCell::new({
            let mut db = ProjectDatabase::new();
            db.set_project_root(Path::new("/baml-fragment-scratch"));
            db
        });
    }
    SCRATCH_DB.with(|db| {
        let db = &mut *db.borrow_mut();
        let file = db.add_or_update_file(Path::new("/baml-fragment-scratch/fragment.baml"), text);
        let mut toks = semantic_tokens(db, file).clone();
        toks.sort_by_key(|t| t.range.start());
        styled_from_tokens(text, 0, &toks)
    })
}

/// Render `slice` (the source text starting at byte offset `slice_start`) with
/// every overlapping token colored; bytes no token claims stay plain. Assumes
/// `tokens` is sorted by start; on overlap the first writer wins.
fn styled_from_tokens(slice: &str, slice_start: usize, tokens: &[SemanticToken]) -> String {
    let end = slice_start + slice.len();
    let mut out = String::new();
    let mut cursor = 0usize;
    for tok in tokens {
        let ts: usize = tok.range.start().into();
        let te: usize = tok.range.end().into();
        if te <= slice_start {
            continue;
        }
        if ts >= end {
            break;
        }
        let s = ts.max(slice_start) - slice_start;
        let e = te.min(end) - slice_start;
        if s < cursor {
            continue;
        }
        out.push_str(&slice[cursor..s]);
        push_styled(
            &mut out,
            &slice[s..e],
            &style_for(tok.token_type, tok.modifiers),
        );
        cursor = e;
    }
    out.push_str(&slice[cursor..]);
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
            style_for_plain(SemanticTokenType::Keyword)
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
        styled_from_tokens(slice, start, &self.tokens(file))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_lsp2_actions::{ModifierSet, SemanticTokenType};
    use baml_project::ProjectDatabase;

    use super::{Highlighter, highlight_str, style_for};

    /// One rendering effect of an SGR escape, decoded from its parameter list.
    /// Modeling decoded effects (not raw byte fragments) keeps the assertions
    /// valid however `console` chooses to batch attributes into sequences
    /// (`\x1b[1m\x1b[33m` and `\x1b[1;33m` decode identically).
    #[derive(Debug, PartialEq)]
    enum Sgr {
        Attr(u16),
        /// `38;5;N` indexed foreground.
        Palette(u16),
        /// `38;2;r;g;b` truecolor foreground.
        Rgb,
    }

    /// Decode every SGR effect applied anywhere in `out`.
    fn sgr_effects(out: &str) -> Vec<Sgr> {
        let mut effects = Vec::new();
        for seq in out.split("\u{1b}[").skip(1) {
            let Some(params) = seq.split('m').next() else {
                continue;
            };
            let mut nums = params.split(';').map(|n| n.parse::<u16>().unwrap_or(0));
            while let Some(n) = nums.next() {
                match n {
                    38 | 48 => match nums.next() {
                        Some(5) => {
                            effects.push(Sgr::Palette(nums.next().unwrap_or(0)));
                        }
                        Some(2) => {
                            let _ = (nums.next(), nums.next(), nums.next());
                            effects.push(Sgr::Rgb);
                        }
                        _ => {}
                    },
                    n => effects.push(Sgr::Attr(n)),
                }
            }
        }
        effects
    }

    /// The effects active exactly where `needle` is rendered: decode the text
    /// before it and drop everything cancelled by a reset (`0`).
    fn effects_at(out: &str, needle: &str) -> Vec<Sgr> {
        let prefix = &out[..out.find(needle).unwrap_or_else(|| {
            panic!("{needle:?} not found in {out:?}");
        })];
        let mut active = Vec::new();
        for effect in sgr_effects(prefix) {
            if effect == Sgr::Attr(0) {
                active.clear();
            } else {
                active.push(effect);
            }
        }
        active
    }

    const BOLD: Sgr = Sgr::Attr(1);
    const ITALIC: Sgr = Sgr::Attr(3);
    const YELLOW: Sgr = Sgr::Attr(33);

    /// Ordinary names must keep the terminal's default foreground: forcing a
    /// concrete color (the old white) breaks on light backgrounds.
    #[test]
    fn variable_keeps_default_foreground() {
        let styled = style_for(SemanticTokenType::Variable, ModifierSet::empty())
            .apply_to("x")
            .to_string();
        assert_eq!(styled, "x", "no escape codes expected, got {styled:?}");
    }

    #[test]
    fn modifiers_overlay_attributes() {
        let decl = style_for(SemanticTokenType::Class, ModifierSet::DECLARATION)
            .apply_to("Foo")
            .to_string();
        assert!(
            effects_at(&decl, "Foo").contains(&BOLD),
            "declaration not bold: {decl:?}"
        );
        let lib = style_for(SemanticTokenType::Type, ModifierSet::DEFAULT_LIBRARY)
            .apply_to("string")
            .to_string();
        assert!(
            effects_at(&lib, "string").contains(&ITALIC),
            "defaultLibrary not italic: {lib:?}"
        );
    }

    /// The whole palette must be background-agnostic: named ANSI colors and
    /// attributes only — no fixed 256-color values, no truecolor, no forced
    /// white/black.
    #[test]
    fn rendered_body_uses_no_fixed_colors() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/test"));
        let src = r#"/// Doc.
class Point { x int }
function make(v: int) -> Point {
  let p = Point { x: v };
  return p
}
"#;
        let file = db.add_or_update_file(Path::new("/test/main.baml"), src);
        let hl = Highlighter::new(&db);
        let out = hl.range(
            file,
            text_size::TextRange::new(0.into(), (src.len() as u32).into()),
        );
        for effect in sgr_effects(&out) {
            match effect {
                // Palette slots 0-15 are theme-controlled (console renders
                // bright colors as `38;5;8..=15`); 16 and up is the fixed
                // cube, which ignores the terminal scheme.
                Sgr::Palette(idx) => {
                    assert!(idx < 16, "fixed-cube 256-color {idx} in: {out:?}");
                }
                Sgr::Rgb => panic!("truecolor in: {out:?}"),
                // 30/37 are concrete black/white foregrounds.
                Sgr::Attr(n) => {
                    assert!(n != 30 && n != 37, "forced black/white in: {out:?}");
                }
            }
        }
    }

    /// Fragments run through the real compiler classifier, not a side lexer:
    /// a declaration gets the class color *and* the bold declaration modifier,
    /// and stdlib types the italic `defaultLibrary` modifier — signals the old
    /// lexer path could never produce.
    #[test]
    fn fragment_classifies_via_compiler() {
        let out = highlight_str("class Foo {\n  x int\n}");
        let at_decl = effects_at(&out, "Foo");
        assert!(
            at_decl.contains(&YELLOW) && at_decl.contains(&BOLD),
            "class decl not bold+yellow ({at_decl:?}) in: {out:?}"
        );
        let at_type = effects_at(&out, "int");
        assert!(
            at_type.contains(&ITALIC),
            "stdlib type not italic ({at_type:?}) in: {out:?}"
        );
    }

    /// A fragment that mentions names with no project behind them must not
    /// crash. Syntactic positions still classify (a constructor name reads as
    /// a type even unresolved), but a member on an unresolvable receiver has
    /// no signal and stays at the default foreground — the same neutrality an
    /// editor shows.
    #[test]
    fn fragment_with_unresolved_names_stays_neutral() {
        let out = highlight_str("let x = SomeUnknownClass { field: rcv.member() };");
        assert!(out.contains("SomeUnknownClass"), "text preserved: {out:?}");
        assert!(
            effects_at(&out, "SomeUnknownClass").contains(&YELLOW),
            "constructor position not typed: {out:?}"
        );
        assert!(
            effects_at(&out, "member").is_empty(),
            "member on unresolved receiver styled: {out:?}"
        );
    }
}
