//! Query guides live in the shared topic registry. Table documentation is a view of the
//! executable local catalog, never a second schema. Neither path opens an index or a connection.
use std::io::{self, Write};

use anyhow::{Result, bail};
use baml_builtins2::LanguageTopic;
use baml_query_btel::catalog::{self, Column, Relation};
use serde_json::{Value, json};

const TABLES: &str = "query.tables";
pub(crate) const LOCAL_CATALOG: &str =
    "Local catalog bundled with this CLI; not live cloud metadata.";

pub(crate) fn owns(name: &str) -> bool {
    name == "query" || name.starts_with("query.")
}

enum Document {
    Guide(&'static LanguageTopic),
    Table(&'static Relation),
    Column(&'static Relation, &'static Column),
}

fn resolve(name: &str) -> Result<Document> {
    if let Some(doc) = baml_builtins2::language_topic(name).filter(|_| owns(name)) {
        return Ok(Document::Guide(doc));
    }
    if let Some(path) = name.strip_prefix("query.tables.") {
        let (table_name, column_name) = path
            .split_once('.')
            .map_or((path, None), |(t, c)| (t, Some(c)));
        if let Some(table) = catalog::relation(table_name) {
            if let Some(column_name) = column_name {
                if let Some(column) = table
                    .columns
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(column_name))
                {
                    return Ok(Document::Column(table, column));
                }
            } else {
                return Ok(Document::Table(table));
            }
        }
    }
    bail!(
        "unknown query topic `{name}`; use `baml describe query` for guides or `baml describe query.tables` for table names"
    )
}

pub(crate) fn write(w: &mut impl Write, name: &str) -> Result<()> {
    match resolve(name)? {
        Document::Guide(doc) => {
            crate::describe_command::write_language_topic(w, name, doc)?;
            if name == TABLES {
                for table in catalog::RELATIONS {
                    writeln!(w, "  baml describe {TABLES}.{}", table.name)?;
                    writeln!(w, "    {}\n", table.doc)?;
                }
            }
        }
        Document::Column(table, column) => {
            writeln!(w, "{LOCAL_CATALOG}\n")?;
            writeln!(w, "{}.{}", table.name, column.name)?;
            write_column(w, column)?;
            writeln!(w, "\nTable: baml describe {TABLES}.{}", table.name)?;
            writeln!(w, "Value navigation: baml describe query.values")?;
        }
        Document::Table(table) => {
            writeln!(w, "{LOCAL_CATALOG}\n")?;
            write_relation(w, table)?;
            writeln!(
                w,
                "\nInspect these columns as JSON: baml query --schema --table {} --format json",
                table.name
            )?;
            writeln!(w, "Learn value navigation: baml describe query.values")?;
            writeln!(w, "All tables: baml describe {TABLES}")?;
        }
    }
    Ok(())
}

pub(crate) fn json(name: &str) -> Result<Value> {
    Ok(match resolve(name)? {
        Document::Guide(doc) => {
            let mut value = crate::describe_command::language_topic_json(name, doc);
            if name == TABLES {
                value["catalog"] = json!("local");
                value["tables"] = json!(catalog::RELATIONS);
            }
            value
        }
        Document::Column(table, column) => json!({
            "type": "query_column",
            "name": format!("{TABLES}.{}.{}", table.name, column.name),
            "catalog": "local",
            "table": table.name,
            "column": column,
        }),
        Document::Table(table) => json!({
            "type": "query_table",
            "name": format!("{TABLES}.{}", table.name),
            "catalog": "local",
            "table": table,
        }),
    })
}

/// Shared by `query --schema` and `describe query.tables.<name>`.
pub(crate) fn write_relation(w: &mut impl Write, table: &Relation) -> io::Result<()> {
    writeln!(w, "{}: {}", table.name, table.doc)?;
    for column in table.columns {
        write_column(w, column)?;
    }
    Ok(())
}

fn write_column(w: &mut impl Write, column: &Column) -> io::Result<()> {
    writeln!(
        w,
        "  {:<26} {:<11}  {}",
        column.name, column.sql_type, column.doc
    )
}

pub(crate) fn search(query: &str) -> Vec<baml_ide::SearchHit> {
    let guides = baml_builtins2::language_topics()
        .iter()
        .filter(|(name, _)| owns(name))
        .map(|(name, doc)| {
            (
                name.clone(),
                format!(
                    "{}\n{}\n{}",
                    doc.summary,
                    doc.syntax.as_deref().unwrap_or_default(),
                    doc.details.as_deref().unwrap_or_default()
                ),
            )
        });
    let tables = catalog::RELATIONS.iter().flat_map(|table| {
        std::iter::once((format!("{TABLES}.{}", table.name), table.doc.to_owned())).chain(
            table.columns.iter().map(move |column| {
                (
                    format!("{TABLES}.{}.{}", table.name, column.name),
                    format!("{}\n{}", column.doc, column.sql_type),
                )
            }),
        )
    });
    baml_ide::search::search_documentation(query, guides.chain(tables))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_table_is_addressable_without_a_second_registry() {
        let index = json(TABLES).unwrap();
        assert_eq!(index["tables"], json!(catalog::RELATIONS));
        for table in catalog::RELATIONS {
            let name = format!("{TABLES}.{}", table.name);
            assert_eq!(json(&name).unwrap()["table"], json!(table));
            let mut rendered = Vec::new();
            write(&mut rendered, &name).unwrap();
            let rendered = String::from_utf8(rendered).unwrap();
            for column in table.columns {
                assert!(rendered.contains(column.name));
                assert!(rendered.contains(column.doc));
                let column_doc = json(&format!("{name}.{}", column.name)).unwrap();
                assert_eq!(column_doc["column"], json!(column));
            }
        }
    }

    #[test]
    fn guide_links_resolve_and_search_results_are_addressable() {
        for (name, doc) in baml_builtins2::language_topics()
            .iter()
            .filter(|(name, _)| owns(name))
        {
            resolve(name).unwrap();
            for text in [doc.syntax.as_deref(), doc.details.as_deref()]
                .into_iter()
                .flatten()
            {
                for tail in text.split("baml describe ").skip(1) {
                    let name = tail
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_')
                        .next()
                        .unwrap()
                        .trim_end_matches('.');
                    if owns(name) {
                        resolve(name).unwrap_or_else(|e| panic!("{name}: {e}"));
                    }
                }
            }
        }
        for term in ["query cloud", "input_args", "profiler", "missing values"] {
            let hits = search(term);
            assert!(!hits.is_empty(), "{term}");
            for hit in hits {
                resolve(&hit.path).unwrap();
            }
        }
        assert!(
            search("input_args")
                .iter()
                .any(|hit| hit.path == "query.tables.spans.input_args")
        );
        assert!(resolve("query.tables.values").is_err());
        assert!(resolve("query.tables.spans.not_a_column").is_err());
    }
}
