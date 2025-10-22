class Bar {
    a int
    b string
}

function Foo() -> Bar {
  let x = Bar { a: "hello", c: 12 };
  let req_bad = baml.Request {
    base_url: 10,
    headers: {},
    query_params: { a 10 },
  };
  let req_good = baml.Request {
    base_url: "https://example.com",
    headers: { Authorization "Bearer mytoken" },
    query_params: { foo "bar" },
  };
  x
}