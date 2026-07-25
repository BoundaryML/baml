//! Markdown-driven tests for `simplify_sap::simplify`.
//!
//! Test cases live in `simplify_sap_tests.md`.  A tiny type DSL is parsed
//! into `baml_type::RuntimeTy` values (with attrs), fed through `simplify`, and
//! the result is compared structurally (including attrs) against the expected
//! output parsed from the same DSL.

use std::collections::{HashMap, HashSet};

use baml_type::{
    Freshness, Literal, RuntimeTy, TyAttr, TyAttrValue, TypeName,
    simplify_sap::{simplify, simplify_parse_target},
};

// =========================================================================
// Markdown → TestCase parser
// =========================================================================

struct TestCase {
    name: String,
    aliases: HashMap<TypeName, RuntimeTy>,
    input: RuntimeTy,
    expected: RuntimeTy,
}

fn parse_test_cases(markdown: &str) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let mut lines = markdown.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        // `## name` but not `### subsection` or `# section`
        if !trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            continue;
        }

        let name = trimmed.strip_prefix("## ").unwrap().trim().to_string();

        let mut aliases_str = String::new();
        let mut input_str = String::new();
        let mut expected_str = String::new();

        while let Some(&line) = lines.peek() {
            let t = line.trim();
            if t.starts_with("## ") && !t.starts_with("### ") {
                break;
            }
            if t == "---" {
                lines.next();
                break;
            }

            if t == "### aliases" {
                lines.next();
                collect_section_body(&mut lines, &mut aliases_str, '\n');
            } else if t == "### input" {
                lines.next();
                collect_section_body(&mut lines, &mut input_str, ' ');
            } else if t == "### expected" {
                lines.next();
                collect_section_body(&mut lines, &mut expected_str, ' ');
            } else {
                lines.next();
            }
        }

        if input_str.is_empty() || expected_str.is_empty() {
            continue;
        }

        let aliases = parse_aliases(&aliases_str);
        let input = parse_ty(&input_str);
        let expected = parse_ty(&expected_str);
        cases.push(TestCase {
            name,
            aliases,
            input,
            expected,
        });
    }

    cases
}

/// Collect non-empty, non-heading lines until the next `###`, `##`, or `---`.
/// `sep` is the separator between collected lines (space for types, newline for aliases).
fn collect_section_body(
    lines: &mut std::iter::Peekable<std::str::Lines<'_>>,
    out: &mut String,
    sep: char,
) {
    while let Some(&line) = lines.peek() {
        let t = line.trim();
        if t.starts_with("### ") || t == "---" || (t.starts_with("## ") && !t.starts_with("### ")) {
            break;
        }
        lines.next();
        if !t.is_empty() && !t.starts_with('#') {
            if !out.is_empty() {
                out.push(sep);
            }
            out.push_str(t);
        }
    }
}

// =========================================================================
// Alias helpers
// =========================================================================

fn parse_aliases(s: &str) -> HashMap<TypeName, RuntimeTy> {
    let mut map = HashMap::new();
    for line in s.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(eq_pos) = line.find('=') {
            let name = line[..eq_pos].trim();
            let ty_str = line[eq_pos + 1..].trim();
            map.insert(TypeName::local(name.into()), parse_ty(ty_str));
        }
    }
    map
}

fn find_recursive_aliases(aliases: &HashMap<TypeName, RuntimeTy>) -> HashSet<TypeName> {
    let mut recursive = HashSet::new();
    for name in aliases.keys() {
        let mut visiting = HashSet::new();
        if is_alias_recursive(name, aliases, &mut visiting) {
            recursive.insert(name.clone());
        }
    }
    recursive
}

fn is_alias_recursive(
    name: &TypeName,
    aliases: &HashMap<TypeName, RuntimeTy>,
    visiting: &mut HashSet<TypeName>,
) -> bool {
    if !visiting.insert(name.clone()) {
        return true;
    }
    if let Some(ty) = aliases.get(name) {
        for ref_name in collect_alias_refs(ty) {
            if is_alias_recursive(&ref_name, aliases, visiting) {
                visiting.remove(name);
                return true;
            }
        }
    }
    visiting.remove(name);
    false
}

