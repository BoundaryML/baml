use super::*;

fn ok(sql: &str) -> Translation {
    translate(sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn fails(sql: &str) -> String {
    translate(sql).expect_err(sql).0
}

#[test]
fn bracket_paths_become_constant_navigation_on_value_columns() {
    let t = ok(
        "SELECT span_id, output_value['items'][0]['name'] AS name FROM spans WHERE status = 'return'",
    );
    assert!(
        t.sql
            .contains("__btel_render(__btel_nav(output_value, 'items', 0, 'name')) AS \"name\""),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("__btel_kind(__btel_nav(output_value, 'items', 0, 'name'))")
    );
    assert!(
        t.sql.contains("WHERE status = 'return'"),
        "plain comparisons stay SQL: {}",
        t.sql
    );
    assert_eq!(
        t.columns,
        vec![
            OutputColumn {
                name: "span_id".into(),
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
    let t = ok("SELECT c.span_id FROM spans c WHERE c.input_args['customer']['age'] >= 30");
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(c.input_args, 'customer', 'age'), '>=', 30, 'sql')"),
        "{}",
        t.sql
    );
    // Constant on the left: the operator flips.
    let t = ok("SELECT span_id FROM spans WHERE 30 < input_args['customer']['age']");
    assert!(t.sql.contains("'>', 30, 'sql')"), "{}", t.sql);
    let t = ok("SELECT span_id FROM spans WHERE output_value['ok'] = true");
    assert!(t.sql.contains("'=', true, 'bool')"), "{}", t.sql);
    let t = ok(
        "SELECT span_id FROM spans WHERE output_value['n'] IN (1, 2) AND input_args['x'] IS NOT NULL",
    );
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(output_value, 'n'), '=', 1, 'sql') OR"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("NOT __btel_is_null(__btel_nav(input_args, 'x'))"),
        "{}",
        t.sql
    );
    let t = ok("SELECT span_id FROM spans WHERE output_value['a'] = input_args['b']");
    assert!(t.sql.contains("__btel_cmp_values("), "{}", t.sql);
}

#[test]
fn json_literals_are_validated_constant_comparison_operands() {
    for query in [
        "SELECT span_id FROM spans WHERE input_args['tags'] = baml_value_json('[1,2]')",
        "SELECT span_id FROM spans WHERE (BAML_VALUE_JSON('[1,2]')) <> (output_value)",
        "WITH x AS (SELECT output_value AS v FROM spans) SELECT v = baml_value_json('null') FROM x",
    ] {
        assert!(ok(query).sql.contains("__btel_cmp_json("));
    }
    for query in [
        "SELECT baml_value_json('{}') FROM spans",
        "SELECT output_value = baml_value_json('{') FROM spans",
        "SELECT output_value = baml_value_json(span_name) FROM spans",
        "SELECT output_value = baml_value_json('{}', '{}') FROM spans",
        "SELECT output_value = baml_value_json(DISTINCT '{}') FROM spans",
        "SELECT output_value = baml_value_json('{}') OVER () FROM spans",
        "SELECT output_value < baml_value_json('{}') FROM spans",
        "SELECT baml_value_json('{}') = baml_value_json('{}') FROM spans",
        "SELECT span_name = baml_value_json('{}') FROM spans",
        "SELECT output_value IN (baml_value_json('{}')) FROM spans",
    ] {
        assert!(fails(query).contains("baml_value_json"), "{query}");
    }
}

#[test]
fn explicit_materialized_ctes_keep_value_handles_and_statement_policy() {
    let query = ok(
        "WITH x AS MATERIALIZED (SELECT output_value FROM spans WHERE span_name = 'user.Extract') SELECT a.output_value = b.output_value FROM x a JOIN x b ON a.output_value['total'] = b.output_value['total']",
    );
    assert!(query.sql.contains("AS MATERIALIZED"));
    assert!(
        query
            .sql
            .contains("__btel_cmp_values(a.output_value, '=', b.output_value)")
    );
    for invalid in [
        "WITH x AS MATERIALIZED (SELECT output_value FROM spans) DELETE FROM spans",
        "WITH x AS MATERIALIZED (DELETE FROM spans RETURNING output_value) SELECT * FROM x",
        "WITH x AS MATERIALIZED (SELECT * FROM sqlite_master) SELECT * FROM x",
        "SELECT 1; WITH x AS MATERIALIZED (SELECT * FROM spans) SELECT * FROM x",
    ] {
        assert!(translate(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn handles_flow_through_ctes_subqueries_and_aliases() {
    let t = ok(
        "WITH x AS (SELECT span_id AS id, input_args AS a FROM spans)
                SELECT id, a['customer']['name'] FROM x WHERE a['customer']['age'] > 1",
    );
    // The CTE keeps the handle; the outer query navigates and renders it.
    assert!(t.sql.contains("input_args AS \"a\""), "{}", t.sql);
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
    let t = ok("SELECT s.o['total'] FROM (SELECT output_value AS o FROM spans) s");
    assert!(t.sql.contains("__btel_nav(s.o, 'total')"), "{}", t.sql);
    // A navigated projection inside a CTE stays navigable.
    let t = ok(
        "WITH c AS (SELECT input_args['customer'] AS customer FROM spans) SELECT customer['age'] FROM c",
    );
    assert!(
        t.sql
            .contains("__btel_nav(__btel_nav(input_args, 'customer'), 'age')")
            || t.sql.contains("__btel_nav(customer, 'age')"),
        "{}",
        t.sql
    );
}

#[test]
fn a_name_reused_in_separate_scopes_binds_to_its_own_relation() {
    // `output_value` is a value in `spans` but a plain text column of the CTE, and
    // the alias `c` means different relations inside and outside EXISTS.
    let t = ok("WITH c AS (SELECT 'x' AS output_value, span_id FROM spans)
                SELECT c.output_value FROM c
                WHERE EXISTS (SELECT 1 FROM spans c WHERE c.output_value['n'] = 1)");
    assert!(t.sql.contains("SELECT c.output_value FROM c"), "{}", t.sql);
    assert!(
        t.sql
            .contains("__btel_cmp(__btel_nav(c.output_value, 'n'), '=', 1, 'sql')"),
        "{}",
        t.sql
    );
    assert!(!t.columns[0].value);
    assert!(
        fails("WITH c AS (SELECT 'x' AS output_value FROM spans) SELECT output_value['n'] FROM c")
            .contains("not a BAML value")
    );
    // Inner scope shadows the outer column of the same name.
    let t = ok("SELECT (SELECT COUNT(*) FROM spans WHERE output_value['n'] = 1) FROM processes");
    assert!(
        t.sql.contains("__btel_cmp(__btel_nav(output_value, 'n')"),
        "{}",
        t.sql
    );
}

#[test]
fn wildcards_expand_so_values_render() {
    let t = ok("SELECT * FROM spans");
    assert!(
        t.sql
            .contains("__btel_render(\"spans\".\"input_args\") AS \"input_args\""),
        "{}",
        t.sql
    );
    assert_eq!(t.columns.iter().filter(|c| c.value).count(), 6);
    let t = ok(
        "SELECT p.*, c.output_value FROM processes p JOIN spans c ON c.process_id = p.process_id",
    );
    assert!(t.sql.contains("\"p\".\"process_id\""), "{}", t.sql);
}

#[test]
fn count_reads_a_value_inside_a_capture_but_not_the_capture() {
    let t = ok(
        "SELECT COUNT(output_value), COUNT(input_args['k']), COUNT(DISTINCT input_args['k']),
           COUNT((input_args['p'])) FROM spans",
    );
    assert!(
        t.sql
            .contains("COUNT(NULLIF(__btel_is_null((__btel_nav(input_args, 'p'))), 1))"),
        "{}",
        t.sql
    );
    assert!(t.sql.contains("COUNT(output_value)"), "{}", t.sql);
    assert!(
        t.sql
            .contains("COUNT(NULLIF(__btel_is_null(__btel_nav(input_args, 'k')), 1))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("COUNT(DISTINCT __btel_render(__btel_nav(input_args, 'k')))"),
        "{}",
        t.sql
    );
}

#[test]
fn values_in_scalar_positions_are_rendered() {
    let t = ok(
        "SELECT span_name, COUNT(output_value), MAX(output_value['score']) FROM spans GROUP BY input_args['kind'] ORDER BY output_value['score']",
    );
    assert!(t.sql.contains("COUNT(output_value)"), "{}", t.sql);
    assert!(
        t.sql
            .contains("MAX(__btel_render(__btel_nav(output_value, 'score')))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("GROUP BY __btel_render(__btel_nav(input_args, 'kind'))"),
        "{}",
        t.sql
    );
    assert!(
        t.sql
            .contains("ORDER BY __btel_render(__btel_nav(output_value, 'score'))"),
        "{}",
        t.sql
    );
    let t = ok("SELECT span_id FROM spans WHERE output_value['flag']");
    assert!(
        t.sql
            .contains("WHERE __btel_truthy(__btel_nav(output_value, 'flag'))"),
        "{}",
        t.sql
    );
}

#[test]
fn policy_rejects_writes_internals_and_unsupported_paths() {
    assert!(fails("DELETE FROM spans").contains("read-only"));
    assert!(fails("SELECT 1; SELECT 2").contains("exactly one"));
    assert!(fails("SELECT * FROM call").contains("unknown relation"));
    assert!(fails("SELECT * FROM calls").contains("unknown relation"));
    assert!(fails("SELECT * FROM main.spans").contains("qualified"));
    assert!(fails("SELECT __btel_u64(x) FROM spans").contains("reserved"));
    assert!(fails("SELECT * FROM sqlite_schema").contains("reserved"));
    assert!(fails("SELECT output_value[span_id] FROM spans").contains("computed paths"));
    assert!(fails("SELECT output_value['a'].b FROM spans").contains("['field']"));
    assert!(fails("SELECT status['a'] FROM spans").contains("not a BAML value"));
    assert!(fails("SELECT process_id FROM spans c JOIN processes p ON 1").contains("ambiguous"));
}

#[test]
fn negative_indices_and_escaped_keys_are_constants() {
    let t = ok("SELECT output_value['it''s'][-1] FROM spans");
    assert!(
        t.sql.contains("__btel_nav(output_value, 'it''s', -1)"),
        "{}",
        t.sql
    );
}

#[test]
fn set_operations_keep_handles_until_the_output_boundary() {
    let t = ok(
        "SELECT span_name, output_value FROM spans WHERE status = 'return'
                UNION ALL SELECT span_name, error_value FROM spans WHERE status = 'user_error'
                ORDER BY 2 LIMIT 5",
    );
    assert!(
        t.sql
            .starts_with("WITH \"__set\"(\"__c0\", \"__c1\") AS (SELECT span_name, output_value"),
        "branches keep handles: {}",
        t.sql
    );
    assert!(
        t.sql
            .contains("__btel_render(\"__c1\") AS \"output_value\", __btel_kind(\"__c1\")"),
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
                name: "span_name".into(),
                value: false
            },
            OutputColumn {
                name: "output_value".into(),
                value: true
            }
        ]
    );
    // User CTEs stay first and visible to the branches.
    let t = ok(
        "WITH errs AS (SELECT error_value FROM spans WHERE status = 'user_error')
                SELECT error_value FROM errs UNION SELECT output_value FROM spans",
    );
    assert!(t.sql.starts_with("WITH errs AS ("), "{}", t.sql);
    assert!(t.sql.contains(", \"__set\"(\"__c0\") AS ("), "{}", t.sql);
    // Inside a CTE a set operation keeps handles for the outer query.
    let t = ok(
        "WITH v AS (SELECT output_value FROM spans UNION ALL SELECT error_value FROM spans)
                SELECT v.output_value['name'] FROM v",
    );
    assert!(
        t.sql
            .contains("__btel_render(__btel_nav(v.output_value, 'name'))"),
        "{}",
        t.sql
    );
}

#[test]
fn set_operation_branches_agree_on_value_columns() {
    for sql in [
        "SELECT span_name FROM spans UNION ALL SELECT output_value FROM spans",
        "SELECT output_value FROM spans UNION SELECT span_name FROM spans",
        "WITH v AS (SELECT span_name FROM spans UNION ALL SELECT error_value FROM spans) SELECT * FROM v",
    ] {
        assert!(
            fails(sql).contains("is a BAML value in one branch but not the other"),
            "{sql}"
        );
    }
    // Expression subqueries render values in each branch, so kinds may differ.
    ok(
        "SELECT span_name FROM spans WHERE span_name IN (SELECT span_name FROM spans UNION SELECT output_value FROM spans)",
    );
}

#[test]
fn positions_skip_hidden_kind_columns() {
    let t =
        ok("SELECT output_value, span_name, COUNT(*) FROM spans GROUP BY 1, 2 ORDER BY 2, 3 DESC");
    // Physical columns: output_value, output_value kind, span_name, count.
    assert!(t.sql.contains("GROUP BY 1, 3"), "{}", t.sql);
    assert!(t.sql.contains("ORDER BY 3, 4 DESC"), "{}", t.sql);
    assert!(
        fails("SELECT span_name FROM spans ORDER BY 2").contains("outside the 1 result columns")
    );
}

#[test]
fn value_inspection_functions_take_values() {
    let t =
        ok("SELECT baml_value_state(input_args['customer']), baml_kind(output_value) FROM spans");
    assert!(
        t.sql
            .contains("__btel_value_state(__btel_nav(input_args, 'customer'))"),
        "{}",
        t.sql
    );
    assert!(t.sql.contains("__btel_kind(output_value)"), "{}", t.sql);
    assert!(fails("SELECT baml_kind(span_name) FROM spans").contains("takes a BAML value"));
    assert!(
        fails("SELECT baml_value_state(input_args, output_value) FROM spans")
            .contains("one BAML value")
    );
}

/// SQLite's plan for the translation, on an empty index with every view.
fn plan(sql: &str) -> String {
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::store::ensure_schema(&mut conn).unwrap();
    crate::functions::register(&conn, &crate::functions::ContextSlot::default()).unwrap();
    for relation in crate::catalog::RELATIONS {
        conn.execute_batch(&relation.create_sql()).unwrap();
    }
    let t = ok(sql);
    let mut statement = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {}", t.sql))
        .unwrap();
    let params = vec![rusqlite::types::Value::Text("00:1".into()); statement.parameter_count()];
    statement
        .query_map(rusqlite::params_from_iter(params), |r| {
            r.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
}

#[test]
fn id_filters_keep_the_test_and_add_the_stored_parts() {
    let t = ok("SELECT span_name FROM spans WHERE parent_span_id = ?1");
    assert!(
        t.sql.contains(
            "parent_span_id = ?1 AND \"__parent_span_id_rec\" = (SELECT rec FROM main.recording"
        ) && t
            .sql
            .contains("\"__parent_span_id_key\" = __btel_key(?1, '')"),
        "{}",
        t.sql
    );
    let t = ok("SELECT 1 FROM spans c JOIN span_announcements a ON a.span_id = c.parent_span_id");
    assert!(
        t.sql
            .contains("\"a\".\"__span_id_key\" = \"c\".\"__parent_span_id_key\""),
        "{}",
        t.sql
    );
    let t = ok("SELECT 1 FROM spans WHERE span_id IN ('a:1', ?2)");
    assert!(
        t.sql
            .contains("\"__span_id_key\" IN (__btel_key('a:1', ''), __btel_key(?2, ''))"),
        "{}",
        t.sql
    );
    // Left alone: a bare `?` (repeating it renumbers parameters), negation,
    // other operators, and columns of a CTE.
    for sql in [
        "SELECT 1 FROM spans WHERE span_id = ?",
        "SELECT 1 FROM spans WHERE span_id NOT IN ('a:1')",
        "SELECT 1 FROM spans WHERE span_id > 'a:1'",
        "WITH x AS (SELECT span_id FROM spans) SELECT 1 FROM x WHERE span_id = 'a:1'",
    ] {
        assert!(!ok(sql).sql.contains("__btel_key"), "{sql}");
    }
}

#[test]
fn id_filters_are_index_lookups() {
    for (sql, lookups) in [
        (
            "SELECT status FROM spans WHERE span_id = ?1",
            &[
                "SEARCH c USING PRIMARY KEY (rec=? AND call_id=?)",
                "SEARCH t USING PRIMARY KEY (rec=? AND thread_id=?)",
            ][..],
        ),
        (
            "SELECT span_id FROM spans WHERE parent_span_id = ?1",
            &[
                "call_by_parent (rec=? AND parent_id=?)",
                "thread_by_parent (rec=? AND parent_id=?)",
            ],
        ),
        (
            "SELECT span_id FROM span_announcements WHERE span_id IN (?1, ?2)",
            &[
                "SEARCH c USING PRIMARY KEY (rec=? AND call_id=?)",
                "SEARCH t USING PRIMARY KEY (rec=? AND thread_id=?)",
            ],
        ),
    ] {
        let plan = plan(sql);
        for lookup in lookups {
            assert!(plan.contains(lookup), "{sql}: no {lookup}\n{plan}");
        }
        // Scans only read a subquery's already-filtered rows.
        let subqueries: Vec<&str> = plan
            .lines()
            .filter_map(|line| line.strip_prefix("CO-ROUTINE "))
            .collect();
        for line in plan.lines() {
            if let Some(target) = line.strip_prefix("SCAN ") {
                let name = target.split_whitespace().next().unwrap_or_default();
                assert!(subqueries.contains(&name), "{sql}: {line}\n{plan}");
            }
        }
    }
}

#[test]
fn recursive_ctes_walk_span_trees_with_values() {
    let t = ok("WITH RECURSIVE tree(id, depth, args) AS (
                  SELECT span_id, 0, input_args FROM spans WHERE parent_span_id IS NULL
                  UNION ALL
                  SELECT s.span_id, t.depth + 1, s.input_args FROM spans s
                  JOIN tree t ON s.parent_span_id = t.id)
                SELECT id, depth, args['n'] FROM tree");
    assert!(t.sql.contains("WITH RECURSIVE"), "{}", t.sql);
    assert!(
        t.sql.contains("__btel_render(__btel_nav(args, 'n'))"),
        "the step keeps the anchor's value column: {}",
        t.sql
    );
    assert!(
        fails(
            "WITH RECURSIVE r(v) AS (SELECT input_args FROM spans UNION ALL SELECT span_id FROM r)
               SELECT * FROM r"
        )
        .contains("BAML value in one branch")
    );
    // A non-recursive CTE in a WITH RECURSIVE clause translates as usual.
    ok("WITH RECURSIVE x AS (SELECT span_id FROM spans) SELECT * FROM x");
}
