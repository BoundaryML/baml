/* Proposed usage and bridge pseudocode. Fixture functions and imports are omitted. */
(function(root){
'use strict';
const examples={};
function add(id,language,blocks,prelude='',note='Proposed SDK usage. Fixture functions and imports are omitted; these APIs are not all implemented today.'){
 (examples[id]??={})[language]={note,lines:[...prelude.split('\n').filter(Boolean).map(text=>({text,step:-1})),...blocks.flatMap((block,step)=>block.split('\n').map(text=>({text,step})))]};
}
const bridge=(id,blocks)=>add(id,'Bridge pseudocode',blocks,'','Implementation pseudocode: operation names describe the proposed ownership protocol, not current ABI exports.');
add('L01','Python',[
 'greeter = PythonGreeter("Hello")  # Native object; no binding yet.',
 'message = await Welcome_async(greeter, "Ada")  # Bind on first use.',
 '# Welcome has returned; its temporary invocation lease is gone.',
 'del greeter  # Eligible for native collection; no retained VM owner.',
], 'class PythonGreeter(GreeterImplementation):\n    def __init__(self, prefix):\n        self.prefix = prefix\n    async def greet(self, name):\n        return f"{self.prefix}, {name}"\n');
add('L01','TypeScript',[
 'const greeter = Greeter.implement({\n  greet: async name => `Hello, ${name}`,\n}); // No runtime binding yet.',
 'const message = await Welcome_async(greeter, "Ada");',
 '// Welcome has returned; the invocation lease is released.',
 'greeter.close(); // Release local ownership.',
]);
add('L01','Rust',[
 'let greeter = Greeter::implement(RustGreeter::new("Hello"));',
 'let message = Welcome_async(&greeter, "Ada", &ctx).await?;',
 '// The invocation has returned and released its temporary lease.',
 'drop(greeter); // No VM owner remains in this example.',
]);
add('L02','TypeScript',[
 'let id: string;\n{\n  using source = Greeter.implement(host);\n  id = await SaveGreeter_async(source); // BAML stores the receiver.',
 '} // Symbol.dispose releases source; BAML still owns the receiver.',
 'await GreetSaved_async(id, "Ada"); // The same host state runs.',
 'await ForgetGreeter_async(id); // Remove VM retention; collect later.',
]);
add('L02','Python',[
 'host = PythonGreeter("Hello")\nref = await GreeterRef.bind(host)\nid = await SaveGreeter_async(ref)',
 'ref.close()\ndel ref, host  # BAML retention still keeps host state alive.',
 'await GreetSaved_async(id, "Ada")',
 'await ForgetGreeter_async(id)  # VM collection releases its owner.',
]);
add('L02','Rust',[
 'let greeter = Greeter::implement(host);\nlet id = SaveGreeter_async(&greeter, &ctx).await?;',
 'drop(greeter); // BAML owns a separate reference.',
 'GreetSaved_async(&id, "Ada", &ctx).await?;',
 'ForgetGreeter_async(&id, &ctx).await?;',
]);
add('L03','Python',[
 'a = await GetGreeter_async()\nb = a  # Two names; one proxy lease.',
 'c = a.clone()  # New lease; same receiver.',
 'a.close()\na.close()  # Idempotent. b is also closed; c is still usable.',
 'c.close()\nawait ForgetGreeter_async()  # Release the modeled BAML root.',
]);
add('L03','TypeScript',[
 'const a = await GetGreeter_async();\nconst b = a; // Same wrapper, same lease.',
 'const c = a.clone(); // Independent ownership, same receiver.',
 'a.close();\na.close(); // b is closed too. c remains usable.',
 'c.close();\nawait ForgetGreeter_async();',
]);
add('L03','Rust',[
 'let a = GetGreeter_async(&ctx).await?;\nlet b = &a; // A borrow adds no owned lease.',
 'let c = a.clone(); // Independent owned reference.',
 '// b cannot outlive a: the borrow checker enforces this.\ndrop(a); // Rust moves/Drop replace explicit double-close.',
 'drop(c);\nForgetGreeter_async(&ctx).await?;',
]);
add('L04','Python',[
 '# BAML MakeCounter returns a closure capturing its counter.\npending = asyncio.create_task(MakeCounter_async(40))\nawait callee_returned.wait()  # Fixture pauses before host adoption.',
 'counter = await pending  # Adopt a checked BAML closure.',
 'value = await Apply_async(counter, 1)  # Pass it back unchanged.',
 'counter.close()  # Proposed unified callable ownership API.',
]);
bridge('L04',[
 'tx = encode_owned_view(baml_closure)\n// tx pins the original receiver and declaration.',
 'proxy = decode_and_adopt(tx)',
 'if is_checked_capability(proxy):\n    input = retain_original_view(proxy)\nelse:\n    input = adapt_explicit_host_implementation(proxy)',
 'release(input)\nrelease(proxy)\n// Never wrap a returned BAML closure in a new host callback.',
]);
add('L05','Python',[
 'client = ResponsesClient(...)  # Generated concrete client facade.',
 'task = asyncio.create_task(Extract_async(text, baml_options={"client": client}))\nawait call_admitted.wait()  # Fixture barrier: Client view retained.',
 'client.close()  # In-flight calls own independent leases.',
 'result = await task\n# Remaining BAML roots release their views when done.',
]);
add('L05','TypeScript',[
 'const client = new ResponsesClient(/* configuration */);',
 'const pending = Extract_async(text, {client});\nawait callAdmitted; // Fixture barrier; no client.as_client().',
 'client.close(); // An admitted call retains its own ownership.',
 'const result = await pending;\n// Final VM release can now reclaim the receiver.',
]);
bridge('L06',[
 'caller = checked_ref(V)  // Caller owns its original lease.',
 'tx = TransferArena()\ntx.retain(ref_in_first_field)\ntx.retain(ref_in_nested_array)',
 'try:\n    tx.encode(invalid_tail)\n    receiver.adopt_all(tx)\nexcept:\n    tx.rollback_all_unadopted()  // Caller ownership is untouched.',
 'caller.close()\nrelease_final_vm_root(V)',
]);
bridge('L07',[
 'call = admit(receiver)\ncall.retain_receiver_before_unlocking()',
 'proxy.close()  // Releases only the proxy lease.',
 'result = await host_callback(call.receiver)\n// call still owns everything the callback can access.',
 'complete_once(call, result)\ncall.release_after_actual_completion()',
]);
add('L07','Python',[
 'task = asyncio.create_task(ref.greet("Ada"))\nawait callback_started.wait()  # Fixture barrier: call is admitted.',
 'ref.close()  # Safe: the admitted call owns another lease.',
 'allow_callback_to_finish.set()\nmessage = await task',
 '# Callback completion releases the final invocation lease.',
]);
bridge('L08',[
 'worker = admit_host_call(receiver)\nworker.retain_receiver()',
 'cancel_waiter_once(worker)\nrequest_cooperative_cancellation(worker)\n// Do not release the running worker\'s ownership.',
 'late_tx = await actual_worker_completion(worker)\n// May contain newly exported interfaces or callbacks.',
 'if waiter_already_completed(worker):\n    late_tx.discard_and_release_all()\nworker.release_receiver_once()',
]);
add('L09','TypeScript',[
 'const scope = await bamlSdk.bridgeScope();\nconst ref = await GreeterRef.bind(host, {scope});\nconst work = RunScoped_async(ref); // Fixture waits until admitted.',
 'const closing = scope.close(); // Revoke new calls immediately.',
 'ref.close();\n// closing still waits for actual admitted work and cleanup.',
 'await work.catch(handleCancellation);\nawait closing; // Only resolves after real drain.',
]);
bridge('L09',[
 'scope.state = OPEN\ncall = scope.admit_and_retain(receiver)',
 'scope.state = CLOSING  // Atomic with admission.\nscope.reject_new_calls()\nscope.request_cancellation()',
 'scope.retire_inactive_views()\n// Keep active-call roots until actual host work ends.',
 'await scope.drain_actual_work_and_cleanup()\nscope.state = CLOSED\nresolve_all_close_waiters_once()',
]);
bridge('L10',[
 'async host_callback(ctx):\n    // This call is itself part of ctx.scope.active_calls.',
 '    await ctx.scope.close()\n    // BAD if close waits for this callback: self-deadlock.',
 '// Proposed close policy:\nbegin_close(scope)  // Revoke without waiting.\nif current_call_is_in_drain_set(scope):\n    raise ScopeSelfDrainError  // Let this callback unwind.',
 'await outside_owner.close(scope)\n// An external waiter can now observe actual drain.',
]);
add('L11','Python',[
 'async with baml_sdk.bridge_scope() as scope:\n    host = HostState()\n    host.vm = await NewVmState_async()\n    ref = await HostStateRef.bind(host, scope=scope)\n    await host.vm.set_host(ref)\n    ref.close()  # Remove temporary binder ownership.',
 '    del host, ref\n    # Separate collectors still see each other\'s bridge roots.',
 '# Context exit awaits scope closure and actual work drain.\n# Retiring a bridge edge then makes this cycle collectible.',
]);
bridge('L11',[
 'H.owns(proxy_to(V))\nV.owns(interface_view_of(H))\nuser.owns(H)',
 'user.release(H)\ncollect_each_runtime_separately()\n// The cross-runtime cycle remains rooted.',
 'revoke_origin_scope()\nawait drain_actual_calls()\nretire_bridge_roots()\n// Removing a bridge edge makes this idle cycle collectible.',
]);
add('L12','Python',[
 'token = DetachedReleaseToken(runtime_generation, lease_id)\nfinalizer = weakref.finalize(proxy, enqueue_release, token)\n# token must not retain proxy or proxy.close.',
 'del proxy\n# If collected: finalize enqueues token. No await or BAML reentry.',
 '# Runtime-owned release queue:\nrelease_once(token)  # Same token used by explicit proxy.close().',
 '# Shutdown retires/drains the release table before executor teardown.\n# A later finalizer token is generation-checked and harmless.',
], '', 'SDK implementation sketch, not code users need to write. Python collection timing is not a correctness oracle.');
bridge('L13',[
 'token = {runtime: A, generation: 7, key: 12, witness: W}',
 'invoke_in(B, token)\n// Reject wrong runtime before adopting a lease or dispatching.',
 'await A.shutdown()\nretire_generation(7)\n// Surviving SDK wrappers are closed tombstones.',
 'new_token = {runtime: A2, generation: 8, key: 12, witness: W2}\ninvoke_in(A2, token) // Still rejected: old generation and owner.',
]);
bridge('L14',[
 'image = OwnedImageBuffer(bytes, "image/png")\nview = project_as_media(image, runtime)\n// Both retain the immutable buffer independently.',
 'await runtime.shutdown()\n// view is closed; image.bytes are still owned native data.',
 'drop(image)\n// Last engine-independent buffer owner releases the bytes.',
]);
add('L15','Python',[
 'async def greet(self, name):  # BAML → Python callback',
 '    return await OtherBamlFunction_async(self, name)\n    # Python → BAML reentry, with independent call ownership.',
 '# If the inner call throws: release its temporary transfer/call state.',
 '# If the outer call propagates it: release that frame too.\n# Any adopted live children in an error retain their own leases.',
]);
bridge('L16',[
 'source = DeclaredImplementation(host, fixed_interface_bundle)',
 'A, B = concurrent_calls(source)\nentry = registry.singleflight(source, runtime, generation, scope)\n// Publish at most one registration for this key.',
 'cancel(A)\nA.release_own_obligations()\n// B still owns the shared publication and its call lease.',
 'B.complete_and_release()\nsource.close()\n// Failed initialization must roll back all staged ownership.',
]);
bridge('L17',[
 'ref_A = bind(host, scope=A)\nref_A.origin_scope = A',
 'ref_B = pass_or_project(ref_A, caller_scope=B)\nassert ref_B.origin_scope == A  // No authority laundering.',
 'A.begin_close()\ninvoke(ref_B) // Rejected even if B remains open.',
 'await A.drain_and_retire()\n// Drop registration roots; a native application owner survives.',
 'application.drop(host)\n// New explicit registration would have a distinct identity.',
]);
bridge('L18',[
 'T = transfers.prepare(owned_tokens)\n// T is the sole transfer owner.',
 'receiver.stage(T)\nreceiver.validate_entire_aggregate(T)\n// Nothing has escaped to user code.',
 'outcome = T.commit_or_cancel_atomically()\nif outcome == COMMITTED:\n    receiver.adopt_all(T)\nelse:\n    receiver.rollback_all(T)',
 'on_duplicate_or_lost_ack(T):\n    return ledger.recorded_outcome(T)\n    // Never duplicate-retain or double-release.',
 'receiver_proxy.close()\nretire_transaction_record_when_duplicate_delivery_is_safe(T)',
]);
bridge('L19',[
 'worker = spawn_native_work(receiver)\nworker.retain_receiver()',
 'scope.begin_close()\nrequest_cancel(worker)\n// Native work ignores cancellation and continues running.',
 'if close_timeout_expires():\n    report_incomplete_close()\n    retain_safe_runtime_and_worker_state()\n    // Never free storage still accessible by the worker.',
]);
add('L20','Python',[
 'connection = await open_connection()\nhost = DatabaseAdapter(connection)\nref = await DatabaseRef.bind(host)',
 'ref.close()\n# This releases a bridge reference; it does not own connection.close.',
 'await connection.aclose()  # Explicit application resource cleanup.',
 'del ref, host, connection\n# A separately specified owned-resource adapter could automate this.',
]);
const py={
 M01:['saved = await counter.get_count()  # saved == 1','python_owner.count = 2  # Direct write to the retained native owner.','current = await ReadInBaml_async(counter)  # 2; saved is still 1.','await WriteInBaml_async(counter, 3)  # python_owner.count becomes 3.'],
 M02:['counter = await GetCounter_async()  # A live proxy.','counter.count = 9  # Rejected: cannot create a shadow property.','await counter.set_count(9)  # Owner commits before success returns.'],
 M03:['state = await MakeStateWithSharedFields_async()\n# state.items and state.also_items refer to the same list A.','items = await state.get_items()  # Checked ListRef to A.','await items.push(2)  # Both fields now observe [1, 2].','state.close()  # Child view still owns A.','items.close()  # Last remaining owner of A in this scenario.'],
 M04:['state = await MakeState_async([1])\nold = await state.get_items()  # Alias to list A.','await state.set_items([99])  # New storage B from native data.','await old.push(2)  # A = [1, 2]; state.items still names B = [99].','state.close()  # Parent and its new B can be collected.','old.close()  # A can now be collected too.'],
 M05:['state = await BindHostState_async(host)\nvm_items = await NewSharedList_async([7])','await state.set_items(vm_items)  # Retain B; do not copy into list.','await host.items.push(8)  # vm_items also observes [7, 8].','state.close()  # VM-owned B still has its own local reference.','vm_items.close()'],
 M06:['result = await agent.run(spec)\n# Copied outer record; live result.journal.','await result.journal.append(entry)  # Mutates shared J.','result.journal = other_journal  # Local record replacement only.','# Drop all independent journal owners when done.\ndel result, other_journal\nawait ForgetEngineJournal_async()'],
 M07:['counter = await NewCounter_async(0)','a = await counter.get_count()  # Task A reads 0.\nb = await counter.get_count()  # Task B also reads 0.','await counter.set_count(a + 1)  # Commits 1.','await counter.set_count(b + 1)  # Also commits 1: lost update.','await counter.set_count(0)  # Restart the comparison.','await asyncio.gather(counter.increment(), counter.increment())\n# Result 2 ONLY if increment implements synchronization.'],
 M08:['counter = await NewCounter_async(1)','task = asyncio.create_task(counter.set_count(5))\nawait setter_admitted.wait()  # Fixture barrier.','await owner_committed.wait()  # Owner has already written 5.','task.cancel()\n# Cancellation is not rollback; the owner may already contain 5.'],
 M09:['host = CounterModel(count=1)\nref = await CounterRef.bind(host)','await ref.set_count("2")  # Rejected; count stays 1.','object.__setattr__(host, "count", "invalid")  # Native bypass.','await ref.get_count()  # Rejected before producing a BAML int.','host.count = 2\nassert await ref.get_count() == 2'],
 M10:['items = await NewSharedList_async([1])','copy = await items.to_value()  # Supported data only; detached.','copy.append(2)  # copy = [1, 2]; live list remains [1].','items.close()\nassert copy == [1, 2]  # No runtime needed for this native data.'],
 M11:['parent = await GetScopedState_async()\nchild = await parent.get_items()','parent.close()\n# child still owns its storage while the scope remains open.','closing = asyncio.create_task(scope.close())\nawait scope_revoked.wait()  # Fixture barrier.\nawait child.push(2)  # Rejected after origin-scope revocation.','child.close()\nawait closing  # Resolves only after actual work drains.'],
 M12:['items = await NewSharedList_async([])\n# Two tasks hold the same storage identity.','await items.push(1)  # Task A commits one operation.','await items.push(2)  # Task B commits one operation.\n# Both retained; reverse commit order could yield [2, 1].','# Multiple reads / iteration are NOT implicitly a transaction.\n# Specify their consistency independently of single-operation atomicity.'],
 M13:['state = await MakeNestedState_async()\nold_child = await state.get_child()','await old_child.set_value(12)\nvalues = await state.get_values()\nawait values.set("a", 11)\nawait values.set("b", 2)\n# A BAML default method reads the updated child and map.','await state.set_child(Child(value=8))\nawait old_child.set_value(120)\n# Default method sees the NEW child value 8.','old_child.close()\nvalues.close()\nstate.close()'],
};
for(const [id,blocks]of Object.entries(py))add(id,'Python',blocks);
add('M01','BAML',['let saved = counter.count; // saved == 1','// Host performs: python_owner.count = 2','let current = counter.count; // 2; saved remains 1','counter.count = 3; // Dispatches to the same host owner.']);
add('M03','BAML',['// Fixture: state.items and state.also_items refer to list A.','let items = state.items; // A shared child, not a copy.','items.push(2); // Both fields observe [1, 2].','// Parent becomes unreachable; items still retains A.','// When items also becomes unreachable, A can be collected.']);
add('M04','BAML',['let old = state.items; // A = [1]','state.items = [99]; // New list B','old.push(2); // A = [1, 2]; B stays [99]','// Parent becomes unreachable. B loses its parent edge.','// Old alias becomes unreachable. A loses its last edge.']);
add('M05','BAML',['let baml_items = [7]; // Separate from host-owned state.items.','state.items = baml_items; // Foreign field must retain this identity.','// Host calls: await host.items.push(8)\n// baml_items is now [7, 8].','// Host parent is released; baml_items still owns the shared list.','// The last BAML alias becomes unreachable.']);
add('M07','BAML',['// counter.count starts at 0.','let a = counter.count; // Task A\nlet b = counter.count; // Task B','counter.count = a + 1; // Task A writes 1.','counter.count = b + 1; // Task B also writes 1.','counter.count = 0;','counter.increment(); // Task A\ncounter.increment(); // Task B\n// Must use actual owner-side synchronization to guarantee 2.']);
add('M04','TypeScript',['const state = await MakeState_async([1]);\nconst old = await state.get_items(); // List A','await state.set_items([99]); // New list B','await old.push(2); // A = [1, 2]; B = [99]','state.close(); // Drops the parent, not the old alias.','old.close();']);
add('M04','Rust',['let state = MakeState_async(vec![1], &ctx).await?;\nlet old = state.get_items(&ctx).await?; // List A','state.set_items(vec![99], &ctx).await?; // Native initializer → B','old.push(2, &ctx).await?; // A changes; B stays [99].','drop(state); // old still owns A.','drop(old);']);
add('M01','TypeScript',['const saved = await counter.get_count(); // 1','nativeOwner.count = 2;','const current = await ReadInBaml_async(counter); // 2','await WriteInBaml_async(counter, 3); // Native owner now has 3.']);
add('M01','Rust',['let saved = counter.get_count(&ctx).await?; // 1','host.count.store(2, Ordering::SeqCst); // Example atomic owner storage.','let current = ReadInBaml_async(&counter, &ctx).await?; // 2','WriteInBaml_async(&counter, 3, &ctx).await?; // Host storage becomes 3.']);
add('M02','TypeScript',['const counter = await GetCounter_async();','counter.count = 9; // TS error and JS runtime guard.','await counter.set_count(9);']);
add('M02','Rust',['let counter = GetCounter_async(&ctx).await?;','counter.count = 9; // Compile error: no assignable proxy field.','counter.set_count(9, &ctx).await?;']);
add('M03','TypeScript',['const state = await MakeStateWithSharedFields_async();','const items = await state.get_items(); // ListRef, not number[]','await items.push(2); // Both fields observe [1, 2].','state.close();','items.close();']);
add('M03','Rust',['let state = MakeStateWithSharedFields_async(&ctx).await?;','let items = state.get_items(&ctx).await?; // Shared child','items.push(2, &ctx).await?;','drop(state); // items remains usable.','drop(items);']);
add('M10','TypeScript',['const items = await NewSharedList_async([1]);','const copy = await items.to_value(); // Native number[]','copy.push(2); // Does not mutate shared storage.','items.close();\n// copy remains [1, 2].']);
add('M10','Rust',['let items = NewSharedList_async(vec![1], &ctx).await?;','let mut copy = items.to_value(&ctx).await?; // Native Vec','copy.push(2); // No shared-storage mutation.','drop(items);\nassert_eq!(copy, vec![1, 2]);']);
if(typeof module!=='undefined'&&module.exports)module.exports=examples;else root.BamlReviewExamples=examples;
})(typeof globalThis!=='undefined'?globalThis:this);
