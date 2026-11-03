// Recursive aliases through tuples: `null` ends a chain of required pairs,
// so `(1, (2, null))` is a finite `Chain`.

type TpChain = (int, TpChain?)

type TpChainOrNull = (int, TpChainOrNull) | null

function tp_chain() -> TpChain {
    (1, (2, null))
}

function tp_chain_sum(chain: TpChain?) -> int {
    if (chain == null) {
        0
    } else {
        chain[0] + tp_chain_sum(chain[1])
    }
}

// A match on the optional alias sees through it to the tuple arm.
function tp_chain_match_sum(chain: TpChain?) -> int {
    match (chain) {
        null => 0,
        (let head, let rest) => head + tp_chain_match_sum(rest),
    }
}

// `chain[1]` is the optional alias `TpChain?`: `?.[` sees through it to the
// tuple.
function tp_chain_second(chain: TpChain) -> int? {
    chain[1]?.[0]
}

function tp_chain_or_null_len(chain: TpChainOrNull) -> int {
    match (chain) {
        null => 0,
        (_, let rest) => 1 + tp_chain_or_null_len(rest),
    }
}

test "tuple_recursive_alias_chain" {
    let chain = tp_chain();
    assert.equal(chain[0], 1)
    assert.equal(tp_chain_sum(chain), 3)
    assert.equal(tp_chain_match_sum(chain), 3)
    assert.equal(tp_chain_second(chain), 2)
    assert.equal(tp_chain_second((1, null)), null)
    assert.equal(tp_chain_or_null_len((1, (2, (3, null)))), 3)
    assert.equal(tp_chain_or_null_len(null), 0)
}
