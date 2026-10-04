/// Everything a TOML `Table` can hold.
///
/// TOML has no null, so every arm is a real value and there is no absent case
/// to narrow away — unlike `baml.json.json`. Tables nest as `Table`, arrays as
/// `Item[]`, and the scalars map onto the BAML primitives.
///
/// TOML's four datetime kinds map losslessly onto the `baml.time` types (see
/// BEP-021): offset datetime → `ZonedDateTime` (fixed offset), local datetime
/// → `PlainDateTime`, local date → `PlainDate`, local time → `PlainTime`. That
/// is why `Item` is its own type rather than a reuse of `baml.json.json`,
/// which would flatten all four to strings.
type Item = Table
    | Item[]
    | bool
    | int
    | float
    | string
    | baml.time.ZonedDateTime
    | baml.time.PlainDateTime
    | baml.time.PlainDate
    | baml.time.PlainTime;

/// A TOML [table](https://toml.io/en/latest#table): a collection of key/value
/// pairs.
///
/// A parsed document is itself a `Table`. Nested tables — `[a.b]`, inline
/// `{ ... }`, and array-of-table entries — are `Table` values inside `items`.
///
/// ```baml
/// let config = baml.toml.Table.parse(source);
/// config.table("server").set("port", 3000);
/// config.table("server").set("enabled", true);
/// let text = config.to_string();
/// ```
class Table {
    items: map<string, Item>,
    /// (internal) Original document text, retained by parse for serialization.
    _source: string?,
    /// (internal) Source keys for edited entries; null marks a newly inserted key.
    _renamed: map<string, string?>?,

    /// Creates an empty table. Values can also be edited directly through `items`.
    function new() -> Table throws never {
        Table { items: map {} }
    }

    /// Returns a value, or null when the key is absent.
    function get(self, key: string) -> Item? throws never {
        self.items.get(key)
    }

    /// Sets any TOML value: scalars, datetimes, arrays, or nested tables.
    function set(self, key: string, value: Item) -> void throws never {
        if (!self.items.has(key)) {
            let renamed: map<string, string?> = self._renamed ?? map {};
            renamed[key] = null;
            self._renamed = renamed;
        }
        self.items[key] = value;
    }

    /// Removes and returns a value, or null when the key is absent.
    function remove(self, key: string) -> Item? throws never {
        let removed = self.items.delete(key);
        let renamed: map<string, string?> = self._renamed ?? map {};
        renamed.delete(key);
        self._renamed = renamed;
        removed
    }

    /// Returns a child table, creating it when absent. Throws if the existing
    /// value is not a table. Chain calls to work with nested configuration.
    function table(self, key: string) -> Table throws root.errors.InvalidArgument {
        match (self.items.get(key)) {
            let child: Table => child,
            null => {
                let child = Table.new();
                self.set(key, child);
                child
            },
            _ => throw root.errors.InvalidArgument { message: "Expected TOML table: " + key },
        }
    }

    /// Renames a key together with its value and comments. Returns false when
    /// absent; throws if the destination already exists. Renaming to the same
    /// name is a no-op. No comments move implicitly when removing/adding keys.
    function rename(self, from: string, to: string) -> bool throws root.errors.InvalidArgument {
        if (!self.items.has(from)) {
            return false;
        }
        if (from == to) {
            return true;
        }
        if (self.items.has(to)) {
            throw root.errors.InvalidArgument { message: "TOML key already exists: " + to };
        }
        let value = self.items.delete(from) ?? root.sys.panic("TOML key disappeared during rename");
        self.items[to] = value;
        let renamed: map<string, string?> = self._renamed ?? map {};
        let original = if (renamed.has(from)) {
            renamed.delete(from)
        } else {
            from
        };
        renamed[to] = original;
        self._renamed = renamed;
        true
    }

