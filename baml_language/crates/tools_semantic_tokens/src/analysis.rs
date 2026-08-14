//! Semantic-token computation and reconciliation against committed test
//! expectations.
//!
//! The viewer needs three things per fixture:
//! 1. the BAML source (the part before the `//----` separator),
//! 2. the *current* tokens — what `semantic_tokens` produces today, and
//! 3. the *expected* tokens — the committed `//- semantic_tokens` block.
//!
//! (1) and (3) come from the test harness parser; (2) is recomputed live with
//! the same `ProjectDatabase` + `semantic_tokens` call the LSP and tests use.
//! "Accept" goes through the real `runner::run_test` + `updater::update_test_file`
//! path, so it is byte-for-byte what `UPDATE_EXPECT=1 cargo test` would write.

use std::{collections::HashMap, fs, path::Path};

use baml_lsp2_actions::semantic_tokens;
use baml_lsp2_actions_tests::{parser, runner, updater};
use baml_project::ProjectDatabase;
use serde::Serialize;

/// One classified token.
///
/// Positioned redundantly: `line`/`col`/`len` (1-based, byte columns) match the
/// snapshot format and key the diff; `start`/`end` are absolute byte offsets the
/// frontend uses to slice the source when rendering.
#[derive(Serialize, Clone)]
pub(crate) struct Token {
    pub(crate) line: usize,
    pub(crate) col: usize,
    pub(crate) len: usize,
    #[serde(rename = "type")]
    pub(crate) ty: String,
    pub(crate) mods: Vec<String>,
    pub(crate) text: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// A fixture's source plus its current and committed-expected token sets.
pub(crate) struct Fixture {
    pub(crate) source: String,
    pub(crate) current: Vec<Token>,
    pub(crate) expected: Vec<Token>,
}

/// Replicates `runner::offset_to_line_col`: 1-based line, 1-based byte column.
fn offset_to_line_col(content: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(content.len());
    let safe = (0..=clamped)
        .rev()
        .find(|&i| content.is_char_boundary(i))
        .unwrap_or(0);
    let before = &content[..safe];
    let line = before.matches('\n').count() + 1;
    let last_newline = before.rfind('\n').map_or(0, |p| p + 1);
    (line, safe - last_newline + 1)
}

/// Byte offset of the start of each line (index 0 == line 1).
fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0usize];
    for (i, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// UTF-16 code-unit offset of `byte_offset` within `source`.
///
/// The frontend slices the source as a JS string (UTF-16), so render positions
/// must be UTF-16 offsets — not byte offsets — or non-ASCII source (e.g. an
/// em-dash in a comment) shifts every later token's coloring.
fn utf16_offset(source: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(source.len());
    let safe = (0..=clamped)
        .rev()
        .find(|&i| source.is_char_boundary(i))
        .unwrap_or(0);
    source[..safe].encode_utf16().count()
}

/// Compute live semantic tokens for an arbitrary BAML source string.
pub(crate) fn compute_tokens(source: &str) -> Vec<Token> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    let file = db.add_or_update_file(Path::new("scratch.baml"), source);

    let mut tokens = semantic_tokens(&db, file).clone();
    tokens.sort_by_key(|t| t.range.start());

    tokens
        .iter()
        .map(|t| {
            let start: usize = t.range.start().into();
            let end: usize = t.range.end().into();
            let (line, col) = offset_to_line_col(source, start);
            Token {
                line,
                col,
                len: end - start,
                ty: t.token_type.as_str().to_string(),
                mods: t.modifiers.names().map(str::to_string).collect(),
                text: source.get(start..end).unwrap_or_default().to_string(),
                start: utf16_offset(source, start),
                end: utf16_offset(source, end),
            }
        })
        .collect()
}

