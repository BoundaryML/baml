type Item = Table | Item[] | bool | int | float | string | Datetime

class Table {
    items: map<string, Item>

    function to_json(self) -> baml.json.json {
        self.items.to_json()
    }
    /// Converts a JSON object to a TOML table.
    ///
    /// This is lossy: JSON `null` values are skipped.
    function from_json(json: baml.json.json) -> Table throws baml.json.JsonDecodeError {
        match (json) {
            map<string, baml.json.json> => {
                let items: map<string, Item> = map {};
                for (let key in json.keys()) {
                    items[key] = match (json[key]) {
                        null => { continue; }, // skip null values
                        let item => item_from_json(item)
                    }
                }
                Table { items: items }
            },
            _ => throw baml.json.JsonDecodeError { message: "expected map", path: "" }
        }
    }

    /// Parses the text of a TOML file into a table.
    //baml:mut_vm
    function parse(s: string) -> Table throws TomlParseError {
        $rust_function
    }
}

/// TODO: use proper datetime stuff when it is added.
class Datetime {
    value: string

    function to_json(self) -> baml.json.json {
        self.value
    }
    function from_json(json: baml.json.json) -> Datetime throws baml.json.JsonDecodeError {
        match (json) {
            string => Datetime { value: json },
            _ => throw baml.json.JsonDecodeError { message: "expected string", path: "" }
        }
    }
}

/// Converts a TOML item to a JSON value.
///
/// This is lossy: TOML `Datetime` values are converted to JSON strings.
function item_to_json(item: Item) -> baml.json.json {
    match (item) {
        bool | int | float | string => item,
        Table => item.to_json(),
        Item[] => item.map(item_to_json),
        Datetime => item.value,
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
    message: string
}
