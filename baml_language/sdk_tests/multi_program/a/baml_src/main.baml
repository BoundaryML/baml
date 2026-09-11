class Result {
  value int
  function read(self) -> int { self.value }
}
function value() -> int { 11 }
function result() -> Result { Result { value: 11 } }
function roundtrip(value: Result) -> Result { value }
function closure() -> () -> int throws never { () => { 11 } }
function callback(cb: (Result) -> Result throws never) -> Result { cb(result()) }

function stream() -> ai.stream.Stream<int, int> {
    let sent = false;
    ai.stream.Stream<int, int> {
        _next_text: () -> {
            if (sent) { return null; }
            sent = true;
            "11"
        },
        _cache: baml.sap._new_parse_cache<int, int>(),
        _text: "",
        _finished: false,
    }
}
