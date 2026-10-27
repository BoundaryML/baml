// Run with WRAPPER_CANDIDATES, WRAPPER_STUB, WRAPPER_E2E_HELPER and
// WRAPPER_E2E_PYTHON set, or use tools/wrapper-e2e/run for build adapters.
function configured_case(id: string) -> null {
    for (let candidate in candidates()) {
        let reason = excluded(candidate, id);
        if (reason == "") {
            run_case(candidate, id);
        } else {
            baml.io.println(`SKIP ${candidate.label} ${id}: ${reason}`);
        }
    }
    null
}

test "S01" {
    configured_case("S01");
}

test "S02" {
    configured_case("S02");
}

test "S03" {
    configured_case("S03");
}

test "S04" {
    configured_case("S04");
}

test "S05" {
    configured_case("S05");
}

test "S06" {
    configured_case("S06");
}

test "S07" {
    configured_case("S07");
}

test "S08-empty" {
    configured_case("S08-empty");
}

test "S08-space" {
    configured_case("S08-space");
}

test "S09" {
    configured_case("S09");
}

test "S10" {
    configured_case("S10");
}

test "S11" {
    configured_case("S11");
}

test "S12" {
    configured_case("S12");
}

test "S13" {
    configured_case("S13");
}

test "S15-syntax" {
    configured_case("S15-syntax");
}

test "S15-field" {
    configured_case("S15-field");
}

test "S16-valid" {
    configured_case("S16-valid");
}

test "S16-invalid" {
    configured_case("S16-invalid");
}

test "S17" {
    configured_case("S17");
}

test "S18" {
    configured_case("S18");
}

test "S19" {
    configured_case("S19");
}

test "L01" {
    configured_case("L01");
}

test "L02" {
    configured_case("L02");
}

test "L03" {
    configured_case("L03");
}

test "L04" {
    configured_case("L04");
}

test "L05" {
    configured_case("L05");
}

test "L06" {
    configured_case("L06");
}

test "L06-symlink" {
    configured_case("L06-symlink");
}

test "L07" {
    configured_case("L07");
}

test "L08" {
    configured_case("L08");
}

test "L09" {
    configured_case("L09");
}

test "L10" {
    configured_case("L10");
}

test "L14" {
    configured_case("L14");
}

test "L15" {
    configured_case("L15");
}

test "L16" {
    configured_case("L16");
}

test "I01" {
    configured_case("I01");
}

test "I02" {
    configured_case("I02");
}

test "I03" {
    configured_case("I03");
}

test "I04" {
    configured_case("I04");
}

test "I05" {
    configured_case("I05");
}

test "I06" {
    configured_case("I06");
}

test "I07" {
    configured_case("I07");
}

test "I08" {
    configured_case("I08");
}

test "I09-binary" {
    configured_case("I09-binary");
}

test "I09-VERSION" {
    configured_case("I09-VERSION");
}

test "I09-mismatch" {
    configured_case("I09-mismatch");
}

test "I09-link" {
    configured_case("I09-link");
}

test "I09-file" {
    configured_case("I09-file");
}

test "I10" {
    configured_case("I10");
}

test "I11" {
    configured_case("I11");
}

test "I11-failure" {
    configured_case("I11-failure");
}

test "I12-unreachable" {
    configured_case("I12-unreachable");
}

test "I12-timeout" {
    configured_case("I12-timeout");
}

test "I12-404" {
    configured_case("I12-404");
}

test "I12-500" {
    configured_case("I12-500");
}

test "I12-json" {
    configured_case("I12-json");
}

test "I12-schema" {
    configured_case("I12-schema");
}

test "I12-target" {
    configured_case("I12-target");
}

test "I13-404" {
    configured_case("I13-404");
}

test "I13-disconnect" {
    configured_case("I13-disconnect");
}

test "I13-checksum" {
    configured_case("I13-checksum");
}

test "I14-cli" {
    configured_case("I14-cli");
}

test "I14-layout" {
    configured_case("I14-layout");
}

test "I15-traversal" {
    configured_case("I15-traversal");
}

test "I15-link" {
    configured_case("I15-link");
}

test "I16" {
    configured_case("I16");
}

