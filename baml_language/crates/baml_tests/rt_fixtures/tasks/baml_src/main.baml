function main() -> int {
  let s = 0;
  for (let i = 0; i < 10000; i += 1) {
    s += await spawn { 1 };
  };
  s
}

function RtSpawnAwait() -> int { main() }

class TaskResource {
    events: string[],
    function cleanup(self) -> void { self.events.push("cleanup"); }
}
function RtTaskControls() -> int {
    let events: string[] = [];
    let resource = TaskResource { events: events };
    {
        defer { resource.cleanup(); }
        let child = spawn { 7 };
        let other = spawn { 11 };
        let index = baml.future.__await_any([child, other]);
        let _ = await child;
        let _ = await other;
        if (!child.is_result() || !other.is_settled() || index < 0) { return -1; }
        resource.cleanup();
    };
    resource.cleanup();
    let token = baml.spawn.CancelToken.new();
    token.cancel();
    let pending = spawn with token { events.push("wrong"); 1 };
    let first = pending.cancel();
    let second = pending.cancel();
    let value = (await pending) catch_all_panics (e) { _ => -1 };
    if (second || !pending.is_cancelled() || value != -1) { return -2; }
    events.length()
}
function RtTaskSleep() -> int {
    let events: string[] = [];
    let child = spawn {
        defer {
            events.push("defer");
            baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
            events.push("done");
        }
        events.push("ready");
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000));
        7
    };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
    child.cancel();
    let result = (await child) catch_all_panics (e) { _ => -1 };
    if (result != -1 || events.length() != 3) { return -1; }
    events.length()
}

class DoubleTask {}
implements<E> baml.spawn.Modifier<int,E> for DoubleTask {
    function apply(self,plan:baml.spawn.Plan<int,E>)->baml.spawn.Plan<int,E> throws never {
        plan.wrap((body: () -> int throws E) -> int throws E {body()*2})
    }
}
function RtTaskModifiers()->int {
    let limit=baml.spawn.Limit.new(1);
    let token=baml.spawn.CancelToken.new();
    let combined=baml.spawn.CancelToken.any([token]);
    let double=DoubleTask {};
    let wrapped=spawn "wrapped" with double,limit {21};
    let first=token.cancel();
    let second=token.cancel();
    let cancelled=spawn with limit,combined {99};
    let answer=await wrapped;
    let result=(await cancelled) catch_all_panics (e) {_=>-1};
    if (first!=1 || second!=0 || result!=-1 || !combined.is_cancelled()) {return -1;}
    answer
}

function RtScalarTaskOrder()->int {
    let events:string[]=[];
    let first=spawn {events.push("ready"); 7};
    let answer=await spawn {2};
    if(events.length()!=1) {return -1;}
    let prior=await first;
    let stored=spawn {3};
    let second=await stored;
    prior+answer+second
}
