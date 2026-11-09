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

function exit() -> int { baml.sys.exit(7); 0 }
function exit_zero() -> int { baml.sys.exit(0); 0 }
function exit_large() -> int { baml.sys.exit(2147483648); 0 }
function exit_negative() -> int { baml.sys.exit(-2147483649); 0 }

function handles() -> int {
  let first = baml.regex.new("a");
  let alias = first;
  assert.equal(first, alias);
  assert.is_true(first != baml.regex.new("a"));
  1
}

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

function throw_caught() -> int {
  fail() catch (e) { let f: Failure => {
    f.message = "changed";
    f.count = 23;
    throw f
  } }
}

enum Marker { One, Two }
enum OtherMarker { One }
function enums() -> int {
  assert.is_true(Marker.One == Marker.One);
  assert.is_true(Marker.One != Marker.Two);
  assert.equal(Marker.One, Marker.One);
  let erased: unknown = Marker.One;
  assert.is_true(erased != OtherMarker.One);
  { throw Marker.One; 0 } catch (e) { Marker => 1 }
}
function enum_throw() -> int { throw Marker.Two }


class RenderLink { next: RenderLink? }
function cycle_text() -> string {
    let node = RenderLink { next: null };
    node.next = node;
    { throw node; "" } catch (e, ctx) { _ => ctx.to_string() }
}
function deep_text() -> string {
    let head = RenderLink { next: null };
    let cursor = head;
    for (let i = 0; i < 40; i += 1) {
        let next = RenderLink { next: null };
        cursor.next = next;
        cursor = next;
    }
    { throw head; "" } catch (e, ctx) { _ => ctx.to_string() }
}
function wide_values() -> int[] {
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283, 284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297, 298, 299]
}
function wide_text() -> string {
    { throw wide_values(); "" } catch (e, ctx) { _ => ctx.to_string() }
}
function diagnostics() -> int {
    let cyclic = cycle_text();
    assert.contains(cyclic, "user.RenderLink { next: … }");
    let deep = deep_text();
    assert.contains(deep, "…");
    let wide = wide_text();
    assert.equal(wide.split("…").length(), 2);
    assert.is_true(wide.length() < 2000);
    cyclic.length() + deep.length() + wide.length()
}
function wide_throw() -> int { throw wide_values() }

function selectors() -> int {
    assert.is_true(testing._glob_match("root.ns::héllo", "root*héllo"));
    assert.is_true(testing._glob_match("root.ns::test", "ns::"));
    assert.is_true(testing._glob_match("root.ns::test", "**::te**st"));
    assert.is_true(!testing._glob_match("root.ns::test", "root*ns"));
    assert.is_true(!testing._glob_match("aa", "aa*aa"));
    assert.is_true(testing._any_pattern_matches(["absent", "test"], "root::test"));
    assert.is_true(testing._leaf_selected("root::test", [], []));
    assert.is_true(!testing._leaf_selected("root::test", ["test"], ["test"]));
    assert.is_true(testing._leaf_selected_layered("root.ns::test", ["root*"], [], ["test"], []));
    assert.is_true(!testing._leaf_selected_layered("root.ns::test", ["other*"], [], ["test"], []));
    assert.is_true(testing._subtree_excluded("root.ns", ["root.ns::*"]));
    assert.is_true(!testing._subtree_excluded("root.ns", ["root.ns::one"]));
    assert.is_true(testing._pattern_may_match_descendant("root.ns::*", "root.ns"));
    assert.is_true(!testing._pattern_may_match_descendant("elsewhere::*", "root.ns"));
    assert.is_true(testing._subtree_may_be_selected("root.ns", ["*test"], []));
    assert.is_true(!testing._subtree_may_be_selected("root.ns", [], ["root.ns::*"]));
    16
}

function float_throw() -> int { throw 1.0e-7 }
function bytes_throw() -> int { throw "héllo".to_utf8() }
function null_throw() -> int { throw null }

function positive_infinity_throw() -> int { throw (1.0 / 0.0) }
// Float negation lives in the arithmetic job; subtraction keeps this probe local.
function negative_infinity_throw() -> int { throw ((0.0 - 1.0) / 0.0) }
function nan_throw() -> int { throw (0.0 / 0.0) }
function approx_nan_epsilon() -> int { assert.approx_equal(1.0, 1.0, 0.0 / 0.0); 0 }
function approx_nan_delta() -> int { assert.approx_equal(0.0 / 0.0, 1.0, 0.1); 0 }
