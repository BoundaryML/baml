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
  assert.equal(assert.is_type<int>(3), 3);
  assert.equal(assert.is_type<Detail>(Detail {code: 5, text: "type"}).code, 5);
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
  n += { assert.is_type<Detail>("wrong"); 0 } catch_all (e) { baml.panics.AssertionFailed {message} => message.length(), _ => 0 };
  n += { baml.sys.panic("user panic"); 0 } catch_all (e) { baml.panics.UserPanic {message} => message.length(), _ => 0 };
  n += { baml.sys.exit(9); 0 } catch_all (e) { baml.panics.Exit {code} => code, _ => 0 };
  n
}
function uncaught() -> int { fail() }
function assertion_failure() -> int { assert.equal("one", "two"); 0 }

function context_leaf() -> int { fail() }
function context_middle() -> int {
  context_leaf() catch (e, ctx) {
    let f: Failure => {
      assert.equal(ctx.error, f);
      assert.equal(ctx.cause, null);
      assert.equal(ctx.root_cause().error, f);
      let text = ctx.to_string();
      assert.contains(text, "user.context_middle");
      assert.contains(text, "user.context_leaf");
      assert.contains(text, "user.fail");
      text.length()
    }
  }
}
function contexts() -> int { context_middle() }

function chain_leaf() -> int { throw baml.errors.InvalidArgument {message: "second"} }
function chain_middle() -> int {
  fail() catch (e, ctx) {
    Failure => {
      chain_leaf() catch (other, next) {
        baml.errors.InvalidArgument => {
          assert.equal(next.cause, ctx);
          assert.equal(next.root_cause(), ctx);
          assert.contains(next.to_string(), "During handling of the above error, another error occurred:");
          next.to_string().length()
        }
      }
    }
  }
}
function chains() -> int { chain_middle() }

function scalar_leaf() -> int { throw "kaboom" }
function scalar_middle() -> int {
  scalar_leaf() catch (e, ctx) { _ => {
    assert.equal(e, "kaboom");
    assert.contains(ctx.to_string(), "\nkaboom");
    ctx.to_string().length()
  } }
}
function scalars() -> int { scalar_middle() }
