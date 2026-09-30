// User declarations that call stdlib functions. Generated SDKs do not expose
// stdlib functions directly, so host tests reach stdlib sysops (HTTP, files,
// environment) through user functions like these.

class FetchedText {
  status_code int
  headers map<string, string>
  text string
}

function fetched_text(response: baml.http.Response) -> FetchedText throws baml.errors.Io | baml.errors.Timeout {
  FetchedText {
    status_code: response.status_code,
    headers: response.headers,
    text: response.text(),
  }
}

function fetch_text(url: string) -> FetchedText throws baml.errors.Io | baml.errors.Timeout {
  fetched_text(baml.http.fetch(url))
}

// Reads one response body twice; the second read must fail because the body
// was already consumed.
function fetch_text_then_bytes(url: string) -> uint8array throws baml.errors.Io | baml.errors.Timeout {
  let response = baml.http.fetch(url);
  let _text = response.text();
  response.bytes()
}

function fetch_text_with_timeout(url: string, timeout_nanos: bigint) -> FetchedText throws baml.errors.Io | baml.errors.Timeout {
  fetched_text(baml.http.fetch(url, timeout = baml.time.Duration.from_nanoseconds(timeout_nanos)))
}

function send_for_bytes(method: string, url: string, headers: map<string, string>, body: string) -> uint8array throws baml.errors.Io | baml.errors.Timeout | baml.errors.InvalidArgument {
  baml.http.send(baml.http.Request { method: method, url: url, headers: headers, body: body }).bytes()
}

function open_sse(url: string) -> null throws baml.errors.Io | baml.errors.Timeout | baml.errors.InvalidArgument {
  let _stream = baml.http.fetch_sse(baml.http.Request { method: "GET", url: url, headers: {}, body: "" });
  null
}

function read_text_file(path: string) -> string throws baml.errors.Io | baml.errors.ParseError {
  baml.fs.read(path)
}

function path_exists(path: string) -> bool throws baml.errors.Io {
  baml.fs.exists(path)
}

function open_and_read(path: string) -> string throws baml.errors.Io | baml.errors.ParseError | baml.errors.InvalidArgument {
  baml.fs.open(path, "r").text()
}

function env_var(key: string) -> string? throws baml.errors.Io | baml.errors.ParseError {
  baml.env.get(key)
}