fn collect_alias_refs(ty: &RuntimeTy) -> Vec<TypeName> {
    let mut refs = Vec::new();
    collect_alias_refs_inner(ty, &mut refs);
    refs
}

fn collect_alias_refs_inner(ty: &RuntimeTy, out: &mut Vec<TypeName>) {
    match ty {
        RuntimeTy::TypeAlias(name, _) => out.push(name.clone()),
        RuntimeTy::List(inner, _) => collect_alias_refs_inner(inner, out),
        RuntimeTy::Future(value, error, _) => {
            collect_alias_refs_inner(value, out);
            collect_alias_refs_inner(error, out);
        }
        RuntimeTy::Union(members, _) => {
            for m in members {
                collect_alias_refs_inner(m, out);
            }
        }
        RuntimeTy::Map { key, value, .. } => {
            collect_alias_refs_inner(key, out);
            collect_alias_refs_inner(value, out);
        }
        _ => {}
    }
}

// =========================================================================
// Type DSL parser
// =========================================================================
//
// Grammar:
//   type        := union_expr attrs?
//   union_expr  := element ('|' element)*
//   element     := postfix attrs?
//   postfix     := atom ('[]' | '?')*
//   atom        := 'int' | 'float' | 'string' | 'bool' | 'null'
//                | 'true' | 'false' | INT_LIT
//                | 'map' '<' type ',' type '>'
//                | '$' IDENT          (type alias ref)
//                | IDENT              (class name)
//                | '(' type ')'
//   attrs       := attr+
//   attr        := '@sap.parse_without_null'
//                | '@sap.pending_never'
//                | '@sap.in_progress_never'

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> char {
        let c = self.chars[self.pos];
        self.pos += 1;
        c
    }

    fn remaining(&self) -> String {
        self.chars[self.pos..].iter().collect()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    fn consume_str(&mut self, s: &str) {
        for expected in s.chars() {
            let actual = self.advance();
            assert_eq!(actual, expected, "expected '{s}'");
        }
    }

    fn read_word(&mut self) -> String {
        let mut word = String::new();
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        word
    }

    fn read_int(&mut self) -> i64 {
        let mut s = String::new();
        if self.peek() == Some('-') {
            s.push(self.advance());
        }
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_ascii_digit() {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        s.parse().expect("expected integer literal")
    }

    // type := union_expr attrs?
    fn parse_type(&mut self) -> RuntimeTy {
        let ty = self.parse_union();
        let attr = self.parse_attrs();
        apply_attr(ty, attr)
    }

    // union_expr := element ('|' element)*
    fn parse_union(&mut self) -> RuntimeTy {
        let first = self.parse_element();
        let mut members = vec![first];
        loop {
            self.skip_ws();
            if self.peek() == Some('|') {
                self.advance();
                members.push(self.parse_element());
            } else {
                break;
            }
        }
        if members.len() == 1 {
            members.pop().unwrap()
        } else {
            RuntimeTy::Union(members, TyAttr::default())
        }
    }

    // element := postfix attrs?
    fn parse_element(&mut self) -> RuntimeTy {
        let ty = self.parse_postfix();
        let attr = self.parse_attrs();
        apply_attr(ty, attr)
    }

    // postfix := atom ('[]' | '?')*
    fn parse_postfix(&mut self) -> RuntimeTy {
        let mut ty = self.parse_atom();
        loop {
            self.skip_ws();
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '['
                && self.chars[self.pos + 1] == ']'
            {
                self.pos += 2;
                ty = RuntimeTy::List(Box::new(ty), TyAttr::default());
            } else if self.peek() == Some('?') {
                self.advance();
                ty = RuntimeTy::optional(ty);
            } else {
                break;
            }
        }
        ty
    }

    fn parse_atom(&mut self) -> RuntimeTy {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.advance();
                let ty = self.parse_type();
                self.skip_ws();
                assert_eq!(self.advance(), ')', "expected ')'");
                ty
            }
            Some('$') => {
                self.advance();
                let name = self.read_word();
                RuntimeTy::TypeAlias(TypeName::local(name.into()), TyAttr::default())
            }
            Some(c) if c.is_ascii_digit() || c == '-' => {
                let n = self.read_int();
                RuntimeTy::Literal(Literal::Int(n), Freshness::Regular, TyAttr::default())
            }
            Some(c) if c.is_alphabetic() => {
                let word = self.read_word();
                match word.as_str() {
                    "int" => RuntimeTy::int(),
                    "float" => RuntimeTy::float(),
                    "string" => RuntimeTy::string(),
                    "bool" => RuntimeTy::bool(),
                    "null" => RuntimeTy::null(),
                    "true" => RuntimeTy::Literal(
                        Literal::Bool(true),
                        Freshness::Regular,
                        TyAttr::default(),
                    ),
                    "false" => RuntimeTy::Literal(
                        Literal::Bool(false),
                        Freshness::Regular,
                        TyAttr::default(),
                    ),
                    "map" => {
                        self.skip_ws();
                        assert_eq!(self.advance(), '<');
                        let key = self.parse_type();
                        self.skip_ws();
                        assert_eq!(self.advance(), ',');
                        let value = self.parse_type();
                        self.skip_ws();
                        assert_eq!(self.advance(), '>');
                        RuntimeTy::Map {
                            key: Box::new(key),
                            value: Box::new(value),
                            attr: TyAttr::default(),
                        }
                    }
                    // Capitalized or otherwise — treat as class name.
                    name => RuntimeTy::Class(
                        TypeName::local(name.into()),
                        Vec::new(),
                        TyAttr::default(),
                    ),
                }
            }
            other => panic!("unexpected {:?} at pos {} in type DSL", other, self.pos),
        }
    }

    // attrs := attr*
    fn parse_attrs(&mut self) -> TyAttr {
        let mut attr = TyAttr::default();
        loop {
            self.skip_ws();
            if !self.starts_with("@") {
                break;
            }
            if self.starts_with("@sap.parse_without_null") {
                self.consume_str("@sap.parse_without_null");
                attr.sap_parse_without_null = TyAttrValue::Set;
            } else if self.starts_with("@sap.pending_never") {
                self.consume_str("@sap.pending_never");
                attr.sap_pending_never = TyAttrValue::Set;
            } else if self.starts_with("@sap.in_progress_never") {
                self.consume_str("@sap.in_progress_never");
                attr.sap_in_progress_never = TyAttrValue::Set;
            } else {
                break;
            }
        }
        attr
    }
}

