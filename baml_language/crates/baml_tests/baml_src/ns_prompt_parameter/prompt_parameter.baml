function echo_multiline(prompt: string) -> string throws never {
    prompt
}

function echo_single_line(prompt: string) -> string throws never { prompt }

function add_suffix(
    prefix: string,
    /* The prompt is intentionally not renamed. */
    prompt: string,
    suffix: string,
) -> string throws never {
    prefix + prompt + suffix
}

function prompt_then_string(prompt: string) -> string throws never {
    prompt;
    "tail"
}

function call_then_prompt_then_string(
    client: (string) -> string throws never,
    prompt: string,
) -> string throws never {
    client("ignored");
    prompt;
    "tail"
}

interface Echo {
    function echo(self, prompt: string) -> string throws never {
        prompt
    }
}

class EchoImpl {
    implements Echo {}
}

test "prompt parameter expression bodies" {
    assert.equal(echo_multiline("multiline"), "multiline");
    assert.equal(echo_single_line("single line"), "single line");
    assert.equal(add_suffix("[", "middle", "]"), "[middle]");
    assert.equal(prompt_then_string("ignored"), "tail");
    assert.equal(
        call_then_prompt_then_string(
            (_: string) -> string {
                "unused"
            },
            "ignored",
        ),
        "tail",
    );
    assert.equal(EchoImpl {  }.echo("default method"), "default method")
}
