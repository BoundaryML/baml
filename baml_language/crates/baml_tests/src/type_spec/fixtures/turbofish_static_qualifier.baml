// generic-applied type qualifiers resolve statics and the from_json desugar
class TfBox<T> {
    value: T
}
function tf_from_json() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"value\": 42}")
    let b: TfBox<int> = TfBox<int>.from_json(j)
     // ^ user.TfBox<int>
    b.value
}
