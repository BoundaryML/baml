function string_trim_both_sides() -> string { `[${`${rep(" ", 3)}a b${rep(" ", 2)}`.trim()}]` }
function string_trim_nothing() -> string { `[${rep("ab ", 2).slice(0, 5).trim()}]` }
function string_trim_all_whitespace() -> string { `[${rep(" ", 4).trim()}]` }
function string_trim_empty() -> string { `[${rep(" ", 0).trim()}]` }
