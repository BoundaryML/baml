// Passive channel freshness checks, caching, and retry throttling.

function cached_newer_channel_emits_update_warning(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    let warning = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\n";
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.manage(candidate, ["toolchain", "status"]);
    fixture.check(candidate, "0.11.0", expected_stderr = warning);
    let requests = fixture.requests().length();
    require(requests == 1, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = warning);
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U01-cached-outdated" {
    configured_case("U01-cached-outdated", cached_newer_channel_emits_update_warning);
}

function cached_current_channel_emits_no_warning(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    fixture.manage(candidate, ["toolchain", "status"]);
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    let requests = fixture.requests().length();
    require(requests == 1, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U02-cached-current" {
    configured_case("U02-cached-current", cached_current_channel_emits_no_warning);
}

function passive_refresh_warns_and_is_throttled(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    let warning = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\n";
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.write("baml/config.toml", "[update]\nauto_check = true\n");
    fixture.check(candidate, "0.11.0", expected_stderr = warning);
    let requests = fixture.requests().length();
    require(requests == 1, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = warning);
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U03-refresh" {
    configured_case("U03-refresh", passive_refresh_warns_and_is_throttled);
}

function offline_passive_refresh_is_throttled(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    fixture.write("baml/config.toml", "[update]\nauto_check = true\n");
    fixture.write("server/canary.json.status", "500");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    let requests = fixture.requests().length();
    require(requests == 1, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U04-offline" {
    configured_case("U04-offline", offline_passive_refresh_is_throttled);
}

function disabled_passive_refresh_uses_no_network(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    let requests = fixture.requests().length();
    require(requests == 0, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U05-optout" {
    configured_case("U05-optout", disabled_passive_refresh_uses_no_network);
}

function passive_refresh_reuses_status_cache(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("canary", key = "channel");
    fixture.manage(candidate, ["toolchain", "status"]);
    fixture.write("baml/config.toml", "[update]\nauto_check = true\n");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    let requests = fixture.requests().length();
    require(requests == 1, "unexpected freshness request count");
    fixture.check(candidate, "0.11.0", expected_stderr = "");
    require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    null
}

test "U06-status-cache" {
    configured_case("U06-status-cache", passive_refresh_reuses_status_cache);
}

function passive_refresh_skips_local_selection(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("baml/config.toml", "[update]\nauto_check = true\n");
    fixture.project("./build/baml-cli", key = "path");
    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "non-channel selection refreshed metadata");
    null
}

test "U07-local" {
    configured_case("U07-local", passive_refresh_skips_local_selection);
}

function passive_refresh_skips_exact_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("baml/config.toml", "[update]\nauto_check = true\n");
    fixture.project("0.11.0");
    fixture.check(candidate, "0.11.0");
    require(fixture.requests().length() == 0, "non-channel selection refreshed metadata");
    null
}

test "U08-exact" {
    configured_case("U08-exact", passive_refresh_skips_exact_version);
}
