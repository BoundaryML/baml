function string_to_upper_case_ascii() -> string { rep("Hello, World 42! ", 2).to_upper_case() }
function string_to_upper_case_unchanged() -> string { rep("ALREADY UPPER 1", 2).to_upper_case() }
function string_to_upper_case_wide() -> string { rep("àé straße ŉ 😀 ", 2).to_upper_case() }
function string_to_upper_case_empty() -> string { `[${rep("a", 0).to_upper_case()}]` }
function string_to_upper_case_length() -> int { rep("ß", 3).to_upper_case().length() }
