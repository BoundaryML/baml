// Unit tests for the /linear/webhook routing logic (ingress.baml) — the
// resting-state gate + label-id extraction that decide whether a Linear card move
// flips the Convex issue lifecycle. The label-id->status mapping itself reads env
// (LINEAR_STATUS_*), so it's exercised live; here we pin the env-free pieces.

testset "linear_webhook" {
    // ---- resting-state gate: only honor a move from a valid resting status ----
    test "approve_from honors resting states" {
        assert.is_true(approve_from("open"));
        assert.is_true(approve_from("confirmed"));
        assert.is_true(approve_from("needs_human"));
        assert.is_true(approve_from("redraft"));
        assert.is_true(approve_from("failed"));
    }

    test "approve_from rejects in-flight / terminal states (echo guard)" {
        assert.is_true(approve_from("dispatching") == false);
        assert.is_true(approve_from("tocursor") == false);
        assert.is_true(approve_from("prprep") == false);
        assert.is_true(approve_from("closed") == false);
    }

    test "redraft_from honors its resting states" {
        assert.is_true(redraft_from("approved"));
        assert.is_true(redraft_from("needs_human"));
        assert.is_true(redraft_from("open"));
        assert.is_true(redraft_from("tocursor") == false);
    }

    // ---- label-id extraction: explicit labelIds, else the labels[] fallback ----
    test "webhook_label_ids prefers explicit labelIds" {
        let d = LinearWebhookData { id: "lin-1", labelIds: ["a", "b"], labels: null };
        let ids = webhook_label_ids(d);
        assert.equal(ids.length(), 2);
        assert.is_true(in_list(ids, "a"));
        assert.is_true(in_list(ids, "b"));
    }

    test "webhook_label_ids falls back to the labels array" {
        let d = LinearWebhookData {
            id: "lin-1",
            labelIds: null,
            labels: [LinearLabelRef { id: "x" }, LinearLabelRef { id: "y" }],
        };
        let ids = webhook_label_ids(d);
        assert.equal(ids.length(), 2);
        assert.is_true(in_list(ids, "x"));
        assert.is_true(in_list(ids, "y"));
    }

    test "webhook_label_ids on an empty event is empty" {
        let d = LinearWebhookData { id: "lin-1", labelIds: null, labels: null };
        assert.equal(webhook_label_ids(d).length(), 0);
    }

    // ---- a non-status-group label is a no-op (route returns "") ----
    test "route_linear_status: an unrelated label does nothing" {
        let issue = WebhookIssue { _id: "i1", status: "open" };
        assert.equal(route_linear_status(issue, ["some-random-label-id"]), "");
    }
}
