// A client is a stateless codec + transport for one provider wire format.
// Mirrors _plan/pages/02_guides/04_models.md.

class ProviderRequest {
    url: string,
    headers: map<string, string>,
    body_json: string,
}

class ProviderResponse {
    status: int,
    body_json: string,
}

// Duties: render (journal -> provider-native request), invoke (one
// stateless call), ingest (response -> events, raw payload preserved).
// Clients render any event union but only ever PRODUCE built-in events.
interface Client {
    type Ev = Event

    function id(self) -> string throws never
    function render(self, j: Journal<Self.Ev>, tb: Toolbox, output_schema: string) -> ProviderRequest throws unknown
    function invoke(self, req: ProviderRequest) -> ProviderResponse throws unknown
    function ingest(self, resp: ProviderResponse) -> Event[] throws unknown
}

//# A fake provider: drives the whole loop deterministically in tests.

// Mirrors _plan/pages/04_advanced/02_evals.md.
class ScriptedClient<X> {
    turns: Event[][], // what "the model" does on call 1, 2, 3...
    i: int,

    implements Client {
        type Ev = Event | X

        function id(self) -> string {
            "scripted"
        }
        function render(
            self,
            j: Journal<Event | X>,
            tb: Toolbox,
            output_schema: string,
        ) -> ProviderRequest {
            ProviderRequest { url: "", headers: {}, body_json: "" }
        }
        function invoke(self, req: ProviderRequest) -> ProviderResponse {
            ProviderResponse { status: 200, body_json: "" }
        }
        function ingest(self, resp: ProviderResponse) -> Event[] {
            let batch: Event[] = self.turns.at(self.i) ?? [];
            self.i += 1;
            batch
        }
    }
}
