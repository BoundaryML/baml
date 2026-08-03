//! Parameterized scenario matrix for broad formatter invariants.

use std::collections::BTreeSet;

use crate::{FormatOptions, format};

const FAMILIES: [&str; 16] = [
    "class_fields",
    "enum_values",
    "function_params",
    "type_aliases",
    "arrays",
    "maps",
    "objects",
    "binary_exprs",
    "calls",
    "chains",
    "if_exprs",
    "match_exprs",
    "lambdas",
    "literal_forms",
    "nested_blocks",
    "width_boundaries",
];

#[test]
fn formatter_scenario_matrix() {
    let mut coverage_keys = BTreeSet::new();
    let mut cores = BTreeSet::new();
    let mut sources = BTreeSet::new();

    for (family_index, family) in FAMILIES.iter().enumerate() {
        for semantic in 0..8 {
            for trivia in 0..4 {
                for layout in 0..2 {
                    let ordinal = family_index * 64 + semantic * 8 + trivia * 2 + layout;
                    let coverage_key =
                        format!("{family}/semantic-{semantic}/trivia-{trivia}/layout-{layout}");
                    assert!(
                        coverage_keys.insert(coverage_key.clone()),
                        "duplicate formatter coverage key: {coverage_key}"
                    );
                    let (core, required, forbidden) = build_case(family, semantic, layout);
                    if trivia == 0 {
                        assert!(
                            cores.insert(core.clone()),
                            "duplicate formatter scenario core for {coverage_key}"
                        );
                    }
                    let (source, marker) = decorate(core, trivia, ordinal);
                    assert!(
                        sources.insert(source.clone()),
                        "duplicate formatter source for {coverage_key}"
                    );
                    assert_scenario(&coverage_key, &source, &required, &forbidden, &marker);
                }
            }
        }
    }

    assert_eq!(coverage_keys.len(), 1024);
    assert_eq!(cores.len(), 256);
    assert_eq!(sources.len(), 1024);
}

fn assert_scenario(
    coverage_key: &str,
    source: &str,
    required: &str,
    forbidden: &str,
    marker: &str,
) {
    let options = FormatOptions::default();
    let formatted = format(source, &options).unwrap_or_else(|error| {
        panic!("formatter rejected scenario {coverage_key}: {error:?}\nsource:\n{source}")
    });
    assert!(
        formatted.ends_with('\n'),
        "{coverage_key}: output needs one final newline"
    );
    assert!(
        !formatted.ends_with("\n\n"),
        "{coverage_key}: output has multiple final newlines"
    );
    assert!(
        !formatted.contains('\t'),
        "{coverage_key}: canonical output contains a tab"
    );
    if !required.is_empty() {
        assert!(
            formatted.contains(required),
            "{coverage_key}: output omitted required fragment {required:?}\noutput:\n{formatted}"
        );
    }
    if !forbidden.is_empty() {
        assert!(
            !formatted.contains(forbidden),
            "{coverage_key}: output retained forbidden fragment {forbidden:?}\noutput:\n{formatted}"
        );
    }
    if !marker.is_empty() {
        assert_eq!(
            formatted.matches(marker).count(),
            1,
            "{coverage_key}: marker comment must survive exactly once\noutput:\n{formatted}"
        );
    }
    let second = format(&formatted, &options).unwrap_or_else(|error| {
        panic!(
            "formatter output stopped parsing for {coverage_key}: {error:?}\noutput:\n{formatted}"
        )
    });
    assert_eq!(
        formatted, second,
        "{coverage_key}: formatter is not idempotent"
    );
}

