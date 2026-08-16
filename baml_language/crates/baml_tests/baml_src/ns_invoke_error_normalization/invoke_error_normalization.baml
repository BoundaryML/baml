function normalized_kind(error: unknown) -> string {
    match (ai.errors.normalize_invoke(error)) {
        let cancelled: baml.panics.Cancelled => `Cancelled:${cancelled.message}`,
        let network: ai.errors.NetworkFailure => `NetworkFailure:${network.detail}`,
        let unknown_error: baml.errors.UnknownError => {
            `UnknownError:${unknown_error.message.join("|")}`
        },
        _ => "other",
    }
}

test "normalize_invoke_preserves_cancelled_identity" {
    let original = baml.panics.Cancelled { message: "before" };
    let normalized = ai.errors.normalize_invoke(original);
    original.message = "after";
    assert.equal(normalized_kind(normalized), "Cancelled:after")
}

test "normalize_invoke_preserves_failure_identity" {
    let original = ai.errors.NetworkFailure { provider: "test", detail: "before", raw_body: null };
    let normalized = ai.errors.normalize_invoke(original);
    original.detail = "after";
    assert.equal(normalized_kind(normalized), "NetworkFailure:after")
}

test "normalize_invoke_preserves_unknown_error_identity" {
    let original = baml.errors.UnknownError { data: "payload", message: ["before"] };
    let normalized = ai.errors.normalize_invoke(original);
    original.message = ["after"];
    assert.equal(normalized_kind(normalized), "UnknownError:after")
}

test "normalize_invoke_wraps_other_unknown_values" {
    let original = baml.errors.InvalidArgument { message: "invalid" };
    let normalized = ai.errors.normalize_invoke(original);
    match (normalized) {
        let unknown_error: baml.errors.UnknownError => {
            assert.equal(unknown_error.message, [original.to_string()]);
            match (unknown_error.data) {
                let preserved: baml.errors.InvalidArgument => {
                    assert.equal(preserved.message, "invalid")
                },
                _ => assert.is_true(false),
            }
        },
        _ => assert.is_true(false),
    }
}
