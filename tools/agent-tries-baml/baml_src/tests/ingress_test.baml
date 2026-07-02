// Ingress parity tests: directive grammar, changelog tag classification,
// signature plumbing, and the small pure helpers behind the endpoints.

testset "directives" {
    test "plain prompt has no directives" {
        let d = parse_run_directives("build a recipe parser");
        assert.equal(d.cleaned, "build a recipe parser");
        assert.equal(d.baml_channel, "");
        assert.equal(d.baml_pin, "");
        assert.is_true(d.cold_start == false);
        assert.equal(d.arena_branches.length(), 0);
        assert.is_true(has_bench_directives("build a recipe parser") == false);
    }

    test "channel + pin + arena parse and strip" {
        let d = parse_run_directives("fix the [Canary] parser [baml=2] please [skill arena: main, exp-a]");
        assert.equal(d.cleaned, "fix the parser please");
        assert.equal(d.baml_channel, "canary");
        assert.equal(d.baml_pin, "2");
        assert.equal(d.arena_branches.length(), 2);
        assert.is_true(has_bench_directives("[nightly] x"));
    }

    test "cold start wins over a pin" {
        let d = parse_run_directives("try [coldstart] with [baml=3]");
        assert.is_true(d.cold_start);
        assert.equal(d.baml_pin, "");
        assert.equal(d.cleaned, "try with");
    }

    test "cold alias and whitespace tolerance" {
        let d = parse_run_directives("run [ cold ] and [ NIGHTLY ] now");
        assert.is_true(d.cold_start);
        assert.equal(d.baml_channel, "nightly");
    }

    test "non-directive brackets are kept verbatim" {
        let d = parse_run_directives("parse [1, 2, 3] into a list");
        assert.equal(d.cleaned, "parse [1, 2, 3] into a list");
        assert.is_true(has_bench_directives("parse [1, 2, 3] into a list") == false);
    }

    test "bare skill arena uses the default branches" {
        let d = parse_run_directives("compare skills [skill arena]");
        assert.is_true(d.arena_branches.length() > 0);
    }

    test "parse_branches trims and dedups" {
        let b = parse_branches(" main , exp-a ,main,, ");
        assert.equal(b.length(), 2);
        assert.equal(b.join(","), "main,exp-a");
    }
}

testset "changelog_tags" {
    test "channel_of classifies the live channels" {
        assert.equal(channel_of("baml-language-0.12.2-nightly.20260624.a"), "nightly");
        assert.equal(channel_of("baml-language-0.13.0"), "canary");
        assert.equal(channel_of("baml-language-1.2.3-alpha.4"), "alpha");
        assert.equal(channel_of("v1.2.3"), "engine");
        assert.equal(channel_of("1.2.3"), "engine");
    }

    test "channel_of rejects unrelated tags" {
        assert.equal(channel_of("baml-wrapper-1.0.0"), "");
        assert.equal(channel_of("nonsense"), "");
        assert.equal(channel_of("baml-language-not-a-version"), "");
    }

    test "normalize_tag strips prefixes" {
        assert.equal(normalize_tag("baml-language-0.13.0"), "0.13.0");
        assert.equal(normalize_tag("baml-language-0.12.2-nightly.20260624.a"), "0.12.2-nightly.20260624.a");
        assert.equal(normalize_tag("v1.2.3"), "1.2.3");
        assert.equal(normalize_tag("1.2.3"), "1.2.3");
    }

    test "entry sort key orders by date then createdAt" {
        let older = EntryRow {
            _id: null,
            version: null,
            date: "2026-06-01",
            title: null,
            body: null,
            authors: null,
            channel: null,
            status: null,
            createdAt: 100.0,
        };
        let newer = EntryRow {
            _id: null,
            version: null,
            date: "2026-06-02",
            title: null,
            body: null,
            authors: null,
            channel: null,
            status: null,
            createdAt: 50.0,
        };
        assert.is_true(entry_sort_key(newer) > entry_sort_key(older));
    }
}

testset "webhook_auth" {
    test "hmac_sha256_hex matches the reference vector" {
        // python: hmac.new(b"mysecret", b"hello world", sha256).hexdigest()
        assert.equal(
            hmac_sha256_hex("mysecret", "hello world"),
            "5dc9f8d0806f528337ec17cfa299ccac8b5b5eedf6e8a8926aabeccc0233870d",
        );
    }

    test "digest_eq is exact and rejects empties" {
        assert.is_true(digest_eq("abc", "abc"));
        assert.is_true(digest_eq("abc", "abd") == false);
        assert.is_true(digest_eq("abc", "abcd") == false);
        assert.is_true(digest_eq("", "") == false);
    }

    test "sanitize_key strips path characters" {
        assert.equal(sanitize_key("Ev-123_ABC/../x"), "ev-123_abcx");
    }
}

testset "ingress_helpers" {
    test "collapse_ws unifies whitespace" {
        assert.equal(collapse_ws("  a\n b\tc  "), "a b c");
    }

    test "is_all_digits handles the split boundary quirk" {
        assert.is_true(is_all_digits("123"));
        assert.is_true(is_all_digits("1a3") == false);
        assert.is_true(is_all_digits("") == false);
    }

    test "oversized feedback body is rejected before any state change" {
        let big = "x";
        let body = big.repeat(4194305);
        let req = baml.http.Request {
            method: "POST",
            url: "/feedback",
            headers: {},
            body: body,
        };
        let resp = handle_feedback(req);
        assert.equal(resp.status_code, 413);
    }
}

testset "evidence_links" {
    test "evidence entries become dashboard run links" {
        let ev: json[] = [
            baml.json.parse("{\"trophyId\":\"t1\",\"turn_index\":3.0}") catch (_) {
                _ => 0
            },
            baml.json.parse("{\"trophyId\":\"t2\"}") catch (_) {
                _ => 0
            },
            baml.json.parse("{\"note\":\"no trophy\"}") catch (_) {
                _ => 0
            },
        ];
        let links = evidence_links_of(ev);
        assert.equal(links.length(), 2);
        let first = match (links.at(0)) {
            let v: string => v,
            null => "",
        };
        let second = match (links.at(1)) {
            let v: string => v,
            null => "",
        };
        assert.is_true(first.ends_with("/runs/t1#turn-3"));
        assert.is_true(second.ends_with("/runs/t2"));
    }

    test "null evidence yields no links" {
        assert.equal(evidence_links_of(null).length(), 0);
    }
}

testset "gql_envelope" {
    test "data containing the word errors is not an error" {
        assert.is_true(gql_has_errors("{\"data\":{\"issue\":{\"title\":\"fix \\\"errors\\\" handling\"}}}") == false);
    }

    test "a real errors array throws-worthy" {
        assert.is_true(gql_has_errors("{\"errors\":[{\"message\":\"bad\"}],\"data\":null}"));
    }

    test "empty errors array is success" {
        assert.is_true(gql_has_errors("{\"errors\":[],\"data\":{}}") == false);
    }

    test "mutation_ok reads the success flag" {
        assert.is_true(mutation_ok("{\"data\":{\"issueUpdate\":{\"success\":true}}}"));
        assert.is_true(mutation_ok("{\"data\":{\"issueUpdate\":{\"success\":false}}}") == false);
        assert.is_true(mutation_ok("{\"data\":{}}") == false);
    }
}
