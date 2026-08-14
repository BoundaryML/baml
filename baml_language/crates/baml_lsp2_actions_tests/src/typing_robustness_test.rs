//! Robustness: the semantic-tokens classifier (and the resolution it drives)
//! must never panic on the *incomplete* / mid-edit source that exists at every
//! keystroke — not just on the static, valid fixtures.
//!
//! Two shapes of "typing":
//!  - prefixes of a source (typing forward, incomplete tail), and
//!  - inserting content one char at a time *inside an already-closed nested
//!    scope* (function body, match arm, implements block, lambda, catch/spawn
//!    body, call args) — a different family of partial states.
//!
//! Each intermediate string is run through `semantic_tokens` + a couple of
//! `semantic_tokens_in_range` queries under `catch_unwind`; a panic fails the
//! test and prints the offending input. A fresh db per probe avoids salsa
//! revision bloat over the thousands of edits.

#[cfg(test)]
mod tests {
    use std::{
        panic::AssertUnwindSafe,
        path::{Path, PathBuf},
    };

    use baml_lsp2_actions::tokens::{semantic_tokens, semantic_tokens_in_range};
    use baml_project::ProjectDatabase;

    /// Short sources exercising the trickier constructs, each typed forward one
    /// char at a time (so e.g. a half-typed raw string or `.as<` is probed).
    const CURATED: &[&str] = &[
        "function f() -> string { baml.json.stringify({\"x\": 1n, \"b\": true, \"n\": null}) }",
        "enum E { A, B }\nfunction g(e: E) -> int { match (e) { E.A => 0, E.B => 1 } }",
        "interface I { name: string }\nclass C { dn: string\n  implements I { name as dn }\n}",
        "function h(x: int) -> int throws string { throw \"x\" }\n\
         function j() -> int { h(1) catch (e) { _ => e } }",
        "class Box<T> { v: T }\n\
         function k(b: Box<int>) -> int {\n\
           let f = spawn with baml.spawn.options() { baml.time.Duration.from_milliseconds(5n); 1 };\n\
           b.v + (await f)\n\
         }",
        "function s(q: string, n: int = 1) -> int { n }\n\
         function u() -> int { s(q = \"a\", n = 2) }",
        "type Alias = int[]\ninterface It { type Item\n  function next(self) -> Item }",
        "function o(p: Person?) -> string? { p?.name }\nclass Person { name: string }",
        // Raw strings — the construct whose incomplete state panicked the parser.
        "function r() -> string { #\"raw text\"# }",
        "function r2() -> string { ##\"two #hashes\"## }",
        // Backtick template string with interpolation.
        "function b() -> string { `hello ${1 + 2}` }",
        // String escapes + a client value declaration.
        "client Cl = openai.OpenAiClient.new(model = \"gpt-4o\");\nfunction e() -> string { \"\\n\\t\\u{1f600}\" }",
    ];

    /// `(prefix, content, suffix)` — `content` is typed one char at a time
    /// *between* `prefix` and `suffix`, which stay structurally complete.
    const NESTED: &[(&str, &str, &str)] = &[
        (
            "enum E { A }\nfunction f() -> int {\n  ",
            "baml.json.stringify({\"a\": 1n}).length() + E.A",
            "\n}",
        ),
        (
            "enum E { A, B }\nfunction g(e: E) -> int {\n  match (e) {\n    E.A => ",
            "baml.time.Duration.from_milliseconds(5n)",
            ",\n    E.B => 1\n  }\n}",
        ),
        (
            "interface I { name: string }\nclass C {\n  dn: string\n  implements I {\n    ",
            "name as dn",
            "\n  }\n}",
        ),
        (
            "enum E { A }\nfunction h() -> int {\n  let m = (x: int) -> int {\n    ",
            "x + E.A",
            "\n  };\n  m(1)\n}",
        ),
        (
            "function j(x: int) -> int throws string {\n  throw \"e\"\n}\n\
             function k() -> int {\n  j(1) catch (e) {\n    _ => ",
            "e",
            "\n  }\n}",
        ),
        (
            "function s() -> int {\n  let f = spawn with baml.spawn.options() {\n    ",
            "baml.sys.sleep(baml.time.Duration.from_milliseconds(5n)); 1",
            "\n  };\n  await f\n}",
        ),
        (
            "function q(a: string, b: int = 1) -> int { b }\nfunction r() -> int {\n  q(",
            "a = \"x\", b = 2",
            ")\n}",
        ),
    ];

    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("."));
        db
    }

    /// Run the classifier (full + range) on one input, catching any panic. The
    /// db is reused across probes so the `baml` stdlib is built once, not per
    /// keystroke (rebuilding it each time is ~100x slower).
    fn survives(db: &mut ProjectDatabase, input: &str) -> bool {
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            let file = db.add_or_update_file(Path::new("typing.baml"), input);
            // Full document (the index path an editor calls once).
            let _ = semantic_tokens(&*db, file);
            // Viewport-sized windows (the on-demand range path editors call per
            // scroll) at the start, middle, and end of the document.
            let len = u32::try_from(input.len()).unwrap_or(u32::MAX);
            let window = 200u32.min(len);
            for start in [
                0,
                len.saturating_sub(window) / 2,
                len.saturating_sub(window),
            ] {
                let _ = semantic_tokens_in_range(&*db, file, start, (start + window).min(len));
            }
        }))
        .is_ok()
    }

    fn type_prefixes(db: &mut ProjectDatabase, src: &str) {
        for end in 1..=src.len() {
            if src.is_char_boundary(end) {
                let input = &src[..end];
                assert!(
                    survives(db, input),
                    "panicked typing prefix ({end}B):\n{input}\n"
                );
            }
        }
    }

    fn type_in_scope(db: &mut ProjectDatabase, prefix: &str, content: &str, suffix: &str) {
        for k in 0..=content.len() {
            if content.is_char_boundary(k) {
                let input = format!("{prefix}{}{suffix}", &content[..k]);
                assert!(
                    survives(db, &input),
                    "panicked typing in a nested scope ({k}B):\n{input}\n"
                );
            }
        }
    }

    fn fixture_sources() -> Vec<String> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_files/semantic_tokens");
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                // The source is everything before the `//----` snapshot block.
                let source = content
                    .split("//----")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                if !source.trim().is_empty() {
                    sources.push(source);
                }
            }
        }
        sources
    }

    #[test]
    fn no_crash_typing_curated() {
        let mut db = make_db();
        for src in CURATED {
            type_prefixes(&mut db, src);
        }
    }

    #[test]
    fn no_crash_typing_in_nested_scope() {
        let mut db = make_db();
        for (prefix, content, suffix) in NESTED {
            type_in_scope(&mut db, prefix, content, suffix);
        }
    }

    /// Thorough broad sweep: type *every committed fixture* one char at a time,
    /// full length. This rebuilds the resolution index per keystroke for ~80
    /// real-world files, so it runs for minutes and is `#[ignore]`d — run it
    /// on-demand (`cargo test -p baml_lsp2_actions_tests -- --ignored typing`)
    /// when touching the parser/classifier. The always-run `curated` + nested
    /// tests carry the fast, fine-grained construct coverage.
    #[test]
    #[ignore = "slow broad fuzz (~minutes); run with --ignored"]
    fn no_crash_typing_fixtures() {
        let mut db = make_db();
        for src in fixture_sources() {
            for end in 1..=src.len() {
                if src.is_char_boundary(end) {
                    let input = &src[..end];
                    assert!(
                        survives(&mut db, input),
                        "panicked typing fixture prefix ({end}B):\n{input}\n"
                    );
                }
            }
        }
    }
}
