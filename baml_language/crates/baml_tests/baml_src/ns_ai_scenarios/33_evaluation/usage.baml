// scenario 33 — evaluation: ../../../../../llm-provider/ideas/scenarios/33-evaluation/README.md
// (also 36 negotiation). Live tier = eval_judge_live (Rust ai_provider suite).

class EvalScore {
    value: float,
    reason: string,
}

interface Scorer {
    function score(self, input: string, output: string) -> EvalScore throws never
}

class KeywordJudge {
    keyword: string,
    implements Scorer {
        function score(self, input: string, output: string) -> EvalScore throws never {
            if (output.includes(self.keyword)) {
                EvalScore { value: 1.0, reason: "contains " + self.keyword }
            } else {
                EvalScore { value: 0.0, reason: "missing " + self.keyword }
            }
        }
    }
}

function run_eval(
    task: baml.ai.Provider,
    judge: Scorer,
    input: string,
) -> EvalScore throws baml.errors.CallError | baml.errors.UnknownError {
    let output = match (task) {
        let h: baml.ai.HttpProvider => h.call<string>(input),
        _ => throw baml.errors.Unsupported { message: "task client cannot produce a value" },
    };
    judge.score(input, output)
}

test "33: the eval runner scores task output with a judge" {
    let s = run_eval(EchoProvider { reply: "the answer is paris" }, KeywordJudge { keyword: "paris" }, "capital of france?") catch (e) {
        _ => EvalScore { value: -1.0, reason: "eval failed" },
    };
    assert.is_true(s.value > 0.5)
}

test "36: the Support lattice is queryable before sending" {
    let p: baml.ai.Provider = baml.ai.OpenAi { model: "m", api_key: "k", base_url: null };
    let label = match (p) {
        let cap: baml.ai.Capabilities => match (cap.structured_output()) {
            baml.ai.Support.Yes => "yes",
            baml.ai.Support.No => "no",
            baml.ai.Support.Maybe => "maybe",
        },
        _ => "unknown",
    };
    assert.is_true(label == "yes" || label == "maybe")
}
