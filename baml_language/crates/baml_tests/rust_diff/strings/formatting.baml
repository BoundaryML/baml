class Label {
    value: string
    implements baml.ToString {
        function to_string(self) -> string { `label(${self.value})` }
    }
}
class Record { name: string values: int[] item: Label }
class DefaultRecord {
    value: string
    implements baml.ToString {}
}
enum Tone { Red Blue }
function format_record(value: Record) -> string { `${value}` }
function format_unknown(value: unknown) -> string { string.from(value) }
function main() -> int {
    let value = Record { name: "é\n", values: [1, 2], item: Label { value: "🦀" } };
    if (format_record(value) != "Record { name: \"é\\n\", values: [1, 2], item: label(🦀) }") { return -1; };
    if (string.from(["a", "é"]) != "[\"a\", \"é\"]") { return -2; };
    if (string.from(Tone.Blue) != "Blue" || string.from(Tone.Red) != "Red") { return -3; };
    let map: map<string, int> = {};
    map.set("x", 3); map.set("y", 4);
    if (string.from(map) != "{\"x\": 3, \"y\": 4}") { return -4; };
    if (format_unknown(value) != format_record(value)) { return -5; };
    if (format_unknown(42) != "42" || format_unknown(1.25) != "1.25") { return -6; };
    if (format_unknown(true) != "true" || format_unknown(null) != "null") { return -7; };
    if (format_unknown(Tone.Red) != "Red") { return -8; };
    if (format_unknown(DefaultRecord { value: "x" }) != "DefaultRecord { value: \"x\" }") { return -9; };
    return 4;
}
