use super::*;

fn ok(sql: &str) -> Translation {
    translate(sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn fails(sql: &str) -> String {
    translate(sql).expect_err(sql).0
}

#[test]
fn bracket_paths_become_constant_navigation_on_value_columns() {
    let t = ok("SELECT call_id, output['items'][0]['name'] AS name FROM calls WHERE status = 'ok'");
    assert!(
        t.sql
            .contains("__btel_render(__btel_nav(output, 'items', 0, 'name')) AS \"name\""),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("__btel_kind(__btel_nav(output, 'items', 0, 'name'))")
    );
    assert!(
        t.sql.contains("WHERE status = 'ok'"),
        "plain comparisons stay SQL: {}",
        t.sql
    );
    assert_eq!(
        t.columns,
        vec![
            OutputColumn {
                name: "call_id".into(),
                value: false
            },
            OutputColumn {
                name: "name".into(),
                value: true
            }
        ]
    );
}

#[test]
fn qualified_value_columns_and_comparisons_use_baml_semantics() {
    let t = ok("SELECT c.call_id FROM calls c WHERE c.args['customer']['age'] >= 30");
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(c.args, 'customer', 'age'), '>=', 30, 'sql')"),
        "{}",
        t.sql
    );
    // Constant on the left: the operator flips.
    let t = ok("SELECT call_id FROM calls WHERE 30 < args['customer']['age']");
    assert!(t.sql.contains("'>', 30, 'sql')"), "{}", t.sql);
    let t = ok("SELECT call_id FROM calls WHERE output['ok'] = true");
    assert!(t.sql.contains("'=', true, 'bool')"), "{}", t.sql);
    let t = ok("SELECT call_id FROM calls WHERE output['n'] IN (1, 2) AND args['x'] IS NOT NULL");
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(output, 'n'), '=', 1, 'sql') OR"),
        "{}",
        t.sql
    );
    assert!(
        t.sql.contains("NOT __btel_is_null(__btel_nav(args, 'x'))"),
        "{}",
        t.sql
    );
    let t = ok("SELECT call_id FROM calls WHERE output['a'] = args['b']");
    assert!(t.sql.contains("__btel_cmp_values("), "{}", t.sql);
}

#[test]
fn json_literals_are_validated_constant_comparison_operands() {
    for query in [
        "SELECT call_id FROM calls WHERE args['tags'] = baml_value_json('[1,2]')",
        "SELECT call_id FROM calls WHERE (BAML_VALUE_JSON('[1,2]')) <> (output)",
        "WITH x AS (SELECT output AS v FROM calls) SELECT v = baml_value_json('null') FROM x",
    ] {
        assert!(ok(query).sql.contains("__btel_cmp_json("));
    }
    for query in [
        "SELECT baml_value_json('{}') FROM calls",
        "SELECT output = baml_value_json('{') FROM calls",
        "SELECT output = baml_value_json(fqn) FROM calls",
        "SELECT output = baml_value_json('{}', '{}') FROM calls",
        "SELECT output = baml_value_json(DISTINCT '{}') FROM calls",
        "SELECT output = baml_value_json('{}') OVER () FROM calls",
        "SELECT output < baml_value_json('{}') FROM calls",
        "SELECT baml_value_json('{}') = baml_value_json('{}') FROM calls",
        "SELECT fqn = baml_value_json('{}') FROM calls",
        "SELECT output IN (baml_value_json('{}')) FROM calls",
    ] {
        assert!(fails(query).contains("baml_value_json"), "{query}");
    }
}

