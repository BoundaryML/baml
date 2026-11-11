use baml_syntax::token::{TokenIdx, TokenKind};
use text_size::TextRange;

use super::lex;

/// `(kind, text)` per token.
fn toks(text: &str) -> Vec<(TokenKind, &str)> {
    let tokens = lex(text);
    tokens
        .iter()
        .map(|i| (tokens.kind(i), tokens.text(text, i)))
        .collect()
}

/// Like [`toks`] without trivia.
fn sig(text: &str) -> Vec<(TokenKind, &str)> {
    toks(text)
        .into_iter()
        .filter(|(k, _)| !k.is_trivia())
        .collect()
}

fn round_trips(text: &str) {
    let tokens = lex(text);
    let joined: String = tokens.iter().map(|i| tokens.text(text, i)).collect();
    assert_eq!(joined, text);
}

use TokenKind as K;

#[test]
fn every_fixed_spelling_lexes_to_its_kind() {
    for &kind in TokenKind::ALL {
        if kind.is_keyword() || kind.is_contextual_keyword() || kind.is_punct() {
            let spelling = kind.describe();
            if kind == K::DollarLBrace {
                continue; // only a token inside template text
            }
            assert_eq!(toks(spelling), vec![(kind, spelling)], "{spelling:?}");
        }
    }
}

#[test]
fn composites_that_are_not_tokens_split() {
    assert_eq!(
        sig("a ?? b"),
        vec![
            (K::Word, "a"),
            (K::Question, "?"),
            (K::Question, "?"),
            (K::Word, "b")
        ]
    );
    assert_eq!(
        sig("x >> 1"),
        vec![
            (K::Word, "x"),
            (K::Greater, ">"),
            (K::Greater, ">"),
            (K::Integer, "1")
        ]
    );
    assert_eq!(
        sig("x >>= 1"),
        vec![
            (K::Word, "x"),
            (K::Greater, ">"),
            (K::Greater, ">"),
            (K::Equals, "="),
            (K::Integer, "1")
        ]
    );
    assert_eq!(sig("...."), vec![(K::DotDotDot, "..."), (K::Dot, ".")]);
    assert_eq!(
        sig("x->y"),
        vec![(K::Word, "x-"), (K::Greater, ">"), (K::Word, "y")]
    );
}

#[test]
fn words_hyphens_and_dollars() {
    assert_eq!(
        sig("gpt-4o a- a--b x-1"),
        vec![
            (K::Word, "gpt-4o"),
            (K::Word, "a-"),
            (K::Word, "a--b"),
            (K::Word, "x-1")
        ]
    );
    assert_eq!(sig("Foo$bar"), vec![(K::Word, "Foo$bar")]);
    assert_eq!(sig("foo$"), vec![(K::Word, "foo"), (K::Dollar, "$")]);
    assert_eq!(
        sig("$foo-bar"),
        vec![(K::Word, "$foo"), (K::Minus, "-"), (K::Word, "bar")]
    );
    assert_eq!(sig("$1"), vec![(K::Dollar, "$"), (K::Integer, "1")]);
    assert_eq!(sig("_ _1"), vec![(K::Underscore, "_"), (K::Word, "_1")]);
    assert_eq!(
        sig("get_client catch_all_panics catch_all classy"),
        vec![
            (K::Word, "get_client"),
            (K::CatchAllPanics, "catch_all_panics"),
            (K::CatchAll, "catch_all"),
            (K::Word, "classy"),
        ]
    );
    assert_eq!(
        sig("type-x type"),
        vec![(K::Word, "type-x"), (K::Type, "type")]
    );
}

#[test]
fn non_ascii_outside_strings_is_one_error_per_char() {
    let t = toks("处理");
    assert_eq!(t, vec![(K::Error, "处"), (K::Error, "理")]);
    assert_eq!(toks("'"), vec![(K::Error, "'")]);
    round_trips("héllo");
}

