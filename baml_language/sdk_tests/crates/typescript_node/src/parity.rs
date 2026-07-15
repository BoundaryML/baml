use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

type Cases = BTreeMap<String, Vec<Location>>;

// Ratchet at the current baseline: every Python-shared case is either parity
// or an explicitly documented host-language N/A. A new fixable Node gap must
// fail this guard instead of silently consuming historical headroom.
const INITIAL_NODE_GAP_BUDGET: usize = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VitestCaseKind {
    Plain,
    Parity,
    NodeGap,
    NodeNa,
}

#[derive(Debug)]
struct ParsedVitestCase {
    id: String,
    offset: usize,
    kind: VitestCaseKind,
}

#[derive(Debug)]
struct Location {
    path: PathBuf,
    line: usize,
}

#[test]
fn python_and_typescript_common_case_ids_match_by_fixture() {
    assert_parser_contracts();

    let typescript_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates_root = typescript_root
        .parent()
        .expect("typescript_node must live under sdk_tests/crates/");
    let python_root = crates_root.join("python_pydantic2");

    let mut fixtures = discover_fixtures(&python_root)
        .unwrap_or_else(|error| panic!("discover Python SDK-test fixtures: {error}"));
    fixtures.extend(
        discover_fixtures(&typescript_root)
            .unwrap_or_else(|error| panic!("discover TypeScript SDK-test fixtures: {error}")),
    );
    assert!(
        !fixtures.is_empty(),
        "no SDK-test fixtures found under {} or {}",
        python_root.display(),
        typescript_root.display()
    );

    let mut failures = Vec::new();
    for fixture in fixtures {
        let python_cases = collect_cases(
            &python_root.join(&fixture).join("customizable"),
            "py",
            parse_python_test_defs,
            crates_root,
        )
        .unwrap_or_else(|error| panic!("collect Python cases for {fixture}: {error}"));
        let typescript_cases = collect_cases(
            &typescript_root.join(&fixture).join("customizable"),
            "ts",
            parse_vitest_cases,
            crates_root,
        )
        .unwrap_or_else(|error| panic!("collect TypeScript cases for {fixture}: {error}"));

        let missing = case_deltas(&python_cases, &typescript_cases);
        let extra = case_deltas(&typescript_cases, &python_cases);
        if missing.is_empty() && extra.is_empty() {
            continue;
        }

        let mut failure = format!("fixture `{fixture}`:");
        if !missing.is_empty() {
            failure.push_str("\n  missing from TypeScript/Node:");
            for delta in missing {
                failure.push_str(&format_delta(&delta, "Python reference"));
            }
        }
        if !extra.is_empty() {
            failure.push_str("\n  extra in TypeScript/Node:");
            for delta in extra {
                failure.push_str(&format_delta(&delta, "TypeScript source"));
            }
        }
        failures.push(failure);
    }

    assert!(
        failures.is_empty(),
        "Python/TypeScript SDK-test case parity failed.\n\
         Common Vitest cases must use the exact Python `test_*` ID with the same multiplicity.\n\
         Add missing cases under `sdk_tests/crates/typescript_node/<fixture>/customizable/`.\n\
         Add cross-language extras to Python first; put host-only cases under a path segment named \
         `language_specific`.\n\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn documented_node_gap_budget_does_not_grow() {
    let typescript_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_source_files(&typescript_root, "ts", &mut files)
        .expect("collect TypeScript SDK-test sources");

    let mut gaps = Vec::new();
    let mut not_applicable = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for case in parse_vitest_classified_cases(&source) {
            let line = source[..case.offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            let entry = format!("{}:{} ({})", path.display(), line, case.id);
            match case.kind {
                VitestCaseKind::NodeGap => gaps.push(entry),
                VitestCaseKind::NodeNa => not_applicable.push(entry),
                VitestCaseKind::Plain | VitestCaseKind::Parity => {}
            }
        }
    }

    assert!(
        gaps.len() <= INITIAL_NODE_GAP_BUDGET,
        "TypeScript/Node documented gaps grew from the initial budget of \
         {INITIAL_NODE_GAP_BUDGET} to {}. New shared behavior belongs in Python first; \
         a real host-language mismatch must use nodeNA with a concrete rationale.\n{}",
        gaps.len(),
        gaps.join("\n")
    );

    eprintln!(
        "TypeScript/Node parity inventory: {} fixable gap(s), {} host-language N/A case(s)",
        gaps.len(),
        not_applicable.len()
    );
}

struct Delta<'a> {
    id: &'a str,
    count: usize,
    reference_count: usize,
    actual_count: usize,
    locations: &'a [Location],
}

fn case_deltas<'a>(reference: &'a Cases, actual: &Cases) -> Vec<Delta<'a>> {
    reference
        .iter()
        .filter_map(|(id, locations)| {
            let reference_count = locations.len();
            let actual_count = actual.get(id).map_or(0, Vec::len);
            (reference_count > actual_count).then_some(Delta {
                id,
                count: reference_count - actual_count,
                reference_count,
                actual_count,
                locations,
            })
        })
        .collect()
}

