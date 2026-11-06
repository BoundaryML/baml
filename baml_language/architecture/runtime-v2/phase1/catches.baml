class Failure { code: int }
function fail() -> int { throw Failure {code: 17}; }
function inner() -> int { fail() catch (e) { Failure => { throw e; } } }
function main() -> int {
  let xs = [1, 2, 3];
  let total = 0;
  for (let n in xs) { total += n; }
  let recovered = inner() catch (e) { Failure => e.code };
  total + recovered
}
