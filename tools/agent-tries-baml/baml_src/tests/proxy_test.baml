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

// cleanup_staging only ever removes a single path segment directly under
// STAGING_ROOT — anything else is a guarded no-op.
testset "staging_cleanup" {
    test "guards refuse foreign, nested, and root paths" {
        // These must all no-op (return 0 without touching the fs).
        assert.equal(cleanup_staging("/etc", null), 0);
        assert.equal(cleanup_staging("/tmp/staging-evil/x", null), 0);
        assert.equal(cleanup_staging(staging_root() + "/a/../b", null), 0);
        assert.equal(cleanup_staging(staging_root() + "/", null), 0);
        assert.equal(cleanup_staging(staging_root() + "/nested/child", null), 0);
    }

    test "a real cell dir under the root is removed" {
        let dir = staging_root() + "/cleanup-test-cell";
        let _ = baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true }) catch (_) {
            _ => null
        };
        let _ = baml.fs.write(dir + "/trophy.json", "{}") catch (_) {
            _ => 0
        };
        let _ = cleanup_staging(dir, null);
        let gone = baml.fs.exists(dir) catch (_) {
            _ => true
        };
        assert.is_true(gone == false);
    }

    test "skill install detection reads .claude/skills" {
        let dir = staging_root() + "/skilldetect-test";
        let _ = baml.fs.mkdir(dir + "/.claude/skills/baml-core", baml.fs.MkdirOptions { recursive: true }) catch (_) {
            _ => null
        };
        assert.is_true(match (detect_skill_install(dir)) {
            let b: bool => b,
            null => false,
        });
        let _ = cleanup_staging(dir, null);
    }
}