/// Parse a committed `//- semantic_tokens` block back into tokens.
///
/// Each line looks like: `// name.baml:LINE:COL (type) len=N "text"`. We rebuild
/// absolute byte offsets from LINE/COL against `source` so the expected pane can
/// render the same way the current pane does.
fn parse_expected(block: &str, source: &str) -> Vec<Token> {
    let line_starts = line_start_offsets(source);
    let mut out = Vec::new();

    for raw in block.lines() {
        let line = raw.trim_start();
        let line = line.strip_prefix("//").unwrap_or(line).trim_start();

        let Some(paren) = line.find(" (") else {
            continue;
        };
        let loc = &line[..paren];
        let rest = &line[paren + 2..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        let ty = rest[..close].to_string();
        let after = &rest[close + 1..];

        // Optional ` [mod,mod]` modifier list precedes ` len=`.
        let mods = after
            .find('[')
            .zip(after.find(']'))
            .filter(|&(open, end)| open < end && open < after.find("len=").unwrap_or(usize::MAX))
            .map(|(open, end)| {
                after[open + 1..end]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        // `loc` is `name:LINE:COL`; take the last two colon-separated numbers.
        let mut nums = loc.rsplit(':');
        let col: usize = nums.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let lineno: usize = nums.next().and_then(|s| s.parse().ok()).unwrap_or(0);

        let len = after
            .find("len=")
            .map(|i| {
                after[i + 4..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let text = after
            .find('"')
            .zip(after.rfind('"'))
            .filter(|(i, j)| j > i)
            .map(|(i, j)| after[i + 1..j].to_string())
            .unwrap_or_default();

        if lineno == 0 || col == 0 {
            continue;
        }
        let start_byte = line_starts.get(lineno - 1).copied().unwrap_or(0) + (col - 1);
        out.push(Token {
            line: lineno,
            col,
            len,
            ty,
            mods,
            text,
            start: utf16_offset(source, start_byte),
            end: utf16_offset(source, start_byte + len),
        });
    }

    out
}

/// Read a fixture and compute its source + current + expected token sets.
pub(crate) fn load_fixture(path: &Path) -> std::io::Result<Fixture> {
    let content = fs::read_to_string(path)?.replace("\r\n", "\n");
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "fixture.baml".to_string());

    let parsed = parser::parse_test_file(&content, &filename);
    // Semantic-token fixtures are single-file; take the first virtual file.
    let source = parsed
        .files
        .values()
        .next()
        .map(|f| f.content.clone())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("no virtual file parsed from fixture {}", path.display()),
            )
        })?;

    let current = compute_tokens(&source);
    let expected = parsed
        .expected_semantic_tokens
        .as_deref()
        .map(|block| parse_expected(block, &source))
        .unwrap_or_default();

    Ok(Fixture {
        source,
        current,
        expected,
    })
}

/// Number of tokens that differ between current and expected (changed type,
/// modifiers, or lexeme; added; or removed), keyed by (line, col, len) to match
/// the snapshot. The lexeme is compared so a same-length rename that keeps its
/// classification still counts as a diff (otherwise accept would silently
/// rewrite the committed line while the viewer showed zero diffs).
pub(crate) fn diff_count(current: &[Token], expected: &[Token]) -> usize {
    let key = |t: &Token| (t.line, t.col, t.len);
    // The committed block stores each lexeme in Rust debug form (escaped, no
    // outer quotes), so escape `current` the same way before comparing.
    let cur_sig = |t: &Token| {
        let dbg = format!("{:?}", t.text);
        (
            t.ty.clone(),
            t.mods.clone(),
            dbg[1..dbg.len() - 1].to_string(),
        )
    };
    // `expected` lexemes were parsed straight out of that debug form already.
    let exp_sig = |t: &Token| (t.ty.clone(), t.mods.clone(), t.text.clone());
    let cur: HashMap<_, _> = current.iter().map(|t| (key(t), cur_sig(t))).collect();
    let exp: HashMap<_, _> = expected.iter().map(|t| (key(t), exp_sig(t))).collect();

    let mut diff = 0;
    for (k, v) in &cur {
        if exp.get(k) != Some(v) {
            diff += 1;
        }
    }
    for k in exp.keys() {
        if !cur.contains_key(k) {
            diff += 1;
        }
    }
    diff
}

/// Rewrite a fixture's expectation block to match current output, exactly as
/// `UPDATE_EXPECT=1 cargo test` would (regenerates every `//-` section).
pub(crate) fn accept_fixture(path: &Path) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?.replace("\r\n", "\n");
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "fixture.baml".to_string());

    let parsed = parser::parse_test_file(&content, &filename);
    let result = runner::run_test(&parsed);
    updater::update_test_file(path, &content, &result)?;
    Ok(())
}

/// List `*.baml` fixture file names in `dir`, sorted.
pub(crate) fn list_fixture_names(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("baml"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}
