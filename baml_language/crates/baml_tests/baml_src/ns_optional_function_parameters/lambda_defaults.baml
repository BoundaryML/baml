// A lambda written as a parameter default lives in the defaults' own arena,
// apart from the function body's. Its expressions are typed there, so
// lowering must read them there too: each function below gives its body
// expressions that would answer differently if a default's lambda were read
// against the body.

class OfpAccount {
    balance: int,

    function peek(self) -> int { self.balance }
    function overdrawn(self) -> int { -1 }
}

// The lambda's own bindings and arithmetic.
function ofp_lambda_default_locals(
    x: int,
    f: (int) -> int throws never = (q: int) -> int { let w = q + 1; w * 2 },
) -> int {
    f(x)
}

// A method call in the lambda, beside a different one in the body.
function ofp_lambda_default_method(
    account: OfpAccount,
    f: (OfpAccount) -> int throws never = (a: OfpAccount) -> int { a.peek() },
) -> int {
    let unrelated = account.overdrawn();
    f(account) - unrelated - 1
}

// Integer arithmetic in the lambda, beside float arithmetic in the body.
function ofp_lambda_default_arithmetic(
    x: int,
    f: (int) -> int throws never = (q: int) -> int { q + 3 },
) -> int {
    let half = 1.5;
    let whole = half + 2.5;
    if (whole > 3.5) { f(x) } else { 0 }
}

//baml:tagged_string
function ofp_values(body: () -> baml.TaggedString) -> unknown[] {
    body().values
}

// A tagged template in the lambda, with a lambda of its own inside.
function ofp_lambda_default_tagged(
    x: int,
    f: (int) -> unknown[] throws never = (q: int) -> unknown[] {
        ofp_values`a${q + 1}b${ (z: int) -> int { z * 2 } }`
    },
) -> int {
    f(x).length()
}

class OfpScaler {
    function scale(
        self,
        x: int,
        f: (int) -> int throws never = (q: int) -> int { let w = q * 10; w + 1 },
    ) -> int {
        f(x)
    }
}

test "a lambda in a parameter default is typed with the defaults" {
    assert.equal(ofp_lambda_default_locals(1), 4);
    assert.equal(ofp_lambda_default_method(OfpAccount { balance: 7 }), 7);
    assert.equal(ofp_lambda_default_arithmetic(4), 7);
    assert.equal(ofp_lambda_default_tagged(1), 2);
    assert.equal(OfpScaler {}.scale(2), 21);
    assert.equal(ofp_lambda_default_locals(1, f = (q: int) -> int { q }), 1)
}
