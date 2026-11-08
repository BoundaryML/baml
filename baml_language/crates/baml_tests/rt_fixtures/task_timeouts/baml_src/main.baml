function RtTaskLimits()->int {
    let limit=baml.spawn.Limit.new(2);
    if(limit.capacity()!=2 || limit.active_count()!=0 || limit.queued_count()!=0) {return -1;}
    let bad=baml.spawn.Limit.new(0) catch(e) {let e:baml.errors.InvalidArgument=>null};
    2
}
function RtTaskTimeout()->int {
    // Let both producers enter before either result settles, so cancellation
    // does not race the VM executor's initial scheduling of a deadline task.
    let a=baml.future.with_timeout(baml.time.Duration.from_milliseconds(60000),()->int {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(50)); 17
    });
    let b=baml.future.with_timeout(baml.time.Duration.from_milliseconds(50),()->int {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000)); 99
    }) catch(e) {let e:baml.errors.Timeout=>25};
    a+b
}
class CollectedTaskResource {events:string[], function cleanup(self)->void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1));
    self.events.push("cleanup");
}}
function release_task_resource(events:string[],n:int)->int {
    if(n>0) {release_task_resource(events,n - 1)} else {let resource=CollectedTaskResource {events:events}; 0}
}
function RtAutomaticCleanup()->int {
    let events:string[]=[];
    let producer=spawn {release_task_resource(events,1)};
    let count=await producer;
    baml.sys.collect_garbage();
    events.length()
}
