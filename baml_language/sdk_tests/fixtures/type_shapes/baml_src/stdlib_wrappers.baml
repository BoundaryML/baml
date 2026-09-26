// User types that wrap handle-backed stdlib types. Generated SDKs do not
// expose stdlib functions or methods, so hosts reach these handles only
// through user declarations: the host receives a user class with the stdlib
// value embedded, passes it back, and the engine must see the original handle.

class HttpExchange {
  label string
  response baml.http.Response
}

function make_http_exchange(label: string, body: string) -> HttpExchange {
  HttpExchange {
    label: label,
    response: baml.http.Response.new(200, { "x-label": label }, body.to_utf8()),
  }
}

function fetch_http_exchange(url: string) -> HttpExchange throws baml.errors.Io | baml.errors.Timeout {
  HttpExchange { label: url, response: baml.http.fetch(url) }
}

function http_exchange_text(exchange: HttpExchange) -> string throws baml.errors.Io | baml.errors.Timeout {
  exchange.response.text()
}

class OpenFile {
  path string
  file baml.fs.File
}

function open_read_only(path: string) -> OpenFile throws baml.errors.Io | baml.errors.InvalidArgument {
  OpenFile { path: path, file: baml.fs.open(path, "r") }
}

function seek_open_file(open_file: OpenFile, whence: "start" | "current" | "end", offset: int) -> int throws baml.errors.Io | baml.errors.InvalidArgument {
  open_file.file.seek_from(whence, offset)
}

function read_open_file(open_file: OpenFile) -> string throws baml.errors.Io | baml.errors.ParseError {
  open_file.file.text()
}

function close_open_file(open_file: OpenFile) -> null throws baml.errors.Io {
  open_file.file.close()
}