#[test]
fn numbers() {
    assert_eq!(
        sig("42n 42na 42n.5"),
        vec![
            (K::Bigint, "42n"),
            (K::Bigint, "42n"),
            (K::Word, "a"),
            (K::Bigint, "42n"),
            (K::Dot, "."),
            (K::Integer, "5"),
        ]
    );
    assert_eq!(
        sig("0xfn 1e5n"),
        vec![(K::Bigint, "0xfn"), (K::Float, "1e5"), (K::Word, "n")]
    );
    assert_eq!(
        sig("0x 0b_ 0X1F 0b123 0o18"),
        vec![
            (K::Integer, "0x"),
            (K::Integer, "0b_"),
            (K::Integer, "0X1F"),
            (K::Integer, "0b123"),
            (K::Integer, "0o18"),
        ]
    );
    assert_eq!(
        sig("0xG 0b1f 0x1.5"),
        vec![
            (K::Integer, "0x"),
            (K::Word, "G"),
            (K::Integer, "0b1"),
            (K::Word, "f"),
            (K::Integer, "0x1"),
            (K::Dot, "."),
            (K::Integer, "5"),
        ]
    );
    assert_eq!(
        sig("3.foo 1._5"),
        vec![
            (K::Integer, "3"),
            (K::Dot, "."),
            (K::Word, "foo"),
            (K::Integer, "1"),
            (K::Dot, "."),
            (K::Word, "_5"),
        ]
    );
    assert_eq!(
        sig("1..2 1...2 1.5.3"),
        vec![
            (K::Integer, "1"),
            (K::DotDot, ".."),
            (K::Integer, "2"),
            (K::Integer, "1"),
            (K::DotDotDot, "..."),
            (K::Integer, "2"),
            (K::Float, "1.5"),
            (K::Dot, "."),
            (K::Integer, "3"),
        ]
    );
    assert_eq!(
        sig("1_.5 1_000.000_1 1e1_ 1_0e1_0 1_"),
        vec![
            (K::Float, "1_.5"),
            (K::Float, "1_000.000_1"),
            (K::Float, "1e1_"),
            (K::Float, "1_0e1_0"),
            (K::Integer, "1_"),
        ]
    );
    assert_eq!(
        sig("1e 1e_1 1e+ 1.e5"),
        vec![
            (K::Integer, "1"),
            (K::Word, "e"),
            (K::Integer, "1"),
            (K::Word, "e_1"),
            (K::Integer, "1"),
            (K::Word, "e"),
            (K::Plus, "+"),
            (K::Integer, "1"),
            (K::Dot, "."),
            (K::Word, "e5"),
        ]
    );
    assert_eq!(
        sig("1.0 3.14 1E3 1e-3 2e+5 1.5e-3"),
        vec![
            (K::Float, "1.0"),
            (K::Float, "3.14"),
            (K::Float, "1E3"),
            (K::Float, "1e-3"),
            (K::Float, "2e+5"),
            (K::Float, "1.5e-3"),
        ]
    );
}

#[test]
fn comments() {
    assert_eq!(
        toks("// a  \r\nx"),
        vec![
            (K::LineComment, "// a  "),
            (K::Newline, "\r\n"),
            (K::Word, "x")
        ]
    );
    assert_eq!(
        toks("//# section\n"),
        vec![(K::HeaderComment, "//# section"), (K::Newline, "\n")]
    );
    assert_eq!(toks("/// doc"), vec![(K::DocComment, "/// doc")]);
    // A fourth slash makes a plain comment: `////` banners document nothing.
    assert_eq!(toks("////x"), vec![(K::LineComment, "////x")]);
    assert_eq!(toks("///#"), vec![(K::DocComment, "///#")]);
    assert_eq!(
        toks("//baml:mut_self"),
        vec![(K::DirectiveComment, "//baml:mut_self")]
    );
    assert_eq!(toks("// baml:x"), vec![(K::LineComment, "// baml:x")]);
    assert_eq!(toks("// #"), vec![(K::LineComment, "// #")]);
    // `//=…` is a comment (the previous lexer's token boundaries made it code).
    assert_eq!(toks("//====="), vec![(K::LineComment, "//=====")]);
    assert_eq!(toks("///="), vec![(K::DocComment, "///=")]);
    assert_eq!(
        toks("#!/usr/bin/env baml\nx"),
        vec![
            (K::LineComment, "#!/usr/bin/env baml"),
            (K::Newline, "\n"),
            (K::Word, "x")
        ]
    );
    assert_eq!(toks("#!="), vec![(K::LineComment, "#!=")]);
    assert_eq!(
        toks("/* a **/x"),
        vec![(K::BlockComment, "/* a **/"), (K::Word, "x")]
    );
    assert_eq!(toks("/**/"), vec![(K::BlockComment, "/**/")]);
    assert_eq!(
        toks("/* /* */ */"),
        vec![
            (K::BlockComment, "/* /* */"),
            (K::Whitespace, " "),
            (K::Star, "*"),
            (K::Slash, "/")
        ]
    );
    assert_eq!(toks("/*/ x"), vec![(K::UnclosedBlockComment, "/*/ x")]);
    assert_eq!(
        toks("/* */= x"),
        vec![
            (K::BlockComment, "/* */"),
            (K::Equals, "="),
            (K::Whitespace, " "),
            (K::Word, "x")
        ]
    );
    assert_eq!(toks("/* open"), vec![(K::UnclosedBlockComment, "/* open")]);
    assert_eq!(toks("/*= x"), vec![(K::UnclosedBlockComment, "/*= x")]);
    assert_eq!(
        toks("/* a\n*/ b"),
        vec![
            (K::BlockComment, "/* a\n*/"),
            (K::Whitespace, " "),
            (K::Word, "b")
        ]
    );
}

