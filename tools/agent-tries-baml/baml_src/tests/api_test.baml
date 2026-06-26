// Unit tests for the api gateway's pure routing/translation helpers.

testset "api" {
    test "path_segments splits and drops empties" {
        let s = path_segments("/tasks/abc/transition");
        assert.equal(s.length(), 3);
        assert.equal(seg(s, 0), "tasks");
        assert.equal(seg(s, 2), "transition");
    }

    test "seg out of range is empty" {
        assert.equal(seg(path_segments("/healthz"), 5), "");
    }

    test "merge_id inserts id before posted fields" {
        assert
            .equal(
                merge_id("X1", #"{"to":"failed","field":"status"}"#),
                #"{"id":"X1","to":"failed","field":"status"}"#,
            );
    }

    test "merge_id handles empty body" {
        assert.equal(merge_id("X1", "{}"), #"{"id":"X1"}"#);
    }

    test "query_to_json quotes strings and coerces digits" {
        assert
            .equal(
                query_to_json("/tasks?field=status&value=queued&limit=5"),
                #"{"field":"status","value":"queued","limit":5}"#,
            );
    }

    test "query_to_json empty when no query" {
        assert.equal(query_to_json("/tasks"), "{}");
    }

    test "is_digits" {
        assert.is_true(is_digits("12345"));
        assert.is_true(is_digits("0"));
    }

    test "is_digits rejects non-digits and empty" {
        assert.is_true(is_digits("12a") == false);
        assert.is_true(is_digits("") == false);
    }
}
