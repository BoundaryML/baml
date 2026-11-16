function f(sections: string[], pet_name: string) -> null {
    assert.is_true(
        (pet_name == `Bella`)
            && !(
                (
                    (
                        (sections).map((item) -> {
                            item.to_string()
                        })
                    )
                        .join(` `)
                )
                    .includes(`WarningSignsContact`)
            ),
    );
    null
}
