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
class Table {
    items: map<string, Item>,

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
            let items: map<string, Item> = map {  };
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
/// - `baml.json.DecodeError` if `json` is `null`, or if a `null` appears as an
///   array element anywhere beneath it. Nulls in *object* values are skipped
///   by the `Table.from_json` this recurses through.
function item_from_json(json: baml.json.json) -> Item throws baml.json.DecodeError {
    match (json) {
        bool | int | float | string => json,
        null => throw baml.json.DecodeError { message: "TOML does not support null", path: "" },
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
