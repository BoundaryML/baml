//! Discover the scorecard's actual sources; no copied workloads to drift.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub struct Fixture {
    pub name: String,
    pub slug: String,
    pub source: PathBuf,
}

// Error probes must match the oracle's failed exit and exact diagnostic. Real
// scorecard workloads and successful semantic probes must still exit cleanly.
pub fn expects_success(name: &str) -> bool {
    !matches!(
        name,
        "strings::parse error"
            | "strings::codepoint error"
            | "strings::float error"
            | "strings::utf8 error"
            | "strings::regex error"
    )
}

pub fn string_probe_output(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "strings::semantics" => b"288\n",
        "strings::float" => b"8\n",
        "strings::utf8" => b"6\n",
        "strings::regex" => b"12\n",
        "strings::formatting" => b"4\n",
        _ => return None,
    })
}

pub fn fixtures(workspace: &Path, only: Option<&str>) -> Vec<Fixture> {
    let root = workspace.join("tools/speedtest");
    let scorecard: toml::Value = std::fs::read_to_string(root.join("kpis.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let mut wanted: BTreeSet<String> = scorecard["kpi"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|k| {
            k["workloads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap().to_owned())
        })
        .collect();
    let mut fixtures = Vec::new();
    for entry in walkdir::WalkDir::new(root.join("workloads")) {
        let entry = entry.unwrap();
        if entry.file_name() != "bench.toml" {
            continue;
        }
        let dir = entry.path().parent().unwrap();
        let meta: toml::Value = std::fs::read_to_string(entry.path())
            .unwrap()
            .parse()
            .unwrap();
        let category = dir.parent().unwrap().file_name().unwrap().to_str().unwrap();
        let name = format!("{category}::{}", meta["name"].as_str().unwrap());
        if !wanted.remove(&name) {
            continue;
        }
        if only.is_some_and(|o| !o.split(',').any(|p| p == name)) {
            continue;
        }
        let source = dir.join("main.baml");
        assert!(source.is_file(), "KPI missing BAML: {name}");
        fixtures.push(Fixture {
            name,
            slug: format!("{category}-{}", dir.file_name().unwrap().to_str().unwrap()),
            source,
        });
    }
    assert!(wanted.is_empty(), "unknown KPI workloads: {wanted:?}");
    // Isolated semantic probes share the same pack/run/telemetry matrix as the
    // scorecard, without depending on unrelated project-wide initializers.
    for (name, file) in [
        ("strings::semantics", "semantics"),
        ("strings::float", "float"),
        ("strings::utf8", "utf8"),
        ("strings::utf8 error", "utf8_error"),
        ("strings::regex", "regex"),
        ("strings::regex error", "regex_error"),
        ("strings::formatting", "formatting"),
        ("strings::float error", "float_error"),
        ("strings::parse error", "parse_error"),
        ("strings::codepoint error", "codepoint_error"),
    ] {
        if only.is_none_or(|only| only.split(',').any(|selected| selected == name)) {
            fixtures.push(Fixture {
                name: name.into(),
                slug: format!("strings-{file}"),
                source: workspace.join(format!("crates/baml_tests/rust_diff/strings/{file}.baml")),
            });
        }
    }
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    fixtures
}
