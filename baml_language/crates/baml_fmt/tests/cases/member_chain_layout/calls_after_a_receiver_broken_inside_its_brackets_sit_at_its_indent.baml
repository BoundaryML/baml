function judge(programs: string[], compiles: bool[], claims: string[], reasoning: string) -> Judged {
    grade@spec(programs, compiles, claims, answer_sentence(answer), asked_sentence(answer), reasoning).call(client = judge_client(key, model)) catch (e) {
        ai.errors.Failure => Unjudged { why: `${e}` },
    }
}

function literals() -> int {
    let joined = [first_element, second_element, third_element, fourth_element, fifth].join(", ").length();
    let field = Config { first_field: first_value, second_field: second_value }.first_field.length();
    let result = baml.future.with_timeout(baml.time.Duration.from_milliseconds(c.timeout_ms), () -> {
        let input = 1;
        input
    }) catch (e) {
        baml.errors.Timeout => 0,
    };
    0
}
