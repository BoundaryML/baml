function nap() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(200n)) catch (e) { let e => 0 };
  1
}
function main() -> int {
  let a = spawn { nap() };
  let b = spawn { nap() };
  let c = spawn { nap() };
  (await a) + (await b) + (await c)
}
