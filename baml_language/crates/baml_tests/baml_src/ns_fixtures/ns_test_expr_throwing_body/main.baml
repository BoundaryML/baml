function risky() -> void throws string {
  throw "boom"
}

test "throwing body becomes failure" {
  risky()
}
