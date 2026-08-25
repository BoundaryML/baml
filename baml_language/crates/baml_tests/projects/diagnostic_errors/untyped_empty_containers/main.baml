function fail() -> never throws string {
    throw "boom"
}

function catch_array() -> null {
    let values = fail() catch (e) {
        string => []
    };
    for (let value in values) {
        log.info(value)
    }
    null
}

function bare_array() -> null {
    let values = [];
    null
}

function bare_map() -> null {
    let values = {};
    null
}

function bare_map_constructor() -> null {
    let values = map {  };
    null
}

function contextual_types_are_enough() -> null {
    let array: int[] = [];
    let object_map: map<string, int> = {};
    let constructor_map: map<string, int> = map {  };
    null
}
