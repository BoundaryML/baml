// Test that baml.http builtins work correctly

function FetchExample() -> baml.http.Response {
  http.fetch("https://example.com")
}

function SendExample() -> baml.http.Response {
  http.send(baml.http.Request {
    method: "GET",
    url: "https://example.com",
    headers: {},
    body: "",
  })
}

//----
//- diagnostics
// <no-diagnostics-expected>