fn build_case(family: &str, semantic: usize, layout: usize) -> (String, String, String) {
    let indent = if layout == 0 { "    " } else { "  " };
    let gap = if layout == 0 { " " } else { "   " };
    let compact = layout == 1;
    let types = [
        "string",
        "int",
        "bool",
        "float",
        "string?",
        "int[]",
        "map<string, int>",
        "string | int",
    ];
    let ty = types[semantic];
    let name = format!("Generated_{family}_{semantic}");

    match family {
        "class_fields" => {
            let delimiter = if compact { ";" } else { "" };
            let colon = if compact { "" } else { ":" };
            let field = format!("field_{semantic}");
            (
                format!("class {name}{gap}{{\n{indent}{field}{colon}{gap}{ty}{delimiter}\n}}\n"),
                format!("{field}: {ty},"),
                format!("{field} {ty}"),
            )
        }
        "enum_values" => {
            let mut values = (0..=semantic)
                .map(|index| format!("{indent}Value{index}"))
                .collect::<Vec<_>>();
            if compact {
                values
                    .last_mut()
                    .expect("enum has at least one value")
                    .push(';');
            }
            let last_value = format!("Value{semantic}");
            (
                format!("enum {name}{gap}{{\n{}\n}}\n", values.join("\n")),
                format!("{last_value},"),
                format!("{last_value};"),
            )
        }
        "function_params" => {
            let colon = if compact { "" } else { ":" };
            let comma = if compact { "" } else { "," };
            let param = format!("arg_{semantic}");
            (
                format!(
                    "function {name}({param}{colon}{gap}{ty}{comma}){gap}->{gap}int{{\n{indent}1\n}}\n"
                ),
                format!("{param}: {ty}"),
                String::new(),
            )
        }
        "type_aliases" => (
            format!("type {name}{gap}={gap}{ty}\n"),
            format!("type {name} = {ty}"),
            String::new(),
        ),
        "arrays" => {
            let exprs = [
                "1", "true", "null", "item", "-1", "1 + 2", "foo(1)", "[1, 2]",
            ];
            let expr = exprs[semantic];
            let separator = if compact && semantic != 7 { " " } else { ", " };
            (
                function_with_tail(&name, indent, &format!("[{expr}{separator}{expr}]")),
                "[".to_string(),
                String::new(),
            )
        }
        "maps" => {
            let values = [
                "1", "true", "null", "item", "-1", "1 + 2", "foo(1)", "[1, 2]",
            ];
            let value = values[semantic];
            let separator = if compact { " " } else { ", " };
            (
                function_with_tail(
                    &name,
                    indent,
                    &format!("{{ \"left\":{gap}{value}{separator}\"right\":{gap}{value} }}"),
                ),
                "\"left\":".to_string(),
                String::new(),
            )
        }
        "objects" => {
            let values = [
                "1", "true", "null", "item", "-1", "1 + 2", "foo(1)", "[1, 2]",
            ];
            let value = values[semantic];
            (
                format!(
                    "class Box{semantic} {{ value: int }}\nfunction {name}() -> Box{semantic} {{\n{indent}Box{semantic} {{ value:{gap}{value} }}\n}}\n"
                ),
                value.to_string(),
                String::new(),
            )
        }
        "binary_exprs" => {
            let ops = ["+", "-", "*", "/", "%", "==", "&&", "||"];
            let op = ops[semantic];
            (
                function_with_tail(&name, indent, &format!("left{gap}{op}{gap}right")),
                format!(" {op} "),
                String::new(),
            )
        }
        "calls" => {
            let args = [
                "",
                "1",
                "1, 2",
                "value = 1",
                "1, value = 2",
                "foo(1)",
                "[1, 2]",
                "{ \"key\": 1 }",
            ];
            (
                function_with_tail(&name, indent, &format!("callee({})", args[semantic])),
                "callee(".to_string(),
                String::new(),
            )
        }
        "chains" => {
            let chains = [
                "value.field",
                "value.field.other",
                "value.method()",
                "value.method().field",
                "value[0].field",
                "value?.field",
                "value?.[0]",
                "value?.method()",
            ];
            (
                function_with_tail(&name, indent, chains[semantic]),
                chains[semantic].to_string(),
                String::new(),
            )
        }
        "if_exprs" => {
            let conditions = [
                "true", "false", "x == 1", "x != 1", "x < 1", "x <= 1", "x > 1", "x >= 1",
            ];
            (
                function_with_tail(
                    &name,
                    indent,
                    &format!(
                        "if{}({}){}{{ 1 }} else {{ 2 }}",
                        gap, conditions[semantic], gap
                    ),
                ),
                "if (".to_string(),
                String::new(),
            )
        }
        "match_exprs" => {
            let patterns = ["0", "1", "true", "false", "null", "\"x\"", "-1", "_"];
            (
                function_with_tail(
                    &name,
                    indent,
                    &format!("match (value) {{ {} => 1, _ => 0 }}", patterns[semantic]),
                ),
                "=>".to_string(),
                String::new(),
            )
        }
        "lambdas" => {
            let lambdas = [
                "() => { 1 }",
                "(x) => { x }",
                "(x: int) => { x + 1 }",
                "(x: int) -> { x }",
                "(x: int) -> int { x }",
                "(x: int, y: int) => { x + y }",
                "() => { throw \"boom\" }",
                "(x: int) => { return x }",
            ];
            (
                format!(
                    "function {name}() -> int {{\n{indent}let callback = {}\n{indent}1\n}}\n",
                    lambdas[semantic]
                ),
                "let callback =".to_string(),
                "=>".to_string(),
            )
        }
        "literal_forms" => {
            let literals = [
                "\"plain\"",
                "\"escaped\\nvalue\"",
                "42",
                "42n",
                "3.14",
                "true",
                "null",
                "`hello`",
            ];
            (
                function_with_tail(&name, indent, literals[semantic]),
                literals[semantic].to_string(),
                String::new(),
            )
        }
        "nested_blocks" => {
            let depths = semantic + 1;
            let mut body = "1".to_string();
            for depth in 0..depths {
                body = format!("if (level_{depth}) {{ {body} }} else {{ 0 }}");
            }
            (
                function_with_tail(&name, indent, &body),
                format!("level_{}", depths - 1),
                String::new(),
            )
        }
        "width_boundaries" => {
            let lengths = [8, 16, 24, 32, 48, 64, 80, 96];
            let identifier = format!("value_{}", "x".repeat(lengths[semantic]));
            (
                function_with_tail(&name, indent, &format!("callee({identifier})")),
                identifier,
                String::new(),
            )
        }
        _ => unreachable!("unknown formatter family"),
    }
}

fn function_with_tail(name: &str, indent: &str, tail: &str) -> String {
    format!("function {name}() -> int {{\n{indent}{tail}\n}}\n")
}

fn decorate(core: String, trivia: usize, ordinal: usize) -> (String, String) {
    let marker = format!("generated-case-{ordinal}");
    match trivia {
        0 => (core, String::new()),
        1 => (format!("/* {marker} */\n{core}"), marker),
        2 => (format!("// {marker}\n{core}"), marker),
        3 => (format!("{} // {marker}\n", core.trim_end()), marker),
        _ => unreachable!("trivia dimension is 0..4"),
    }
}