#[test]
fn explicit_materialized_ctes_keep_value_handles_and_statement_policy() {
    let query = ok(
        "WITH x AS MATERIALIZED (SELECT output FROM calls WHERE fqn = 'user.Extract') SELECT a.output = b.output FROM x a JOIN x b ON a.output['total'] = b.output['total']",
    );
    assert!(query.sql.contains("AS MATERIALIZED"));
    assert!(
        query
            .sql
            .contains("__btel_cmp_values(a.output, '=', b.output)")
    );
    for invalid in [
        "WITH x AS MATERIALIZED (SELECT output FROM calls) DELETE FROM calls",
        "WITH x AS MATERIALIZED (DELETE FROM calls RETURNING output) SELECT * FROM x",
        "WITH x AS MATERIALIZED (SELECT * FROM sqlite_master) SELECT * FROM x",
        "SELECT 1; WITH x AS MATERIALIZED (SELECT * FROM calls) SELECT * FROM x",
    ] {
        assert!(translate(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn handles_flow_through_ctes_subqueries_and_aliases() {
    let t = ok("WITH x AS (SELECT call_id AS id, args AS a FROM calls)
                SELECT id, a['customer']['name'] FROM x WHERE a['customer']['age'] > 1");
    // The CTE keeps the handle; the outer query navigates and renders it.
    assert!(t.sql.contains("args AS \"a\""), "{}", t.sql);
    assert!(
        t.sql
            .contains("__btel_render(__btel_nav(a, 'customer', 'name'))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(a, 'customer', 'age'), '>', 1, 'sql')")
    );
    assert_eq!(t.columns[1].name, "a['customer']['name']");
    let t = ok("SELECT s.o['total'] FROM (SELECT output AS o FROM calls) s");
    assert!(t.sql.contains("__btel_nav(s.o, 'total')"), "{}", t.sql);
    // A navigated projection inside a CTE stays navigable.
    let t = ok(
        "WITH c AS (SELECT args['customer'] AS customer FROM calls) SELECT customer['age'] FROM c",
    );
    assert!(
        t.sql
            .contains("__btel_nav(__btel_nav(args, 'customer'), 'age')")
            || t.sql.contains("__btel_nav(customer, 'age')"),
        "{}",
        t.sql
    );
}

#[test]
fn a_name_reused_in_separate_scopes_binds_to_its_own_relation() {
    // `output` is a value in `calls` but a plain text column of the CTE, and
    // the alias `c` means different relations inside and outside EXISTS.
    let t = ok("WITH c AS (SELECT 'x' AS output, call_id FROM calls)
                SELECT c.output FROM c
                WHERE EXISTS (SELECT 1 FROM calls c WHERE c.output['n'] = 1)");
    assert!(t.sql.contains("SELECT c.output FROM c"), "{}", t.sql);
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(c.output, 'n'), '=', 1, 'sql')"),
        "{}",
        t.sql
    );
    assert!(!t.columns[0].value);
    assert!(
        fails("WITH c AS (SELECT 'x' AS output FROM calls) SELECT output['n'] FROM c")
            .contains("not a BAML value")
    );
    // Inner scope shadows the outer column of the same name.
    let t = ok("SELECT (SELECT COUNT(*) FROM calls WHERE output['n'] = 1) FROM executions");
    assert!(
        t.sql.contains("__btel_cmp(__btel_nav(output, 'n')"),
        "{}",
        t.sql
    );
}

#[test]
fn wildcards_expand_so_values_render() {
    let t = ok("SELECT * FROM calls");
    assert!(
        t.sql
            .contains("__btel_render(\"calls\".\"args\") AS \"args\""),
        "{}",
        t.sql
    );
    assert_eq!(t.columns.iter().filter(|c| c.value).count(), 3);
    let t = ok(
        "SELECT e.*, c.output FROM executions e JOIN calls c ON c.execution_id = e.execution_id",
    );
    assert!(t.sql.contains("\"e\".\"execution_id\""), "{}", t.sql);
}

#[test]
fn values_in_scalar_positions_are_rendered() {
    let t = ok(
        "SELECT fqn, COUNT(output), MAX(output['score']) FROM calls GROUP BY args['kind'] ORDER BY output['score']",
    );
    assert!(t.sql.contains("COUNT(output)"), "{}", t.sql);
    assert!(
        t.sql
            .contains("MAX(__btel_render(__btel_nav(output, 'score')))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("GROUP BY __btel_render(__btel_nav(args, 'kind'))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("ORDER BY __btel_render(__btel_nav(output, 'score'))"),
        "{}",
        t.sql
    );
    let t = ok("SELECT call_id FROM calls WHERE output['flag']");
    assert!(
        t.sql
            .contains("WHERE __btel_truthy(__btel_nav(output, 'flag'))"),
        "{}",
        t.sql
    );
}

#[test]
fn policy_rejects_writes_internals_and_unsupported_paths() {
    assert!(fails("DELETE FROM calls").contains("read-only"));
    assert!(fails("SELECT 1; SELECT 2").contains("exactly one"));
    assert!(fails("SELECT * FROM call").contains("unknown relation"));
    assert!(fails("SELECT * FROM main.calls").contains("qualified"));
    assert!(fails("SELECT __btel_u64(x) FROM calls").contains("reserved"));
    assert!(fails("SELECT * FROM sqlite_schema").contains("reserved"));
    assert!(fails("SELECT output[call_id] FROM calls").contains("computed paths"));
    assert!(fails("SELECT output['a'].b FROM calls").contains("['field']"));
    assert!(fails("SELECT status['a'] FROM calls").contains("not a BAML value"));
    assert!(fails("WITH RECURSIVE r AS (SELECT 1) SELECT * FROM r").contains("recursive"));
    assert!(fails("SELECT recording_id FROM calls c JOIN executions e ON 1").contains("ambiguous"));
}

#[test]
fn negative_indices_and_escaped_keys_are_constants() {
    let t = ok("SELECT output['it''s'][-1] FROM calls");
    assert!(
        t.sql.contains("__btel_nav(output, 'it''s', -1)"),
        "{}",
        t.sql
    );
}

#[test]
fn set_operations_keep_handles_until_the_output_boundary() {
    let t = ok("SELECT fqn, output FROM calls WHERE status = 'ok'
                UNION ALL SELECT fqn, error FROM calls WHERE status = 'errored'
                ORDER BY 2 LIMIT 5");
    assert!(
        t.sql
            .starts_with("WITH \"__set\"(\"__c0\", \"__c1\") AS (SELECT fqn, output"),
        "branches keep handles: {}",
        t.sql
    );
    assert!(
        t.sql
            .contains("__btel_render(\"__c1\") AS \"output\", __btel_kind(\"__c1\")"),
        "{}",
        t.sql
    );
    assert!(
        t.sql.ends_with("ORDER BY 2 LIMIT 5"),
        "logical column 2 is physical column 2: {}",
        t.sql
    );
    assert_eq!(
        t.columns,
        vec![
            OutputColumn {
                name: "fqn".into(),
                value: false
            },
            OutputColumn {
                name: "output".into(),
                value: true
            }
        ]
    );
    // User CTEs stay first and visible to the branches.
    let t = ok(
        "WITH errs AS (SELECT error FROM calls WHERE status = 'errored')
                SELECT error FROM errs UNION SELECT output FROM calls",
    );
    assert!(t.sql.starts_with("WITH errs AS ("), "{}", t.sql);
    assert!(t.sql.contains(", \"__set\"(\"__c0\") AS ("), "{}", t.sql);
    // Inside a CTE a set operation keeps handles for the outer query.
    let t = ok(
        "WITH v AS (SELECT output FROM calls UNION ALL SELECT error FROM calls)
                SELECT v.output['name'] FROM v",
    );
    assert!(
        t.sql
            .contains("__btel_render(__btel_nav(v.output, 'name'))"),
        "{}",
        t.sql
    );
}

#[test]
fn positions_skip_hidden_kind_columns() {
    let t = ok("SELECT output, fqn, COUNT(*) FROM calls GROUP BY 1, 2 ORDER BY 2, 3 DESC");
    // Physical columns: output, output kind, fqn, count.
    assert!(t.sql.contains("GROUP BY 1, 3"), "{}", t.sql);
    assert!(t.sql.contains("ORDER BY 3, 4 DESC"), "{}", t.sql);
    assert!(fails("SELECT fqn FROM calls ORDER BY 2").contains("outside the 1 result columns"));
}

#[test]
fn value_inspection_functions_take_values() {
    let t = ok("SELECT baml_value_state(args['customer']), baml_kind(output) FROM calls");
    assert!(
        t.sql
            .contains("__btel_value_state(__btel_nav(args, 'customer'))"),
        "{}",
        t.sql
    );
    assert!(t.sql.contains("__btel_kind(output)"), "{}", t.sql);
    assert!(fails("SELECT baml_kind(fqn) FROM calls").contains("takes a BAML value"));
    assert!(fails("SELECT baml_value_state(args, output) FROM calls").contains("one BAML value"));
}

#[test]
fn old_relations_explain_what_replaced_them() {
    let message = fails("SELECT * FROM errors");
    assert!(message.contains("error_calls"), "{message}");
    assert!(fails("SELECT * FROM store_files").contains("recording_files"));
    // New relations are catalog relations with value columns where due.
    let t = ok("SELECT error['code'] FROM error_calls");
    assert!(t.columns[0].value);
    let t = ok(
        "SELECT fqn, self_ns FROM call_path_stats WHERE execution_id IN (SELECT execution_id FROM executions)",
    );
    assert_eq!(t.columns.len(), 2);
}
