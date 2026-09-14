// Testset with setup code — let bindings should flow into nested tests.

testset "with shared setup" {
  let base_url = "https://api.example.com"
  let timeout = 5000

  test "uses setup vars" {
    assert.not_null(base_url)
    assert.equal(timeout, 5000)
  }

  test "also uses setup" {
    assert.contains(base_url, "example")
  }
}
