function capture_optional_groups(values: baml.regex.Group?[]) -> bool { string.from(values).includes("null") }
function capture_optional_named(values: map<string,baml.regex.Group?>) -> bool { string.from(values).includes("null") }
function inspect_match(value: baml.regex.Match) -> int {
    if (value.named.length() != 1 || !value.named.has("letter")) { return -1; };
    let named = value.named.get("letter");
    match (named) { null => { return -2; }, let group: baml.regex.Group => { group.text = "changed"; } };
    let numbered = value.groups[1];
    match (numbered) { null => { return -3; }, let group: baml.regex.Group => { if (group.text != "changed") { return -4; }; } };
    if (value.named.keys().join("|") != "letter") { return -5; };
    return 1;
}
function inspect_absent(value: baml.regex.Match) -> int {
    if (!value.named.has("x") || value.named.get("x") != null || value.groups[1] != null) { return -1; };
    return 1;
}

function main() -> int {
    if ("a🦀a".replace("a", "$1") != "$1🦀a") { return -1; };
    if ("a🦀a".replace_all("a", "$1") != "$1🦀$1") { return -2; };
    if ("ab".replace_all("", "-") != "-a-b-") { return -3; };
    let pattern = baml.regex.new("(?P<letter>a)(b)?");
    if (!pattern.has_match("🦀a")) { return -4; };
    if ("🦀ab a".replace_all(pattern, "${letter}:$2") != "🦀a:b a:") { return -5; };
    let first = pattern.match_once("🦀ab");
    match (first) { null => { return -6; }, let m: baml.regex.Match => {
        if (m.text != "ab" || m.start != 1 || m.end != 3) { return -7; };
        if (m.groups.length() != 3) { return -8; };
        if (inspect_match(m) != 1) { return -13; };
    }};
    if (pattern.match_full("🦀ab") != null) { return -9; };
    if (pattern.match_all("a ab").length() != 2) { return -10; };
    if (pattern.split("🦀ab🦀a").join("|") != "🦀|a|b|🦀|a|") { return -11; };
    if (baml.regex.escape("a.b") != "a\\.b") { return -12; };
    let optional = baml.regex.new("(?P<x>a)?b").match_once("b");
    match (optional) { null => { return -14; }, let value: baml.regex.Match => { if (inspect_absent(value) != 1) { return -15; }; } };
    if (baml.regex.new("").match_all("é🦀").length() != 3) { return -16; };
    if (!baml.regex.new("(?<=é)🦀", backtracking = true).has_match("é🦀")) { return -17; };
    if (!capture_optional_groups([null, baml.regex.Group { text: "x", start: 0, end: 1 }])) { return -18; };
    let named: map<string,baml.regex.Group?> = { "x": null };
    if (!capture_optional_named(named) || !named.has("x")) { return -19; };
    return 12;
}
