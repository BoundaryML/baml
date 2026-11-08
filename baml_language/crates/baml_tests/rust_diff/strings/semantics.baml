function string_capture(text: string, pieces: string[]) -> string {
    if (pieces.length() < 2) { return "wrong pieces"; };
    text
}

class StringEnvelope { text: string }

function main() -> int {
    let text = "";
    for (let i = 0; i < 96; i += 1) { text = text + "é🦀|"; };
    if (text.length() != 288) { return -1; };
    if (text.byte_length() != 672) { return -2; };
    let suffix = text.slice(-6, -1);
    if (string_capture(suffix, text.split("|")) != "é🦀|é🦀") { return -3; };
    if ((suffix.at(-1) ?? "") != "🦀") { return -4; };
    if ((suffix.code_point_at(-1) ?? -1) != 129408) { return -5; };
    if ((suffix.index_of("🦀") ?? -1) != 1) { return -6; };
    if ((suffix.last_index_of("🦀") ?? -1) != 4) { return -7; };
    if (suffix.at(-99) != null) { return -8; };
    let parts = text.split("|");
    parts.push("changed");
    if (text.split("|").length() != 97) { return -9; };
    if ("é🦀".split("").length() != 2) { return -10; };
    if ("é🦀".chars().length() != 2) { return -27; };
    if ("".split("").length() != 0) { return -11; };
    if ("a\r\nb\n".lines().length() != 2) { return -12; };
    if (" \u{2003}ΟΣß \n".trim_start().trim_end().trim().to_lower_case().to_upper_case() != "ΟΣSS") { return -13; };
    if ("x|x"._replace_literal("x", "$1")._replace_all_literal("x", "$2") != "$1|$2") { return -14; };
    if ("🦀".repeat(3) != "🦀🦀🦀") { return -15; };
    if (string.from_code_points([233, 129408]) != "é🦀") { return -16; };
    if (int.parse("+42") != 42) { return -17; };
    let error = int.parse("no") catch (e) { baml.errors.ParseError => e.message.length() };
    if (error < 10) { return -18; };
    if (`${true}:${42}` != "true:42") { return -19; };
    let prefix = " ".repeat(80) + "{\"text\":\"";
    let encoded = prefix + suffix + "\"}";
    let decoded = baml.json.decode<StringEnvelope>(encoded);
    if (decoded.text != suffix) { return -20; };
    let wrapped = "x" + encoded + "x";
    let decoded_view = baml.json.decode<StringEnvelope>(wrapped.slice(1, -1));
    if (decoded_view.text != suffix) { return -28; };
    if (!"🦀".includes("🦀")) { return -21; };
    if (!"é🦀".starts_with("é")) { return -22; };
    if (!"é🦀".ends_with("🦀")) { return -23; };
    if (!"".is_numeric()) { return -24; };
    if (!"Σ".is_uppercase()) { return -25; };
    if (!"deadBEEF".is_ascii_hex()) { return -26; };
    let ascii = "a".repeat(80);
    let earlier = ascii;
    for (let i = 0; i < 160; i += 1) { ascii = ascii + "x"; };
    if (earlier != "a".repeat(80) || ascii.length() != 240) { return -29; };
    if (ascii.slice(75, 85) != "aaaaaxxxxx") { return -30; };
    if (ascii != "a".repeat(80) + "x".repeat(160)) { return -31; };
    if ("x".repeat(-3) != "" || "x".repeat(0) != "") { return -32; };
    if ("ab"._replace_all_literal("", "-") != "-a-b-") { return -33; };
    if ("hi".at(99) != null || "hi".code_point_at(-99) != null) { return -34; };
    if ("hi".index_of("z") != null || "hi".last_index_of("z") != null) { return -35; };
    if (!"".is_empty() || "a".is_empty()) { return -36; };
    if (!"é".is_alphabetic() || !"é1".is_alphanumeric()) { return -37; };
    if (!"é".is_lowercase() || !"\u{2003}".is_whitespace()) { return -38; };
    if (!"\n".is_control() || !"é".is_graphic()) { return -39; };
    if (!"A1!".is_ascii() || "é".is_ascii()) { return -40; };
    if (!"123".is_ascii_numeric() || !"Az".is_ascii_alphabetic()) { return -41; };
    if (!"Az1".is_ascii_alphanumeric() || !"AZ".is_ascii_uppercase()) { return -42; };
    if (!"az".is_ascii_lowercase() || !" \t".is_ascii_whitespace()) { return -43; };
    if (!"\n".is_ascii_control() || !"!".is_ascii_graphic()) { return -44; };
    if (ascii.to_code_points().length() != 240) { return -45; };
    return 288;
}
