// The Journal: the append-only typed record of one run.
//
// The event types themselves live in root.events, and the content blocks they
// carry live in root.content. Journal stays at the package root because it is
// one of the nine names the BEP's flow diagram names directly.
//
// Mirrors _planv2/pages/02_guides/04_the_journal.md.

/// The COMPLETE record. Repair attempts, feedback, failures — everything that
/// happened is an event.
class Journal {
    log: root.events.Event[],

    function entries(self) -> root.events.Event[] {
        self.log
    }

    /// A journal for one run of a spec, with RunStarted appended.
    function new<Out>(spec: FunctionSpec<Out>) -> Journal {
        Journal {
            log: [root.events.RunStarted { spec_name: spec.name(), arguments: spec.arguments() }],
        }
    }

    /// The driving runner is the only writer; clients and tools never append.
    function append_all(self, events: root.events.Event[]) -> void {
        for (let e in events) {
            self.log.push(e);
        }
    }
}
