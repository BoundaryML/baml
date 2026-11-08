class Detail { code: int, text: string }
class Failure { message: string, detail: Detail, flag: bool, count: int }
function overflow() -> int { int.max_value() + 1 }
function negative_shift() -> int { 1 << -1 }
function bad_index() -> int { let xs = [1]; xs[3] }
function fail() -> int {
  throw Failure {message: "bad", detail: Detail {code: 7, text: "detail"}, flag: true, count: 9}
}
function rethrow() -> int {
  fail() catch (e) { baml.errors.InvalidArgument => 0 }
}
function main() -> int {
  assert.is_true(true);
  assert.not_null(3);
  assert.equal(4, 4);
  assert.equal("héllo", "héllo");
  assert.equal(true, true);
  assert.equal(null, null);
  assert.equal([1, 2], [1, 2]);
  assert.equal(Detail {code: 2, text: "x"}, Detail {code: 2, text: "x"});
  assert.approx_equal(1.0, 1.0001, 0.001);
  assert.contains("héllo", "é");
  let n = overflow() catch_all (e) { baml.panics.IntegerOverflow {message} => message.length(), _ => 0 };
  n += negative_shift() catch_all (e) { baml.panics.NegativeBitShift {message} => message.length(), _ => 0 };
  n += bad_index() catch_all (e) { baml.panics.IndexOutOfBounds {index, length} => index + length, _ => 0 };
  n += rethrow() catch (e) { let f: Failure => { assert.equal(f.message, "bad"); assert.is_true(f.flag); f.detail.code + f.count } };
  n += { assert.is_true(false); 0 } catch_all (e) { baml.panics.AssertionFailed {message} => message.length(), _ => 0 };
  n += { assert.equal(1, 2); 0 } catch_all (e) { baml.panics.AssertionFailed {message} => message.length(), _ => 0 };
  n
}
function uncaught() -> int { fail() }
function assertion_failure() -> int { assert.equal("one", "two"); 0 }
