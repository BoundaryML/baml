/* Executable review model, not BAML runtime code. See README.md for its limits. */
(function (root) {
'use strict';
const clone = value => JSON.parse(JSON.stringify(value));
const evidence = {
  design: {label:'Proposed ABI • not implemented', href:'../MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md'},
  baml: {label:'Related BAML probes • this bridge case unimplemented', href:'../interface_probes/baml/RESULTS.md'},
  python: {label:'Python ownership model • no general interface ABI', href:'../interface_probes/python/pydantic_lifetime.py'},
  fields: {label:'BAML execution + Pydantic adapter model', href:'../interface_probes/python/pydantic_live_fields.py'},
  disposal: {label:'Real TS cleanup syntax + simulated ownership', href:'../interface_probes/typescript/README.md'},
  closure: {label:'Current Python runtime failure reproduced', href:'../INTERFACE_PROBE_RESULTS.md'},
  rust: {label:'Rust compiler + Arc ownership model', href:'../INTERFACE_PROBE_RESULTS.md'}
};
class Model {
  constructor() { this.objects={}; this.edges={}; this.vars={}; this.events=[]; this.scope='open'; this.frames=[]; this.allocations=0; this.releases=0; }
  object(id,label,value={},kind='receiver') { if(this.objects[id]) throw Error('Duplicate object '+id); this.objects[id]={id,label,value:clone(value),kind,status:'alive'}; return this; }
  own(id,label,to,kind='lease',from=null) { if(this.edges[id]) throw Error('Duplicate ownership '+id); if(this.objects[to]?.status!=='alive') throw Error('Own invalid receiver '+to); this.edges[id]={id,label,to,kind,from}; this.allocations++; return this; }
  transfer(id,label,from=null,kind) { const e=this.edges[id]; if(!e) throw Error('Transfer missing lease '+id); e.label=label; e.from=from; if(kind)e.kind=kind; return this; }
  drop(id) { if(this.edges[id]) {delete this.edges[id]; this.releases++;} return this; }
  value(id,value) { if(this.objects[id]?.status!=='alive') throw Error('Write freed object'); this.objects[id].value=clone(value); return this; }
  local(name,value) {this.vars[name]=clone(value);return this;}
  count(id) {return Object.values(this.edges).filter(e=>!id||e.to===id).length;}
  collect() { let changed; do {changed=false; for(const o of Object.values(this.objects)) if(o.status==='alive' && this.count(o.id)===0) {o.status='released'; for(const e of Object.values(this.edges)) if(e.from===o.id)this.drop(e.id); changed=true;}} while(changed); return this; }
  revoke() { this.scope='closing'; return this; }
  closeScope() {if(Object.values(this.edges).some(e=>e.kind==='pending'))throw Error('Cannot close scope with active work');this.scope='closed';return this;}
  frame(title,detail,checks=[],code='',warning='') {
    const dangling=Object.values(this.edges).filter(e=>this.objects[e.to]?.status!=='alive'||(e.from&&this.objects[e.from]?.status!=='alive'));
    const base=[['No ownership edge targets released storage',dangling.length===0],['Ownership accounting balances',this.allocations-this.releases===this.count()]];
    this.frames.push({title,detail,code,warning,objects:clone(this.objects),edges:clone(this.edges),vars:clone(this.vars),scope:this.scope,checks:[...base,...checks].map(([label,pass])=>({label,pass:!!pass})),allocations:this.allocations,releases:this.releases});return this;
  }
}
const cases=[];
function story(id,group,title,summary,source,gate,build,options={}) {
  const m=new Model();build(m);cases.push({id,group,title,summary,evidence:evidence[source],gate,frames:m.frames,...options});
}
function host(m) {return m.object('H','Host receiver',{count:0}).own('local','Local implementation','H','native');}
function vm(m) {return m.object('V','BAML receiver',{count:0}).own('root','BAML root','V','reach');}
function setupField(m) {return m.object('P','State owner',{count:1},'receiver').own('p','Live state ref','P');}
function child(m,id,label,value) {return m.object(id,label,value,'storage');}
function link(m,id,label,from,to) {return m.own(id,label,to,'reach',from);}
const countCheck=(m,n)=>['Exactly '+n+' live ownership edges',m.count()===n];
story('L01','Lifetime','An ordinary call ends','Construct locally, bind on first use, release the call’s temporary ownership, then release the local implementation.','design','Concurrent first use must bind once per source/runtime/scope; injected registration failure must unwind every waiter.',m=>{
 host(m).frame('Construct the implementation','Factory construction owns native state but creates no engine registration.', [['No bridge lease yet',Object.values(m.edges).every(e=>e.kind!=='lease')]],'greeter = Greeter.implement(host)');
 m.own('call','Invocation lease','H').frame('First async call binds','The invocation validates the fixed interface bundle and takes its own lease. Registry metadata is retained by these owners; it is not an extra immortal owner.',[countCheck(m,2)],'await Welcome_async(greeter, "Ada")');
 m.drop('call').frame('Call completes','The call releases its lease. The local implementation still owns its native receiver.',[countCheck(m,1)]);
 m.drop('local').collect().frame('Local ownership ends','There is no VM retention or worker left. The receiver and registration can be released.',[['Receiver released',m.objects.H.status==='released'],countCheck(m,0)]);
});
story('L02','Lifetime','BAML keeps the object','Disposing your local wrapper cannot invalidate a separately retained BAML reference.','python','Real VM retention must survive host proxy collection, disposal and callback-frame exit in every SDK.',m=>{
 host(m).own('vm','Retained BAML reference','H').frame('BAML stores the receiver','The two owners share one receiver identity. The registry retains host state independently of the SDK wrapper.',[countCheck(m,2)]);
 m.drop('local').frame('Dispose the local wrapper','The wrapper is closed for future use. BAML can still call its independently retained receiver.',[['Receiver remains alive',m.objects.H.status==='alive'],countCheck(m,1)],'using source = Greeter.implement(host)\n// Leaving this block releases only source.');
 m.value('H',{count:1}).frame('BAML calls it again','The call reaches the same host state, even though the local wrapper has gone away.',[['Same state changed',m.objects.H.value.count===1]]);
 m.drop('vm').collect().frame('BAML drops its final reference','After actual VM collection/release, the last ownership obligation ends.',[['Receiver released',m.objects.H.status==='released']]);
});
story('L03','Lifetime','Alias, clone and double close','Assignment aliases a wrapper. Explicit cloning creates independent ownership of the same receiver.','rust','Check alias invalidation, independent clone survival, and exactly-once native release under explicit close plus finalization.',m=>{
 vm(m).own('a','Wrapper A (also named B)','V').local('a and b','same wrapper').frame('Assign another variable','a and b name one wrapper and one lease. Rust moves differ; use Clone to obtain a second owned wrapper.',[countCheck(m,2)],'const b = a; // TypeScript alias');
 m.own('c','Wrapper C (explicit clone)','V').frame('Clone explicitly','C owns another lease but shares the receiver; cloning does not copy fields.',[countCheck(m,3)],'const c = a.clone(); // proposed proxy API');
 m.drop('a').drop('a').frame('Close A twice','B is also closed because it is the same wrapper. C and the BAML root remain valid. Repeated close releases nothing extra.',[countCheck(m,2)]);
 m.drop('c').drop('root').collect().frame('Remaining owners finish','Only the final release makes the receiver eligible for reclamation.',[countCheck(m,0)]);
});
story('L04','Lifetime','Return, retain, pass back','Returning a BAML interface or function creates a checked proxy. Passing it back preserves the original receiver.','closure','The current Python returned-closure pass-back probe panics. Replace adaptation order and test interface, function, field and nested-container round trips.',m=>{
 vm(m).own('out','Outbound transfer','V').frame('BAML returns a live value','Encoding transfers one owned lease. Its receiver, witness and runtime generation stay pinned.',[countCheck(m,2)]);
 m.transfer('out','Host checked proxy').drop('root').frame('Host adopts the return','The original call frame can finish; the adopted proxy now keeps the receiver alive.',[countCheck(m,1)]);
 m.own('in','Temporary input lease','V').frame('Pass the proxy back','Recognize a checked capability before attempting host adaptation. Do not create a new callback that calls this proxy.',[['Only one concrete receiver',Object.keys(m.objects).length===1]],'await Apply_async(returned_closure, 1)','Observed current Python failure: re-registering the returned closure enters a nested runtime and then masks the panic with an unknown SdkPanic type error. This frame specifies the replacement behavior.');
 m.drop('in').drop('out').collect().frame('Both sides finish','No wrapper-on-wrapper cycle or extra receiver was created.',[countCheck(m,0)]);
});
story('L05','Lifetime','Project another interface','Client overrides, required interfaces and media views share identity without user-written casts.','design','Direct client overrides and Agent.run must preserve receiver state, exact pins and independent view lifetime.',m=>{
 vm(m).own('client','ResponsesClient facade','V').frame('A concrete client exists','The facade owns a checked concrete receiver.',[],'client = ResponsesClient(...)');
 m.own('view','Client input lease','V').frame('Use it as Client','The parameter codec projects an implemented interface internally. No .as_client() call and no new receiver.',[['Receiver identity unchanged',Object.keys(m.objects).length===1]],'await Extract_async(text, options={client: client})');
 m.drop('client').frame('Local facade closes','The call’s retained view remains usable until the call finishes.',[countCheck(m,2)]);
 m.drop('view').drop('root').collect().frame('Call and VM ownership end','A witness/default-method dependency cannot outlive its receiver lease or be dropped earlier than it.',[countCheck(m,0)]);
});
story('L06','Lifetime','Partial encode fails','A failed aggregate transfer must release all unadopted leases while preserving the caller’s inputs.','design','Fault-inject every allocation/codec/dispatch/adoption boundary, including nested aggregates and error payloads.',m=>{
 vm(m).own('a','Caller proxy','V').frame('Caller still owns its input','The caller’s lease is never consumed by an attempted call.',[]);
 m.own('t1','Transfer token 1','V').own('t2','Transfer token 2','V').frame('Encode two nested references','The transfer transaction owns each token until aggregate validation and adoption finish.',[countCheck(m,4)],'{first: ref, nested: [ref, invalid_value]}');
 m.drop('t1').drop('t2').frame('The next element fails validation','Roll back both tokens. If any token was staged at the receiver, receiver rollback must release that staged ownership too; no partial value escapes.',[['Caller lease preserved',!!m.edges.a],countCheck(m,2)]);
 m.drop('a').drop('root').collect().frame('Caller later closes normally','The failed call left no transfer ownership behind.',[countCheck(m,0)]);
});
story('L07','Lifetime','Close during an active call','An admitted invocation pins everything it can still access, independently of the initiating proxy.','design','Race close/finalize against lookup, admission, callback dispatch and normal completion; run with deterministic counters.',m=>{
 host(m).own('work','Active invocation','H','pending').frame('The callback is running','Admission takes temporary ownership before releasing lookup locks.',[countCheck(m,2)]);
 m.drop('local').frame('The caller closes its wrapper','The receiver remains live while user code might still access it.',[['Worker still owns receiver',!!m.edges.work]],'ref.close()');
 m.value('H',{count:1}).frame('Callback finishes its work','Local close does not cancel or revoke independent work.',[['Receiver still alive',m.objects.H.status==='alive']]);
 m.drop('work').collect().frame('Completion releases the call lease','Only now can the last host receiver reference disappear.',[countCheck(m,0)]);
});
story('L08','Lifetime','Cancel, then a late result','A canceled waiter may be finished while the host worker is still running. Those are different lifetimes.','design','Cancellation at every boundary, worker non-cooperation, late results with nested refs and duplicate completion must release exactly once.',m=>{
 host(m).own('work','Active host worker','H','pending').frame('Callback starts','The worker has its own receiver lease.',[]);
 m.drop('local').local('caller outcome','canceled; delivered once').frame('Caller is canceled','The waiter completes once, but the running worker remains an owner. Cancellation is cooperative.',[['Worker still retained',!!m.edges.work]],'task.cancel()','If host code never terminates, safe reclamation and bounded shutdown are impossible without isolation or a stronger host contract.');
 m.object('R','Late result receiver',{value:42}).own('late','Unadopted late result','R').frame('Worker returns after cancellation','The result must not be delivered to the already completed waiter. Its transferred resources still need cleanup.',[['Caller remains canceled',m.vars['caller outcome'].startsWith('canceled')]]);
 m.drop('late').drop('work').collect().frame('Discard result and finish worker','Release late-result tokens and the worker lease. Duplicate completion cannot release them again.',[countCheck(m,0)]);
});
story('L09','Lifetime','Scope close drains work','A scope controls permission to call; it is not a permanent strong owner of every registration.','disposal','Test atomic admission vs revocation, parent/child calls, active worker drain, and scope-close result ownership.',m=>{
 host(m).own('saved','Retained BAML view','H').own('work','Active invocation','H','pending').frame('Scope is open','Local state, a retained view and an admitted call own the receiver. The scope itself contributes zero ownership edges.',[countCheck(m,3)]);
 m.revoke().local('new calls','rejected').frame('Begin closing the scope','Revoke new calls before awaiting drain. Retained handles lose permission to call, even while the receiver must remain allocated.',[['Scope revoked',m.scope==='closing']],'await scope.close()');
 m.drop('local').drop('saved').frame('Detach inactive registry ownership','Closed views become harmless tombstones. The admitted worker still owns its receiver and cleanup state.',[countCheck(m,1)],'// Await real worker completion, not just cancellation delivery.');
 m.drop('work').collect().closeScope().local('scope close','resolved').frame('Worker finishes; close resolves','All bridge-owned roots in this modeled scope are gone. An application’s separate native reference, if any, would remain its own responsibility.',[countCheck(m,0)]);
});
story('L10','Lifetime','A callback closes its own scope','Awaiting a scope’s drain from one of that scope’s active callbacks creates a self-wait.','design','Choose and test a self-close policy before implementation; include descendant calls and cleanup callbacks.',m=>{
 host(m).own('work','Callback being drained','H','pending').frame('A scoped callback is active','The callback counts as work that scope.close() must drain.',[]);
 m.revoke().local('wait dependency','callback → scope close → callback').frame('Callback awaits its own scope close','Waiting here deadlocks. Revocation alone does not remove the active worker’s ownership.',[['Worker is still retained',!!m.edges.work]],'async def callback():\n    await my_scope.close()','Decision to review: reject an awaited self-drain deterministically after beginning revocation; expose nonwaiting begin_close(), and let an outside owner await close.');
 m.local('proposed callback outcome','self-drain error or nonwaiting close').drop('work').frame('Proposed policy lets the callback unwind','A nonwaiting revocation request is safe; awaiting one’s own drain is not. This is a policy proposal, not a shipped or settled API.',[countCheck(m,1)]);
 m.drop('local').collect().closeScope().frame('An outside owner awaits drain','After the callback has actually ended, ordinary teardown can complete.',[countCheck(m,0)]);
},{decision:true});
story('L11','Lifetime','A cycle spans two runtimes','Weak SDK caches prevent accidental roots. They do not collect cycles created by application objects.','python','Construct host→proxy→VM→host cycles; show retention before teardown and release after scope/runtime teardown.',m=>{
 m.object('H','Host object',{owns:'BAML proxy'}).object('V','BAML object',{owns:'host interface'}).own('user','User variable','H','native').own('hv','Stored proxy lease','V','lease','H').own('vh','Stored host view','H','lease','V').frame('The objects retain each other','The two cross-runtime ownership edges form a cycle. An ordinary user variable also reaches it.',[countCheck(m,3)]);
 m.drop('user').collect().frame('Drop the last outside variable','Both collectors still see roots from the other runtime. Reference counting / separate tracing does not break this cycle.',[['Both receivers remain alive',m.objects.H.status==='alive'&&m.objects.V.status==='alive']],'' ,'This is retained memory until teardown, not automatic leak freedom. A long-lived default runtime allows application cycles to accumulate.');
 m.revoke().drop('vh').collect().closeScope().frame('Scope teardown breaks a bridge edge','After any active calls drain, invalidating registry ownership breaks the cycle. Collecting H releases its stored V lease.',[['Cycle reclaimed',m.count()===0&&m.objects.H.status==='released'&&m.objects.V.status==='released']]);
},{limit:true});
story('L12','Lifetime','Finalizers and release queues','Finalizers enqueue a detached token. They cannot await, reenter BAML, or retain their own target.','python','Test weak caches, unhashable/equal host models, delayed collection, explicit close plus finalizer, and queue drain after shutdown.',m=>{
 vm(m).own('proxy','Host proxy lease','V').frame('A proxy owns a lease','The SDK finalizer holds detached release state, not a bound method or strong path back to the proxy.',[]);
 m.transfer('proxy','Queued release token').frame('Proxy is collected','The finalizer enqueues a release. Native ownership remains until the queue is drained.',[['Release token still owned',!!m.edges.proxy]],'weakref.finalize(proxy, enqueue_release, detached_token)');
 m.drop('proxy').frame('Runtime drains the queue','Explicit close uses the same idempotent token. JS FinalizationRegistry may run late or never; do not use GC timing as the release test oracle.',[countCheck(m,1)]);
 m.drop('root').collect().frame('Runtime ownership ends','Runtime shutdown also drains or invalidates its release obligations. Late finalizer delivery must be harmless.',[countCheck(m,0)]);
});
story('L13','Lifetime','Stale and foreign handles','Receiver identity includes the owning runtime and generation, not just a numeric key or type name.','design','Try wrong session, wrong role, old generation, forged witness and exact-type mismatch before dispatch.',m=>{
 vm(m).own('proxy','Old-runtime proxy','V').local('token','runtime A / generation 7 / key 12').frame('A valid capability exists','Its immutable witness and session identify the receiver.',[]);
 m.local('attempt in runtime B','rejected before adoption').frame('Pass it to another runtime','Matching type names do not permit transfer of a live capability. The failed attempt allocates no receiver ownership.',[countCheck(m,2)]);
 m.revoke().drop('root').drop('proxy').collect().closeScope().frame('The old runtime shuts down','Outstanding proxy wrappers become closed. Safe late releases cannot resurrect anything.',[countCheck(m,0)]);
 m.object('N','New runtime receiver',{generation:8,key:12}).own('new','New runtime root','N','reach').frame('A new runtime reuses key 12','The old token still fails. Numeric reuse never changes the old receiver’s identity.',[['Old receiver stays released',m.objects.V.status==='released'],['New receiver has independent ownership',m.count('N')===1]]);
});
story('L14','Lifetime','Concrete image versus live view','A portable image payload can outlive an engine. A live interface view remains tied to its runtime.','design','Verify engine-independent media ownership in all SDKs, then media-as-interface shutdown, aliasing and callback round trips.',m=>{
 m.object('I','Owned image bytes',{mime:'image/png'},'payload').own('bytes','Native image value','I','native').object('V','Media interface receiver',{kind:'image'}).own('view','Media checked view','V').own('buffer','Shared immutable buffer','I','native','V').frame('One payload, one live view','The immutable buffer has engine-independent ownership. The view’s witness belongs to a runtime.',[]);
 m.revoke().drop('view').collect().closeScope().frame('Runtime shuts down','The live view is revoked and released. The independent image value still retains its bytes.',[['Payload survives',m.objects.I.status==='alive'],['View receiver released',m.objects.V.status==='released']]);
 m.drop('bytes').collect().frame('Image value is dropped','The last buffer owner releases it. A URL image descriptor does not promise that its external content is immutable.',[countCheck(m,0)]);
});
story('L15','Lifetime','Throws, reentry and returned errors','Callbacks can reenter BAML. Both frames own their receivers, and error payloads participate in transfer cleanup.','design','Exercise nested async reentry, declared error refs, native exceptions, wrong returns, and infrastructure decoding without the user typemap.',m=>{
 host(m).own('outer','Outer callback invocation','H','pending').frame('Host callback starts','Lookup/adoption locks have already been released before user code executes.',[]);
 m.own('inner','Reentrant BAML invocation','H','pending').frame('Host calls BAML again','The inner invocation retains independent ownership. Never block the host executor or hold a global bridge lock across reentry.',[countCheck(m,3)],'async def callback():\n    return await OtherBamlFunction_async(self)');
 m.drop('inner').local('inner outcome','declared error / bridge failure').frame('Inner call fails','Unwind the inner frame and all unadopted result/error tokens. An error carrier with an adopted live child would own its own lease.',[countCheck(m,2)]);
 m.drop('outer').drop('local').collect().frame('Outer frame unwinds','Native exceptions, invalid return values and infrastructure failures must all reach a completion path that releases temporary ownership.',[countCheck(m,0)]);
});
story('M01','Mutation','One scalar field, one owner','A native write is visible on the next BAML read. A BAML write reaches that same owner through a setter.','fields','Test host→VM and VM→host scalar access, renamed fields, defaults, strict reads/writes and executor dispatch.',m=>{
 setupField(m).local('previous scalar read',1).frame('Read count','Reading an int returns a value. It does not subscribe the local variable to later changes.',[],'saved = await counter.get_count()');
 m.value('P',{count:2}).frame('Native owner writes 2','The adapter reads the retained host object each time; no update message or second field copy is needed.',[['Earlier scalar stays 1',m.vars['previous scalar read']===1]],'python_owner.count = 2');
 m.local('next BAML read',2).frame('BAML reads again','The next ordered read sees 2 unless another writer changes it first.',[['Read sees current owner',m.vars['next BAML read']===m.objects.P.value.count]],'let current = counter.count;');
 m.value('P',{count:3}).frame('BAML writes 3','Successful setter completion acknowledges the owner’s committed update.',[['Owner changed',m.objects.P.value.count===3]],'counter.count = 3;');
});
story('M02','Mutation','A proxy is not a native model','Remote fields use awaited accessors. Plain assignment must fail instead of creating an unused shadow property.','fields','Assert Python/JS runtime guards as well as TS/Rust static checks; ordinary copied records must retain local field access.',m=>{
 setupField(m).frame('The host receives a live ref','This is a proxy, not the original Pydantic model or JS implementation object.',[]);
 m.local('plain assignment','rejected; no shadow property').frame('Try ref.count = 9','Python attribute guards and JS property guards reject this write; TS and Rust also reject it statically.',[['Owner unchanged',m.objects.P.value.count===1]],'ref.count = 9  // rejected on a live proxy');
 m.value('P',{count:9}).frame('Await the setter','The async operation validates, dispatches to the owner and acknowledges commit.',[['Owner updated',m.objects.P.value.count===9]],'await ref.set_count(9)');
});
story('M03','Mutation','Two fields share one list','Nested mutation targets the actual child object. Reading a field must not produce a detached native array.','baml','Run index/push/map/class operations through both VM-owned and host-owned storage backends, including default methods and reflection.',m=>{
 setupField(m);child(m,'A','Shared list',[1]);link(m,'f1','items','P','A');link(m,'f2','also_items','P','A');m.frame('Both fields point to list A','The two field links share one mutable identity.',[['Both links target A',m.edges.f1.to===m.edges.f2.to]]);
 m.own('list','Host ListRef','A').frame('Read items','The returned view owns the actual list. It is not a copy and does not merely remember parent.items.',[countCheck(m,4)],'items = await state.get_items()');
 m.value('A',[1,2]).frame('Push through the child view','Both fields now observe [1, 2]. No parent-field setter needs to run.',[['Both aliases observe the same updated child',m.objects.A.value.length===2]],'await items.push(2)');
 m.drop('p').collect().frame('Parent wrapper is released','The parent can be reclaimed while the independently held list stays alive. Its owning scope/runtime must still be open.',[['Parent released',m.objects.P.status==='released'],['Child alive',m.objects.A.status==='alive']]);
 m.drop('list').collect().frame('Last child ref closes','The child’s final owner releases shared storage.',[countCheck(m,0)]);
});
story('M04','Mutation','Replace a field; keep the old alias','Replacing parent.items changes a slot. It does not retarget existing references to the previous list.','baml','Test retained child across root replacement and parent disposal, in both bridge directions and for list/map/class fields.',m=>{
 setupField(m);child(m,'A','Old list A',[1]);link(m,'field','items','P','A');m.own('old','Old ListRef','A').frame('Keep an alias to A','The saved child ref has its own identity and lifetime.',[],'old = await state.get_items()');
 child(m,'B','New list B',[99]);m.drop('field');link(m,'newfield','items','P','B');m.frame('Replace the parent field','The root slot now names B. The old ref still names A.',[['Root and old alias differ',m.edges.newfield.to!==m.edges.old.to]],'await state.set_items([99]) // native data initializer');
 m.value('A',[1,2]).frame('Mutate the old alias','A changes; B stays [99]. A field-path proxy would incorrectly mutate B and violate BAML identity semantics.',[['New field unaffected',JSON.stringify(m.objects.B.value)==='[99]']], 'await old.push(2)');
 m.drop('p').collect().frame('Drop the parent','B is released with its last parent slot. A survives through the old child ref.',[['B released',m.objects.B.status==='released'],['A alive',m.objects.A.status==='alive']]);
 m.drop('old').collect().frame('Drop the old alias','All storage in this scenario can now be released.',[countCheck(m,0)]);
});
story('M05','Mutation','VM storage replaces a host field','A host aggregate field must be able to hold a checked VM-owned child without copying away aliases.','baml','Generated shared storage holders must accept host- and VM-owned values and preserve identity under replacement both ways.',m=>{
 setupField(m);child(m,'A','Host list A',[1]);link(m,'field','items','P','A');child(m,'B','VM list B',[7]);m.own('baml','BAML local list','B','reach').frame('Host and VM start with separate lists','Plain Pydantic list / JS array / Rust Vec fields cannot universally preserve this next assignment by themselves.',[]);
 m.drop('field');link(m,'field2','items','P','B');m.collect().frame('BAML assigns its list to the host field','The shared-storage holder retains B’s checked identity, not a copied native container.',[['B has two owners',m.count('B')===2]],'state.items = baml_items;');
 m.value('B',[7,8]).frame('Host mutates through its field','The BAML local sees [7, 8] because both still refer to B.',[['B updated in place',m.objects.B.value[1]===8]],'await host.items.push(8)');
 m.drop('p').collect().frame('Host parent is released','The VM local retains B independently.',[['BAML alias survives',m.objects.B.status==='alive'&&!!m.edges.baml]]);
 m.drop('baml').collect().frame('VM local is released','The remaining storage can be reclaimed.',[countCheck(m,0)]);
});
story('M06','Mutation','Copied result, live journal','RunResult’s outer record is local data. Its journal remains a live reference with shared state.','baml','Codec tests must distinguish outer DTO replacement from nested capability identity for RunResult and ModelTurnInput.',m=>{
 child(m,'J','Shared journal',['first']);m.own('engine','BAML journal owner','J','reach').own('record','Host result.journal','J').local('result.value',{answer:42}).frame('Receive a result envelope','The native record is copied; its journal is an independently retained live child.',[],'result = await agent.run(spec)');
 m.value('J',['first','second']).frame('Append through result.journal','The engine observes the shared journal mutation. result.value remains ordinary local data.',[['Journal changed',m.objects.J.value.length===2]],'await result.journal.append(entry)');
 child(m,'K','Different journal',['new']);m.drop('record').own('replacement','Host result.journal','K').frame('Replace the outer record’s field','This updates the host record only. It does not replace an earlier engine-side envelope’s journal slot.',[['Engine still owns J',m.edges.engine.to==='J'],['Host now owns K',m.edges.replacement.to==='K']], 'result.journal = other_journal');
 m.drop('engine').drop('replacement').collect().frame('Both independent owners end','Recursive capability ownership must be released when enclosing native values are dropped.',[countCheck(m,0)]);
});
story('M07','Mutation','Lost updates versus an atomic method','Shared identity does not make read-modify-write atomic. Methods are atomic only if their implementation provides it.','design','Use barriers to force interleaving. Verify explicit synchronized operations separately from ordinary field reads/writes.',m=>{
 setupField(m).value('P',{count:0}).frame('Two tasks share one counter','Both tasks are allowed to write the same field.',[]);
 m.local('task A read',0).local('task B read',0).frame('Both read 0','Each has a copied scalar. Neither read reserves the field.',[],'a = await ref.get_count()\nb = await ref.get_count()');
 m.value('P',{count:1}).frame('Task A writes 1','Task A’s write commits.',[]);
 m.value('P',{count:1}).frame('Task B also writes 1','The second update is lost. This is permitted interleaving, not a stale-field-cache bug.',[['Lost-update result is 1',m.objects.P.value.count===1]],'await ref.set_count(b + 1)','BAML compound field assignments can also suspend between foreign read and write.');
 m.value('P',{count:0}).local('task A read','unused by atomic operation').local('task B read','unused by atomic operation').frame('Restart with a synchronized method','The receiver implements increment using an atomic operation, lock or owner transaction.',[]);
 m.value('P',{count:2}).frame('Two protected increments finish','Now the value is 2. Merely naming an async method increment would not guarantee this.',[['Both increments counted',m.objects.P.value.count===2]],'await ref.increment() // implementation provides synchronization');
});
story('M08','Mutation','Cancellation after a write commits','A canceled setter can have changed state even though the caller never received its acknowledgement.','design','Force cancellation before dispatch, before commit and after commit. Do not infer rollback from transport/cancellation failure.',m=>{
 setupField(m).frame('Before dispatch','A request canceled before admission performs no write.',[]);
 m.own('write','Admitted setter','P','pending').frame('Setter is admitted','The operation owns a temporary lease while it can access storage.',[],'await ref.set_count(5)');
 m.value('P',{count:5}).frame('Owner commits 5','The update is committed before the acknowledgement reaches the caller.',[['Owner contains 5',m.objects.P.value.count===5]]);
 m.local('caller outcome','canceled; commit may have happened').drop('write').frame('Caller receives cancellation','There is no implicit rollback. Retrying a non-idempotent operation can apply it twice.',[['Committed value remains',m.objects.P.value.count===5]],'// Read again or use an application operation ID/transaction.');
});
story('M09','Mutation','Validation and frozen models','Pydantic helps native validation; the bridge still validates every read, write and nested operation against exact BAML types.','fields','Check strict type validation, in-place mutation bypass, frozen/writable conflicts, invariant pins and no commit on invalid writes.',m=>{
 setupField(m).frame('The interface requires writable int count','A frozen native field cannot automatically implement this contract.',[]);
 m.local('invalid set_count("2")','rejected without commit').frame('Reject an invalid write','Do not coerce a string into an int at the BAML boundary.',[['Count stays 1',m.objects.P.value.count===1]],'await ref.set_count("2") // contract violation');
 m.value('P',{count:'corrupted by native bypass'}).frame('Native code bypasses validation','Native state can become invalid outside the bridge. Pydantic also cannot observe every in-place container edit.',[],'object.__setattr__(host, "count", "invalid")');
 m.local('BAML read','rejected; no invalid BAML value').frame('Reject the outgoing read','Report a bridge contract failure before injecting invalid state into typed BAML execution.',[['Invalid value was not exported',m.vars['BAML read'].startsWith('rejected')]]);
 m.value('P',{count:2}).local('BAML read',2).frame('Repair the owner','The next valid read succeeds. Mutable ListRef<int> cannot be relabeled as ListRef<int|string>.',[['Valid read resumes',m.vars['BAML read']===2]]);
});
story('M10','Mutation','An explicit data copy is detached','Copying supported data is useful, but it must be an explicit operation with clearly detached mutation semantics.','design','Verify recursive copy behavior, reject nonportable capabilities, and preserve immutable media buffer ownership independently.',m=>{
 child(m,'A','Live list',[1]);m.own('ref','Live ListRef','A').frame('Start with shared storage','Ordinary live-field reads retain this storage.',[]);
 m.local('native copy',[1]).frame('Export supported data','to_value() returns a native data copy when this storage supports data export. It is not a required method on all interfaces.',[],'copy = await items.to_value()');
 m.local('native copy',[1,2]).frame('Mutate the copy','The live list stays [1]. There is no polling, reconciliation or automatic copyback.',[['Live list unchanged',m.objects.A.value.length===1]],'copy.append(2)');
 m.drop('ref').collect().frame('Release the live list','The detached native data survives without the runtime.',[['Storage released',m.objects.A.status==='released'],['Copy survives',m.vars['native copy'].length===2]]);
});
story('M11','Mutation','A child survives its parent, not revocation','A child owns storage independently of its parent wrapper, but its access still depends on the owning scope/runtime.','design','Race parent release and owner revocation against nested storage operations, iterator advancement and field replacement.',m=>{
 setupField(m);child(m,'A','Shared child',[1]);link(m,'field','items','P','A');m.own('child','Child ListRef','A').frame('Read a child view','The child owns a lease independent of the parent.',[]);
 m.drop('p').collect().frame('Release the parent','Child operations remain legal while the owner’s scope is open.',[['Child survives parent',m.objects.P.status==='released'&&m.objects.A.status==='alive']]);
 m.revoke().local('child.push(2)','rejected; scope closed').frame('Revoke the owning scope','A retained lease does not grant authority to call after revocation. No storage operation reaches freed or closed owner state.',[['No write committed',m.objects.A.value.length===1]]);
 m.drop('child').collect().closeScope().frame('Detach the closed child lease','The stale wrapper becomes an inert tombstone; remaining release messages are harmless.',[countCheck(m,0)]);
});
story('L16','Lifetime','Two calls bind at once','Lazy registration needs one publication point per implementation source, runtime generation and scope.','design','Race first binding, cancellation of one waiter, registration failure and retry. A waiter must never tear down another waiter’s published registration.',m=>{
 host(m).local('registration','absent').frame('One source, two callers','Both callers start with the same fixed implementation bundle.',[]);
 m.local('registration','one shared initialization').own('a','Call A admission','H','pending').own('b','Call B admission','H','pending').frame('First-use calls converge','The registry performs one initialization; each call separately owns its eventual invocation lease. A raw second factory call would be a distinct adapter.',[['One receiver identity',Object.keys(m.objects).length===1]]);
 m.local('registration','published once').drop('a').frame('A cancels; B continues','A’s cancellation releases A’s obligation. It does not revoke the source or B’s ownership.',[['B still retained',!!m.edges.b]]);
 m.drop('b').drop('local').collect().frame('Remaining work ends','No losing initialization or abandoned waiter may strand registry state.',[countCheck(m,0)]);
});
story('L17','Lifetime','A view cannot escape its scope','Projecting or passing a view through a longer-lived scope cannot extend the originating registration’s authority.','design','Check projection, returned refs, child storage and reentrant calls across scopes. Separate explicit registrations must have distinct identities.',m=>{
 host(m).own('short','View in short scope A','H').local('origin authority','scope A').frame('Register in a short scope','The receiver is bound to A. A native host reference can survive A, but this registration cannot.',[]);
 m.own('long','View retained by scope B caller','H').frame('Pass it through a longer-lived caller','The new lease preserves A’s revocation authority; scope B does not re-register or retag it.',[['Origin remains A',m.vars['origin authority']==='scope A']]);
 m.revoke().local('B calls old view','rejected: origin A closed').frame('A closes','Every derived view of this registration becomes unusable. A new explicit registration is a distinct adapter, not a resurrection.',[['No authority laundering',m.vars['B calls old view'].startsWith('rejected')]]);
 m.drop('short').drop('long').closeScope().frame('Retire the old registration','The application’s independent native reference still owns its host object. Dropping a registry root must not invoke arbitrary user cleanup.',[['Native host ownership preserved',!!m.edges.local]]);
 m.drop('local').collect().frame('Application releases the native object','Only the last real native owner can make the host object eligible for collection.',[countCheck(m,0)]);
});
story('L18','Lifetime','Who owns an ambiguous transfer?','Completion, cancellation and adoption need an atomic ownership decision. Neither side can guess after a lost acknowledgement.','design','Choose a staging/transaction protocol; test duplicate completion, lost ACK, malformed nested error payloads and cancellation racing adoption.',m=>{
 vm(m).own('transfer','Sender transfer transaction','V').local('transaction T','prepared').frame('Prepare a transfer','The transaction owns one token before receiver adoption.',[]);
 m.transfer('transfer','Receiver staging arena').local('transaction T','staged; not user-visible').frame('Receiver validates in a staging arena','Responsibility moves through a transport-owned ledger; staged values cannot escape to user code.',[['Token has exactly one owner',m.count('V')===2]]);
 m.transfer('transfer','Adopted receiver proxy').local('transaction T','committed').frame('Adoption wins atomically','One commit record determines ownership. The example shows adoption winning; a cancellation winner must roll back all staged tokens instead.',[['One transfer token remains',Object.keys(m.edges).filter(k=>k==='transfer').length===1]],'// Proposed transaction ID + idempotent commit/ack protocol.');
 m.local('duplicate ACK','read same committed outcome').frame('An ACK is lost or delivered twice','Query/replay the same transaction outcome; do not retain a second token or free an already adopted one.',[countCheck(m,2)],'','Protocol decision: local in-process bridges can share an atomic ledger. Any transport that can lose acknowledgements needs an explicit ownership-transfer recovery rule.');
 m.drop('transfer').drop('root').collect().frame('The adopted proxy is later closed','Transaction records may retire once duplicate-delivery safety is guaranteed. The runtime needs a bounded retirement policy.',[countCheck(m,0)]);
},{decision:true});
story('L19','Lifetime','A worker never returns','Safe ownership can require retaining memory indefinitely. A timeout cannot magically make running native code safe to free.','design','Specify shutdown policy and completion-sink lifetime. Test permanently blocked host work in an isolated subprocess.',m=>{
 host(m).own('work','Uncooperative worker','H','pending').frame('Native work begins','This could be Python to_thread work or a native callback that ignores cancellation.',[]);
 m.revoke().drop('local').local('close outcome','still waiting for real work').frame('Scope requests cancellation','The caller can be canceled, but the worker still retains receiver state. Scope close cannot report that it has drained.',[['Worker remains an owner',!!m.edges.work]]);
 m.local('timeout','does not release worker lease').frame('A shutdown timeout expires','The honest choices are to keep waiting, return a timeout while retaining a closing runtime, or use separately specified process isolation. Freeing the receiver is unsafe.',[['Receiver must remain allocated',m.objects.H.status==='alive']], '','Recommended baseline: await real drain; an optional timeout reports incomplete closure and retains safe ownership. Do not silently detach and destroy the runtime.');
},{decision:true,limit:true});
story('L20','Lifetime','Reference release is not resource cleanup','Dropping a bridge root does not automatically close an application’s database connection, file or PIL resource.','design','Specify any opt-in cleanup hook separately, including ownership, exactly-once execution, async failure and shared native objects.',m=>{
 host(m).own('bridge','Registry-backed view','H').local('application resource','open; owned by application').frame('The implementation uses a resource','The bridge owns references to the receiver, not an inferred right to close all resources inside it.',[]);
 m.drop('bridge').frame('Bridge reference is released','The native application owner still exists. Do not discover and call arbitrary close/aclose methods on it.',[['Application owner remains',!!m.edges.local]]);
 m.local('application resource','closed explicitly').frame('Application closes its resource','Use application context managers / disposal or an explicitly declared resource-owner contract for async cleanup.',[['Resource cleanup is explicit',m.vars['application resource']==='closed explicitly']]);
 m.drop('local').collect().frame('Native ownership ends','Garbage collection and reference release do not substitute for awaited application cleanup.',[countCheck(m,0)]);
});
story('M12','Mutation','One operation versus a sequence','A storage operation needs an owner-side commit point. A sequence of operations still allows interleaving.','design','Define and test per-operation linearization across every storage backend and every native holder entrypoint; specify iteration under concurrent mutation.',m=>{
 child(m,'A','Shared list',[]);m.own('a','Task A list ref','A').own('b','Task B list ref','A').frame('Two tasks share a list','This case proposes linearizable individual push/index-write/map-set operations at the owner.',[]);
 m.value('A',[1]).frame('Task A push commits','The first operation makes one complete change; readers cannot observe partially initialized storage.',[],'await list.push(1)');
 m.value('A',[1,2]).frame('Task B push commits','Both elements remain. Reverse commit order would yield [2, 1]. Ordering between concurrent tasks is unspecified.',[['Both pushes retained',m.objects.A.value.length===2]],'await list.push(2)');
 m.local('iteration policy','must be specified by storage operation').frame('Iteration needs its own rule','Do not imply that an iteration, to_value(), or multiple reads sees an atomic snapshot. Snapshot/export consistency and concurrent iteration must have explicit semantics.',[], '', 'Decision to review: linearizable single mutations; no implicit transaction for multi-operation sequences. Never hold a bridge/global lock while invoking user code.');
},{decision:true});
story('M13','Mutation','Maps, nested objects and default methods','The same storage identity rule applies recursively to map entries and class fields read by BAML default methods.','baml','Exercise map insert/remove, class field load/store, defaults, nested aliases, renamed fields, bounds failures and reflection through foreign storage.',m=>{
 setupField(m);child(m,'C','Child object',{value:1});child(m,'D','Shared map',{a:1});link(m,'c','child','P','C');link(m,'d','values','P','D');m.own('child','Old child ref','C').frame('Parent stores a class and a map','Both are shared mutable objects rather than DTO copies.',[]);
 m.value('C',{value:12}).value('D',{a:11,b:2}).local('default method read',{child:12,a:11,b:2}).frame('Mutate the nested storage','A BAML default method sees the current child and map values through ordinary field/storage dispatch.',[['Default sees updated child',m.vars['default method read'].child===12]]);
 child(m,'N','Replacement child',{value:8});m.drop('c');link(m,'n','child','P','N');m.value('C',{value:120}).local('default method read',{child:8,a:11,b:2}).frame('Replace child, then mutate old ref','The default method now follows the new parent slot. The saved ObjectRef still mutates the old child.',[['Default sees replacement',m.vars['default method read'].child===8],['Old alias changed independently',m.objects.C.value.value===120]]);
 m.drop('p').drop('child').collect().frame('All owners release','Recursively retained storage edges must be released when their owning objects become collectible.',[countCheck(m,0)]);
});

cases.sort((a,b)=>a.id.localeCompare(b.id));
const api={Model,cases,evidence};
if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.BamlReview=api;
})(typeof globalThis!=='undefined'?globalThis:this);