test "I17" {
    configured_case("I17");
}

test "I20" {
    configured_case("I20");
}

test "I21" {
    configured_case("I21");
}

test "M01" {
    configured_case("M01");
}

test "M02" {
    configured_case("M02");
}

test "M03" {
    configured_case("M03");
}

test "M04" {
    configured_case("M04");
}

test "M05" {
    configured_case("M05");
}

test "M06-canary" {
    configured_case("M06-canary");
}

test "M06-nightly" {
    configured_case("M06-nightly");
}

test "M07" {
    configured_case("M07");
}

test "M08-version" {
    configured_case("M08-version");
}

test "M08-channel" {
    configured_case("M08-channel");
}

test "M08-path" {
    configured_case("M08-path");
}

test "M09" {
    configured_case("M09");
}

test "M10-project-download" {
    configured_case("M10-project-download");
}

test "M10-local-download" {
    configured_case("M10-local-download");
}

test "M10-project-path" {
    configured_case("M10-project-path");
}

test "M10-local-path" {
    configured_case("M10-local-path");
}

test "M11" {
    configured_case("M11");
}

test "M13" {
    configured_case("M13");
}

test "M14-env-version" {
    configured_case("M14-env-version");
}

test "M14-env-path" {
    configured_case("M14-env-path");
}

test "M14-project-version" {
    configured_case("M14-project-version");
}

test "M14-project-path" {
    configured_case("M14-project-path");
}

test "M14-global-version" {
    configured_case("M14-global-version");
}

test "M14-global-path" {
    configured_case("M14-global-path");
}

test "M15-project" {
    configured_case("M15-project");
}

test "M15-env" {
    configured_case("M15-env");
}

test "M16" {
    configured_case("M16");
}

test "M17-absent" {
    configured_case("M17-absent");
}

test "M17-path" {
    configured_case("M17-path");
}

test "M17-channel" {
    configured_case("M17-channel");
}

test "M18-version" {
    configured_case("M18-version");
}

test "M18-channel" {
    configured_case("M18-channel");
}

test "M19-empty" {
    configured_case("M19-empty");
}

test "M19-installed" {
    configured_case("M19-installed");
}

test "M19-override" {
    configured_case("M19-override");
}

test "M20" {
    configured_case("M20");
}

test "M21" {
    configured_case("M21");
}

test "M22" {
    configured_case("M22");
}

test "M23" {
    configured_case("M23");
}

test "M24" {
    configured_case("M24");
}

test "M25" {
    configured_case("M25");
}

test "M26-explicit" {
    configured_case("M26-explicit");
}

test "M26-home" {
    configured_case("M26-home");
}

test "M27" {
    configured_case("M27");
}

test "V01" {
    configured_case("V01");
}

test "E01" {
    configured_case("E01");
}

test "E02" {
    configured_case("E02");
}

test "E02-metadata" {
    configured_case("E02-metadata");
}

test "E03" {
    configured_case("E03");
}

test "E04" {
    configured_case("E04");
}

test "E05-env" {
    configured_case("E05-env");
}

test "E05-env-prerelease" {
    configured_case("E05-env-prerelease");
}

test "E05-env-overflow" {
    configured_case("E05-env-overflow");
}

test "E05-project" {
    configured_case("E05-project");
}

test "E05-state" {
    configured_case("E05-state");
}

test "E06-version" {
    configured_case("E06-version");
}

test "E06-channel" {
    configured_case("E06-channel");
}

test "E07-config" {
    configured_case("E07-config");
}

test "E07-state" {
    configured_case("E07-state");
}

test "E08" {
    configured_case("E08");
}

test "E09" {
    configured_case("E09");
}

test "E09-dependencies" {
    configured_case("E09-dependencies");
}

test "E09-scripts" {
    configured_case("E09-scripts");
}

test "E09-generator" {
    configured_case("E09-generator");
}

test "E09-test" {
    configured_case("E09-test");
}

test "E09-date" {
    configured_case("E09-date");
}

test "E10-project" {
    configured_case("E10-project");
}

test "E10-local" {
    configured_case("E10-local");
}

test "E11-update" {
    configured_case("E11-update");
}

test "E11-auto" {
    configured_case("E11-auto");
}

