class Record {
    value: string @description(#"legacy description"#)
}

function hash_strings() -> string {
    let scalar = #"value"#;
    let values = {
        #"key"#: ##"nested \"# delimiter"##,
    };
    `${scalar}: ${values.length()}`
}