fn format_delta(delta: &Delta<'_>, location_label: &str) -> String {
    let occurrences = if delta.count == 1 {
        "occurrence"
    } else {
        "occurrences"
    };
    let locations = delta
        .locations
        .iter()
        .map(|location| format!("{}:{}", location.path.display(), location.line))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n    - `{}`: {} {} (python/typescript counts: {}/{}); {}: {}",
        delta.id,
        delta.count,
        occurrences,
        if location_label == "Python reference" {
            delta.reference_count
        } else {
            delta.actual_count
        },
        if location_label == "Python reference" {
            delta.actual_count
        } else {
            delta.reference_count
        },
        location_label,
        locations
    )
}

fn discover_fixtures(generator_root: &Path) -> io::Result<BTreeSet<String>> {
    let mut fixtures = BTreeSet::new();
    for entry in fs::read_dir(generator_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !entry.path().join("customizable").is_dir() {
            continue;
        }
        let name = entry.file_name().into_string().map_err(|name| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("non-UTF-8 fixture directory: {name:?}"),
            )
        })?;
        fixtures.insert(name);
    }
    Ok(fixtures)
}

fn collect_cases(
    root: &Path,
    extension: &str,
    parser: fn(&str) -> Vec<(String, usize)>,
    display_root: &Path,
) -> io::Result<Cases> {
    let mut files = Vec::new();
    collect_source_files(root, extension, &mut files)?;
    files.sort();

    let mut cases = Cases::new();
    for path in files {
        let source = fs::read_to_string(&path)?;
        let display_path = path
            .strip_prefix(display_root)
            .unwrap_or(&path)
            .to_path_buf();
        for (id, offset) in parser(&source) {
            let line = source[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            cases.entry(id).or_default().push(Location {
                path: display_path.clone(),
                line,
            });
        }
    }
    Ok(cases)
}

fn collect_source_files(root: &Path, extension: &str, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if !root.is_dir() || has_excluded_segment(root) {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if has_excluded_segment(&path) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_source_files(&path, extension, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|value| value == extension) {
            files.push(path);
        }
    }
    Ok(())
}

fn has_excluded_segment(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str();
        value == OsStr::new("language_specific") || value == OsStr::new("generated")
    })
}

fn parse_python_test_defs(source: &str) -> Vec<(String, usize)> {
    let bytes = source.as_bytes();
    let mut cases = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'#' => cursor = skip_line_comment(bytes, cursor + 1),
            b'\'' | b'"' => cursor = skip_python_string(bytes, cursor),
            byte if is_identifier_start(byte) => {
                let end = identifier_end(bytes, cursor);
                if &source[cursor..end] == "def" {
                    let mut name_start = end;
                    skip_python_trivia(bytes, &mut name_start);
                    if name_start < bytes.len() && is_identifier_start(bytes[name_start]) {
                        let name_end = identifier_end(bytes, name_start);
                        let name = &source[name_start..name_end];
                        if name.starts_with("test_") {
                            cases.push((name.to_owned(), name_start));
                        }
                    }
                }
                cursor = end;
            }
            _ => cursor += 1,
        }
    }
    cases
}

fn skip_python_trivia(bytes: &[u8], cursor: &mut usize) {
    loop {
        while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if *cursor < bytes.len() && bytes[*cursor] == b'#' {
            *cursor = skip_line_comment(bytes, *cursor + 1);
            continue;
        }
        break;
    }
}

fn skip_python_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let triple = bytes.get(start + 1) == Some(&quote) && bytes.get(start + 2) == Some(&quote);
    let mut cursor = start + if triple { 3 } else { 1 };
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if triple
            && bytes[cursor] == quote
            && bytes.get(cursor + 1) == Some(&quote)
            && bytes.get(cursor + 2) == Some(&quote)
        {
            return cursor + 3;
        } else if !triple && bytes[cursor] == quote {
            return cursor + 1;
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

fn parse_vitest_cases(source: &str) -> Vec<(String, usize)> {
    parse_vitest_classified_cases(source)
        .into_iter()
        .map(|case| (case.id, case.offset))
        .collect()
}

fn parse_vitest_classified_cases(source: &str) -> Vec<ParsedVitestCase> {
    let bytes = source.as_bytes();
    let mut cases = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let before_trivia = cursor;
        skip_javascript_trivia(bytes, &mut cursor);
        if cursor != before_trivia {
            continue;
        }
        if cursor >= bytes.len() {
            break;
        }

        match bytes[cursor] {
            b'\'' | b'"' | b'`' => cursor = skip_javascript_string(bytes, cursor),
            b'/' => {
                cursor = skip_javascript_regex(bytes, cursor).unwrap_or(cursor + 1);
            }
            byte if is_identifier_start(byte) => {
                let end = identifier_end(bytes, cursor);
                let token = &source[cursor..end];
                let kind = match token {
                    "it" | "test" => Some(VitestCaseKind::Plain),
                    "parity" => Some(VitestCaseKind::Parity),
                    "nodeGap" => Some(VitestCaseKind::NodeGap),
                    "nodeNA" => Some(VitestCaseKind::NodeNa),
                    _ => None,
                };
                if let Some(kind) = kind {
                    if let Some((name, name_offset, call_end)) = parse_vitest_call(source, end) {
                        cases.push(ParsedVitestCase {
                            id: name,
                            offset: name_offset,
                            kind,
                        });
                        cursor = call_end;
                        continue;
                    }
                }
                cursor = end;
            }
            _ => cursor += 1,
        }
    }
    cases
}

