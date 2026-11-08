function compile(pattern: string) -> bool { baml.regex.new(pattern).has_match("x") }
function main() -> int { compile("("); 0 }
