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

  function text(self) -> string {
    $rust_io_function
  }

  function ok(self) -> bool {
    self.status_code >= 200 && self.status_code < 300
  }
}
