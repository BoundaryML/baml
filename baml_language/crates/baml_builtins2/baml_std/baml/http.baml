class Request {
  method string
  url string
  headers map<string, string>
  body string
}

class Response {
  status_code int
  headers map<string, string>
  url string
  _body $rust_type

  function text(self) -> string throws root.errors.Io {
    $rust_io_function
  }

  function ok(self) -> bool {
    self.status_code >= 200 && self.status_code < 300
  }
}

function fetch(url: string) -> Response throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

function send(request: Request) -> Response throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}