    /// Serializes the table to TOML. For a parsed document, untouched values,
    /// comments, and formatting are preserved; changed values keep their key's
    /// comments. Edits through `items` and nested tables are included. Panics
    /// for cyclic values or datetimes outside TOML's supported range.
    /// Named timezones serialize as UTC because TOML stores offsets rather
    /// than zone identifiers. Fixed offsets must be whole minutes.
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl() catch (e) {
                root.errors.InvalidArgument { message } => {
                    root.sys.panic("TOML serialization: " + message)
                },
            }
        }
    }

    /// (internal) Fallible TOML encoder used by ToString.
    //baml:mut_vm
    function _to_string_impl(self) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    implements baml.ToJson {
        /// Renders the table as a JSON object.
        ///
        /// Lossy in the same direction `Item` is rich: the `baml.time.*` arms
        /// render as strings, since JSON has no datetime type.
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            // Render the items map structurally; `baml.json.from` recurses each
            // `Item` value, honoring the `baml.ToJson` overrides on the nested
            // `baml.time.*` types (and nested `Table`s).
            baml.json.from(self.items)
        }
    }
    implements baml.FromJson {
        /// Converts a JSON object to a TOML table.
        ///
        /// Lossy: JSON `null` values are skipped, and datetime strings stay
        /// strings, since JSON has no datetime type to tell them apart by.
        ///
        /// Skipping nulls rather than rejecting them is what makes this the
        /// forgiving entry point; `item_from_json` throws on a bare `null`.
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            // Delegate to a concrete-`Table`-returning helper: matching the
            // recursive `map<string, baml.json.json>` pattern against the abstract
            // `Self` return type confuses arm-reachability analysis.
            table_from_json(j)
        }
    }

    /// Parses the text of a TOML file into a table.
    ///
    /// # Throws
    ///
    /// - `baml.toml.ParseError` if `s` is not valid TOML; if an integer does
    ///   not fit BAML's 63-bit `int` (TOML integers are 64-bit, so the top and
    ///   bottom of the TOML range are unrepresentable); or if a datetime names
    ///   a calendar date that does not exist.
    //baml:mut_vm
    function parse(s: string) -> Table throws ParseError {
        $rust_function
    }
}

/// Decodes a JSON object into a `Table`, skipping null values.
///
/// A `null` object value is dropped key and all — the key does not appear in
/// `items` — and nested objects are decoded the same way. A `null` inside an
/// *array* has no key to drop, so it throws: TOML arrays have no hole to put
/// it in.
///
/// The return type is a concrete `Table` so that arm-reachability over the
/// recursive `json` map pattern is analyzed without the abstract `Self` of
/// `Table`'s `baml.FromJson` impl.
///
/// # Throws
///
/// - `baml.json.DecodeError` if `j` is not a JSON object, or if a `null`
///   appears as an array element anywhere beneath it.
function table_from_json(j: baml.json.json) -> Table throws baml.json.DecodeError {
    match (j) {
        map<string, baml.json.json> => {
            let items: map<string, Item> = map {};
            for (let key in j.keys()) {
                items[key]
                    = match (j[key]) {
                        null => {
                            continue;
                        }, // skip null values
                        let item => item_from_json(item),
                    }
            }
            Table { items: items }
        },
        _ => throw baml.json.DecodeError { message: "expected map", path: "" },
    }
}

/// Converts a TOML item to a JSON value.
///
/// Lossy: TOML datetimes become JSON strings — `ZonedDateTime` as RFC 3339,
/// the `Plain*` types as zoneless ISO 8601. The conversion does not
/// round-trip, because `item_from_json` reads those strings back as strings.
///
/// # Throws
///
/// - `baml.json.SerializationError` if a datetime cannot be formatted: a year
///   outside the 4-digit range the ISO 8601 / RFC 3339 forms admit, or a
///   `ZonedDateTime` naming an IANA zone the host does not know.
function item_to_json(item: Item) -> baml.json.json throws baml.json.SerializationError {
    match (item) {
        bool | int | float | string => item,
        Table => item.to_json(),
        Item[] => item.map(item_to_json),
        baml.time.ZonedDateTime => item.to_json(),
        baml.time.PlainDateTime => item.to_json(),
        baml.time.PlainDate => item.to_json(),
        baml.time.PlainTime => item.to_json(),
    }
}

/// Converts a JSON value to a TOML item.
///
/// `Table.from_json` is usually the better entry point, since it skips nulls
/// instead of erroring on them. This inverts `item_to_json` only up to the
/// datetime loss noted there: a JSON string stays a `string`, never a
/// `baml.time` value.
///
/// # Throws
///
/// - `baml.json.DecodeError` if `json` is `null` or `bigint`, or if either
///   appears as an array element anywhere beneath it. Nulls in *object* values
///   are skipped by the `Table.from_json` this recurses through.
function item_from_json(json: baml.json.json) -> Item throws baml.json.DecodeError {
    match (json) {
        bool | int | float | string => json,
        null => throw baml.json.DecodeError { message: "TOML does not support null", path: "" },
        bigint => throw baml.json.DecodeError { message: "TOML does not support bigint", path: "" },
        map<string, baml.json.json> => Table.from_json(json),
        baml.json.json[] => json.map(item_from_json),
    }
}

/// TOML text that cannot be read as a `Table`.
///
/// Covers both syntax errors and values TOML admits but BAML cannot hold,
/// notably 64-bit integers outside BAML's 63-bit `int`. For a syntax error
/// `message` is the parser's own diagnostic, locating the offending span; for
/// an unrepresentable value it names the limit instead.
class ParseError {
    message: string,
}
