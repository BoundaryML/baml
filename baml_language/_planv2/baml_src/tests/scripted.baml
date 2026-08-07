// A deterministic fake: returns pre-written turns in order and records every
// input it received. Drives the loop in tests with no network.

class ScriptedClient {
    turns: root.ai.ModelTurn[],
    cursor: int,
    inputs: root.ai.ModelTurnInput[],

    function new(turns: root.ai.ModelTurn[]) -> ScriptedClient {
        ScriptedClient { turns: turns, cursor: 0, inputs: [] }
    }

    function received(self) -> root.ai.ModelTurnInput[] {
        self.inputs
    }

    implements root.ai.Client {
        function id(self) -> string {
            "scripted"
        }

        function invoke(self, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
            self.inputs.push(input);
            let turn = self.turns.at(self.cursor);
            self.cursor += 1;
            match (turn) {
                let t: root.ai.ModelTurn => t,
                null => {
                    throw baml.errors.InvalidArgument { message: "ScriptedClient ran out of turns" }
                },
            }
        }
    }
}
