function string_to_lower_case_ascii() -> string { rep("Hello, World 42! ", 2).to_lower_case() }
function string_to_lower_case_unchanged() -> string { rep("already lower 1", 2).to_lower_case() }
function string_to_lower_case_wide() -> string { rep("ÀÉ Straße İ 😀 ΟΔΟΣ ", 2).to_lower_case() }
function string_to_lower_case_empty() -> string { `[${rep("A", 0).to_lower_case()}]` }
function string_to_lower_case_length() -> int { rep("İ", 3).to_lower_case().length() }
