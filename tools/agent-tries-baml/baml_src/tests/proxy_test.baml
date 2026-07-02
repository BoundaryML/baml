// Unit tests for the proxy's agent-baml version resolution (proxy.baml) — parsing
// the concrete version out of `baml toolchain install` output so the agent-under-
// test runs the freshly-installed nightly and the trophy records exactly which one.

testset "proxy_baml_version" {
    // ---- parse_channel_version: the `installed BAML toolchain <v>` line ----
    test "parses the install line" {
        assert.equal(
            parse_channel_version("installed BAML toolchain 0.13.1-nightly.20260629.b\n", "nightly"),
            "0.13.1-nightly.20260629.b",
        );
    }

    test "parses the install line amid other output" {
        let out = #"
            downloading...
            installed BAML toolchain 0.13.0
            done
        "#;
        assert.equal(parse_channel_version(out, "canary"), "0.13.0");
    }

    // ---- fallback: a status-style `latest <channel>: <v>` line ----
    test "falls back to the latest-<channel> line" {
        let out = "active selector: 0.12.2-nightly.20260624.a\nlatest nightly: 0.13.1-nightly.20260629.b\n";
        assert.equal(parse_channel_version(out, "nightly"), "0.13.1-nightly.20260629.b");
    }

    test "empty output resolves to empty" {
        assert.equal(parse_channel_version("", "nightly"), "");
    }
}
