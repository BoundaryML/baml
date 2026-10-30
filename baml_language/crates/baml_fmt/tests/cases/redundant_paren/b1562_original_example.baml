class MedicalDoc {
    document: string?,
    follow_up: string?,
}

test "medical_doc" {
    let result = MedicalDoc { document: `Shadow`, follow_up: null };
    let output_document = reflect.Class.get_field<string?>(result, "document");
    let output_follow_up = reflect.Class.get_field<string?>(result, "follow_up");

    assert.is_true(
        (
            (
                (
                    (
                        (
                            (
                                (output_document != null)
                                    && (output_document ?? "").includes(`Shadow`)
                            )
                                && (output_document ?? "").includes(`Amlodipine`)
                        )
                            && (output_document ?? "").includes(`Semintra`)
                    )
                        && (output_document ?? "").includes(`BUN`)
                )
                    && (output_document ?? "").includes(`42 mg/dL`)
            )
                && (output_document ?? "").includes(`11/04/2025`)
        ),
    )
}