/// Apply a parsed attr to a RuntimeTy. If the attr is all-default, return ty unchanged.
fn apply_attr(ty: RuntimeTy, attr: TyAttr) -> RuntimeTy {
    if attr == TyAttr::default() {
        ty
    } else {
        ty.with_attr(attr)
    }
}

fn parse_ty(s: &str) -> RuntimeTy {
    let mut parser = Parser::new(s);
    let ty = parser.parse_type();
    parser.skip_ws();
    assert!(
        parser.pos == parser.chars.len(),
        "trailing characters in type DSL: {:?}",
        &s[parser.pos..],
    );
    ty
}

// =========================================================================
// Test runner
// =========================================================================

#[test]
fn simplify_sap_tests() {
    let markdown = include_str!("simplify_sap_tests.md");
    let cases = parse_test_cases(markdown);
    assert!(!cases.is_empty(), "no test cases parsed from markdown");

    let mut failures = Vec::new();
    for case in &cases {
        let recursive = find_recursive_aliases(&case.aliases);
        let actual = simplify(case.input.clone(), &case.aliases, &recursive);

        if actual != case.expected {
            failures.push(format!(
                "  {}:\n    expected: {:#?}\n    actual:   {:#?}",
                case.name, case.expected, actual,
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n{} simplify_sap test(s) failed:\n{}\n",
            failures.len(),
            failures.join("\n\n"),
        );
    }
}

#[test]
fn parse_target_expands_recursive_union_alias_member_once() {
    let json_name = TypeName::local("Json".into());
    let aliases = HashMap::from([(
        json_name,
        parse_ty("null | bool | $Json[] | map<string, $Json>"),
    )]);
    let recursive = find_recursive_aliases(&aliases);

    let actual = simplify_parse_target(parse_ty("$Json | null"), &aliases, &recursive);
    let expected = parse_ty("bool | $Json[] | map<string, $Json> | null");

    assert_eq!(actual, expected);
}