/// Logos 0.15 miscompiles the literal chain `/` < `//` < `///` next to
/// `//baml:`: a `//b…` that is not a directive used to come back as a
/// two-byte `/`. One callback classifies the `//` comments instead.
#[test]
fn line_comments_sharing_the_directive_prefix() {
    for text in [
        "//b",
        "//ba",
        "//bam",
        "//baml",
        "//baml x",
        "//bad comment",
        "//b`x`",
    ] {
        assert_eq!(toks(text), vec![(K::LineComment, text)], "{text:?}");
    }
    assert_eq!(
        toks("x //bad\ny"),
        vec![
            (K::Word, "x"),
            (K::Whitespace, " "),
            (K::LineComment, "//bad"),
            (K::Newline, "\n"),
            (K::Word, "y")
        ]
    );
    assert_eq!(toks("//baml:x"), vec![(K::DirectiveComment, "//baml:x")]);
    assert_eq!(toks("//#x"), vec![(K::HeaderComment, "//#x")]);
    assert_eq!(toks("///x"), vec![(K::DocComment, "///x")]);
}

/// A lone `#` is one token, so what follows it is lexed normally — the
/// newline after it is still a newline.
#[test]
fn stray_hash() {
    assert_eq!(
        toks("#\n\nclass"),
        vec![
            (K::Hash, "#"),
            (K::Newline, "\n"),
            (K::Newline, "\n"),
            (K::Class, "class")
        ]
    );
    assert_eq!(
        toks("# x"),
        vec![(K::Hash, "#"), (K::Whitespace, " "), (K::Word, "x")]
    );
    assert_eq!(toks("#!x"), vec![(K::LineComment, "#!x")]);
    assert_eq!(toks("#\"x\"#"), vec![(K::LegacyHashString, "#\"x\"#")]);
}

