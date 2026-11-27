function string_includes_found() -> bool { rep("hello ", 2).includes("o h") }
function string_includes_missing() -> bool { rep("hello ", 2).includes("oh") }
function string_includes_empty_needle() -> bool { rep("x", 1).includes(rep("x", 0)) }
function string_includes_in_empty() -> bool { rep("x", 0).includes("x") }
function string_includes_non_ascii() -> bool { rep("naïve ", 2).includes("ïve n") }
