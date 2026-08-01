use std::collections::BTreeMap;

use bex_query::compile_clickhouse;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GoldenCase {
    name: String,
    bql: String,
    sql_contains: Vec<String>,
    param_values: Vec<String>,
    tables: Vec<String>,
}

#[test]
fn clickhouse_aggregate_compiler_matches_launch_golden_corpus() {
    let cases: Vec<GoldenCase> =
        serde_json::from_str(include_str!("golden/clickhouse_aggregate.json")).unwrap();
    assert!(
        cases.len() >= 3,
        "golden corpus must cover all aggregate shapes"
    );
    for case in cases {
        let compiled = compile_clickhouse(&case.bql, &BTreeMap::new())
            .unwrap_or_else(|error| panic!("{} failed to compile: {error}", case.name));
        assert_eq!(compiled.len(), 1, "{}", case.name);
        let query = &compiled[0];
        for fragment in case.sql_contains {
            assert!(
                query.sql.contains(&fragment),
                "{} missing SQL fragment `{fragment}`:\n{}",
                case.name,
                query.sql
            );
        }
        assert_eq!(
            query
                .params
                .iter()
                .map(|parameter| parameter.value.clone())
                .collect::<Vec<_>>(),
            case.param_values,
            "{} parameter drift",
            case.name
        );
        assert_eq!(
            query.required_tables, case.tables,
            "{} table drift",
            case.name
        );
    }
}