#[test]
fn strings() {
    assert_eq!(toks(r#""a\"b""#), vec![(K::String, r#""a\"b""#)]);
    assert_eq!(
        toks(r#""a\\" "#),
        vec![(K::String, r#""a\\""#), (K::Whitespace, " ")]
    );
    assert_eq!(toks(r#""a\\\"b""#), vec![(K::String, r#""a\\\"b""#)]);
    assert_eq!(toks("\"\\ \""), vec![(K::String, "\"\\ \"")]);
    assert_eq!(toks("\"a\nb\""), vec![(K::String, "\"a\nb\"")]);
    assert_eq!(
        toks("\"#! not a comment\""),
        vec![(K::String, "\"#! not a comment\"")]
    );
    assert_eq!(toks("\"http://x\""), vec![(K::String, "\"http://x\"")]);
    // prefixes must touch the quote and be exactly `r`/`b`
    assert_eq!(
        sig(r#"r"\d+\"" b"x" r "x" rb"x" bar"x" 1r"x""#),
        vec![
            (K::RawString, r#"r"\d+\"""#),
            (K::ByteString, r#"b"x""#),
            (K::Word, "r"),
            (K::String, r#""x""#),
            (K::Word, "rb"),
            (K::String, r#""x""#),
            (K::Word, "bar"),
            (K::String, r#""x""#),
            (K::Integer, "1"),
            (K::RawString, r#"r"x""#),
        ]
    );
    // unterminated: trailing trivia stays outside, an escaped run stays inside
    assert_eq!(
        toks("\"ab  \n"),
        vec![
            (K::UnclosedString, "\"ab"),
            (K::Whitespace, "  "),
            (K::Newline, "\n")
        ]
    );
    assert_eq!(toks("\"ab\\   "), vec![(K::UnclosedString, "\"ab\\   ")]);
    assert_eq!(toks("r\"x"), vec![(K::UnclosedRawString, "r\"x")]);
    assert_eq!(toks("b\"x"), vec![(K::UnclosedByteString, "b\"x")]);
}

#[test]
fn hash_strings() {
    assert_eq!(toks("#\"a\"#"), vec![(K::LegacyHashString, "#\"a\"#")]);
    assert_eq!(
        toks("##\"x\"#\"##"),
        vec![(K::LegacyHashString, "##\"x\"#\"##")]
    );
    assert_eq!(
        toks("#\"a\" #b"),
        vec![(K::LegacyHashString, "#\"a\" #"), (K::Word, "b")]
    );
    assert_eq!(toks("# \"a\"#"), vec![(K::LegacyHashString, "# \"a\"#")]);
    // too many closing hashes: not a close (unlike Rust), so unterminated
    assert_eq!(toks("#\"a\"##"), vec![(K::UnclosedHashString, "#\"a\"##")]);
    assert_eq!(
        toks("##"),
        vec![(K::Hash, "#"), (K::Hash, "#")],
        "`#`s that open no string are stray hashes, one token each"
    );
    assert_eq!(
        toks("#\"a\\\"#"),
        vec![(K::LegacyHashString, "#\"a\\\"#")],
        "no escapes in hash strings"
    );
}

#[test]
fn backtick_templates() {
    assert_eq!(
        toks("`hi ${name}!`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "hi "),
            (K::DollarLBrace, "${"),
            (K::Word, "name"),
            (K::RBrace, "}"),
            (K::TemplateChunk, "!"),
            (K::BacktickEnd, "`"),
        ]
    );
    // `$ {` and lone `$` are text; `\${` is escaped text; `$${x}` is `$` then an interpolation
    assert_eq!(
        toks("`$ {x} $`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "$ {x} $"),
            (K::BacktickEnd, "`")
        ]
    );
    assert_eq!(
        toks("`\\${x}`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "\\${x}"),
            (K::BacktickEnd, "`")
        ]
    );
    assert_eq!(
        sig("`$${x}`").iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![
            K::BacktickStart,
            K::TemplateChunk,
            K::DollarLBrace,
            K::Word,
            K::RBrace,
            K::BacktickEnd,
        ]
    );
    // escapes: `\`` is one tick of content; `\` + space then a tick closes
    assert_eq!(
        toks("`a\\`b`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "a\\`b"),
            (K::BacktickEnd, "`")
        ]
    );
    assert_eq!(
        toks("`a\\ `"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "a\\ "),
            (K::BacktickEnd, "`")
        ]
    );
    // braces inside a nested string or comment don't end the interpolation
    assert_eq!(
        sig("`${\"}\"}`")
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        vec![
            K::BacktickStart,
            K::DollarLBrace,
            K::String,
            K::RBrace,
            K::BacktickEnd,
        ]
    );
    assert_eq!(
        sig("`${ x // }\n}`")
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        vec![
            K::BacktickStart,
            K::DollarLBrace,
            K::Word,
            K::RBrace,
            K::BacktickEnd,
        ]
    );
    // nested blocks and a nested template inside an interpolation
    assert_eq!(
        sig("`${ if c { `in` } }`")
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        vec![
            K::BacktickStart,
            K::DollarLBrace,
            K::If,
            K::Word,
            K::LBrace,
            K::BacktickStart,
            K::TemplateChunk,
            K::BacktickEnd,
            K::RBrace,
            K::RBrace,
            K::BacktickEnd,
        ]
    );
    // text spans lines, one chunk
    assert_eq!(
        toks("`a\n  b`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "a\n  b"),
            (K::BacktickEnd, "`")
        ]
    );
    assert_eq!(
        toks("`see // not a comment`"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "see // not a comment"),
            (K::BacktickEnd, "`")
        ]
    );
}

#[test]
fn multi_tick_templates() {
    // anchored close: excess leading ticks are content
    assert_eq!(
        toks("```content````"),
        vec![
            (K::BacktickStart, "```"),
            (K::TemplateChunk, "content`"),
            (K::BacktickEnd, "```")
        ]
    );
    // shorter runs are content; N=1 closes at the first tick
    assert_eq!(
        toks("```a`b``c```"),
        vec![
            (K::BacktickStart, "```"),
            (K::TemplateChunk, "a`b``c"),
            (K::BacktickEnd, "```")
        ]
    );
    // N=1 and a run of 2: the first tick is content, the last one closes (anchored close)
    assert_eq!(
        toks("`a``b"),
        vec![
            (K::BacktickStart, "`"),
            (K::TemplateChunk, "a`"),
            (K::BacktickEnd, "`"),
            (K::Word, "b")
        ]
    );
    // a `` ``: `` `` `` is a 2-tick opener, not an empty 1-tick string
    assert_eq!(
        toks("``x``"),
        vec![
            (K::BacktickStart, "``"),
            (K::TemplateChunk, "x"),
            (K::BacktickEnd, "``")
        ]
    );
    // BEP-049 §8 case 1: an N>=2 opener is greedy. With no close ahead the
    // rest of the text is its content and the parser reports the missing
    // close; the lexer never looks ahead for one.
    assert_eq!(
        toks("let x = ``\nfunction f() -> int { 1 }"),
        vec![
            (K::Let, "let"),
            (K::Whitespace, " "),
            (K::Word, "x"),
            (K::Whitespace, " "),
            (K::Equals, "="),
            (K::Whitespace, " "),
            (K::BacktickStart, "``"),
            (K::TemplateChunk, "\nfunction f() -> int { 1 }"),
        ]
    );
    // ...and a close later on (even on another line) is found
    assert_eq!(
        toks("``\nx``"),
        vec![
            (K::BacktickStart, "``"),
            (K::TemplateChunk, "\nx"),
            (K::BacktickEnd, "``")
        ]
    );
}

#[test]
fn unterminated_templates_run_to_the_end() {
    assert_eq!(
        toks("`abc"),
        vec![(K::BacktickStart, "`"), (K::TemplateChunk, "abc")]
    );
    assert_eq!(
        toks("`${x"),
        vec![
            (K::BacktickStart, "`"),
            (K::DollarLBrace, "${"),
            (K::Word, "x")
        ]
    );
    round_trips("`${x");
    round_trips("`a${`b${`c");
}

#[test]
fn braces_in_code_do_not_end_the_file() {
    assert_eq!(sig("} }"), vec![(K::RBrace, "}"), (K::RBrace, "}")]);
    round_trips("}}}{{{");
}

#[test]
fn newline_tokens() {
    assert_eq!(
        toks("a\r\nb\nc\r"),
        vec![
            (K::Word, "a"),
            (K::Newline, "\r\n"),
            (K::Word, "b"),
            (K::Newline, "\n"),
            (K::Word, "c"),
            (K::Error, "\r"),
        ]
    );
    assert_eq!(toks(" \t "), vec![(K::Whitespace, " \t ")]);
}

#[test]
fn token_indices_line_up() {
    let tokens = lex("let x = 1;");
    assert_eq!(tokens.kind(TokenIdx(0)), K::Let);
    assert_eq!(
        tokens.range(TokenIdx(2)),
        TextRange::new(4.into(), 5.into())
    );
    assert_eq!(tokens.len(), 8);
}

/// Whether `text` is something a token of `kind` can be: a fixed kind is its
/// spelling, a variable kind has the shape its rule accepts. Guards the
/// classification itself, which the round trip cannot (start offsets always
/// concatenate back to the text).
fn text_fits_kind(kind: TokenKind, text: &str) -> bool {
    let line_comment = |prefix: &str| text.starts_with(prefix) && !text.contains('\n');
    match kind {
        K::Whitespace => !text.is_empty() && text.bytes().all(|b| b == b' ' || b == b'\t'),
        K::Newline => text == "\n" || text == "\r\n",
        K::LineComment => {
            (line_comment("//")
                && (!text.starts_with("///") || text.starts_with("////"))
                && !text.starts_with("//#")
                && !text.starts_with("//baml:"))
                || line_comment("#!")
        }
        K::DocComment => line_comment("///") && !text.starts_with("////"),
        K::HeaderComment => line_comment("//#"),
        K::DirectiveComment => line_comment("//baml:"),
        K::BlockComment => text.len() >= 4 && text.starts_with("/*") && text.ends_with("*/"),
        K::UnclosedBlockComment => {
            text.starts_with("/*") && !(text.len() >= 4 && text.ends_with("*/"))
        }
        K::Word => {
            let mut chars = text.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
                && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '$'))
        }
        K::Integer | K::Float | K::Bigint => {
            text.starts_with(|c: char| c.is_ascii_digit())
                && text
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b"_.xXoOn+-".contains(&b))
        }
        K::String => text.len() >= 2 && text.starts_with('"') && text.ends_with('"'),
        K::RawString => text.len() >= 3 && text.starts_with("r\"") && text.ends_with('"'),
        K::ByteString => text.len() >= 3 && text.starts_with("b\"") && text.ends_with('"'),
        K::LegacyHashString => text.starts_with('#') && text.ends_with('#') && text.contains('"'),
        K::UnclosedString => text.starts_with('"'),
        K::UnclosedRawString => text.starts_with("r\""),
        K::UnclosedByteString => text.starts_with("b\""),
        K::UnclosedHashString => text.starts_with('#') && text.contains('"'),
        K::BacktickStart | K::BacktickEnd => !text.is_empty() && text.bytes().all(|b| b == b'`'),
        K::TemplateChunk | K::Error => !text.is_empty(),
        K::Hash => text == "#",
        kind => kind.describe() == text,
    }
}

/// Every token of `text` fits its kind.
fn kinds_fit(text: &str) {
    let tokens = lex(text);
    for i in tokens.iter() {
        let (kind, slice) = (tokens.kind(i), tokens.text(text, i));
        assert!(
            text_fits_kind(kind, slice),
            "{kind:?} does not fit {slice:?} in {text:?}"
        );
    }
}

/// Every `.baml` file the repo owns: each token fits its kind.
#[test]
fn corpus_kinds_fit() {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = 0usize;
    for root in [
        "../baml_tests/baml_src",
        "../baml_tests/projects",
        "../baml_builtins2/baml_std",
        "../../tools/type_quiz",
    ] {
        let root = crate_dir.join(root);
        for entry in walkdir::WalkDir::new(&root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        {
            let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
            if entry.path().extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path())
                .unwrap_or_else(|e| panic!("reading {}: {e}", entry.path().display()));
            kinds_fit(&text);
            files += 1;
        }
    }
    assert!(
        files > 500,
        "expected the whole corpus, found {files} files"
    );
}

/// Deterministic fuzz: random strings over a BAML-ish alphabet never panic,
/// always round-trip, and every token fits its kind.
#[test]
fn random_input_round_trips() {
    const ALPHABET: &[u8] = b"abXz_$-019 \t\n\r\"'`#!/*=<>?.:;,{}()[]|&^~+%@\\\xC3\xA9e";
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2_000 {
        let pick = |bound: usize, value: u64| {
            usize::try_from(value % u64::try_from(bound).unwrap_or_else(|_| unreachable!("small")))
                .unwrap_or_else(|_| unreachable!("below `bound`"))
        };
        let len = pick(40, next());
        let bytes: Vec<u8> = (0..len)
            .map(|_| ALPHABET[pick(ALPHABET.len(), next())])
            .collect();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let tokens = lex(&text);
        let joined: String = tokens.iter().map(|i| tokens.text(&text, i)).collect();
        assert_eq!(joined, text, "round trip of {text:?}");
        for i in tokens.iter() {
            assert!(!tokens.range(i).is_empty(), "empty token in {text:?}");
        }
        kinds_fit(&text);
    }
}
