//! Minimal native SQLite provider. No request data is cached.
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use bex_heap::BexHeap;
use rusqlite::{
    Connection, OpenFlags,
    types::{Value, ValueRef},
};
use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, VmBamlError};

use crate::NativeSysOps;

static CONNECTIONS: OnceLock<Mutex<HashMap<String, Connection>>> = OnceLock::new();

fn query(path: &str, sql: &str, params_json: &str) -> Result<String, Box<dyn std::error::Error>> {
    let params: Vec<serde_json::Value> = serde_json::from_str(params_json)?;
    let params = params
        .into_iter()
        .map(|v| match v {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::String(s) => Ok(Value::Text(s)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Real(f))
                } else {
                    Err("number outside SQLite range")
                }
            }
            _ => Err("parameters must be null, number or string"),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut connections = CONNECTIONS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "SQLite connection lock poisoned")?;
    let conn = match connections.entry(path.to_owned()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            // Do not silently create a missing database.
            let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            conn.busy_timeout(Duration::from_secs(5))?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA cache_size=-65536;")?;
            conn.set_prepared_statement_cache_capacity(32);
            entry.insert(conn)
        }
    };
    let mut stmt = conn.prepare_cached(sql)?;
    let columns = stmt.column_count();
    let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let mut values = Vec::with_capacity(columns);
        for i in 0..columns {
            values.push(match row.get_ref(i)? {
                ValueRef::Null => serde_json::Value::Null,
                ValueRef::Integer(v) => v.into(),
                ValueRef::Real(v) => serde_json::json!(v),
                ValueRef::Text(v) => std::str::from_utf8(v)?.into(),
                ValueRef::Blob(_) => return Err("blob results are not supported".into()),
            });
        }
        result.push(values);
    }
    drop(rows);
    drop(stmt); // finalize/reset RETURNING before reporting a committed write
    Ok(serde_json::json!({"rows": result, "changes": conn.changes()}).to_string())
}

impl io::IoNamespaceSqlite for NativeSysOps {
    fn query(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        sql: String,
        params_json: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        match query(&path, &sql, &params_json) {
            Ok(value) => SysOpOutput::ok(value),
            Err(error) => SysOpOutput::err(VmBamlError::Io {
                message: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_returning_commit_and_fresh_reads() {
        let filename = std::env::temp_dir().join(format!(
            "baml-sqlite-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let observer = Connection::open(&filename).unwrap();
        observer
            .execute_batch("CREATE TABLE items(id INTEGER PRIMARY KEY, body TEXT NOT NULL)")
            .unwrap();
        let path = filename.to_str().unwrap();
        let body = "quote ' and \"; DROP TABLE items; --";
        let params = serde_json::json!([body]).to_string();
        let result: serde_json::Value = serde_json::from_str(
            &query(
                path,
                "INSERT INTO items(body) VALUES (?) RETURNING id,body",
                &params,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result["rows"][0][1], body);
        assert_eq!(result["changes"], 1);
        // A separate connection sees the write before query() returns.
        let seen: String = observer
            .query_row("SELECT body FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seen, body);
        observer
            .execute("UPDATE items SET body='updated'", [])
            .unwrap();
        let result: serde_json::Value =
            serde_json::from_str(&query(path, "SELECT body FROM items", "[]").unwrap()).unwrap();
        assert_eq!(result["rows"][0][0], "updated");
        assert!(query(path, "SELECT ?", "[{}]").is_err());
        assert!(query(path, "SELECT ?", "not JSON").is_err());
        // Windows cannot unlink a database while either connection is open.
        drop(CONNECTIONS.get().unwrap().lock().unwrap().remove(path));
        drop(observer);
        std::fs::remove_file(filename).unwrap();
    }
}