fn parse_vitest_call(source: &str, token_end: usize) -> Option<(String, usize, usize)> {
    let bytes = source.as_bytes();
    let mut cursor = token_end;
    skip_javascript_trivia(bytes, &mut cursor);

    while bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        skip_javascript_trivia(bytes, &mut cursor);
        if cursor >= bytes.len() || !is_identifier_start(bytes[cursor]) {
            return None;
        }
        let modifier_end = identifier_end(bytes, cursor);
        let modifier = &source[cursor..modifier_end];
        if !matches!(modifier, "skip" | "todo" | "only" | "concurrent" | "fails") {
            return None;
        }
        cursor = modifier_end;
        skip_javascript_trivia(bytes, &mut cursor);
    }

    if bytes.get(cursor) != Some(&b'(') {
        return None;
    }
    cursor += 1;
    skip_javascript_trivia(bytes, &mut cursor);
    let name_offset = cursor;
    let (name, end) = parse_static_javascript_string(source, cursor)?;
    Some((name, name_offset, end))
}

fn parse_static_javascript_string(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let quote = *bytes.get(start)?;
    if !matches!(quote, b'\'' | b'"' | b'`') {
        return None;
    }

    let mut value = Vec::new();
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            byte if byte == quote => return String::from_utf8(value).ok().map(|v| (v, cursor + 1)),
            b'\\' => {
                let escaped = *bytes.get(cursor + 1)?;
                value.push(match escaped {
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    other => other,
                });
                cursor += 2;
            }
            b'$' if quote == b'`' && bytes.get(cursor + 1) == Some(&b'{') => return None,
            byte => {
                value.push(byte);
                cursor += 1;
            }
        }
    }
    None
}

fn skip_javascript_trivia(bytes: &[u8], cursor: &mut usize) {
    loop {
        while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if bytes.get(*cursor) == Some(&b'/') && bytes.get(*cursor + 1) == Some(&b'/') {
            *cursor = skip_line_comment(bytes, *cursor + 2);
        } else if bytes.get(*cursor) == Some(&b'/') && bytes.get(*cursor + 1) == Some(&b'*') {
            *cursor += 2;
            while *cursor + 1 < bytes.len()
                && !(bytes[*cursor] == b'*' && bytes[*cursor + 1] == b'/')
            {
                *cursor += 1;
            }
            *cursor = (*cursor + 2).min(bytes.len());
        } else {
            break;
        }
    }
}

fn skip_javascript_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if bytes[cursor] == quote {
            return cursor + 1;
        } else {
            cursor += 1;
        }
    }
    bytes.len()
}

fn skip_javascript_regex(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    let mut in_character_class = false;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\n' | b'\r' => return None,
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'[' => {
                in_character_class = true;
                cursor += 1;
            }
            b']' => {
                in_character_class = false;
                cursor += 1;
            }
            b'/' if !in_character_class => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
                    cursor += 1;
                }
                return Some(cursor);
            }
            _ => cursor += 1,
        }
    }
    None
}

fn skip_line_comment(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        cursor += 1;
    }
    cursor
}

fn identifier_end(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
        cursor += 1;
    }
    cursor
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn assert_parser_contracts() {
    let source = r#"
def test_sync(): pass
async def test_async(): pass
# def test_comment(): pass
EXAMPLE = '''
def test_in_string(): pass
'''
"#;
    let ids = parse_python_test_defs(source)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["test_sync", "test_async"]);

    let source = r#"
it("test_plain", () => {});
test(
  "test_multiline",
  () => {},
);
it.skip('test_skipped', () => {});
test.todo(
  `test_todo`,
);
// it("test_comment", () => {});
const example = 'test("test_in_string", () => {})';
const trace = /^File "(?<file>[^"]*)", line (?<line>\d+)$/;
nodeGap("test_documented_gap", () => {});
nodeNA("test_host_language_not_applicable", () => {});
parity("test_alias_for_supported_behavior", () => {});
"#;
    let ids = parse_vitest_cases(source)
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "test_plain",
            "test_multiline",
            "test_skipped",
            "test_todo",
            "test_documented_gap",
            "test_host_language_not_applicable",
            "test_alias_for_supported_behavior",
        ]
    );
}
