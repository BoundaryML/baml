// A deterministic fake: returns pre-written turns in order and records every
// input it received. Drives the loop in tests with no network.

class ScriptedClient {
    turns: ai.ModelTurn[],
    cursor: int,
    inputs: ai.ModelTurnInput[],

    function new(turns: ai.ModelTurn[]) -> ScriptedClient {
        ScriptedClient { turns: turns, cursor: 0, inputs: [] }
    }

    function received(self) -> ai.ModelTurnInput[] {
        self.inputs
    }

    implements ai.Client {
        function id(self) -> string {
            "scripted"
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            self.inputs.push(input);
            let turn = self.turns.at(self.cursor);
            self.cursor += 1;
            match (turn) {
                let t: ai.ModelTurn => t,
                null => {
                    throw baml.errors.InvalidArgument { message: "ScriptedClient ran out of turns" }
                },
            }
        }
    }
}
