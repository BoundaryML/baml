function render_float(value: float) -> string { `${value}` }
function parse_float(value: string) -> float { float.parse(value) }
function main() -> int {
    if (render_float(1.0) != "1.0") { return -1; };
    if (render_float(-0.0) != "-0.0") { return -2; };
    if (parse_float("+1.25") != 1.25) { return -3; };
    if (string.from(parse_float("NaN")) != "NaN") { return -4; };
    if (string.from(parse_float("inf")) != "Infinity") { return -5; };
    if (string.from(parse_float("-inf")) != "-Infinity") { return -6; };
    if (parse_float(render_float(1.25e-100)) != 1.25e-100) { return -7; };
    if (parse_float(render_float(1.25e100)) != 1.25e100) { return -8; };
    return 8;
}
