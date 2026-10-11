# Initial native SQLite API

BAML HTTP applications need in-process database access. This proposal adds one
small native API that is sufficient for the feed benchmark:

```baml
function query(path: string, sql: string, params_json: string) -> string
    throws baml.errors.Io
```

Call it as `baml.sqlite.query`. The database must already exist. Parameters are
a JSON array of null, numbers, or strings, bound positionally by SQLite. The
result is compact JSON with `rows` (arrays in SELECT column order) and `changes`
(SQLite's last statement change count). Blob results are currently unsupported.
For example:

```baml
let result = baml.json.parse(baml.sqlite.query(
    database_path,
    "SELECT username FROM users WHERE id = ?",
    "[1]",
));
```

The native provider retains connections for the process lifetime, keyed by path,
and serializes calls under a mutex. It reuses up to 32 prepared statements per
connection and never caches row results. Connections open read/write without
creating missing files; their defaults are WAL, synchronous=NORMAL, a 5-second
busy timeout, foreign keys enabled, and a 64 MiB page cache. Statements are
stepped to completion, including INSERT ... RETURNING, so ordinary autocommit
writes finish before the caller receives the result. Explicit SQL transactions
are not a request isolation API and should not be shared between concurrent
callers.

SQLite is bundled through rusqlite. Hosts without this capability retain the
generated default provider, which reports HostUnavailable. No browser SQLite
implementation is proposed here.

This deliberately limited initial surface avoids adding a pool, a separate
service, schema generation, or a query language. Connection handles with explicit
close/transaction ownership and structured parameter/row types can be discussed
as follow-up API design. The draft benchmark submission consumes this capability
from BAML; it does not carry its own runtime patch.

Validation includes a native regression test for bound strings, committed
RETURNING writes visible from another connection, fresh reads after external
updates, and invalid parameter input. A BAML feed server has passed all 42
benchmark correctness checks as a packed executable, including an optimized pack
host build.
