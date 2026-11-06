class Failure { code: int }
function fail() -> int { throw Failure {code: 17}; }
function inner() -> int { fail() catch (e) { Failure => { throw e; } } }
function main() -> int { inner() }
