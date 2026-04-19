// Test that baml.http builtins work correctly

function FetchExample() -> baml.http.Response {
  baml.http.fetch("https://example.com")
}

function SendExample() -> baml.http.Response {
  baml.http.send(baml.http.Request {
    method: "GET",
    url: "https://example.com",
    headers: {},
    body: "",
  })
}

//----
//- diagnostics
// <no-diagnostics-expected>