test "E12-config" {
    configured_case("E12-config");
}

test "E12-state" {
    configured_case("E12-state");
}

test "E12-project" {
    configured_case("E12-project");
}

test "W01" {
    configured_case("W01");
}

test "W02" {
    configured_case("W02");
}

test "W03-download" {
    configured_case("W03-download");
}

test "W03-checksum" {
    configured_case("W03-checksum");
}

test "W03-replace" {
    configured_case("W03-replace");
}

test "W04" {
    configured_case("W04");
}

test "F01" {
    configured_case("F01");
}

test "F02" {
    configured_case("F02");
}

test "F03" {
    configured_case("F03");
}

test "F04" {
    configured_case("F04");
}

test "F05" {
    configured_case("F05");
}

test "F06" {
    configured_case("F06");
}

test "F07" {
    configured_case("F07");
}

test "I21-stale-lock" {
    configured_case("I21-stale-lock");
}

test "F08-env" {
    configured_case("F08-env");
}

test "F08-override" {
    configured_case("F08-override");
}

test "S20-project-prefix" {
    configured_case("S20-project-prefix");
}

test "S21-project-inline" {
    configured_case("S21-project-inline");
}

test "S22-directory" {
    configured_case("S22-directory");
}

test "S23-project-env" {
    configured_case("S23-project-env");
}

test "S24-target-args" {
    configured_case("S24-target-args");
}

test "S25-query-project" {
    configured_case("S25-query-project");
}

test "S26-directory-project" {
    configured_case("S26-directory-project");
}

test "S27-project-management" {
    configured_case("S27-project-management");
}

test "R01-installed" {
    configured_case("R01-installed");
}

test "R02-absent" {
    configured_case("R02-absent");
}

test "R03-install" {
    configured_case("R03-install");
}

test "R04-channel" {
    configured_case("R04-channel");
}

test "R05-channel-install" {
    configured_case("R05-channel-install");
}

test "R06-local" {
    configured_case("R06-local");
}

test "R07-invalid-local" {
    configured_case("R07-invalid-local");
}

test "R08-help" {
    configured_case("R08-help");
}

test "E13-project-missing" {
    configured_case("E13-project-missing");
}

test "E13-project-absent" {
    configured_case("E13-project-absent");
}

test "E13-project-duplicate" {
    configured_case("E13-project-duplicate");
}

test "I22-bare-project" {
    configured_case("I22-bare-project");
}

test "I23-bare-env" {
    configured_case("I23-bare-env");
}

test "I24-bare-channel" {
    configured_case("I24-bare-channel");
}

test "I25-bare-force" {
    configured_case("I25-bare-force");
}

test "U01-cached-outdated" {
    configured_case("U01-cached-outdated");
}

test "U02-cached-current" {
    configured_case("U02-cached-current");
}

test "U03-refresh" {
    configured_case("U03-refresh");
}

test "U04-offline" {
    configured_case("U04-offline");
}

test "U05-optout" {
    configured_case("U05-optout");
}

test "M28-previous" {
    configured_case("M28-previous");
}

test "M29-previous-channel" {
    configured_case("M29-previous-channel");
}

test "M30-update-guidance" {
    configured_case("M30-update-guidance");
}

test "M31-upgrade-guidance" {
    configured_case("M31-upgrade-guidance");
}

test "D01-human-error" {
    configured_case("D01-human-error");
}

test "S28-query-global-option" {
    configured_case("S28-query-global-option");
}

test "S29-directory-env-relative" {
    configured_case("S29-directory-env-relative");
}

test "R09-default-json" {
    configured_case("R09-default-json");
}

test "R10-unrecorded-channel" {
    configured_case("R10-unrecorded-channel");
}

test "U06-status-cache" {
    configured_case("U06-status-cache");
}

test "U07-local" {
    configured_case("U07-local");
}

test "U08-exact" {
    configured_case("U08-exact");
}

test "S30-home-boundary" {
    configured_case("S30-home-boundary");
}

test "S31-relative-unpinned" {
    configured_case("S31-relative-unpinned");
}

test "I26-concurrent-cache" {
    configured_case("I26-concurrent-cache");
}
