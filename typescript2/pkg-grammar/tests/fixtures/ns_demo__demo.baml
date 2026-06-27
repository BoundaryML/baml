// Namespace root.demo — a minimal BAML function for the Python-interop demo.
// Python calls this through the generated `baml_sdk` package (see
// python_demo/hello_baml.py); regenerate with `baml generate` after editing.

class Greeting {
    message: string,
    letters: int,

    // Factory (no self) — construct a Greeting from a raw name.
    function new(name: string) -> Greeting {
        let clean = name.trim();
        Greeting { message: "hello, " + clean + "!", letters: clean.length() }
    }

    // Instance method — derive a new value from self.
    function shout(self) -> string {
        self.message.to_upper_case()
    }

    // Instance method returning another Greeting.
    function repeated(self, times: int) -> Greeting {
        let parts: string[] = [];
        let i = 0;
        while (i < times) {
            parts.push(self.message);
            i = i + 1;
        }
        Greeting { message: parts.join(" "), letters: self.letters * times }
    }
}

function greet(name: string = "vaibhav") -> Greeting {
    let clean = name.trim();
    Greeting { message: "hello, " + clean + "!", letters: clean.length() }
}

// The wire shape the ns_server demo responds with.
class ServerReply {
    hello: string,
    conn: int,
}

// Curl the ns_server demo with BAML's own HTTP client, decoding the JSON
// body into a typed ServerReply. (python_demo/server_roundtrip.py runs server.serve
// in a Python thread, then awaits this concurrently via fetch_reply_async.)
function fetch_reply(url: string) -> ServerReply {
    let res = baml.http.fetch(url);
    if (!res.ok()) {
        throw "HTTP " + baml.unstable.string(res.status_code) + " from " + url;
    };
    baml.json.from_string<ServerReply>(res.text())
}


function retry<T>(times: int, fn: () -> T) -> T {
    for (let i = 0; i < times; i += 1) {
        let result = fn();
        return result;
    }
    throw "Failed to get a result " + baml.unstable.string(times) + " attempts";
}

function foo() -> int {
    throw 5
}

function demo() -> void {
    retry(3, foo);
}