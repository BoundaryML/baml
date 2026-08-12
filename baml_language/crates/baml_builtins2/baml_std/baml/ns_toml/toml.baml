/// TOML's four datetime kinds map losslessly onto the `baml.time` types
/// (see BEP-021): offset datetime → `ZonedDateTime` (fixed offset), local
/// datetime → `PlainDateTime`, local date → `PlainDate`, local time →
/// `PlainTime`.
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

/// A TOML [table](https://toml.io/en/latest#table).
/// Represents a collection of key/value pairs.
class Table {
    items: map<string, Item>,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            // Render the items map structurally; `baml.json.from` recurses each
            // `Item` value, honoring the `baml.ToJson` overrides on the nested
            // `baml.time.*` types (and nested `Table`s).
            baml.json.from(self.items)
        }
    }
    /// Converts a JSON object to a TOML table.
    ///
    /// This is lossy: JSON `null` values are skipped, and datetime strings
    /// stay strings (JSON has no datetime type to distinguish them by).
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            // Delegate to a concrete-`Table`-returning helper: matching the
            // recursive `map<string, baml.json.json>` pattern against the abstract
            // `Self` return type confuses arm-reachability analysis.
            table_from_json(j)
        }
    }

    /// Parses the text of a TOML file into a table.
    //baml:mut_vm
    function parse(s: string) -> Table throws TomlParseError {
        $rust_function
    }
}

/// Decodes a JSON object into a `Table`, skipping null values. Returns a
/// concrete `Table` so arm-reachability over the recursive `json` map pattern
/// is analyzed without the abstract `Self` of `Table`'s `baml.FromJson` impl.
function table_from_json(j: baml.json.json) -> Table throws baml.json.JsonDecodeError {
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
        _ => throw baml.json.JsonDecodeError { message: "expected map", path: "" },
    }
}

/// Converts a TOML item to a JSON value.
///
/// This is lossy: TOML datetime values are converted to JSON strings
/// (`ZonedDateTime` as RFC 3339, the `Plain*` types as zoneless ISO 8601).
function item_to_json(item: Item) -> baml.json.json throws baml.json.JsonSerializationError {
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

/// Errors if the JSON value is `null`.
/// Usually `Table.from_json` is a better entrypoint, as it will skip null values without erroring.
function item_from_json(json: baml.json.json) -> Item throws baml.json.JsonDecodeError {
    match (json) {
        bool | int | float | string => json,
        null => throw baml.json.JsonDecodeError { message: "TOML does not support null", path: "" },
        map<string, baml.json.json> => Table.from_json(json),
        baml.json.json[] => json.map(item_from_json),
    }
}

class TomlParseError {
    message: string,
}
