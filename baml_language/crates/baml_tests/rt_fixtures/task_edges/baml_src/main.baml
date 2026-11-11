class TaskFailure { code:int }
function fail_task()->int throws TaskFailure {throw TaskFailure {code:17};}
function pause_task(value:int)->int {
    defer {baml.sys.sleep(baml.time.Duration.from_milliseconds(1));}
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    value+1
}
function RtTaskErrors()->int {
    let failed=spawn {fail_task()};
    let a=(await failed) catch(e) {let e:TaskFailure=>e.code};
    let b=(await failed) catch(e) {let e:TaskFailure=>e.code};
    let panicked=spawn {let n=4611686018427387903; n+1};
    let c=(await panicked) catch_all_panics(e) {_=>1};
    // A private environment still executes arbitrary calls, suspension and
    // defer. A stored callback keeps the ordinary escaping representation.
    let private_task=spawn {pause_task(4)};
    let stored=()->int {pause_task(8)};
    let captured_task=spawn {stored()};
    if((await private_task)!=5 || (await captured_task)!=9) {return -1;}
    a+b+c
}
function RtBackgroundTask()->int {let background=spawn {4}; 7}
function RtBackgroundError()->int {let background=spawn {let n=4611686018427387903; n+1}; 7}
function RtTokenComposition()->int {
    let first=baml.spawn.CancelToken.new();
    let linked=baml.spawn.CancelToken.any([first]);
    let a=first.cancel();
    let b=linked.cancel();
    let own=baml.spawn.CancelToken.new();
    let outer=baml.spawn.CancelToken.any([own]);
    let c=outer.cancel();
    if(a!=1 || b!=0 || c!=1 || own.is_cancelled()) {return -1;}
    let mut_token=baml.spawn.CancelToken.any([]);
    for(let i=0;i<500;i+=1) {mut_token=baml.spawn.CancelToken.any([mut_token]);}
    if(mut_token.is_cancelled()) {return -2;}
    1
}
function fail_after(code:int, millis:int)->int throws TaskFailure {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(millis));
    throw TaskFailure {code:code};
}
function fail_now(code:int)->int throws TaskFailure {throw TaskFailure {code:code};}
function overflow_after(millis:int)->int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(millis));
    let n=4611686018427387903;
    n+1
}
// Several producers fail unawaited, in an order unrelated to their spawn
// order: late, immediate and panicking failures, interleaved with awaited
// failures, successes, nested producers and spawns after earlier settlement.
function RtUnhandledOrder()->int {
    let late=spawn {fail_after(1,30)};
    let first=spawn {fail_now(2)};
    let ok=spawn {3};
    let panicking=spawn {let n=4611686018427387903; n+1};
    let late_panic=spawn {overflow_after(15)};
    let observed=spawn {fail_now(4)};
    let w=await ok;
    let x=(await observed) catch(e) {let e:TaskFailure=>e.code};
    let nested=spawn {
        let inner=spawn {fail_now(5)};
        let sibling=spawn {fail_after(6,5)};
        11
    };
    let n=await nested;
    let after=spawn {fail_now(7)};
    let middle=spawn {fail_after(8,10)};
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60));
    let last=spawn {fail_now(9)};
    w+x+n
}
